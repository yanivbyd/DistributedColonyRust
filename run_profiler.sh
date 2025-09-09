#!/bin/bash
set -euo pipefail

SVG_OUT="output/cargo-be-flamegraph.svg"

./kill_all.sh

RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile=profiling -p backend -p coordinator -p gui

echo "🔥 Starting backend under cargo-flamegraph (in background)…"
cargo flamegraph \
  --profile=profiling \
  --bin backend \
  --title "Backend CPU Profile" \
  --subtitle "Backend Performance Analysis" \
  -o "${SVG_OUT}" \
  -- 127.0.0.1 8082 &
FLAMEGRAPH_PID=$!

# Wait for backend (launched by cargo-flamegraph) to open its port
echo "⏳ Waiting for backend to listen on 8082…"
for _ in {1..100}; do
  if lsof -iTCP:8082 -sTCP:LISTEN >/dev/null 2>&1; then break; fi
  sleep 0.1
done

# Start the others after backend is up
echo "📡 Starting coordinator…"
cargo run --profile=profiling -p coordinator &

echo "🖥️  Starting GUI, stop it when you want profiling to end"
cargo run --profile=profiling -p gui 

echo "🛑 Stopping flamegraph (and backend)…"
kill -INT "${FLAMEGRAPH_PID}" 2>/dev/null || true
echo "🛑 Waiting for flamegraph (and backend) to stop…"
wait "${FLAMEGRAPH_PID}" 2>/dev/null || true

echo "✅ Flamegraph ready: ${SVG_OUT}"
open "${SVG_OUT}" 2>/dev/null || true
