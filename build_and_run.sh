#!/bin/bash
set -e

pkill -f target/backend_app || true
pkill -f target/frontend_app || true

# Create target directory if it doesn't exist
mkdir -p target

echo -e "\033[1;35m================[ BUILD ] ================\033[0m"

rustc src/backend/be_main.rs -o target/backend_app
rustc src/frontend/fe_main.rs -o target/frontend_app

echo -e "\033[1;35m================[ RUN ] ==================\033[0m"

./target/backend_app &
sleep 1
./target/frontend_app

