#!/bin/bash
set -e

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Change to project root (one directory above scripts)
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# Configuration - matches cluster topology
# Port pairs: (RPC_PORT, HTTP_PORT)
# Coordinator: RPC=8082, HTTP=8083
# Backends: RPC=8084,8086,8088,8090 and HTTP=8085,8087,8089,8091
COORDINATOR_RPC_PORT=8082
COORDINATOR_HTTP_PORT=8083
BACKEND_RPC_PORTS=(8084 8086 8088 8090)
BACKEND_HTTP_PORTS=(8085 8087 8089 8091)
HOSTNAME="127.0.0.1"

./scripts/local_kill.sh
rm -rf output/logs
rm -rf output/ssm

echo "🧪 Running test suite (with cloud feature) ..."
cargo test --all --features cloud

# Validate port configuration
if [ ${#BACKEND_RPC_PORTS[@]} -ne ${#BACKEND_HTTP_PORTS[@]} ]; then
    echo "❌ Error: Number of RPC ports (${#BACKEND_RPC_PORTS[@]}) does not match HTTP ports (${#BACKEND_HTTP_PORTS[@]})"
    exit 1
fi

# Check for port conflicts
check_port() {
    local port=$1
    if lsof -i :$port >/dev/null 2>&1; then
        echo "❌ Error: Port $port is already in use"
        exit 1
    fi
}

echo "🔍 Validating ports..."
check_port $COORDINATOR_RPC_PORT
check_port $COORDINATOR_HTTP_PORT
for i in "${!BACKEND_RPC_PORTS[@]}"; do
    check_port ${BACKEND_RPC_PORTS[$i]}
    check_port ${BACKEND_HTTP_PORTS[$i]}
done

echo "🚀 Starting ${#BACKEND_RPC_PORTS[@]} backend instances in localhost mode (release profile)..."

# Start all backends
for i in "${!BACKEND_RPC_PORTS[@]}"; do
    rpc_port=${BACKEND_RPC_PORTS[$i]}
    http_port=${BACKEND_HTTP_PORTS[$i]}
    echo "🔥 Starting backend on RPC port $rpc_port, HTTP port $http_port (release)..."
    (cd "$PROJECT_ROOT" && cargo run --release -p backend -- $HOSTNAME $rpc_port $http_port localhost) &
done
sleep 3

echo "📡 Starting coordinator in localhost mode (RPC port $COORDINATOR_RPC_PORT, HTTP port $COORDINATOR_HTTP_PORT, release profile)..."
(cd "$PROJECT_ROOT" && cargo run --release -p coordinator -- $COORDINATOR_RPC_PORT $COORDINATOR_HTTP_PORT localhost) &
sleep 1

# Start S3 upload daemon
echo "☁️  Starting S3 upload daemon..."
# Check if daemon is already running
if pgrep -f "s3_upload_daemon.sh" > /dev/null; then
    echo "⚠️  S3 upload daemon is already running, skipping start"
else
    # Create output/s3 directory if it doesn't exist
    mkdir -p "$PROJECT_ROOT/output/s3"
    mkdir -p "$PROJECT_ROOT/output/logs"
    # Start daemon in background with logging
    nohup "$PROJECT_ROOT/scripts/s3_upload_daemon.sh" > "$PROJECT_ROOT/output/logs/s3_upload_daemon.log" 2>&1 &
    S3_DAEMON_PID=$!
    echo "$S3_DAEMON_PID" > "$PROJECT_ROOT/output/s3_upload_daemon.pid"
    echo "✅ S3 upload daemon started (PID: $S3_DAEMON_PID)"
fi

echo "🖥️  Starting GUI (release profile)..."
(cd "$PROJECT_ROOT" && cargo run --release -p gui)