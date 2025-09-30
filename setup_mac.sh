#!/bin/bash
set -e

# Check for Homebrew and install if missing
if ! command -v brew &> /dev/null; then
    echo "Homebrew not found. Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
fi

# Install Rust if not present
if ! command -v rustc &> /dev/null; then
    echo "Rust not found. Installing Rust..."
    brew install rust
fi

# Make build_and_run.sh executable
chmod +x build_and_run.sh

echo "Setup complete. You can now run ./build_and_run.sh" 