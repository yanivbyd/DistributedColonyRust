#!/bin/bash
set -e

./kill_all.sh
rm -rf output/logs/*

cargo run --profile=balanced -p backend -- 127.0.0.1 8082 &
sleep 1

cargo run --profile=balanced -p backend -- 127.0.0.1 8084 &
sleep 1

cargo run --profile=balanced -p coordinator &
sleep 1

cargo run --profile=balanced -p gui