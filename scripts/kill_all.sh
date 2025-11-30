#!/bin/bash

# Configuration - matches cluster topology
BACKEND_PORTS=(8082 8084 8085 8086)
COORDINATOR_PORT=8083

echo "🔄 Killing all backend, coordinator, and GUI processes..."

# First try graceful termination
pkill -x backend || true
pkill -x coordinator || true
pkill -x gui || true

# Wait a moment for graceful termination
sleep 2

# Force kill any remaining processes
pkill -9 -x backend || true
pkill -9 -x coordinator || true
pkill -9 -x gui || true

# Also kill by port usage as a backup
echo "🔫 Force killing processes by port usage..."

# Kill backend ports
for port in "${BACKEND_PORTS[@]}"; do
    lsof -ti :$port | xargs kill -9 2>/dev/null || true
done

# Kill coordinator port
lsof -ti :$COORDINATOR_PORT | xargs kill -9 2>/dev/null || true

# Wait for forceful termination
sleep 1

# Check if ports are still in use and wait for them to be released
echo "🔍 Checking if ports are still in use..."

timeout=30

# Check backend ports with timeout
for port in "${BACKEND_PORTS[@]}"; do
    counter=0
    while lsof -i :$port >/dev/null 2>&1 && [ $counter -lt $timeout ]; do
        echo "⏳ Port $port still in use, waiting for release... ($counter/$timeout)"
        sleep 1
        counter=$((counter + 1))
    done

    if [ $counter -eq $timeout ]; then
        echo "❌ Timeout waiting for port $port to be released. Force killing..."
        lsof -ti :$port | xargs kill -9 2>/dev/null || true
        sleep 2
    fi
done

# Check coordinator port with timeout
counter=0
while lsof -i :$COORDINATOR_PORT >/dev/null 2>&1 && [ $counter -lt $timeout ]; do
    echo "⏳ Port $COORDINATOR_PORT still in use, waiting for release... ($counter/$timeout)"
    sleep 1
    counter=$((counter + 1))
done

if [ $counter -eq $timeout ]; then
    echo "❌ Timeout waiting for port $COORDINATOR_PORT to be released. Force killing..."
    lsof -ti :$COORDINATOR_PORT | xargs kill -9 2>/dev/null || true
    sleep 2
fi

echo "✅ All ports are now free"