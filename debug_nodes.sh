#!/bin/bash

# Script to debug DistributedColony nodes by calling /debug-ssm endpoint
# This script finds all coordinator and backend instances and calls their debug endpoints

set -e

# Configuration (can be overridden via environment variables or arguments)
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

# Function to call debug-ssm endpoint on a node
call_debug_ssm() {
    local instance_type=$1
    local instance_id=$2
    local public_ip=$3
    
    print_status "Calling /debug-ssm on ${instance_type} ${instance_id} (${public_ip})..."
    
    if [ -z "$public_ip" ] || [ "$public_ip" == "None" ]; then
        print_warning "Instance ${instance_id} has no public IP, skipping..."
        return
    fi
    
    DEBUG_URL="http://${public_ip}:${HTTP_PORT}/debug-ssm"
    log_output "  URL: $DEBUG_URL"
    
    # Use a temporary file for the response
    TEMP_RESPONSE=$(mktemp)
    trap "rm -f $TEMP_RESPONSE" EXIT
    
    # Call the endpoint with timeout
    HTTP_CODE=$(curl -s -o "$TEMP_RESPONSE" -w "%{http_code}" --max-time 10 -X GET "$DEBUG_URL" 2>&1 || echo "000")
    CURL_EXIT_CODE=$?
    
    if [ "$CURL_EXIT_CODE" -eq 0 ] && [ -f "$TEMP_RESPONSE" ]; then
        log_output "  HTTP Status Code: $HTTP_CODE"
        if [ "$HTTP_CODE" -ge 200 ] && [ "$HTTP_CODE" -lt 300 ]; then
            print_status "  Success! Response from ${instance_type} ${instance_id}:"
            log_output "  ---"
            if [ -s "$TEMP_RESPONSE" ]; then
                # Indent the response for readability
                sed 's/^/    /' "$TEMP_RESPONSE" | tee -a "${LOG_FILE:-/dev/stdout}"
            else
                log_output "    (empty response)"
            fi
            log_output "  ---"
        else
            print_warning "  HTTP endpoint returned status code: $HTTP_CODE"
            if [ -s "$TEMP_RESPONSE" ]; then
                log_output "  Response:"
                sed 's/^/    /' "$TEMP_RESPONSE" | tee -a "${LOG_FILE:-/dev/stdout}"
            fi
        fi
    else
        print_warning "  Failed to connect to ${instance_type} ${instance_id} (service may not be ready yet or not reachable)"
        if [ -f "$TEMP_RESPONSE" ] && [ -s "$TEMP_RESPONSE" ]; then
            log_output "  Error response:"
            sed 's/^/    /' "$TEMP_RESPONSE" | tee -a "${LOG_FILE:-/dev/stdout}"
        fi
    fi
    
    log_output ""
    rm -f "$TEMP_RESPONSE"
}

# Main function
main() {
    print_step "Debugging DistributedColony nodes via /debug-ssm endpoint..."
    log_output "AWS Region: $AWS_REGION"
    log_output "HTTP Port: $HTTP_PORT"
    log_output ""

    # Function to get all instances of a type
    get_instance_info() {
        local instance_type=$1
        aws ec2 describe-instances \
          --filters \
            Name=instance-state-name,Values=running \
            Name=tag:Type,Values="${instance_type}" \
            Name=tag:Service,Values=distributed-colony \
          --query 'Reservations[].Instances[].[InstanceId,PublicIpAddress]' \
          --output text \
          --region "${AWS_REGION}"
    }

    # Call debug-ssm on coordinator instances
    print_status "Finding coordinator instances..."
    log_output "Querying AWS for all coordinator instances..."
    COORDINATOR_INSTANCES=$(get_instance_info "coordinator")
    if [ -n "$COORDINATOR_INSTANCES" ]; then
        COORDINATOR_COUNT=$(echo "$COORDINATOR_INSTANCES" | grep -v "^$" | wc -l | tr -d ' ')
        log_output "Found $COORDINATOR_COUNT coordinator instance(s)"
        while read -r instance_id public_ip; do
            if [ -n "$instance_id" ]; then
                log_output "  - Coordinator: $instance_id ($public_ip)"
                call_debug_ssm "coordinator" "$instance_id" "$public_ip"
            fi
        done <<< "$COORDINATOR_INSTANCES"
    else
        print_warning "No coordinator instances found"
        log_output "WARNING: No coordinator instances found"
        log_output ""
    fi

    # Call debug-ssm on backend instances
    print_status "Finding backend instances..."
    log_output "Querying AWS for all backend instances..."
    BACKEND_INSTANCES=$(get_instance_info "backend")
    if [ -n "$BACKEND_INSTANCES" ]; then
        BACKEND_COUNT=$(echo "$BACKEND_INSTANCES" | grep -v "^$" | wc -l | tr -d ' ')
        log_output "Found $BACKEND_COUNT backend instance(s)"
        while read -r instance_id public_ip; do
            if [ -n "$instance_id" ]; then
                log_output "  - Backend: $instance_id ($public_ip)"
                call_debug_ssm "backend" "$instance_id" "$public_ip"
            fi
        done <<< "$BACKEND_INSTANCES"
    else
        print_warning "No backend instances found"
        log_output "WARNING: No backend instances found"
        log_output ""
    fi

    print_status "Debugging completed!"
}

# Run main function
main "$@"

