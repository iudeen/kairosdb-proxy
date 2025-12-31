"""
Integration tests for kairosdb-proxy.

These tests validate that:
1. Payloads are sent through the proxy and remain unaltered
2. Mock servers receive the correct payloads
3. The proxy routes requests to the correct backend based on metric names
4. Responses are returned correctly
"""

import pytest
import requests
import json
import time
import subprocess
import os
import signal
import sys


# Configuration
PROXY_URL = "http://127.0.0.1:8080"
MOCK_SERVERS = [
    {"port": 8081, "name": "kairosdb-1", "url": "http://127.0.0.1:8081"},
    {"port": 8082, "name": "kairosdb-2", "url": "http://127.0.0.1:8082"},
    {"port": 8083, "name": "kairosdb-3", "url": "http://127.0.0.1:8083"},
]


def wait_for_service(url, max_retries=30, timeout=1):
    """Wait for a service to become available."""
    for i in range(max_retries):
        try:
            resp = requests.get(f"{url}/health", timeout=timeout)
            if resp.status_code == 200:
                return True
        except requests.exceptions.RequestException:
            if i == max_retries - 1:
                return False
            time.sleep(0.5)
    return False


def clear_mock_server_requests():
    """Clear all stored requests on mock servers."""
    for server in MOCK_SERVERS:
        try:
            requests.post(f"{server['url']}/debug/clear", timeout=1)
        except:
            pass


def get_mock_server_requests(server_url):
    """Get all requests received by a mock server."""
    try:
        resp = requests.get(f"{server_url}/debug/requests", timeout=1)
        if resp.status_code == 200:
            return resp.json()
    except:
        pass
    return []


class TestProxyHealth:
    """Test the proxy health endpoint."""
    
    def test_proxy_health_endpoint(self):
        """Test that the proxy health endpoint responds correctly."""
        resp = requests.get(f"{PROXY_URL}/health", timeout=5)
        assert resp.status_code == 200
        data = resp.json()
        assert "status" in data
        assert data["status"] == "ok"


class TestPayloadConsistency:
    """Test that payloads remain consistent when passed through the proxy."""
    
    def setup_method(self):
        """Clear mock server state before each test."""
        clear_mock_server_requests()
    
    def test_simple_query_payload_consistency(self):
        """Test that a simple query payload passes through unchanged."""
        # Payload for cpu metric (should route to kairosdb-1)
        payload = {
            "start_relative": {
                "value": "1",
                "unit": "hours"
            },
            "metrics": [
                {
                    "name": "cpu.usage",
                    "tags": {
                        "host": ["server1"]
                    }
                }
            ]
        }
        
        # Send request through proxy
        resp = requests.post(
            f"{PROXY_URL}/api/v1/datapoints/query",
            json=payload,
            headers={"Content-Type": "application/json"},
            timeout=5
        )
        
        assert resp.status_code == 200
        
        # Verify the payload was received by the backend
        backend_requests = get_mock_server_requests(MOCK_SERVERS[0]["url"])
        assert len(backend_requests) > 0
        
        # Check that the payload matches
        received_payload = backend_requests[-1]["payload"]
        assert received_payload == payload
    
    def test_memory_metric_routing(self):
        """Test that memory metrics route to the correct backend."""
        payload = {
            "start_relative": {
                "value": "2",
                "unit": "hours"
            },
            "metrics": [
                {
                    "name": "mem.available",
                    "tags": {
                        "host": ["server2"]
                    }
                }
            ]
        }
        
        # Send request through proxy
        resp = requests.post(
            f"{PROXY_URL}/api/v1/datapoints/query",
            json=payload,
            headers={"Content-Type": "application/json"},
            timeout=5
        )
        
        assert resp.status_code == 200
        
        # Verify the payload was received by kairosdb-2 (mem backend)
        backend_requests = get_mock_server_requests(MOCK_SERVERS[1]["url"])
        assert len(backend_requests) > 0
        
        # Check that the payload matches
        received_payload = backend_requests[-1]["payload"]
        assert received_payload == payload
    
    def test_complex_query_payload_consistency(self):
        """Test that complex queries with aggregators pass through correctly."""
        payload = {
            "start_absolute": 1609459200000,
            "end_absolute": 1609545600000,
            "metrics": [
                {
                    "name": "cpu.load",
                    "tags": {
                        "host": ["server1", "server2"],
                        "dc": ["us-east"]
                    },
                    "aggregators": [
                        {
                            "name": "avg",
                            "sampling": {
                                "value": "1",
                                "unit": "minutes"
                            }
                        }
                    ]
                }
            ]
        }
        
        # Send request through proxy
        resp = requests.post(
            f"{PROXY_URL}/api/v1/datapoints/query",
            json=payload,
            headers={"Content-Type": "application/json"},
            timeout=5
        )
        
        assert resp.status_code == 200
        
        # Verify the payload was received
        backend_requests = get_mock_server_requests(MOCK_SERVERS[0]["url"])
        assert len(backend_requests) > 0
        
        # Check that the complex payload matches exactly
        received_payload = backend_requests[-1]["payload"]
        assert received_payload == payload


class TestTagQueries:
    """Test tag query endpoint."""
    
    def setup_method(self):
        """Clear mock server state before each test."""
        clear_mock_server_requests()
    
    def test_tag_query_payload_consistency(self):
        """Test that tag query payloads pass through unchanged."""
        payload = {
            "start_relative": {
                "value": "1",
                "unit": "days"
            },
            "metrics": [
                {
                    "name": "cpu.idle"
                }
            ]
        }
        
        # Send request through proxy
        resp = requests.post(
            f"{PROXY_URL}/api/v1/datapoints/query/tags",
            json=payload,
            headers={"Content-Type": "application/json"},
            timeout=5
        )
        
        assert resp.status_code == 200
        
        # Verify the payload was received
        backend_requests = get_mock_server_requests(MOCK_SERVERS[0]["url"])
        assert len(backend_requests) > 0
        
        # Check that the payload matches and endpoint is correct
        received = backend_requests[-1]
        assert received["payload"] == payload
        assert received["endpoint"] == "/api/v1/datapoints/query/tags"


class TestResponseConsistency:
    """Test that responses from mock servers are returned correctly."""
    
    def test_response_structure(self):
        """Test that responses have the expected KairosDB structure."""
        payload = {
            "start_relative": {
                "value": "1",
                "unit": "hours"
            },
            "metrics": [
                {
                    "name": "cpu.system"
                }
            ]
        }
        
        resp = requests.post(
            f"{PROXY_URL}/api/v1/datapoints/query",
            json=payload,
            headers={"Content-Type": "application/json"},
            timeout=5
        )
        
        assert resp.status_code == 200
        data = resp.json()
        
        # Verify response structure
        assert "queries" in data
        assert isinstance(data["queries"], list)
        assert len(data["queries"]) > 0
        
        # Verify query result structure
        query_result = data["queries"][0]
        assert "results" in query_result
        assert isinstance(query_result["results"], list)
    
    def test_metric_name_in_response(self):
        """Test that the metric name appears correctly in the response."""
        metric_name = "cpu.user"
        payload = {
            "start_relative": {
                "value": "1",
                "unit": "hours"
            },
            "metrics": [
                {
                    "name": metric_name
                }
            ]
        }
        
        resp = requests.post(
            f"{PROXY_URL}/api/v1/datapoints/query",
            json=payload,
            headers={"Content-Type": "application/json"},
            timeout=5
        )
        
        assert resp.status_code == 200
        data = resp.json()
        
        # Verify the metric name is in the response
        assert len(data["queries"]) > 0
        results = data["queries"][0]["results"]
        assert len(results) > 0
        assert results[0]["name"] == metric_name


class TestMultipleMetrics:
    """Test handling of multiple metrics in a single request."""
    
    def setup_method(self):
        """Clear mock server state before each test."""
        clear_mock_server_requests()
    
    def test_multiple_cpu_metrics(self):
        """Test a query with multiple CPU metrics."""
        payload = {
            "start_relative": {
                "value": "1",
                "unit": "hours"
            },
            "metrics": [
                {"name": "cpu.user"},
                {"name": "cpu.system"},
                {"name": "cpu.idle"}
            ]
        }
        
        resp = requests.post(
            f"{PROXY_URL}/api/v1/datapoints/query",
            json=payload,
            headers={"Content-Type": "application/json"},
            timeout=5
        )
        
        # Should succeed (routing based on first metric in Simple mode)
        assert resp.status_code == 200


if __name__ == "__main__":
    pytest.main([__file__, "-v", "--tb=short"])
