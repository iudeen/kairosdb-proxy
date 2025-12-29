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

/// Helper function to forward a request to a backend in Simple mode
async fn forward_to_backend_simple(
    state: &AppState,
    url: &str,
    token: &Option<String>,
    body_bytes: Bytes,
    headers: &hyper::HeaderMap,
    endpoint: &str,
) -> Result<Response, StatusCode> {
    // Forward request to chosen backend
    let mut builder = state
        .client
        .post(format!("{}{}", url.trim_end_matches('/'), endpoint))
        .body(body_bytes);
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

    // Standard hop-by-hop headers that should not be forwarded
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
    // Attach remaining headers from backend
    response.headers_mut().extend(headers);
    Ok(response.into_response())
}

pub async fn query_metric_handler(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
) -> Result<Response, StatusCode> {
    debug!("Received query_metric request");

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
            .iter()
            .find(|(name, _)| name.as_str().eq_ignore_ascii_case("x-metricname"))
            .and_then(|(_, value)| value.to_str().ok())
            .map(|s| s.to_string());

        let metric_name = if let Some(name) = metric_name_from_header {
            // Use header value for routing, skip body parsing
            name
        } else {
            // Parse JSON body for metric extraction
            let json: serde_json::Value = match serde_json::from_slice(&body_bytes) {
                Ok(j) => j,
                Err(_) => return Err(StatusCode::BAD_REQUEST),
            };

            // Extract first metric name from body
            let first_metric = json
                .get("metrics")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .ok_or(StatusCode::BAD_REQUEST)?;

            first_metric
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or(StatusCode::BAD_REQUEST)?
                .to_string()
        };

        // Find backend matching the metric name
        let mut target: Option<(String, Option<String>)> = None;
        for (re, url, token) in state.backends.iter() {
            if re.is_match(&metric_name) {
                target = Some((url.clone(), token.clone()));
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
            &url,
            &token,
            body_bytes,
            req.headers(),
            "/api/v1/datapoints/query",
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
    let mut backend_info: HashMap<usize, (&str, Option<&str>)> = HashMap::new();

    debug!(
        "Processing request in Multi mode with {} metric(s)",
        metrics.len()
    );

    for metric in metrics.iter() {
        let name = metric.get("name").and_then(|v| v.as_str());
        let mut found = false;
        if let Some(name) = name {
            for (i, (re, url, token)) in state.backends.iter().enumerate() {
                if re.is_match(name) {
                    backend_metrics.entry(i).or_default().push(metric.clone());
                    backend_info.insert(i, (url.as_str(), token.as_deref()));
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
        "Routing {} metric(s) to {} backend(s)",
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
            // Acquire permit for bounded concurrency
            let _permit = match sem.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return None,
            };

            let mut builder = client
                .post(format!(
                    "{}{}",
                    url.trim_end_matches('/'),
                    "/api/v1/datapoints/query"
                ))
                .body(body);
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
            // permit dropped here
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
        "Successfully merged responses from {} backend(s)",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Backend, Config, Mode};
    use axum::{routing::post, Router};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    async fn spawn_mock_server() -> (String, Arc<Mutex<Option<serde_json::Value>>>) {
        let received: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
        let rec1 = received.clone();
        let rec2 = received.clone();

        let app = Router::new()
            .route(
                "/api/v1/datapoints/query",
                post(move |body: bytes::Bytes| {
                    let rec = rec1.clone();
                    async move {
                        let v: serde_json::Value =
                            serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
                        *rec.lock().await = Some(v.clone());
                        // echo back a KairosDB-style response with one result per metric
                        let mut results = Vec::new();
                        if let Some(metrics) = v.get("metrics").and_then(|m| m.as_array()) {
                            for m in metrics {
                                if let Some(name) = m.get("name").and_then(|s| s.as_str()) {
                                    results.push(json!({
                                        "name": name,
                                        "tags": {},
                                        "values": []
                                    }));
                                }
                            }
                        }
                        let resp = json!({ "queries": [{ "results": results }] });
                        (axum::http::StatusCode::OK, axum::Json(resp))
                    }
                }),
            )
            .route(
                "/api/v1/datapoints/query/tags",
                post(move |body: bytes::Bytes| {
                    let rec = rec2.clone();
                    async move {
                        let v: serde_json::Value =
                            serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
                        *rec.lock().await = Some(v.clone());
                        // respond similarly for tags endpoint
                        let mut results = Vec::new();
                        if let Some(metrics) = v.get("metrics").and_then(|m| m.as_array()) {
                            for m in metrics {
                                if let Some(name) = m.get("name").and_then(|s| s.as_str()) {
                                    results.push(json!({
                                        "name": name,
                                        "tags": {},
                                        "values": []
                                    }));
                                }
                            }
                        }
                        let resp = json!({ "queries": [{ "results": results }] });
                        (axum::http::StatusCode::OK, axum::Json(resp))
                    }
                }),
            );

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = axum::Server::from_tcp(listener)
            .expect("server")
            .serve(app.into_make_service());
        tokio::spawn(server);
        (format!("http://127.0.0.1:{}", addr.port()), received)
    }

    #[tokio::test]
    async fn multi_mode_splits_and_merges() {
        let (b1_url, _r1) = spawn_mock_server().await;
        let (b2_url, _r2) = spawn_mock_server().await;

        let cfg = Config {
            listen: None,
            backends: vec![
                Backend {
                    pattern: "^cpu\\..*".to_string(),
                    url: b1_url.clone(),
                    token: None,
                },
                Backend {
                    pattern: "^mem\\..*".to_string(),
                    url: b2_url.clone(),
                    token: None,
                },
            ],
            timeout_secs: Some(2),
            max_outbound_concurrency: Some(8),
            mode: Some(Mode::Multi),
            max_request_body_bytes: None,
        };
        let state = Arc::new(AppState::from_config(&cfg).expect("state"));

        let payload = json!({ "metrics": [ { "name": "cpu.test" }, { "name": "mem.test" } ] });
        let body = serde_json::to_vec(&payload).unwrap();
        let req = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/api/v1/datapoints/query")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let resp = query_metric_handler(State(state), req).await.expect("resp");
        let bytes = hyper::body::to_bytes(resp.into_body())
            .await
            .expect("bytes");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let results = v
            .get("queries")
            .and_then(|q| q.get(0))
            .and_then(|o| o.get("results"))
            .and_then(|r| r.as_array())
            .expect("results array");
        let names: Vec<String> = results
            .iter()
            .filter_map(|r| r.get("name"))
            .filter_map(|n| n.as_str())
            .map(|s| s.to_string())
            .collect();
        assert!(names.contains(&"cpu.test".to_string()));
        assert!(names.contains(&"mem.test".to_string()));
    }

    #[tokio::test]
    async fn simple_mode_forwards_full_payload_to_first_backend() {
        let (b1_url, r1) = spawn_mock_server().await;
        let (b2_url, _r2) = spawn_mock_server().await;

        let cfg = Config {
            listen: None,
            backends: vec![
                Backend {
                    pattern: "^cpu\\..*".to_string(),
                    url: b1_url.clone(),
                    token: None,
                },
                Backend {
                    pattern: "^mem\\..*".to_string(),
                    url: b2_url.clone(),
                    token: None,
                },
            ],
            timeout_secs: Some(2),
            max_outbound_concurrency: Some(8),
            mode: Some(Mode::Simple),
            max_request_body_bytes: None,
        };
        let state = Arc::new(AppState::from_config(&cfg).expect("state"));

        let payload = json!({ "metrics": [ { "name": "cpu.first" }, { "name": "mem.other" } ], "extra": { "k": "v" } });
        let body = serde_json::to_vec(&payload).unwrap();
        let req = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/api/v1/datapoints/query")
            .header("content-type", "application/json")
            .body(Body::from(body.clone()))
            .unwrap();

        let resp = query_metric_handler(State(state.clone()), req)
            .await
            .expect("resp");
        let bytes = hyper::body::to_bytes(resp.into_body())
            .await
            .expect("bytes");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        let results = v
            .get("queries")
            .and_then(|q| q.get(0))
            .and_then(|o| o.get("results"))
            .and_then(|r| r.as_array())
            .expect("results array");
        // In simple mode the full backend response is returned (both metrics)
        assert_eq!(results.len(), 2);
        let names: Vec<String> = results
            .iter()
            .filter_map(|r| r.get("name"))
            .filter_map(|n| n.as_str())
            .map(|s| s.to_string())
            .collect();
        assert!(names.contains(&"cpu.first".to_string()));
        assert!(names.contains(&"mem.other".to_string()));

        // ensure the backend received the full payload (two metrics)
        let rec = r1.lock().await;
        let received = rec.as_ref().expect("received payload");
        let metrics = received
            .get("metrics")
            .and_then(|m| m.as_array())
            .expect("metrics arr");
        assert_eq!(
            metrics.len(),
            2,
            "backend should have received full payload with two metrics"
        );
        // and extra field preserved
        assert_eq!(
            received
                .get("extra")
                .and_then(|e| e.get("k"))
                .and_then(|v| v.as_str())
                .unwrap(),
            "v"
        );
    }

    #[tokio::test]
    async fn simple_mode_uses_x_metricname_header_when_present() {
        let (b1_url, r1) = spawn_mock_server().await;
        let (b2_url, r2) = spawn_mock_server().await;

        let cfg = Config {
            listen: None,
            backends: vec![
                Backend {
                    pattern: "^cpu\\..*".to_string(),
                    url: b1_url.clone(),
                    token: None,
                },
                Backend {
                    pattern: "^mem\\..*".to_string(),
                    url: b2_url.clone(),
                    token: None,
                },
            ],
            timeout_secs: Some(2),
            max_outbound_concurrency: Some(8),
            mode: Some(Mode::Simple),
            max_request_body_bytes: None,
        };
        let state = Arc::new(AppState::from_config(&cfg).expect("state"));

        // Payload has cpu.first in body, but header specifies mem.test
        let payload = json!({ "metrics": [ { "name": "cpu.first" } ] });
        let body = serde_json::to_vec(&payload).unwrap();
        let req = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/api/v1/datapoints/query")
            .header("content-type", "application/json")
            .header("X-METRICNAME", "mem.test") // Header should take precedence
            .body(Body::from(body))
            .unwrap();

        let resp = query_metric_handler(State(state), req).await.expect("resp");
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        // Verify that b2 (mem backend) received the request, not b1 (cpu backend)
        let rec2 = r2.lock().await;
        assert!(
            rec2.is_some(),
            "mem backend should have received the request"
        );

        let rec1 = r1.lock().await;
        assert!(
            rec1.is_none(),
            "cpu backend should NOT have received the request"
        );
    }

    #[tokio::test]
    async fn simple_mode_header_case_insensitive() {
        let (b1_url, _r1) = spawn_mock_server().await;
        let (b2_url, r2) = spawn_mock_server().await;

        let cfg = Config {
            listen: None,
            backends: vec![
                Backend {
                    pattern: "^cpu\\..*".to_string(),
                    url: b1_url.clone(),
                    token: None,
                },
                Backend {
                    pattern: "^mem\\..*".to_string(),
                    url: b2_url.clone(),
                    token: None,
                },
            ],
            timeout_secs: Some(2),
            max_outbound_concurrency: Some(8),
            mode: Some(Mode::Simple),
            max_request_body_bytes: None,
        };
        let state = Arc::new(AppState::from_config(&cfg).expect("state"));

        let payload = json!({ "metrics": [ { "name": "cpu.first" } ] });
        let body = serde_json::to_vec(&payload).unwrap();

        // Test with different case variations
        for header_name in &["x-metricname", "X-MetricName", "X-METRICNAME"] {
            let req = Request::builder()
                .method(axum::http::Method::POST)
                .uri("/api/v1/datapoints/query")
                .header("content-type", "application/json")
                .header(*header_name, "mem.test")
                .body(Body::from(body.clone()))
                .unwrap();

            let resp = query_metric_handler(State(state.clone()), req)
                .await
                .expect("resp");
            assert_eq!(resp.status(), axum::http::StatusCode::OK);
        }

        // At least one request should have reached the mem backend
        let rec2 = r2.lock().await;
        assert!(rec2.is_some(), "mem backend should have received requests");
    }

    #[tokio::test]
    async fn simple_mode_falls_back_to_body_when_no_header() {
        let (b1_url, r1) = spawn_mock_server().await;
        let (b2_url, _r2) = spawn_mock_server().await;

        let cfg = Config {
            listen: None,
            backends: vec![
                Backend {
                    pattern: "^cpu\\..*".to_string(),
                    url: b1_url.clone(),
                    token: None,
                },
                Backend {
                    pattern: "^mem\\..*".to_string(),
                    url: b2_url.clone(),
                    token: None,
                },
            ],
            timeout_secs: Some(2),
            max_outbound_concurrency: Some(8),
            mode: Some(Mode::Simple),
            max_request_body_bytes: None,
        };
        let state = Arc::new(AppState::from_config(&cfg).expect("state"));

        // No X-METRICNAME header, should use body parsing
        let payload = json!({ "metrics": [ { "name": "cpu.first" } ] });
        let body = serde_json::to_vec(&payload).unwrap();
        let req = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/api/v1/datapoints/query")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let resp = query_metric_handler(State(state), req).await.expect("resp");
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        // Verify that b1 (cpu backend) received the request
        let rec1 = r1.lock().await;
        assert!(
            rec1.is_some(),
            "cpu backend should have received the request from body parsing"
        );
    }

    #[tokio::test]
    async fn request_body_size_limit_enforced() {
        let (b1_url, _r1) = spawn_mock_server().await;

        const TEST_SIZE_LIMIT: usize = 100;
        const LARGE_DATA_SIZE: usize = 200;
        
        let cfg = Config {
            listen: None,
            backends: vec![Backend {
                pattern: "^cpu\\..*".to_string(),
                url: b1_url.clone(),
                token: None,
            }],
            timeout_secs: Some(2),
            max_outbound_concurrency: Some(8),
            mode: Some(Mode::Simple),
            max_request_body_bytes: Some(TEST_SIZE_LIMIT),
        };
        let state = Arc::new(AppState::from_config(&cfg).expect("state"));

        // Create a request body larger than TEST_SIZE_LIMIT bytes
        let large_payload = json!({
            "metrics": [{
                "name": "cpu.test",
                "tags": {
                    "host": "server1",
                    "datacenter": "dc1"
                },
                "data": "x".repeat(LARGE_DATA_SIZE)
            }]
        });
        let body = serde_json::to_vec(&large_payload).unwrap();
        assert!(body.len() > TEST_SIZE_LIMIT, "test body should exceed size limit");

        let req = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/api/v1/datapoints/query")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let result = query_metric_handler(State(state), req).await;
        assert!(result.is_err(), "should return error for oversized body");
        assert_eq!(
            result.unwrap_err(),
            StatusCode::PAYLOAD_TOO_LARGE,
            "should return 413 Payload Too Large"
        );
    }

    #[tokio::test]
    async fn request_body_within_limit_succeeds() {
        let (b1_url, _r1) = spawn_mock_server().await;

        let cfg = Config {
            listen: None,
            backends: vec![Backend {
                pattern: "^cpu\\..*".to_string(),
                url: b1_url.clone(),
                token: None,
            }],
            timeout_secs: Some(2),
            max_outbound_concurrency: Some(8),
            mode: Some(Mode::Simple),
            max_request_body_bytes: Some(1000), // 1000 bytes limit
        };
        let state = Arc::new(AppState::from_config(&cfg).expect("state"));

        // Create a small request body well within the limit
        let payload = json!({ "metrics": [{ "name": "cpu.test" }] });
        let body = serde_json::to_vec(&payload).unwrap();
        assert!(body.len() < 1000, "test body should be under 1000 bytes");

        let req = Request::builder()
            .method(axum::http::Method::POST)
            .uri("/api/v1/datapoints/query")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let result = query_metric_handler(State(state), req).await;
        assert!(result.is_ok(), "should succeed for body within limit");
    }
}
