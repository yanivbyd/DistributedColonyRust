#!/bin/bash
set -e

# Configuration - matches cluster topology
# Port pairs: (RPC_PORT, HTTP_PORT)
# Coordinator: RPC=8082, HTTP=8083
# Backends: RPC=8084,8086,8088,8090 and HTTP=8085,8087,8089,8091
COORDINATOR_RPC_PORT=8082
COORDINATOR_HTTP_PORT=8083
BACKEND_RPC_PORTS=(8084 8086 8088 8090)
BACKEND_HTTP_PORTS=(8085 8087 8089 8091)
HOSTNAME="127.0.0.1"

./kill_all.sh
rm -rf output

# Validate port configuration
if [ ${#BACKEND_RPC_PORTS[@]} -ne ${#BACKEND_HTTP_PORTS[@]} ]; then
    echo "❌ Error: Number of RPC ports (${#BACKEND_RPC_PORTS[@]}) does not match HTTP ports (${#BACKEND_HTTP_PORTS[@]})"
    exit 1
fi

echo "🚀 Starting ${#BACKEND_RPC_PORTS[@]} backend instances..."

# Start all backends
for i in "${!BACKEND_RPC_PORTS[@]}"; do
    rpc_port=${BACKEND_RPC_PORTS[$i]}
    http_port=${BACKEND_HTTP_PORTS[$i]}
    echo "🔥 Starting backend on RPC port $rpc_port, HTTP port $http_port..."
    cargo run --profile=balanced -p backend -- $HOSTNAME $rpc_port $http_port localhost &
done
sleep 3

echo "📡 Starting coordinator (RPC port $COORDINATOR_RPC_PORT, HTTP port $COORDINATOR_HTTP_PORT)..."
cargo run --profile=balanced -p coordinator -- $COORDINATOR_RPC_PORT $COORDINATOR_HTTP_PORT localhost &
sleep 1

echo "🖥️  Starting GUI..."
cargo run --profile=balanced -p gui