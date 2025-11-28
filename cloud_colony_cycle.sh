#!/bin/bash
set -euo pipefail

# Full automation for the dev cloud cycle:
# 1) Builds/pushes the Docker image to ECR
# 2) Deploys the CDK stack (spinning up spot instances, which cost money)
# 3) SSHes into coordinator/backends to validate logs
# 4) Triggers the cloud-start workflow from the coordinator
# Remember to stop the spot fleets via "cd CDK && ./stop.sh" when you're done.
#
# Autonomous agent instructions:
# When told to "Run the full cloud colony cycle and fix any backend/coordinator issues you encounter":
# - Run this script end-to-end
# - If errors are found in logs, analyze them, fix the code, rebuild, and run again
# - Repeat this cycle up to 5 times total (initial run + up to 4 retries with fixes)
# - After each failed run, investigate the errors, make code changes, ensure builds pass,
#   then re-run the full cycle. Only stop and report if all 5 attempts are exhausted.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$ROOT_DIR/Docker"
CDK_DIR="$ROOT_DIR/CDK"
AWS_REGION="${AWS_REGION:-eu-west-1}"
STACK_NAME="${STACK_NAME:-DistributedColonySpotInstances}"
COORDINATOR_PORT="${COORDINATOR_PORT:-8083}"
BACKEND_PORT="${BACKEND_PORT:-8082}"
COORDINATOR_HTTP_PORT="${COORDINATOR_HTTP_PORT:-8084}"
SSH_USER="${COLONY_SSH_USER:-ec2-user}"
KEY_PATH="${COLONY_SSH_KEY:-$CDK_DIR/distributed-colony-key.pem}"
KEY_PATH="${KEY_PATH/#~/$HOME}"
WAIT_TIMEOUT="${WAIT_TIMEOUT:-900}"
WAIT_INTERVAL="${WAIT_INTERVAL:-20}"

log_info() {
  echo "[INFO] $*" >&2
}

log_warn() {
  echo "[WARN] $*" >&2
}

log_error() {
  echo "[ERROR] $*" >&2
}

require_binary() {
  if ! command -v "$1" >/dev/null 2>&1; then
    log_error "Missing required binary: $1"
    exit 1
  fi
}

ensure_prereqs() {
  require_binary aws
  require_binary docker
  require_binary ssh
  require_binary npm
  if ! command -v cdk >/dev/null 2>&1; then
    require_binary npx
  fi

  if [ ! -f "$KEY_PATH" ]; then
    log_error "SSH key not found at $KEY_PATH"
    exit 1
  fi

  log_info "Using AWS region: $AWS_REGION"
  log_info "Using CloudFormation stack: $STACK_NAME"
  log_info "Using SSH key: $KEY_PATH"
  aws --region "$AWS_REGION" sts get-caller-identity >/dev/null
}

ensure_cdk_dependencies() {
  if [ ! -d "$CDK_DIR/node_modules" ]; then
    log_info "Installing CDK dependencies..."
    (cd "$CDK_DIR" && npm install >/dev/null)
  fi
}

run_cdk() {
  if command -v cdk >/dev/null 2>&1; then
    (cd "$CDK_DIR" && cdk "$@")
  else
    (cd "$CDK_DIR" && npx cdk "$@")
  fi
}

build_and_push() {
  log_info "Step 1: Building Docker image and pushing to ECR..."
  (cd "$DOCKER_DIR" && ./build-and-push.sh)
}

deploy_cdk() {
  log_info "Step 2: Deploying CDK stack (spot instances will start now)..."
  log_warn "Spot instances incur cost; stop via 'cd CDK && ./stop.sh' when finished."
  ensure_cdk_dependencies
  run_cdk bootstrap
  run_cdk deploy "$STACK_NAME" --require-approval never
}

parse_expected_backends() {
  if [ -n "${EXPECTED_BACKENDS:-}" ]; then
    return
  fi

  if command -v python3 >/dev/null 2>&1 && [ -f "$CDK_DIR/cdk.json" ]; then
    local expected
    if expected=$(python3 - <<PY 2>/dev/null
import json
from pathlib import Path
cdk = Path("$CDK_DIR") / "cdk.json"
data = json.loads(cdk.read_text())
print(data.get("context", {}).get("targetCapacity", ""))
PY
); then
      if [[ "$expected" =~ ^[0-9]+$ ]] && [ "$expected" -gt 0 ]; then
        EXPECTED_BACKENDS="$expected"
        return
      fi
    fi
  fi

  EXPECTED_BACKENDS=1
}

aws_instance_query() {
  local type="$1"
  aws --region "$AWS_REGION" ec2 describe-instances \
    --filters \
      "Name=instance-state-name,Values=running" \
      "Name=tag:Type,Values=${type}" \
      "Name=tag:Service,Values=distributed-colony" \
    --query 'Reservations[].Instances[].[InstanceId,PublicIpAddress]' \
    --output text
}

wait_for_instances() {
  local type="$1"
  local target="$2"
  local waited=0

  while [ "$waited" -le "$WAIT_TIMEOUT" ]; do
    local instances_str
    instances_str="$(aws_instance_query "$type" | awk '$2 != "None" {print $1":"$2}')"
    local instances=()
    if [ -n "$instances_str" ]; then
      while IFS= read -r line; do
        [ -n "$line" ] && instances+=("$line")
      done <<<"$instances_str"
    fi

    if [ "${#instances[@]}" -ge "$target" ]; then
      printf "%s\n" "${instances[@]}"
      return 0
    fi

    log_info "Waiting for $type instances (have ${#instances[@]}, need $target)..."
    sleep "$WAIT_INTERVAL"
    waited=$((waited + WAIT_INTERVAL))
  done

  log_error "Timed out waiting for $type instances to reach $target ready nodes"
  exit 1
}

ssh_exec() {
  local host="$1"
  shift
  ssh -o StrictHostKeyChecking=no -i "$KEY_PATH" "$SSH_USER@$host" "$@"
}

check_logs() {
  local host="$1"
  local log_path="$2"
  local label="$3"

  log_info "Checking $label logs on $host at $log_path..."
  ssh_exec "$host" bash -se <<EOF
set -euo pipefail
LOG_PATH="$log_path"
if [ ! -f "\$LOG_PATH" ]; then
  echo "[WARN] Log file \$LOG_PATH not found"
  exit 1
fi
echo "[INFO] --- Last 100 lines of \$LOG_PATH ---"
sudo tail -n 100 "\$LOG_PATH"
# Check for errors, but ignore expected startup errors
ERROR_COUNT=\$(sudo grep -c "\\[ERROR\\]" "\$LOG_PATH" || echo "0")
if [ "\$ERROR_COUNT" -gt 0 ]; then
  # Count non-startup errors (errors that aren't about SSM parameter not found)
  CRITICAL_ERRORS=\$(sudo grep "\\[ERROR\\]" "\$LOG_PATH" | grep -v "parameter not found yet" | grep -v "this is normal during startup" | wc -l || echo "0")
  if [ "\$CRITICAL_ERRORS" -gt 0 ]; then
    echo "[ERROR] Found \$CRITICAL_ERRORS critical error entries in \$LOG_PATH"
    exit 2
  else
    echo "[INFO] Found \$ERROR_COUNT error entries, but all are expected startup errors"
  fi
fi
EOF
}

start_colony() {
  local host="$1"
  log_info "Triggering colony start via coordinator at $host..."
  ssh_exec "$host" bash -se <<EOF
set -euo pipefail
TMP=\$(mktemp)
STATUS=\$(curl -s -o "\$TMP" -w '%{http_code}' -X POST "http://127.0.0.1:${COORDINATOR_HTTP_PORT}/cloud-start")
echo "[INFO] cloud-start HTTP status: \$STATUS"
cat "\$TMP"
rm -f "\$TMP"
if [ "\$STATUS" -ge 400 ]; then
  echo "[ERROR] cloud-start request failed"
  exit 1
fi
EOF
}

main() {
  ensure_prereqs
  parse_expected_backends
  build_and_push
  deploy_cdk

  log_info "Step 3: Waiting for coordinator instance..."
  local coordinator_entry
  coordinator_entry=$(wait_for_instances "coordinator" 1 | head -n 1)
  local coordinator_id="${coordinator_entry%%:*}"
  local coordinator_ip="${coordinator_entry##*:}"
  log_info "Coordinator ready: $coordinator_id @ $coordinator_ip"

  log_info "Step 4: Waiting for backend instances..."
  local backend_entries_raw
  backend_entries_raw="$(wait_for_instances "backend" "${EXPECTED_BACKENDS}")"
  local backend_entries=()
  if [ -n "$backend_entries_raw" ]; then
    while IFS= read -r line; do
      [ -n "$line" ] && backend_entries+=("$line")
    done <<<"$backend_entries_raw"
  fi
  log_info "Backend instances ready: ${backend_entries[*]}"

  log_info "Waiting for services to register in SSM and start up (30 seconds)..."
  sleep 30

  local coordinator_log="/data/distributed-colony/output/logs/coordinator_${COORDINATOR_PORT}.log"
  local backend_log="/data/distributed-colony/output/logs/be_${BACKEND_PORT}.log"

  check_logs "$coordinator_ip" "$coordinator_log" "coordinator"
  for entry in "${backend_entries[@]}"; do
    local ip="${entry##*:}"
    check_logs "$ip" "$backend_log" "backend"
  done

  start_colony "$coordinator_ip"

  log_info "All steps completed. Remember to stop spot fleets when finished (cd CDK && ./stop.sh)."
  log_info "Coordinator IP: $coordinator_ip"
  log_info "Backend IP(s): ${backend_entries[*]}"
}

main "$@"

