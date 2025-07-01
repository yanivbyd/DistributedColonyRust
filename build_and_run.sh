#!/bin/bash
set -e

pkill -x backend || true

cargo run -p backend &
sleep 1

cargo run -p frontend

open output/colony.png