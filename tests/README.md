# Integration Tests

This directory contains Python-based integration tests for the kairosdb-proxy project.

## Overview

The integration tests validate that payloads remain consistent when passed through the proxy. The tests use Python mock servers to simulate 3 KairosDB instances.

## Components

- **`mock_kairosdb_server.py`**: Bottle.py-based mock server that simulates KairosDB behavior (lightweight, minimal dependencies)
- **`launch_mock_servers.py`**: Script to launch 3 mock KairosDB servers on different ports
- **`test_integration.py`**: pytest-based integration test suite
- **`test_config.toml`**: Configuration file for the proxy during testing
- **`requirements.txt`**: Python dependencies (minimal: bottle, pytest, requests)

## Mock Servers

The mock servers run on the following ports:
- `kairosdb-1`: port 8081 (handles `cpu.*` metrics)
- `kairosdb-2`: port 8082 (handles `mem.*` metrics)
- `kairosdb-3`: port 8083 (handles all other metrics)

Each mock server:
- Captures incoming request payloads
- Returns consistent KairosDB-style responses
- Provides debug endpoints to inspect received requests

## Running Tests Locally

### Prerequisites

```bash
# Install Python dependencies
pip install -r requirements.txt

# Build the proxy
cargo build --manifest-path ../kairos-proxy/Cargo.toml --release
```

### Run Integration Tests

```bash
# From the tests directory
pytest test_integration.py -v

# Or with more verbose output
pytest test_integration.py -v -s
```

### Manual Testing

You can also run the components manually:

```bash
# Terminal 1: Start mock servers
python launch_mock_servers.py

# Terminal 2: Start the proxy
KAIROS_PROXY_CONFIG=tests/test_config.toml ./target/release/kairos-proxy

# Terminal 3: Run tests
pytest tests/test_integration.py -v
```

## Test Coverage

The integration tests cover:

1. **Payload Consistency**: Validates that payloads sent through the proxy remain unaltered
2. **Routing Logic**: Ensures requests are routed to the correct backend based on metric patterns
3. **Health Checks**: Tests the `/health` endpoint
4. **Query Endpoints**: Tests both `/api/v1/datapoints/query` and `/api/v1/datapoints/query/tags`
5. **Complex Queries**: Validates handling of queries with aggregators and multiple tags
6. **Response Consistency**: Ensures mock servers return properly formatted responses

## CI/CD Integration

These tests run automatically in GitHub Actions on merges to the `main` branch. See `.github/workflows/integration-tests.yml` for the workflow configuration.
