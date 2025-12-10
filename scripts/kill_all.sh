#!/bin/bash

# Configuration - matches cluster topology
# Port pairs: (RPC_PORT, HTTP_PORT)
# Coordinator: RPC=8082, HTTP=8083
# Backends: RPC=8084,8086,8088,8090 and HTTP=8085,8087,8089,8091
COORDINATOR_RPC_PORT=8082
COORDINATOR_HTTP_PORT=8083
BACKEND_RPC_PORTS=(8084 8086 8088 8090)
BACKEND_HTTP_PORTS=(8085 8087 8089 8091)

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

# Kill backend RPC ports
for port in "${BACKEND_RPC_PORTS[@]}"; do
    lsof -ti :$port | xargs kill -9 2>/dev/null || true
done

# Kill backend HTTP ports
for port in "${BACKEND_HTTP_PORTS[@]}"; do
    lsof -ti :$port | xargs kill -9 2>/dev/null || true
done

# Kill coordinator ports
lsof -ti :$COORDINATOR_RPC_PORT | xargs kill -9 2>/dev/null || true
lsof -ti :$COORDINATOR_HTTP_PORT | xargs kill -9 2>/dev/null || true

# Wait for forceful termination
sleep 1

# Check if ports are still in use and wait for them to be released
echo "🔍 Checking if ports are still in use..."

timeout=30

# Check backend RPC ports with timeout
for port in "${BACKEND_RPC_PORTS[@]}"; do
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

# Check backend HTTP ports with timeout
for port in "${BACKEND_HTTP_PORTS[@]}"; do
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

# Check coordinator RPC port with timeout
counter=0
while lsof -i :$COORDINATOR_RPC_PORT >/dev/null 2>&1 && [ $counter -lt $timeout ]; do
    echo "⏳ Port $COORDINATOR_RPC_PORT still in use, waiting for release... ($counter/$timeout)"
    sleep 1
    counter=$((counter + 1))
done

if [ $counter -eq $timeout ]; then
    echo "❌ Timeout waiting for port $COORDINATOR_RPC_PORT to be released. Force killing..."
    lsof -ti :$COORDINATOR_RPC_PORT | xargs kill -9 2>/dev/null || true
    sleep 2
fi

# Check coordinator HTTP port with timeout
counter=0
while lsof -i :$COORDINATOR_HTTP_PORT >/dev/null 2>&1 && [ $counter -lt $timeout ]; do
    echo "⏳ Port $COORDINATOR_HTTP_PORT still in use, waiting for release... ($counter/$timeout)"
    sleep 1
    counter=$((counter + 1))
done

if [ $counter -eq $timeout ]; then
    echo "❌ Timeout waiting for port $COORDINATOR_HTTP_PORT to be released. Force killing..."
    lsof -ti :$COORDINATOR_HTTP_PORT | xargs kill -9 2>/dev/null || true
    sleep 2
fi

echo "✅ All ports are now free"