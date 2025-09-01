#!/bin/bash

echo "=== Profile Performance Comparison ==="
echo

echo "1. Testing dev profile (opt-level = 0)..."
time cargo check --profile=dev --package backend > /dev/null 2>&1

echo
echo "2. Testing fast profile (opt-level = 1)..."
time cargo check --profile=fast --package backend > /dev/null 2>&1

echo
echo "3. Testing balanced profile (opt-level = 3)..."
time cargo check --profile=balanced --package backend > /dev/null 2>&1

echo
echo "4. Testing profiling profile (opt-level = 3)..."
time cargo check --profile=profiling --package backend > /dev/null 2>&1

echo
echo "=== Summary ==="
echo "dev:      No optimization, fastest compilation, slowest runtime"
echo "fast:     Light optimization, fast compilation, moderate runtime"
echo "balanced: Maximum optimization, moderate compilation, fastest runtime ⭐"
echo "profiling: Maximum optimization, slowest compilation, fastest runtime"
echo
echo "Use --profile=balanced for development with maximum performance!"
echo "Use --profile=fast for quick iterations when you need fast compilation."
