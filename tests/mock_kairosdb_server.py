#!/usr/bin/env python3
"""
Mock KairosDB server for integration testing.
This server simulates KairosDB behavior by capturing payloads and returning consistent responses.
"""

import json
import sys
from flask import Flask, request, jsonify
from waitress import serve

# Store received requests for validation
received_requests = []


def create_app(server_name):
    """Create a Flask app that simulates KairosDB behavior."""
    app = Flask(__name__)
    app.config['SERVER_NAME_LABEL'] = server_name

    @app.route('/health', methods=['GET'])
    def health():
        """Health check endpoint."""
        return jsonify({"status": "ok", "server": server_name}), 200

    @app.route('/api/v1/datapoints/query', methods=['POST'])
    def query_datapoints():
        """
        Simulate KairosDB query endpoint.
        Captures the request payload and returns a consistent response.
        """
        try:
            payload = request.get_json()
            
            # Store the request for validation
            received_requests.append({
                'endpoint': '/api/v1/datapoints/query',
                'payload': payload,
                'server': server_name,
                'headers': dict(request.headers)
            })
            
            # Return a mock KairosDB response
            response = {
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
                    response["queries"].append(query_result)
            
            return jsonify(response), 200
            
        except Exception as e:
            return jsonify({"error": str(e)}), 400

    @app.route('/api/v1/datapoints/query/tags', methods=['POST'])
    def query_tags():
        """
        Simulate KairosDB query tags endpoint.
        Captures the request payload and returns a consistent response.
        """
        try:
            payload = request.get_json()
            
            # Store the request for validation
            received_requests.append({
                'endpoint': '/api/v1/datapoints/query/tags',
                'payload': payload,
                'server': server_name,
                'headers': dict(request.headers)
            })
            
            # Return a mock tag query response
            response = {
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
                    response["results"].append(tag_result)
            
            return jsonify(response), 200
            
        except Exception as e:
            return jsonify({"error": str(e)}), 400

    @app.route('/debug/requests', methods=['GET'])
    def get_requests():
        """Debug endpoint to retrieve all received requests."""
        return jsonify(received_requests), 200

    @app.route('/debug/clear', methods=['POST'])
    def clear_requests():
        """Debug endpoint to clear stored requests."""
        received_requests.clear()
        return jsonify({"status": "cleared"}), 200

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
    
    # Use waitress for production-ready serving
    serve(app, host='127.0.0.1', port=port)


if __name__ == '__main__':
    main()
