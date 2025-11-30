#!/bin/bash
set -euo pipefail

SVG_OUT="output/cargo-be-flamegraph.svg"

./kill_all.sh

RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile=profiling -p backend -p coordinator -p gui

echo "🔥 Starting backend 1 under cargo-flamegraph (in background)…"
cargo flamegraph \
  --profile=profiling \
  --bin backend \
  --title "Backend CPU Profile" \
  --subtitle "Backend Performance Analysis" \
  -o "${SVG_OUT}" \
  -- 127.0.0.1 8082 &
FLAMEGRAPH_PID=$!

# Wait for backend 1 (launched by cargo-flamegraph) to open its port
echo "⏳ Waiting for backend 1 to listen on 8082…"
for _ in {1..100}; do
  if lsof -iTCP:8082 -sTCP:LISTEN >/dev/null 2>&1; then break; fi
  sleep 0.1
done

echo "🔥 Starting backend 2 (in background)…"
cargo run --profile=profiling -p backend -- 127.0.0.1 8084 &
BACKEND2_PID=$!

# Wait for backend 2 to open its port
echo "⏳ Waiting for backend 2 to listen on 8084…"
for _ in {1..100}; do
  if lsof -iTCP:8084 -sTCP:LISTEN >/dev/null 2>&1; then break; fi
  sleep 0.1
done

echo "🔥 Starting backend 3 (in background)…"
cargo run --profile=profiling -p backend -- 127.0.0.1 8085 &
BACKEND3_PID=$!

# Wait for backend 3 to open its port
echo "⏳ Waiting for backend 3 to listen on 8085…"
for _ in {1..100}; do
  if lsof -iTCP:8085 -sTCP:LISTEN >/dev/null 2>&1; then break; fi
  sleep 0.1
done

echo "🔥 Starting backend 4 (in background)…"
cargo run --profile=profiling -p backend -- 127.0.0.1 8086 &
BACKEND4_PID=$!

# Wait for backend 4 to open its port
echo "⏳ Waiting for backend 4 to listen on 8086…"
for _ in {1..100}; do
  if lsof -iTCP:8086 -sTCP:LISTEN >/dev/null 2>&1; then break; fi
  sleep 0.1
done

# Start the others after backend is up
echo "📡 Starting coordinator…"
cargo run --profile=profiling -p coordinator &

echo "🖥️  Starting GUI, stop it when you want profiling to end"
cargo run --profile=profiling -p gui 

echo "🛑 Stopping flamegraph (and backend 1)…"
kill -INT "${FLAMEGRAPH_PID}" 2>/dev/null || true
echo "🛑 Stopping backend 2…"
kill "${BACKEND2_PID}" 2>/dev/null || true
echo "🛑 Stopping backend 3…"
kill "${BACKEND3_PID}" 2>/dev/null || true
echo "🛑 Stopping backend 4…"
kill "${BACKEND4_PID}" 2>/dev/null || true
echo "🛑 Waiting for flamegraph (and backend 1) to stop…"
wait "${FLAMEGRAPH_PID}" 2>/dev/null || true
echo "🛑 Waiting for backend 2 to stop…"
wait "${BACKEND2_PID}" 2>/dev/null || true
echo "🛑 Waiting for backend 3 to stop…"
wait "${BACKEND3_PID}" 2>/dev/null || true
echo "🛑 Waiting for backend 4 to stop…"
wait "${BACKEND4_PID}" 2>/dev/null || true

echo "✅ Flamegraph ready: ${SVG_OUT}"
open "${SVG_OUT}" 2>/dev/null || true
