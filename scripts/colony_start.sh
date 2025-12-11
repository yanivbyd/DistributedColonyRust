#!/bin/bash

# Script to verify nodes are up and trigger colony-start on the coordinator
# This script checks that coordinator and at least one backend are available,
# then calls the colony-start endpoint

set -e

# Configuration (can be overridden via environment variables)
AWS_REGION=${AWS_REGION:-"eu-west-1"}
HTTP_PORT=${HTTP_PORT:-8084}
LOG_FILE=${LOG_FILE:-""}  # Optional: if provided, will append to this log file

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Function to print colored output
print_status() {
    local message="$1"
    if [ -n "$LOG_FILE" ]; then
        echo -e "${GREEN}[INFO]${NC} $message" | tee -a "$LOG_FILE"
    else
        echo -e "${GREEN}[INFO]${NC} $message"
    fi
}

print_warning() {
    local message="$1"
    if [ -n "$LOG_FILE" ]; then
        echo -e "${YELLOW}[WARNING]${NC} $message" | tee -a "$LOG_FILE"
    else
        echo -e "${YELLOW}[WARNING]${NC} $message"
    fi
}

print_error() {
    local message="$1"
    if [ -n "$LOG_FILE" ]; then
        echo -e "${RED}[ERROR]${NC} $message" | tee -a "$LOG_FILE"
    else
        echo -e "${RED}[ERROR]${NC} $message"
    fi
}

print_step() {
    local message="$1"
    if [ -n "$LOG_FILE" ]; then
        echo -e "${BLUE}[STEP]${NC} $message" | tee -a "$LOG_FILE"
    else
        echo -e "${BLUE}[STEP]${NC} $message"
    fi
}

log_output() {
    if [ -n "$LOG_FILE" ]; then
        echo "$@" | tee -a "$LOG_FILE"
    else
        echo "$@"
    fi
}

# Function to call debug-ssm endpoint and get response
call_debug_ssm() {
    local public_ip=$1
    
    if [ -z "$public_ip" ] || [ "$public_ip" == "None" ]; then
        echo ""
        return 1
    fi
    
    DEBUG_URL="http://${public_ip}:${HTTP_PORT}/debug-ssm"
    TEMP_RESPONSE=$(mktemp)
    TEMP_STDERR=$(mktemp)
    
    HTTP_CODE=$(curl -s -S -o "$TEMP_RESPONSE" -w "%{http_code}" --max-time 10 -X GET "$DEBUG_URL" 2>"$TEMP_STDERR")
    CURL_EXIT_CODE=$?
    
    if [ "$CURL_EXIT_CODE" -eq 0 ] && [ "$HTTP_CODE" -ge 200 ] && [ "$HTTP_CODE" -lt 300 ] && [ -f "$TEMP_RESPONSE" ] && [ -s "$TEMP_RESPONSE" ]; then
        cat "$TEMP_RESPONSE"
        rm -f "$TEMP_RESPONSE" "$TEMP_STDERR"
        return 0
    else
        rm -f "$TEMP_RESPONSE" "$TEMP_STDERR"
        return 1
    fi
}


# Function to check if coordinator exists in response
check_coordinator() {
    local response="$1"
    # Check if response contains a coordinator line (not "<none>")
    echo "$response" | grep -q "^Coordinator: " && ! echo "$response" | grep -q "^Coordinator: <none>" && return 0 || return 1
}

# Main function
main() {
    print_step "Colony Start - Verifying nodes and triggering colony-start..."
    log_output "AWS Region: $AWS_REGION"
    log_output "HTTP Port: $HTTP_PORT"
    log_output ""

    # Function to get coordinator instance
    get_coordinator_info() {
        aws ec2 describe-instances \
          --filters \
            Name=instance-state-name,Values=running \
            Name=tag:Type,Values=coordinator \
            Name=tag:Service,Values=distributed-colony \
          --query 'Reservations[].Instances[].[InstanceId,PublicIpAddress]' \
          --output text \
          --region "${AWS_REGION}" 2>/dev/null | head -1
    }

    # Step 1: Find coordinator instance
    print_status "Finding coordinator instance..."
    COORDINATOR_INFO=$(get_coordinator_info)
    
    if [ -z "$COORDINATOR_INFO" ]; then
        print_error "No running coordinator instances found"
        exit 1
    fi
    
    COORDINATOR_INSTANCE_ID=$(echo "$COORDINATOR_INFO" | awk '{print $1}')
    COORDINATOR_IP=$(echo "$COORDINATOR_INFO" | awk '{print $2}')
    
    if [ -z "$COORDINATOR_IP" ] || [ "$COORDINATOR_IP" == "None" ]; then
        print_error "Coordinator instance does not have a public IP"
        exit 1
    fi
    
    log_output "Coordinator: $COORDINATOR_INSTANCE_ID ($COORDINATOR_IP)"
    
    # Step 2: Verify coordinator and backends via debug-ssm
    print_status "Verifying coordinator and backends via debug-ssm..."
    DEBUG_RESPONSE=$(call_debug_ssm "$COORDINATOR_IP")
    
    if [ $? -ne 0 ] || [ -z "$DEBUG_RESPONSE" ]; then
        print_error "Failed to get debug-ssm response from coordinator"
        exit 1
    fi
    
    # Remove duplicates from response (macOS compatible)
    DEBUG_RESPONSE=$(echo "$DEBUG_RESPONSE" | awk '!seen[$0]++')
    
    log_output "Debug-SSM response:"
    echo "$DEBUG_RESPONSE" | sed 's/^/  /' | tee -a "${LOG_FILE:-/dev/stdout}"
    log_output ""
    
    # Step 3: Verify coordinator exists
    if ! check_coordinator "$DEBUG_RESPONSE"; then
        print_error "Coordinator not found in SSM (may not be registered yet)"
        exit 1
    fi
    
    print_status "Coordinator verified ✓"
    
    # Step 4: Verify at least one backend exists
    # Parse backend count - macOS compatible (no grep -P)
    BACKEND_COUNT=$(echo "$DEBUG_RESPONSE" | grep "Backends (" | sed -n 's/.*Backends (\([0-9]*\) total).*/\1/p' | head -1)
    if [ -z "$BACKEND_COUNT" ]; then
        BACKEND_COUNT="0"
    fi
    
    if [ "$BACKEND_COUNT" -eq 0 ] || [ -z "$BACKEND_COUNT" ]; then
        print_error "No backends found in SSM (found: $BACKEND_COUNT)"
        exit 1
    fi
    
    print_status "Backends verified: $BACKEND_COUNT backend(s) available ✓"
    log_output ""
    
    # Step 5: Call colony-start endpoint
    print_status "Triggering colony-start on coordinator..."
    
    # Generate idempotency_key (must be generated by client, not server)
    IDEMPOTENCY_KEY=$(date +%s)
    log_output "Generated idempotency_key: $IDEMPOTENCY_KEY"
    
    COLONY_START_URL="http://${COORDINATOR_IP}:${HTTP_PORT}/colony-start?idempotency_key=${IDEMPOTENCY_KEY}"
    log_output "URL: $COLONY_START_URL"
    
    TEMP_RESPONSE=$(mktemp)
    TEMP_STDERR=$(mktemp)
    
    HTTP_CODE=$(curl -s -S -o "$TEMP_RESPONSE" -w "%{http_code}" --max-time 30 -X POST "$COLONY_START_URL" 2>"$TEMP_STDERR")
    CURL_EXIT_CODE=$?
    
    log_output "HTTP Status Code: $HTTP_CODE"
    
    if [ "$CURL_EXIT_CODE" -ne 0 ]; then
        print_error "Failed to connect to colony-start endpoint"
        if [ -s "$TEMP_STDERR" ]; then
            ERROR_MSG=$(head -1 "$TEMP_STDERR")
            log_output "Error: $ERROR_MSG"
        fi
        rm -f "$TEMP_RESPONSE" "$TEMP_STDERR"
        exit 1
    fi
    
    # Handle different HTTP response codes
    if [ "$HTTP_CODE" -eq 200 ]; then
        RESPONSE_BODY=$(cat "$TEMP_RESPONSE" 2>/dev/null || echo "")
        if echo "$RESPONSE_BODY" | grep -q "idempotent"; then
            print_status "Colony-start already completed with same idempotency_key (HTTP 200 - idempotent)"
            if [ -s "$TEMP_RESPONSE" ]; then
                log_output "Response:"
                cat "$TEMP_RESPONSE" | sed 's/^/  /' | tee -a "${LOG_FILE:-/dev/stdout}"
            fi
        else
            print_status "Colony-start succeeded (HTTP 200)"
            if [ -s "$TEMP_RESPONSE" ]; then
                log_output "Response:"
                cat "$TEMP_RESPONSE" | sed 's/^/  /' | tee -a "${LOG_FILE:-/dev/stdout}"
            fi
        fi
    elif [ "$HTTP_CODE" -eq 202 ]; then
        print_status "Colony-start triggered successfully! (HTTP 202 Accepted)"
        if [ -s "$TEMP_RESPONSE" ]; then
            log_output "Response:"
            cat "$TEMP_RESPONSE" | sed 's/^/  /' | tee -a "${LOG_FILE:-/dev/stdout}"
        fi
        log_output ""
        print_status "Colony-start initiated. Check coordinator logs for progress."
    elif [ "$HTTP_CODE" -eq 409 ]; then
        print_error "Colony already started with different idempotency_key (HTTP 409 Conflict)"
        if [ -s "$TEMP_RESPONSE" ]; then
            log_output "Response:"
            cat "$TEMP_RESPONSE" | sed 's/^/  /'
        fi
        rm -f "$TEMP_RESPONSE" "$TEMP_STDERR"
        exit 1
    elif [ "$HTTP_CODE" -eq 400 ]; then
        print_error "Invalid request - idempotency_key required (HTTP 400 Bad Request)"
        if [ -s "$TEMP_RESPONSE" ]; then
            log_output "Response:"
            cat "$TEMP_RESPONSE" | sed 's/^/  /'
        fi
        rm -f "$TEMP_RESPONSE" "$TEMP_STDERR"
        exit 1
    else
        print_error "Colony-start failed (HTTP $HTTP_CODE)"
        if [ -s "$TEMP_STDERR" ]; then
            ERROR_MSG=$(head -1 "$TEMP_STDERR")
            log_output "Error: $ERROR_MSG"
        fi
        if [ -s "$TEMP_RESPONSE" ]; then
            log_output "Response:"
            cat "$TEMP_RESPONSE" | sed 's/^/  /'
        fi
        rm -f "$TEMP_RESPONSE" "$TEMP_STDERR"
        exit 1
    fi
    
    rm -f "$TEMP_RESPONSE" "$TEMP_STDERR"
    print_status "Colony-start script completed successfully!"
}

# Run main function
main "$@"
