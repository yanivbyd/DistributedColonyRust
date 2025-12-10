#!/bin/bash
set -e

# Get the directory where this script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Change to project root (one directory above scripts)
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# Configuration - matches cluster topology
BACKEND_PORTS=(8082 8084 8085 8086)
HOSTNAME="127.0.0.1"

./scripts/kill_all.sh
rm -rf output/logs
rm -rf output/ssm

echo "🧪 Running test suite (with cloud feature) ..."
cargo test --all --features cloud

echo "🚀 Starting ${#BACKEND_PORTS[@]} backend instances in localhost mode..."

# Start all backends
for port in "${BACKEND_PORTS[@]}"; do
    echo "🔥 Starting backend on port $port..."
    (cd "$PROJECT_ROOT" && cargo run --profile=balanced -p backend -- $HOSTNAME $port localhost) &
done
sleep 3

echo "📡 Starting coordinator in localhost mode..."
(cd "$PROJECT_ROOT" && cargo run --profile=balanced -p coordinator -- localhost) &
sleep 1

echo "🖥️  Starting GUI..."
(cd "$PROJECT_ROOT" && cargo run --profile=balanced -p gui)