# KairosDB Proxy Helm Chart

This Helm chart deploys the KairosDB Proxy to a Kubernetes cluster. The proxy routes KairosDB REST requests to different backends based on metric names, providing a single endpoint for multiple KairosDB instances.

## Prerequisites

- Kubernetes 1.31+
- Helm 3.0+
- Access to the container registry hosting the KairosDB Proxy image

## Installation

### Quick Install

Install the chart with default values:

```bash
helm install kairosdb-proxy . -n <namespace>
```

### Install with Custom Values

Create a custom values file or override specific values:

```bash
helm install kairosdb-proxy . -n <namespace> -f custom-values.yaml
```

Or use `--set` flags:

```bash
helm install kairosdb-proxy . -n <namespace> \
  --set image.tag=v1.00.00-latest \
  --set replicaCount=3
```

## Configuration

### Image Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `image.repository` | Container image repository | `kairosdb-proxy` |
| `image.tag` | Container image tag | `v1.00.00-7cdccce` |
| `image.pullPolicy` | Image pull policy | `Always` |
| `image.pullSecrets` | Image pull secrets | `[azregistry]` |

### Service Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `service.type` | Kubernetes service type | `NodePort` |
| `service.port` | Service port | `8080` |
| `service.nodePort` | NodePort (when type is NodePort) | `30126` |
| `service.annotations` | Service annotations | `{}` |

### Resource Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `resources.limits.cpu` | CPU limit | `1000m` |
| `resources.limits.memory` | Memory limit | `2Gi` |
| `resources.requests.cpu` | CPU request | `100m` |
| `resources.requests.memory` | Memory request | `256Mi` |

### Environment Variables

| Parameter | Description | Default |
|-----------|-------------|---------|
| `env.KAIROS_PROXY_CONFIG` | Path to config file | `/app/config.toml` |
| `env.LOG_LEVEL` | Logging level | `info` |

### Health Probes

| Parameter | Description | Default |
|-----------|-------------|---------|
| `probes.readiness.path` | Readiness probe path | `/health` |
| `probes.readiness.initialDelaySeconds` | Initial delay for readiness probe | `5` |
| `probes.readiness.periodSeconds` | Period for readiness probe | `10` |
| `probes.liveness.path` | Liveness probe path | `/health` |
| `probes.liveness.initialDelaySeconds` | Initial delay for liveness probe | `15` |
| `probes.liveness.periodSeconds` | Period for liveness probe | `10` |

### Proxy Configuration

The proxy configuration is stored in `config.toml` and mounted as a ConfigMap. To customize the proxy behavior, modify [config.toml](config.toml) before deploying.

Key configuration sections in `config.toml`:
- `listen`: Proxy listen address (default: `0.0.0.0:8080`)
- `backends`: List of KairosDB backends with metric name patterns and URLs
- `timeout_secs`: Request timeout for backend calls
- `max_concurrent_requests`: Maximum concurrent requests to backends

## Deployment Architecture

The Helm chart creates the following Kubernetes resources:

1. **Deployment**: Runs the KairosDB Proxy container(s)
2. **Service**: Exposes the proxy via NodePort (or configurable type)
3. **ConfigMap**: Stores the `config.toml` configuration file

### Configuration Updates

When you update `config.toml`, the deployment automatically detects changes via a checksum annotation and triggers a rolling update.

## Usage Examples

### Example 1: Deploy with LoadBalancer Service

```bash
helm install kairosdb-proxy . -n monitoring \
  --set service.type=LoadBalancer \
  --set service.nodePort=null
```

### Example 2: Scale to Multiple Replicas

```bash
helm upgrade kairosdb-proxy . -n monitoring \
  --set replicaCount=3
```

### Example 3: Custom Resource Limits

```bash
helm upgrade kairosdb-proxy . -n monitoring \
  --set resources.limits.cpu=2000m \
  --set resources.limits.memory=4Gi
```

### Example 4: Update Configuration

1. Edit `config.toml` with your backend configurations
2. Upgrade the release:

```bash
helm upgrade kairosdb-proxy . -n monitoring
```

The deployment will automatically restart with the new configuration.

## Upgrading

To upgrade an existing release:

```bash
helm upgrade kairosdb-proxy . -n <namespace>
```

## Uninstalling

To uninstall/delete the deployment:

```bash
helm uninstall kairosdb-proxy -n <namespace>
```

This command removes all Kubernetes resources associated with the chart.

## Troubleshooting

### Check Pod Status

```bash
kubectl get pods -n <namespace> -l app.kubernetes.io/name=kairosdb-proxy
```

### View Logs

```bash
kubectl logs -n <namespace> -l app.kubernetes.io/name=kairosdb-proxy
```

### Check Configuration

```bash
kubectl get configmap -n <namespace> kairosdb-proxy-config -o yaml
```

### Test Health Endpoint

```bash
kubectl port-forward -n <namespace> svc/kairosdb-proxy-svc 8080:8080
curl http://localhost:8080/health
```

### Common Issues

1. **ImagePullBackOff**: Verify image registry credentials and image pull secrets
2. **CrashLoopBackOff**: Check logs and ensure `config.toml` is properly formatted
3. **Service not accessible**: Verify NodePort is not conflicting with other services

## Additional Resources

- [Main Project README](../../README.md)
- [KairosDB Proxy Source](../../kairos-proxy/)
- [Example Configuration](../../kairos-proxy/config.toml.example)

## Support

For issues and questions, please refer to the main project repository.
