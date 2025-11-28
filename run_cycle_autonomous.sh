#!/bin/bash
# Autonomous wrapper for cloud_colony_cycle.sh
# Runs the cycle in background and tracks progress, allowing for long-running operations

# Prompt For Agent To Follow: "Run the full cloud colony cycle and fix any backend/coordinator issues you encounter."

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CYCLE_SCRIPT="$ROOT_DIR/cloud_colony_cycle.sh"
LOG_FILE="$ROOT_DIR/cycle_run.log"
PID_FILE="$ROOT_DIR/cycle_run.pid"
MAX_ITERATIONS="${MAX_ITERATIONS:-5}"

log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "$LOG_FILE"
}

check_running() {
  if [ -f "$PID_FILE" ]; then
    local pid=$(cat "$PID_FILE" 2>/dev/null || echo "")
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      return 0
    fi
    rm -f "$PID_FILE"
  fi
  return 1
}

wait_for_completion() {
  local waited=0
  local max_wait=3600  # 1 hour max
  local check_interval=30
  
  while [ $waited -lt $max_wait ]; do
    if ! check_running; then
      return 0
    fi
    sleep $check_interval
    waited=$((waited + check_interval))
    if [ $((waited % 300)) -eq 0 ]; then
      log "Still running... (${waited}s elapsed)"
    fi
  done
  
  log "Timeout waiting for cycle to complete"
  return 1
}

run_iteration() {
  local iter=$1
  log "=== Starting iteration $iter/$MAX_ITERATIONS ==="
  
  # Start cycle in background
  nohup "$CYCLE_SCRIPT" >> "$LOG_FILE" 2>&1 &
  local pid=$!
  echo "$pid" > "$PID_FILE"
  log "Cycle started with PID $pid"
  
  # Wait for completion
  if wait_for_completion; then
    log "Cycle completed"
    # Check exit status by looking at last lines of log
    if tail -n 50 "$LOG_FILE" | grep -q "All steps completed"; then
      log "SUCCESS: Cycle completed successfully"
      return 0
    elif tail -n 50 "$LOG_FILE" | grep -q "ERROR"; then
      log "FAILED: Errors detected, will retry if iterations remain"
      return 1
    else
      log "UNKNOWN: Cycle finished but status unclear"
      return 1
    fi
  else
    log "FAILED: Cycle timed out or was interrupted"
    if check_running; then
      kill "$(cat "$PID_FILE")" 2>/dev/null || true
      rm -f "$PID_FILE"
    fi
    return 1
  fi
}

main() {
  log "Starting autonomous cloud colony cycle (max $MAX_ITERATIONS iterations)"
  rm -f "$LOG_FILE" "$PID_FILE"
  
  for iter in $(seq 1 $MAX_ITERATIONS); do
    if run_iteration "$iter"; then
      log "=== SUCCESS: Colony cycle completed successfully ==="
      exit 0
    fi
    
    if [ $iter -lt $MAX_ITERATIONS ]; then
      log "Waiting 60s before retry..."
      sleep 60
      # Here we would analyze errors and fix code, but for now just retry
    fi
  done
  
  log "=== FAILED: All $MAX_ITERATIONS iterations exhausted ==="
  exit 1
}

main "$@"

