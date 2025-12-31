#!/usr/bin/env python3
"""
Mock KairosDB server for integration testing.
This server simulates KairosDB behavior by capturing payloads and returning consistent responses.
Uses Bottle.py - a lightweight, single-file micro-framework with minimal resource usage.
"""

import json
import sys
from bottle import Bottle, request, response, run

# Store received requests for validation
received_requests = []


def create_app(server_name):
    """Create a Bottle app that simulates KairosDB behavior."""
    app = Bottle()

    @app.route('/health', method='GET')
    def health():
        """Health check endpoint."""
        response.content_type = 'application/json'
        return json.dumps({"status": "ok", "server": server_name})

    @app.route('/api/v1/datapoints/query', method='POST')
    def query_datapoints():
        """
        Simulate KairosDB query endpoint.
        Captures the request payload and returns a consistent response.
        """
        try:
            payload = request.json
            
            # Store the request for validation
            received_requests.append({
                'endpoint': '/api/v1/datapoints/query',
                'payload': payload,
                'server': server_name,
                'headers': dict(request.headers)
            })
            
            # Return a mock KairosDB response
            result = {
                "queries": []
            }
            
            # Generate mock results for each metric in the request
            if payload and 'metrics' in payload:
                for metric in payload['metrics']:
                    metric_name = metric.get('name', 'unknown')
                    query_result = {
                        "sample_size": 10,
                        "results": [
                            {
                                "name": metric_name,
                                "group_by": [],
                                "tags": {
                                    "host": [server_name]
                                },
                                "values": [
                                    [1609459200000, 42.5],
                                    [1609459260000, 43.0],
                                    [1609459320000, 43.5]
                                ]
                            }
                        ]
                    }
                    result["queries"].append(query_result)
            
            response.content_type = 'application/json'
            return json.dumps(result)
            
        except Exception as e:
            response.status = 400
            response.content_type = 'application/json'
            return json.dumps({"error": str(e)})

    @app.route('/api/v1/datapoints/query/tags', method='POST')
    def query_tags():
        """
        Simulate KairosDB query tags endpoint.
        Captures the request payload and returns a consistent response.
        """
        try:
            payload = request.json
            
            # Store the request for validation
            received_requests.append({
                'endpoint': '/api/v1/datapoints/query/tags',
                'payload': payload,
                'server': server_name,
                'headers': dict(request.headers)
            })
            
            # Return a mock tag query response
            result = {
                "results": []
            }
            
            if payload and 'metrics' in payload:
                for metric in payload['metrics']:
                    metric_name = metric.get('name', 'unknown')
                    tag_result = {
                        "name": metric_name,
                        "tags": {
                            "host": [server_name],
                            "region": ["us-east-1"]
                        }
                    }
                    result["results"].append(tag_result)
            
            response.content_type = 'application/json'
            return json.dumps(result)
            
        except Exception as e:
            response.status = 400
            response.content_type = 'application/json'
            return json.dumps({"error": str(e)})

    @app.route('/debug/requests', method='GET')
    def get_requests():
        """Debug endpoint to retrieve all received requests."""
        response.content_type = 'application/json'
        return json.dumps(received_requests)

    @app.route('/debug/clear', method='POST')
    def clear_requests():
        """Debug endpoint to clear stored requests."""
        received_requests.clear()
        response.content_type = 'application/json'
        return json.dumps({"status": "cleared"})

    return app


def main():
    """Run the mock server."""
    if len(sys.argv) < 3:
        print("Usage: python mock_kairosdb_server.py <port> <server_name>")
        sys.exit(1)
    
    port = int(sys.argv[1])
    server_name = sys.argv[2]
    
    app = create_app(server_name)
    print(f"Starting mock KairosDB server '{server_name}' on port {port}...")
    
    # Bottle's built-in server is lightweight and sufficient for testing
    # quiet=True suppresses the default startup message for cleaner output
    run(app, host='127.0.0.1', port=port, quiet=True)


if __name__ == '__main__':
    main()
