#!/bin/bash
set -e

pkill -x backend || true

cargo run -p backend &
sleep 1

cargo run -p frontend

curl http://localhost:9898/metrics