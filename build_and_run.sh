#!/bin/bash
set -e

./kill_all.sh
rm -rf output/logs/*

cargo run -p backend &
sleep 1

cargo run -p coordinator &
sleep 1
cargo run -p frontend

cargo run -p gui