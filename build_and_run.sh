#!/bin/bash
set -e

# Configuration - matches cluster topology
BACKEND_PORTS=(8082 8084 8085 8086)
HOSTNAME="127.0.0.1"

./kill_all.sh
rm -rf output/logs/*

echo "🧪 Running test suite (with cloud feature) ..."
cargo test --all --features cloud

echo "🚀 Starting ${#BACKEND_PORTS[@]} backend instances in localhost mode..."

# Start all backends
for port in "${BACKEND_PORTS[@]}"; do
    echo "🔥 Starting backend on port $port..."
    cargo run --profile=balanced -p backend -- $HOSTNAME $port localhost &
done
sleep 3

echo "📡 Starting coordinator in localhost mode..."
cargo run --profile=balanced -p coordinator -- localhost &
sleep 1

echo "🖥️  Starting GUI..."
cargo run --profile=balanced -p gui