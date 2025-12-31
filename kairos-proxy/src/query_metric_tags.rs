use crate::state::AppState;
use axum::{
    body::{Body, StreamBody},
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures::{stream::FuturesUnordered, StreamExt};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Minimal struct for extracting only the metric name during routing.
/// This avoids deep allocations and HashMap creation from serde_json::Value.
#[derive(serde::Deserialize)]
struct MetricNameOnly {
    name: String,
}

/// Minimal struct for extracting first metric's name from request body.
#[derive(serde::Deserialize)]
struct MetricsRequest {
    metrics: Vec<MetricNameOnly>,
}

/// Helper function to forward a request to a backend in Simple mode
async fn forward_to_backend_simple(
    state: &AppState,
    url: &reqwest::Url,
    token: &Option<String>,
    body_bytes: Bytes,
    headers: &hyper::HeaderMap,
    endpoint: &str,
) -> Result<Response, StatusCode> {
    // Build request URL using Url::join to avoid repeated parsing
    let request_url = url
        .join(endpoint.trim_start_matches('/'))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let mut builder = state.client.post(request_url).body(body_bytes);
    for (name, value) in headers.iter() {
        if name == hyper::http::header::HOST {
            continue;
        }
        builder = builder.header(name, value);
    }
    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {}", t));
    }
    let resp = builder.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    // Clone backend headers before consuming the body
    let mut headers = resp.headers().clone();
    let status = resp.status();

    // Stream the backend response body directly to the client to keep memory usage low
    let stream = resp
        .bytes_stream()
        .map(|res| res.map_err(|e| std::io::Error::other(format!("upstream error: {}", e))));
    let body = StreamBody::new(stream);

    const HOP_BY_HOP: [&str; 9] = [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailers",
        "transfer-encoding",
        "upgrade",
        "host",
    ];
    for name in HOP_BY_HOP.iter() {
        headers.remove(*name);
    }

    let resp_builder = hyper::Response::builder().status(status);
    let mut response = resp_builder
        .body(body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    response.headers_mut().extend(headers);
    Ok(response.into_response())
}

pub async fn query_metric_tags_handler(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    debug!("Received query_metric_tags request");

    // Only POST is allowed for this endpoint
    if req.method() != axum::http::Method::POST {
        warn!("Method not allowed: {}", req.method());
        return Err(StatusCode::METHOD_NOT_ALLOWED);
    }

    // Read and parse the JSON body
    let mut req = req;
    let body_bytes = match to_bytes(req.body_mut(), state.max_request_body_bytes).await {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to read request body: {:?}", e);
            return Err(e);
        }
    };

    // If running in simple mode, check for X-METRICNAME header first
    if matches!(state.mode, crate::config::Mode::Simple) {
        // Check for X-METRICNAME header (case-insensitive)
        let metric_name_from_header = req
            .headers()
            .get("x-metricname")
            .and_then(|value| value.to_str().ok())
            .map(|s| s.to_string());

        let metric_name = if let Some(name) = metric_name_from_header {
            // Use header value for routing, skip body parsing
            name
        } else {
            // Parse JSON body using minimal typed deserialization to extract only metric name.
            // This avoids deep allocations and HashMap creation from serde_json::Value.
            let request: MetricsRequest = match serde_json::from_slice(&body_bytes) {
                Ok(r) => r,
                Err(_) => return Err(StatusCode::BAD_REQUEST),
            };

            // Extract first metric name (move out of Vec to avoid clone)
            request
                .metrics
                .into_iter()
                .next()
                .ok_or(StatusCode::BAD_REQUEST)?
                .name
        };

        // Find backend matching the metric name
        let mut target: Option<(&reqwest::Url, Option<String>)> = None;
        for (re, url, token) in state.backends.iter() {
            if re.is_match(&metric_name) {
                target = Some((url, token.clone()));
                break;
            }
        }
        let (url, token) = match target {
            Some(t) => t,
            None => return Err(StatusCode::BAD_GATEWAY),
        };

        // Forward request to chosen backend using helper function
        return forward_to_backend_simple(
            &state,
            url,
            &token,
            body_bytes,
            req.headers(),
            "/api/v1/datapoints/query/tags",
        )
        .await;
    }

    // Parse JSON body for metric extraction (Multi mode)
    let mut json: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(j) => j,
        Err(e) => {
            error!("Failed to parse JSON body: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Extract metrics array
    let metrics = json.get_mut("metrics").and_then(|v| v.as_array_mut());
    let metrics = match metrics {
        Some(m) if !m.is_empty() => m,
        _ => {
            warn!("Request contains no metrics or invalid metrics array");
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Group metrics by backend (Multi mode)
    use std::collections::HashMap;
    let mut backend_metrics: HashMap<usize, Vec<serde_json::Value>> = HashMap::new();
    let mut backend_info: HashMap<usize, (&reqwest::Url, Option<&str>)> = HashMap::new();

    debug!(
        "Processing tags request in Multi mode with {} metric(s)",
        metrics.len()
    );

    for metric in metrics.iter() {
        let name = metric.get("name").and_then(|v| v.as_str());
        let mut found = false;
        if let Some(name) = name {
            for (i, (re, url, token)) in state.backends.iter().enumerate() {
                if re.is_match(name) {
                    backend_metrics.entry(i).or_default().push(metric.clone());
                    backend_info.insert(i, (url, token.as_deref()));
                    debug!("Metric '{}' matched backend: {}", name, url);
                    found = true;
                    break;
                }
            }
        }
        if !found {
            error!("No backend matched metric: {:?}", name);
            return Err(StatusCode::BAD_GATEWAY);
        }
    }

    info!(
        "Routing {} tags query metric(s) to {} backend(s)",
        metrics.len(),
        backend_metrics.len()
    );

    // Clone headers once to reuse for outbound requests
    let headers = req.headers().clone();

    // Store the count before the move
    let backend_count = backend_metrics.len();

    // For each backend, send a request with only the relevant metrics using bounded concurrency
    let mut futs = FuturesUnordered::new();
    for (i, metrics_for_backend) in backend_metrics {
        let (url, token) = backend_info[&i];
        let client = state.client.clone();
        let headers = headers.clone();
        let sem = state.semaphore.clone();
        // Build a small payload: copy top-level fields except "metrics", insert only relevant metrics
        let mut payload_map = serde_json::Map::new();
        if let Some(obj) = json.as_object() {
            for (k, v) in obj.iter() {
                if k == "metrics" {
                    continue;
                }
                payload_map.insert(k.clone(), v.clone());
            }
        }
        payload_map.insert(
            "metrics".to_string(),
            serde_json::Value::Array(metrics_for_backend),
        );
        let body = match serde_json::to_vec(&serde_json::Value::Object(payload_map)) {
            Ok(b) => b,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };

        futs.push(async move {
            // Acquire permit
            let _permit = match sem.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return None,
            };
            
            // Build request URL using Url::join to avoid repeated parsing
            let request_url = match url.join("api/v1/datapoints/query/tags") {
                Ok(u) => u,
                Err(e) => {
                    error!("Failed to build request URL: {}", e);
                    return None;
                }
            };
            
            let mut builder = client.post(request_url).body(body);
            for (name, value) in headers.iter() {
                if name == hyper::http::header::HOST {
                    continue;
                }
                builder = builder.header(name, value);
            }
            if let Some(t) = token {
                builder = builder.header("Authorization", format!("Bearer {}", t));
            }
            match builder.send().await {
                Ok(r) => r.json::<serde_json::Value>().await.ok(),
                Err(e) => {
                    error!("Backend request to {} failed: {}", url, e);
                    None
                }
            }
        });
    }

    let mut results = Vec::new();
    while let Some(res_opt) = futs.next().await {
        if let Some(res) = res_opt {
            results.push(res);
        }
    }
    debug!("Received {} response(s) from backend(s)", results.len());
    // Merge all backend responses into queries[0].results[] by metric name
    use std::collections::BTreeMap;
    // Map: metric name -> Vec<result objects from all backends>
    let mut metric_results: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    for resp in results.into_iter() {
        if let Some(queries) = resp.get("queries").and_then(|q| q.as_array()) {
            for query in queries {
                if let Some(results) = query.get("results").and_then(|r| r.as_array()) {
                    for result in results {
                        if let Some(name) = result.get("name").and_then(|v| v.as_str()) {
                            metric_results
                                .entry(name.to_string())
                                .or_default()
                                .push(result.clone());
                        }
                    }
                }
            }
        }
    }
    // Merge tags and values for each metric
    let mut merged_results = Vec::new();
    for (name, result_vec) in metric_results {
        let mut merged_tags: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut merged_values: Vec<serde_json::Value> = Vec::new();
        for result in result_vec {
            // Merge tags
            if let Some(tags) = result.get("tags").and_then(|t| t.as_object()) {
                for (k, v) in tags {
                    if let Some(arr) = v.as_array() {
                        for val in arr {
                            if let Some(s) = val.as_str() {
                                let entry = merged_tags.entry(k.clone()).or_default();
                                if !entry.contains(&s.to_string()) {
                                    entry.push(s.to_string());
                                }
                            }
                        }
                    }
                }
            }
            // Merge values
            if let Some(values) = result.get("values").and_then(|v| v.as_array()) {
                for v in values {
                    merged_values.push(v.clone());
                }
            }
        }
        // Build merged result object
        let mut merged_result = serde_json::Map::new();
        merged_result.insert("name".to_string(), serde_json::Value::String(name));
        // Insert merged tags
        let tags_obj = merged_tags
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    serde_json::Value::Array(
                        v.into_iter().map(serde_json::Value::String).collect(),
                    ),
                )
            })
            .collect();
        merged_result.insert("tags".to_string(), serde_json::Value::Object(tags_obj));
        // Insert merged values
        merged_result.insert(
            "values".to_string(),
            serde_json::Value::Array(merged_values),
        );
        merged_results.push(serde_json::Value::Object(merged_result));
    }
    // Build final response: { "queries": [ { "results": [ ... ] } ] }
    let mut queries_arr = Vec::new();
    let mut query_obj = serde_json::Map::new();
    query_obj.insert(
        "results".to_string(),
        serde_json::Value::Array(merged_results),
    );
    queries_arr.push(serde_json::Value::Object(query_obj));
    let mut response = serde_json::Map::new();
    response.insert("queries".to_string(), serde_json::Value::Array(queries_arr));
    let v = serde_json::Value::Object(response);
    info!(
        "Successfully merged tags responses from {} backend(s)",
        backend_count
    );
    Ok((StatusCode::OK, axum::Json(v)).into_response())
}

// Helper to read the full body with size limit
async fn to_bytes(body: &mut Body, max_size: usize) -> Result<Bytes, StatusCode> {
    use axum::body::HttpBody;
    use bytes::BytesMut;

    let mut buf = BytesMut::new();
    let mut total_size: usize = 0;

    while let Some(chunk_res) = body.data().await {
        let chunk = match chunk_res {
            Ok(chunk) => chunk,
            Err(_) => return Err(StatusCode::BAD_REQUEST),
        };

        // Check for overflow and size limit
        total_size = match total_size.checked_add(chunk.len()) {
            Some(new_size) if new_size <= max_size => new_size,
            _ => return Err(StatusCode::PAYLOAD_TOO_LARGE),
        };

        buf.extend_from_slice(&chunk);
    }

    Ok(buf.freeze())
}
