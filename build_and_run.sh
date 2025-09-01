#!/bin/bash
set -e

./kill_all.sh
rm -rf output/logs/*

cargo run --profile=balanced -p backend &
sleep 1

cargo run --profile=balanced -p coordinator &
sleep 1

cargo run --profile=balanced -p gui