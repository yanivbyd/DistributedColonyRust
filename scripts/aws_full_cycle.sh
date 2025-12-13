#!/bin/bash

# Complete AWS deployment cycle script for DistributedColony
# This script builds, pushes, deploys, tests, and launches GUI in AWS mode

set -e

# Configuration
AWS_REGION=${AWS_REGION:-"eu-west-1"}
STACK_NAME="DistributedColonySpotInstances"
KEY_NAME="distributed-colony-key"
KEY_PATH=${KEY_PATH:-"CDK/distributed-colony-key.pem"}
COORDINATOR_HTTP_PORT=8084
BACKEND_PORT=8082
COORDINATOR_PORT=8083

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
# Change to workspace root (one level up from scripts directory)
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$WORKSPACE_ROOT"

# Set up logging directories (must be after WORKSPACE_ROOT is defined)
LOG_DIR="${WORKSPACE_ROOT}/logs"
RUN_LOGS_DIR="${WORKSPACE_ROOT}/run_logs"
mkdir -p "$RUN_LOGS_DIR"
LOG_TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
LOG_FILE="${RUN_LOGS_DIR}/aws_full_cycle_${LOG_TIMESTAMP}.log"

# Function to log to both console and file (for important messages)
log_output() {
    echo "$@" | tee -a "$LOG_FILE"
}

# Function to log only to file (for verbose diagnostic information)
log() {
    echo "$@" >> "$LOG_FILE"
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
    echo "Script Directory: $SCRIPT_DIR"
    echo "Working Directory: $WORKSPACE_ROOT"
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
    # Relative path - resolve from workspace root
    EXPANDED_KEY_PATH="${WORKSPACE_ROOT}/${KEY_PATH}"
fi

print_step "Starting AWS Full Cycle Deployment" | tee -a "$LOG_FILE"
log_output "=========================================="
log_output "Log file: $LOG_FILE"
log_output ""

cd "${WORKSPACE_ROOT}/CDK"

# Check if CDK is installed
if ! command -v cdk &> /dev/null; then
    print_warning "CDK CLI not found. Attempting to install dependencies..."
    if [ -f "package.json" ]; then
        npm install
    else
        print_warning "package.json not found, skipping CDK destroy"
        cd ..
    fi
fi

if command -v cdk &> /dev/null; then
    print_status "Destroying CDK stack: $STACK_NAME"
    log_output "Running: cdk destroy --force $STACK_NAME"
    
    # Capture CDK destroy output
    TEMP_DESTROY_LOG=$(mktemp)
    trap "rm -f $TEMP_DESTROY_LOG" EXIT
    
    if cdk destroy --force "$STACK_NAME" > "$TEMP_DESTROY_LOG" 2>&1; then
        # Extract high-level messages
        grep -E "(Stack|DELETE|COMPLETE|destroyed|Destroyed|does not exist|No stacks found)" "$TEMP_DESTROY_LOG" | tee -a "$LOG_FILE" || true
        print_status "CDK destroy completed (or stack did not exist)"
    else
        DESTROY_EXIT_CODE=$?
        # Check if error is just "stack doesn't exist" - that's fine
        if grep -q "does not exist\|No stacks found" "$TEMP_DESTROY_LOG"; then
            print_status "Stack does not exist (nothing to destroy)"
        else
            # Show errors but don't fail - we want to continue with deployment
            print_warning "CDK destroy encountered errors (continuing anyway)"
            log_output "Destroy output (errors):"
            grep -E "(ERROR|error|Error|failed|Failed)" "$TEMP_DESTROY_LOG" -A 2 -B 1 | tee -a "$LOG_FILE" || true
        fi
    fi
    rm -f "$TEMP_DESTROY_LOG"
else
    print_warning "CDK CLI still not available after npm install attempt, skipping destroy"
fi

cd "${WORKSPACE_ROOT}"

# Clean up stale SSM parameters from previous deployments
print_step "Step 0: Cleaning up stale SSM parameters..."
SSM_CLEANUP_COUNT=0
if SSM_PARAMS=$(aws ssm get-parameters-by-path --path "/colony/backends" --region "${AWS_REGION}" --query 'Parameters[*].Name' --output text 2>&1); then
    if [ -n "$SSM_PARAMS" ] && [ "$SSM_PARAMS" != "None" ]; then
        for param_name in $SSM_PARAMS; do
            if [ -n "$param_name" ]; then
                if aws ssm delete-parameter --name "$param_name" --region "${AWS_REGION}" 2>&1 | tee -a "$LOG_FILE" > /dev/null; then
                    SSM_CLEANUP_COUNT=$((SSM_CLEANUP_COUNT + 1))
                    log "Deleted stale SSM parameter: $param_name"
                fi
            fi
        done
        if [ $SSM_CLEANUP_COUNT -gt 0 ]; then
            print_status "Cleaned up $SSM_CLEANUP_COUNT stale SSM parameter(s)"
        else
            print_status "No stale SSM parameters found (or cleanup failed)"
        fi
    else
        print_status "No SSM parameters to clean up"
    fi
else
    print_warning "Failed to query SSM parameters (may not exist yet, continuing)"
fi

# Check prerequisites
print_step "Step 1: Checking prerequisites..."

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

# Step 2: Build and push Docker image
print_step "Step 2: Building and pushing Docker image..."
cd "${WORKSPACE_ROOT}/Docker"

if [ ! -f "build-and-push-colony.sh" ]; then
    print_error "build-and-push-colony.sh not found in Docker directory"
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 2"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi

# Capture output to a temp file to filter verbose logs
TEMP_BUILD_LOG=$(mktemp)
trap "rm -f $TEMP_BUILD_LOG" EXIT

# Build and push colony image (suppress verbose output)
print_status "Building and pushing Docker image (this may take a few minutes)..."
if ./build-and-push-colony.sh > "$TEMP_BUILD_LOG" 2>&1; then
    # Only show final success message and total duration
    grep -E "(TOTAL DURATION|build and push completed successfully)" "$TEMP_BUILD_LOG" | tee -a "$LOG_FILE" || true
    print_status "Colony Docker image built and pushed successfully!"
else
    BUILD_EXIT_CODE=$?
    # On failure, show errors and context
    print_error "Colony Docker build and push failed!"
    log_output "Build output (errors and context):"
    grep -E "(ERROR|WARNING|failed|Failed|error|Error)" "$TEMP_BUILD_LOG" -A 2 -B 2 | tee -a "$LOG_FILE" || true
    # Also show last 20 lines as context
    log_output "Last 20 lines of build output:"
    tail -20 "$TEMP_BUILD_LOG" | tee -a "$LOG_FILE" || true
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 2 (Colony Image)"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    rm -f "$TEMP_BUILD_LOG"
    exit $BUILD_EXIT_CODE
fi
rm -f "$TEMP_BUILD_LOG"
cd ..

# Step 3: Deploy CDK stack
print_step "Step 3: Deploying CDK infrastructure..."

cd "${WORKSPACE_ROOT}/CDK"

# Verify we're in the right directory and cdk.json exists
log "Current directory: $(pwd)"
if [ ! -f "cdk.json" ]; then
    print_error "cdk.json not found in CDK directory!"
    log_output "Expected location: ${WORKSPACE_ROOT}/CDK/cdk.json"
    log "Directory contents:"
    ls -la >> "$LOG_FILE" || true
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 3: cdk.json not found"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi

# Check if CDK is installed
if ! command -v cdk &> /dev/null; then
    print_warning "CDK CLI not found. Installing dependencies..."
    if ! npm install; then
        print_error "npm install failed!"
        log_output "npm install output:"
        {
            echo ""
            echo "=========================================="
            echo "Deployment FAILED at Step 3: npm install failed"
            echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
            echo "=========================================="
        } >> "$LOG_FILE"
        exit 1
    fi
fi

# Verify CDK is now available
if ! command -v cdk &> /dev/null; then
    print_error "CDK CLI still not available after npm install!"
    log "Checking for CDK in node_modules:"
    ls -la node_modules/.bin/cdk >> "$LOG_FILE" 2>&1 || true
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 3: CDK CLI not available"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi

# Show CDK version for debugging
log "CDK version: $(cdk --version 2>&1 || echo 'unknown')"

# Bootstrap CDK if needed (silent if already bootstrapped)
cdk bootstrap 2>&1 | grep -v "already bootstrapped" || true

# Deploy the stack
log "CDK command: cdk deploy $STACK_NAME --require-approval never"
log "Working directory: $(pwd)"
log "cdk.json exists: $([ -f cdk.json ] && echo 'yes' || echo 'no')"
# Capture CDK output to filter verbose logs
TEMP_CDK_LOG=$(mktemp)
trap "rm -f $TEMP_CDK_LOG" EXIT

if cdk deploy "$STACK_NAME" --require-approval never > "$TEMP_CDK_LOG" 2>&1; then
    # Extract only high-level status messages from CDK output
    grep -E "(Stack|Output|CREATE|UPDATE|COMPLETE|successfully|Deploying|Deployment)" "$TEMP_CDK_LOG" | tee -a "$LOG_FILE" || true
    print_status "CDK deployment completed successfully!"
else
    CDK_EXIT_CODE=$?
    # On failure, show errors and context
    print_error "CDK deployment failed with exit code: $CDK_EXIT_CODE"
    log_output "CDK diagnostic information:"
    log "  Current directory: $(pwd)"
    log "  cdk.json exists: $([ -f cdk.json ] && echo 'yes' || echo 'no')"
    log "  CDK version: $(cdk --version 2>&1 || echo 'unknown')"
    log "  CDK command attempted: cdk deploy $STACK_NAME --require-approval never"
    log ""
    log_output "CDK output (errors and context):"
    grep -E "(ERROR|error|Error|failed|Failed|rollback|Rollback|required|not found|missing)" "$TEMP_CDK_LOG" -A 3 -B 1 | tee -a "$LOG_FILE" || true
    log ""
    log "Full CDK output:"
    cat "$TEMP_CDK_LOG" >> "$LOG_FILE"
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 3: CDK deployment failed"
        echo "Exit code: $CDK_EXIT_CODE"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    rm -f "$TEMP_CDK_LOG"
    exit 1
fi
rm -f "$TEMP_CDK_LOG"
cd ..

# Wait for instances to be ready
print_step "Step 4: Waiting for instances to be ready..."
print_status "Waiting 30 seconds for instances to initialize..."
sleep 30

# Step 5: Get coordinator IP and run curl command
print_step "Step 5: Testing coordinator endpoint..."

print_status "Finding coordinator instance..."
log "Querying AWS for coordinator instances..."
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
        echo "Deployment FAILED at Step 5: No coordinator instances found"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi

print_status "Coordinator Instance ID: $COORDINATOR_INSTANCE_ID"

log "Getting coordinator public IP..."
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
        echo "Deployment FAILED at Step 5: Coordinator has no public IP"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi

print_status "Coordinator Public IP: $COORDINATOR_IP"

# Wait a bit more for services to start
print_status "Waiting 10 more seconds for services to start..."
sleep 10

# Test coordinator HTTP endpoint (using GET to verify server is up without triggering colony-start)
print_status "Testing coordinator HTTP endpoint (health check)..."
COORDINATOR_URL="http://${COORDINATOR_IP}:${COORDINATOR_HTTP_PORT}/colony-start"

# Try to curl the endpoint with GET (don't fail script on HTTP errors)
# GET /colony-start returns "Colony-start API" without triggering the actual colony-start process
RESPONSE_FILE="/tmp/coordinator_curl_response.txt"
log_output "Running: curl -X GET $COORDINATOR_URL"

# Capture HTTP code and response separately
HTTP_CODE=$(curl -s -o "$RESPONSE_FILE" -w "%{http_code}" -X GET "$COORDINATOR_URL" || echo "000")
CURL_EXIT_CODE=$?

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

# Test debug-ssm endpoint
print_status "Testing coordinator debug-ssm endpoint..."
DEBUG_SSM_URL="http://${COORDINATOR_IP}:${COORDINATOR_HTTP_PORT}/debug-ssm"
DEBUG_RESPONSE_FILE="/tmp/coordinator_debug_ssm_response.txt"
log_output "Running: curl -X GET $DEBUG_SSM_URL"

# Capture HTTP code and response separately
DEBUG_HTTP_CODE=$(curl -s -o "$DEBUG_RESPONSE_FILE" -w "%{http_code}" -X GET "$DEBUG_SSM_URL" || echo "000")
DEBUG_CURL_EXIT_CODE=$?

log_output "HTTP Status Code: $DEBUG_HTTP_CODE"
log_output "Curl exit code: $DEBUG_CURL_EXIT_CODE"

if [ "$DEBUG_CURL_EXIT_CODE" -eq 0 ] && [ -f "$DEBUG_RESPONSE_FILE" ]; then
    if [ "$DEBUG_HTTP_CODE" -ge 200 ] && [ "$DEBUG_HTTP_CODE" -lt 300 ]; then
        print_status "Debug-SSM endpoint responded successfully! (HTTP $DEBUG_HTTP_CODE)"
    else
        print_warning "Debug-SSM endpoint returned status code: $DEBUG_HTTP_CODE"
    fi
    log_output "Response body:"
    if [ -s "$DEBUG_RESPONSE_FILE" ]; then
        cat "$DEBUG_RESPONSE_FILE" | tee -a "$LOG_FILE"
    else
        log_output "(empty response)"
    fi
    echo "" >> "$LOG_FILE"
else
    print_warning "Failed to connect to debug-ssm endpoint (service may not be ready yet)"
    if [ -f "$DEBUG_RESPONSE_FILE" ]; then
        log_output "Error response:"
        cat "$DEBUG_RESPONSE_FILE" 2>/dev/null | tee -a "$LOG_FILE" || true
    fi
fi

# Test debug-network endpoint
print_status "Testing coordinator debug-network endpoint..."
DEBUG_NETWORK_URL="http://${COORDINATOR_IP}:${COORDINATOR_HTTP_PORT}/debug-network"
DEBUG_NETWORK_RESPONSE_FILE="/tmp/coordinator_debug_network_response.txt"
log_output "Running: curl -X GET $DEBUG_NETWORK_URL"

# Capture HTTP code and response separately
DEBUG_NETWORK_HTTP_CODE=$(curl -s -o "$DEBUG_NETWORK_RESPONSE_FILE" -w "%{http_code}" -X GET "$DEBUG_NETWORK_URL" || echo "000")
DEBUG_NETWORK_CURL_EXIT_CODE=$?

log_output "HTTP Status Code: $DEBUG_NETWORK_HTTP_CODE"
log_output "Curl exit code: $DEBUG_NETWORK_CURL_EXIT_CODE"

if [ "$DEBUG_NETWORK_CURL_EXIT_CODE" -eq 0 ] && [ -f "$DEBUG_NETWORK_RESPONSE_FILE" ]; then
    if [ "$DEBUG_NETWORK_HTTP_CODE" -ge 200 ] && [ "$DEBUG_NETWORK_HTTP_CODE" -lt 300 ]; then
        print_status "Debug-Network endpoint responded successfully! (HTTP $DEBUG_NETWORK_HTTP_CODE)"
    else
        print_warning "Debug-Network endpoint returned status code: $DEBUG_NETWORK_HTTP_CODE"
    fi
    log_output "Response body:"
    if [ -s "$DEBUG_NETWORK_RESPONSE_FILE" ]; then
        cat "$DEBUG_NETWORK_RESPONSE_FILE" | tee -a "$LOG_FILE"
    else
        log_output "(empty response)"
    fi
    echo "" >> "$LOG_FILE"
else
    print_warning "Failed to connect to debug-network endpoint (service may not be ready yet)"
    if [ -f "$DEBUG_NETWORK_RESPONSE_FILE" ]; then
        log_output "Error response:"
        cat "$DEBUG_NETWORK_RESPONSE_FILE" 2>/dev/null | tee -a "$LOG_FILE" || true
    fi
fi

# Step 5b: Debug all nodes via /debug-ssm endpoint
print_step "Step 5b: Debugging all nodes via /debug-ssm endpoint..."

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

# Step 5c: Build and run GUI in AWS mode
print_step "Step 5c: Building and running GUI in AWS mode..."

cd "${WORKSPACE_ROOT}"

# Check if GUI binary exists, build if needed
GUI_BINARY="${WORKSPACE_ROOT}/target/balanced/gui"
if [ ! -f "$GUI_BINARY" ]; then
    print_status "GUI binary not found, building GUI..."
    log_output "Building GUI with balanced profile..."
    if cargo build --profile=balanced -p gui 2>&1 | tee -a "$LOG_FILE"; then
        print_status "GUI built successfully!"
    else
        BUILD_EXIT_CODE=$?
        print_error "GUI build failed!"
        {
            echo ""
            echo "=========================================="
            echo "Deployment FAILED at Step 5c: GUI build failed"
            echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
            echo "=========================================="
        } >> "$LOG_FILE"
        exit $BUILD_EXIT_CODE
    fi
else
    print_status "GUI binary found at: $GUI_BINARY"
fi

# Verify GUI binary exists
if [ ! -f "$GUI_BINARY" ]; then
    print_error "GUI binary not found after build attempt"
    {
        echo ""
        echo "=========================================="
        echo "Deployment FAILED at Step 5c: GUI binary not found"
        echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
        echo "=========================================="
    } >> "$LOG_FILE"
    exit 1
fi

# Run GUI in AWS mode
print_status "Starting GUI in AWS mode..."
log_output "Running: $GUI_BINARY aws"
log_output "GUI will discover coordinator from SSM and connect to it."
log_output "Close the GUI window to exit."

# Run GUI in foreground (script will wait for GUI to exit)
if "$GUI_BINARY" aws 2>&1 | tee -a "$LOG_FILE"; then
    print_status "GUI exited successfully!"
else
    GUI_EXIT_CODE=$?
    print_warning "GUI exited with code: $GUI_EXIT_CODE (this may be normal if GUI was closed)"
    log_output "GUI exit code: $GUI_EXIT_CODE"
fi

# Summary
print_step "Deployment Cycle Complete!"
log_output "GUI was launched in AWS mode and has exited."

# Add footer to log file
{
    echo "=========================================="
    echo "Ended: $(date -u +"%Y-%m-%d %H:%M:%S UTC")"
    echo "=========================================="
} >> "$LOG_FILE"

print_status "Full cycle completed successfully!"
log_output ""
log_output "Review the detailed log at: $LOG_FILE"

