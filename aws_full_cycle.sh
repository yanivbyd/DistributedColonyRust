#!/bin/bash

# Complete AWS deployment cycle script for DistributedColony
# This script builds, pushes, deploys, tests, and collects logs

set -e

# Configuration
AWS_REGION=${AWS_REGION:-"eu-west-1"}
STACK_NAME="DistributedColonySpotInstances"
KEY_NAME="distributed-colony-key"
KEY_PATH=${KEY_PATH:-"CDK/distributed-colony-key.pem"}
COORDINATOR_HTTP_PORT=8084
BACKEND_PORT=8082
COORDINATOR_PORT=8083
LOG_DIR="./logs"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output (both to console and log file)
print_status() {
    local message="$1"
    echo -e "${GREEN}[INFO]${NC} $message" | tee -a "$LOG_FILE"
}

print_warning() {
    local message="$1"
    echo -e "${YELLOW}[WARNING]${NC} $message" | tee -a "$LOG_FILE"
}

print_error() {
    local message="$1"
    echo -e "${RED}[ERROR]${NC} $message" | tee -a "$LOG_FILE"
}

print_step() {
    local message="$1"
    echo -e "${BLUE}[STEP]${NC} $message" | tee -a "$LOG_FILE"
}

# Get script directory to ensure we can run from anywhere
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Set up logging
RUN_LOGS_DIR="${SCRIPT_DIR}/run_logs"
mkdir -p "$RUN_LOGS_DIR"
LOG_TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
LOG_FILE="${RUN_LOGS_DIR}/aws_full_cycle_${LOG_TIMESTAMP}.log"

# Function to log to both console and file
log_output() {
    echo "$@" | tee -a "$LOG_FILE"
}

# Trap handler to log errors on unexpected exit
cleanup_on_exit() {
    EXIT_CODE=$?
    if [ $EXIT_CODE -ne 0 ]; then
        {
            echo ""
            echo "=========================================="
            echo "Script exited unexpectedly with code: $EXIT_CODE"
            echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
            echo "=========================================="
        } >> "$LOG_FILE"
    fi
}
trap cleanup_on_exit EXIT

# Start log file with header
{
    echo "=========================================="
    echo "AWS Full Cycle Deployment Log"
    echo "Started: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
    echo "Script: $0"
    echo "Working Directory: $SCRIPT_DIR"
    echo "Log File: $LOG_FILE"
    echo "AWS Region: ${AWS_REGION}"
    echo "Stack Name: ${STACK_NAME}"
    echo "=========================================="
    echo ""
} | tee "$LOG_FILE"

# Expand tilde and resolve relative paths for KEY_PATH
if [[ "$KEY_PATH" =~ ^~ ]]; then
    EXPANDED_KEY_PATH="${KEY_PATH/#~/$HOME}"
elif [[ "$KEY_PATH" =~ ^/ ]]; then
    EXPANDED_KEY_PATH="$KEY_PATH"
else
    # Relative path - resolve from script directory
    EXPANDED_KEY_PATH="${SCRIPT_DIR}/${KEY_PATH}"
fi

print_step "Starting AWS Full Cycle Deployment" | tee -a "$LOG_FILE"
log_output "=========================================="
log_output "Log file: $LOG_FILE"
log_output ""

# Check prerequisites
print_step "Step 0: Checking prerequisites..."

if ! command -v aws &> /dev/null; then
    print_error "AWS CLI is not installed!"
    exit 1
fi

if ! aws sts get-caller-identity > /dev/null 2>&1; then
    print_error "AWS CLI is not configured or credentials are invalid"
    exit 1
fi

if [ ! -f "$EXPANDED_KEY_PATH" ]; then
    print_warning "SSH key file not found at: $EXPANDED_KEY_PATH"
    print_warning "You may need to set KEY_PATH environment variable"
    print_warning "Continuing anyway - SSH operations will fail if key is needed..."
fi

# Step 1: Build and push Docker image
print_step "Step 1: Building and pushing Docker image..."
cd Docker

if [ ! -f "build-and-push.sh" ]; then
    print_error "build-and-push.sh not found in Docker directory"
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 1"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi

log_output "Running: ./build-and-push.sh"
if ./build-and-push.sh 2>&1 | tee -a "$LOG_FILE"; then
    print_status "Docker image built and pushed successfully!"
else
    print_error "Docker build and push failed!"
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 1"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi
cd ..

# Step 2: Deploy CDK stack
print_step "Step 2: Deploying CDK infrastructure..."

cd CDK

# Check if CDK is installed
if ! command -v cdk &> /dev/null; then
    print_warning "CDK CLI not found. Installing dependencies..."
    npm install
fi

# Bootstrap CDK if needed (silent if already bootstrapped)
print_status "Bootstrapping CDK (if needed)..."
cdk bootstrap 2>&1 | grep -v "already bootstrapped" || true

# Deploy the stack
print_status "Deploying CDK stack: $STACK_NAME"
log_output "Running: cdk deploy $STACK_NAME --require-approval never"
if cdk deploy "$STACK_NAME" --require-approval never 2>&1 | tee -a "$LOG_FILE"; then
    print_status "CDK deployment completed successfully!"
else
    print_error "CDK deployment failed!"
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 2"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi
cd ..

# Wait for instances to be ready
print_step "Step 3: Waiting for instances to be ready..."
print_status "Waiting 30 seconds for instances to initialize..."
sleep 30

# Step 4: Get coordinator IP and run curl command
print_step "Step 4: Testing coordinator endpoint..."

print_status "Finding coordinator instance..."
log_output "Querying AWS for coordinator instances..."
COORDINATOR_INSTANCE_ID=$(aws ec2 describe-instances \
  --filters \
    Name=instance-state-name,Values=running \
    Name=tag:Type,Values=coordinator \
    Name=tag:Service,Values=distributed-colony \
  --query 'Reservations[].Instances[].InstanceId' \
  --output text \
  --region "${AWS_REGION}" 2>&1 | tee -a "$LOG_FILE" | awk '{print $1}')

if [ -z "$COORDINATOR_INSTANCE_ID" ] || [ "$COORDINATOR_INSTANCE_ID" == "None" ]; then
    print_error "No running coordinator instances found."
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 4: No coordinator instances found"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi

log_output "Found coordinator instance: $COORDINATOR_INSTANCE_ID"
print_status "Coordinator Instance ID: $COORDINATOR_INSTANCE_ID"

log_output "Getting coordinator public IP..."
COORDINATOR_IP=$(aws ec2 describe-instances \
  --instance-ids "${COORDINATOR_INSTANCE_ID}" \
  --query 'Reservations[0].Instances[0].PublicIpAddress' \
  --output text \
  --region "${AWS_REGION}" 2>&1 | tee -a "$LOG_FILE")

if [ -z "$COORDINATOR_IP" ] || [ "$COORDINATOR_IP" == "None" ]; then
    print_error "Coordinator instance does not have a public IP."
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 4: Coordinator has no public IP"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi

log_output "Coordinator Public IP: $COORDINATOR_IP"
print_status "Coordinator Public IP: $COORDINATOR_IP"

# Wait a bit more for services to start
print_status "Waiting 10 more seconds for services to start..."
sleep 10

# Test coordinator HTTP endpoint
print_status "Testing coordinator HTTP endpoint (cloud-start)..."
COORDINATOR_URL="http://${COORDINATOR_IP}:${COORDINATOR_HTTP_PORT}/cloud-start"
log_output "Testing URL: $COORDINATOR_URL"

# Try to curl the endpoint (don't fail script on HTTP errors)
RESPONSE_FILE="/tmp/coordinator_curl_response.txt"
log_output "Running: curl -X POST $COORDINATOR_URL"

# Capture HTTP code and response separately
HTTP_CODE=$(curl -s -o "$RESPONSE_FILE" -w "%{http_code}" -X POST "$COORDINATOR_URL" 2>&1 || echo "000")
CURL_EXIT_CODE=$?

log_output "HTTP Status Code: $HTTP_CODE"
log_output "Curl exit code: $CURL_EXIT_CODE"

if [ "$CURL_EXIT_CODE" -eq 0 ] && [ -f "$RESPONSE_FILE" ]; then
    if [ "$HTTP_CODE" -ge 200 ] && [ "$HTTP_CODE" -lt 300 ]; then
        print_status "Coordinator HTTP endpoint responded successfully! (HTTP $HTTP_CODE)"
    else
        print_warning "Coordinator HTTP endpoint returned status code: $HTTP_CODE"
    fi
    log_output "Response body:"
    if [ -s "$RESPONSE_FILE" ]; then
        cat "$RESPONSE_FILE" | tee -a "$LOG_FILE"
    else
        log_output "(empty response)"
    fi
    echo "" >> "$LOG_FILE"
else
    print_warning "Failed to connect to coordinator HTTP endpoint (service may not be ready yet)"
    if [ -f "$RESPONSE_FILE" ]; then
        log_output "Error response:"
        cat "$RESPONSE_FILE" 2>/dev/null | tee -a "$LOG_FILE" || true
    fi
fi

# Step 4b: Debug all nodes via /debug-ssm endpoint
print_step "Step 4b: Debugging all nodes via /debug-ssm endpoint..."

DEBUG_SCRIPT="${SCRIPT_DIR}/debug_nodes.sh"

if [ ! -f "$DEBUG_SCRIPT" ]; then
    print_warning "debug_nodes.sh not found at: $DEBUG_SCRIPT, skipping debug step"
    log_output "WARNING: debug_nodes.sh not found, skipping debug step"
else
    log_output "Calling debug_nodes.sh to debug all nodes..."
    if AWS_REGION="$AWS_REGION" \
        HTTP_PORT="$COORDINATOR_HTTP_PORT" \
        LOG_FILE="$LOG_FILE" \
        "$DEBUG_SCRIPT" 2>&1 | tee -a "$LOG_FILE"; then
        print_status "Debugging completed!"
    else
        print_warning "Debug script encountered errors (non-fatal)"
    fi
fi

# Step 5: Copy logs from spot instances
print_step "Step 5: Copying application logs from spot instances..."

log_output "Calling gather_logs_from_nodes.sh to collect logs from all instances..."
GATHER_LOGS_SCRIPT="${SCRIPT_DIR}/gather_logs_from_nodes.sh"

if [ ! -f "$GATHER_LOGS_SCRIPT" ]; then
    print_error "gather_logs_from_nodes.sh not found at: $GATHER_LOGS_SCRIPT"
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 5: gather_logs_from_nodes.sh not found"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi

# Call the log gathering script with appropriate environment variables
# Run the script and capture all output to log file
if AWS_REGION="$AWS_REGION" \
    KEY_PATH="$EXPANDED_KEY_PATH" \
    LOG_DIR="$LOG_DIR" \
    COORDINATOR_PORT="$COORDINATOR_PORT" \
    BACKEND_PORT="$BACKEND_PORT" \
    LOG_FILE="$LOG_FILE" \
    "$GATHER_LOGS_SCRIPT" 2>&1 | tee -a "$LOG_FILE"; then
    # Find the most recent log directory created (should be the one we just created)
    INSTANCE_LOG_DIR=$(ls -td "${LOG_DIR}"/*/ 2>/dev/null | head -1 | sed 's|/$||')
    if [ -n "$INSTANCE_LOG_DIR" ] && [ -d "$INSTANCE_LOG_DIR" ]; then
        print_status "Logs gathered successfully to: $INSTANCE_LOG_DIR"
        log_output "Instance log directory: $INSTANCE_LOG_DIR"
    else
        print_warning "Could not determine instance log directory path"
        INSTANCE_LOG_DIR="${LOG_DIR}/$(date +"%Y%m%d_%H%M%S")"
    fi
else
    print_warning "Log gathering script encountered errors"
    # Still try to find the most recent directory
    INSTANCE_LOG_DIR=$(ls -td "${LOG_DIR}"/*/ 2>/dev/null | head -1 | sed 's|/$||' || echo "${LOG_DIR}/$(date +"%Y%m%d_%H%M%S")")
fi

# Summary
print_step "Deployment Cycle Complete!"
log_output ""
log_output "=========================================="
log_output "DEPLOYMENT SUMMARY"
log_output "=========================================="
log_output "Stack Name: $STACK_NAME"
log_output "AWS Region: $AWS_REGION"
log_output ""
log_output "COORDINATOR:"
log_output "  Instance ID: $COORDINATOR_INSTANCE_ID"
log_output "  Public IP: $COORDINATOR_IP"
log_output "  HTTP URL: http://${COORDINATOR_IP}:${COORDINATOR_HTTP_PORT}"
log_output "  Protocol Port: $COORDINATOR_PORT"
log_output ""
log_output "BACKEND:"
log_output "  Port: $BACKEND_PORT"
if [ -n "$BACKEND_INSTANCES" ]; then
    BACKEND_COUNT=$(echo "$BACKEND_INSTANCES" | grep -v "^$" | wc -l | tr -d ' ')
    log_output "  Instance Count: $BACKEND_COUNT"
    echo "$BACKEND_INSTANCES" | while read -r instance_id public_ip; do
        if [ -n "$instance_id" ]; then
            log_output "    - $instance_id ($public_ip)"
        fi
    done
else
    log_output "  Instance Count: 0"
fi
log_output ""
log_output "LOGS:"
if [ -n "$INSTANCE_LOG_DIR" ] && [ -d "$INSTANCE_LOG_DIR" ]; then
    log_output "  Application Logs: $INSTANCE_LOG_DIR"
else
    log_output "  Application Logs: (not available)"
fi
log_output "  Run Log File: $LOG_FILE"
log_output "=========================================="
log_output ""

# Add footer to log file
{
    echo "=========================================="
    echo "Deployment COMPLETED SUCCESSFULLY"
    echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
    echo "=========================================="
} >> "$LOG_FILE"

print_status "Full cycle completed successfully!"
log_output ""
log_output "Review the detailed log at: $LOG_FILE"

