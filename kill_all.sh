#!/bin/bash

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
lsof -ti :8082 | xargs kill -9 2>/dev/null || true
lsof -ti :8083 | xargs kill -9 2>/dev/null || true
lsof -ti :8084 | xargs kill -9 2>/dev/null || true

# Wait for forceful termination
sleep 1

# Check if ports are still in use and wait for them to be released
echo "🔍 Checking if ports are still in use..."

# Check backend port (8082) with timeout
timeout=30
counter=0
while lsof -i :8082 >/dev/null 2>&1 && [ $counter -lt $timeout ]; do
    echo "⏳ Port 8082 still in use, waiting for release... ($counter/$timeout)"
    sleep 1
    counter=$((counter + 1))
done

if [ $counter -eq $timeout ]; then
    echo "❌ Timeout waiting for port 8082 to be released. Force killing..."
    lsof -ti :8082 | xargs kill -9 2>/dev/null || true
    sleep 2
fi

# Check coordinator port (8083) with timeout
counter=0
while lsof -i :8083 >/dev/null 2>&1 && [ $counter -lt $timeout ]; do
    echo "⏳ Port 8083 still in use, waiting for release... ($counter/$timeout)"
    sleep 1
    counter=$((counter + 1))
done

if [ $counter -eq $timeout ]; then
    echo "❌ Timeout waiting for port 8083 to be released. Force killing..."
    lsof -ti :8083 | xargs kill -9 2>/dev/null || true
    sleep 2
fi

# Check backend port 2 (8084) with timeout
counter=0
while lsof -i :8084 >/dev/null 2>&1 && [ $counter -lt $timeout ]; do
    echo "⏳ Port 8084 still in use, waiting for release... ($counter/$timeout)"
    sleep 1
    counter=$((counter + 1))
done

if [ $counter -eq $timeout ]; then
    echo "❌ Timeout waiting for port 8084 to be released. Force killing..."
    lsof -ti :8084 | xargs kill -9 2>/dev/null || true
    sleep 2
fi

echo "✅ All ports are now free"