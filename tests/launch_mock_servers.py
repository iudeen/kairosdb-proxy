#!/usr/bin/env python3
"""
Script to launch multiple mock KairosDB servers for integration testing.
"""

import subprocess
import sys
import time
import requests
import signal
import os

# Server configurations
SERVERS = [
    {"port": 8081, "name": "kairosdb-1"},
    {"port": 8082, "name": "kairosdb-2"},
    {"port": 8083, "name": "kairosdb-3"},
]

processes = []


def signal_handler(sig, frame):
    """Handle termination signals gracefully."""
    print("\nShutting down mock servers...")
    for proc in processes:
        proc.terminate()
        proc.wait()
    sys.exit(0)


def start_servers():
    """Start all mock KairosDB servers."""
    global processes
    
    for server in SERVERS:
        cmd = [
            sys.executable,
            "mock_kairosdb_server.py",
            str(server["port"]),
            server["name"]
        ]
        
        print(f"Starting {server['name']} on port {server['port']}...")
        proc = subprocess.Popen(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=os.path.dirname(__file__)
        )
        processes.append(proc)
        time.sleep(0.5)
    
    # Wait for servers to be ready
    print("Waiting for servers to be ready...")
    for server in SERVERS:
        max_retries = 30
        for i in range(max_retries):
            try:
                resp = requests.get(f"http://127.0.0.1:{server['port']}/health", timeout=1)
                if resp.status_code == 200:
                    print(f"✓ {server['name']} is ready")
                    break
            except requests.exceptions.RequestException as e:
                if i == max_retries - 1:
                    print(f"✗ {server['name']} failed to start after {max_retries} retries")
                    print(f"   Last error: {e}")
                    raise
                # Server not ready yet, continue waiting
                time.sleep(0.5)
    
    print("All mock servers are ready!")
    return processes


def main():
    """Main entry point."""
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    try:
        start_servers()
        
        # Keep running until interrupted
        print("\nMock servers are running. Press Ctrl+C to stop.")
        while True:
            time.sleep(1)
            
    except Exception as e:
        print(f"Error: {e}")
        signal_handler(None, None)
        sys.exit(1)


if __name__ == '__main__':
    main()
