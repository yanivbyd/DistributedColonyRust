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
    
    if [ -z "$public_ip" ] || [ "$public_ip" == "None" ]; then
        print_warning "${instance_type} ${instance_id}: No public IP, skipping..."
        return
    fi
    
    DEBUG_URL="http://${public_ip}:${HTTP_PORT}/debug-ssm"
    
    # Use a temporary file for the response
    TEMP_RESPONSE=$(mktemp)
    TEMP_STDERR=$(mktemp)
    
    # Call the endpoint with timeout, redirect stderr separately
    HTTP_CODE=$(curl -s -S -o "$TEMP_RESPONSE" -w "%{http_code}" --max-time 10 -X GET "$DEBUG_URL" 2>"$TEMP_STDERR")
    CURL_EXIT_CODE=$?
    
    # Output format: "Type (instance_id): <result or error>"
    if [ "$CURL_EXIT_CODE" -eq 0 ] && [ "$HTTP_CODE" != "000" ] && [ -f "$TEMP_RESPONSE" ]; then
        if [ "$HTTP_CODE" -ge 200 ] && [ "$HTTP_CODE" -lt 300 ]; then
            if [ -s "$TEMP_RESPONSE" ]; then
                # Remove duplicates by keeping only unique lines, preserve order
                UNIQUE_RESPONSE=$(awk '!seen[$0]++' "$TEMP_RESPONSE")
                echo "${instance_type} (${instance_id}):"
                echo "$UNIQUE_RESPONSE" | sed 's/^/  /'
                # Also log to file if specified
                if [ -n "$LOG_FILE" ]; then
                    echo "${instance_type} (${instance_id}):" >> "$LOG_FILE"
                    echo "$UNIQUE_RESPONSE" | sed 's/^/  /' >> "$LOG_FILE"
                fi
            else
                echo "${instance_type} (${instance_id}): (empty response)"
                [ -n "$LOG_FILE" ] && echo "${instance_type} (${instance_id}): (empty response)" >> "$LOG_FILE"
            fi
        else
            print_warning "${instance_type} (${instance_id}): HTTP ${HTTP_CODE}"
            if [ -s "$TEMP_RESPONSE" ]; then
                cat "$TEMP_RESPONSE"
                [ -n "$LOG_FILE" ] && cat "$TEMP_RESPONSE" >> "$LOG_FILE"
            fi
        fi
    else
        # Extract error message from stderr, or use a default message
        if [ -s "$TEMP_STDERR" ]; then
            ERROR_MSG=$(head -1 "$TEMP_STDERR" | sed 's/curl: //')
        else
            ERROR_MSG="Connection failed (timeout or refused)"
        fi
        print_warning "${instance_type} (${instance_id}): ${ERROR_MSG}"
        if [ -n "$LOG_FILE" ]; then
            echo "${instance_type} (${instance_id}): ${ERROR_MSG}" >> "$LOG_FILE"
        fi
    fi
    
    rm -f "$TEMP_RESPONSE" "$TEMP_STDERR"
}

# Main function
main() {
    print_step "Debugging DistributedColony nodes via /debug-ssm endpoint..."
    
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
          --region "${AWS_REGION}" 2>/dev/null
    }

    # Call debug-ssm on coordinator instances
    COORDINATOR_INSTANCES=$(get_instance_info "coordinator")
    if [ -n "$COORDINATOR_INSTANCES" ]; then
        while read -r instance_id public_ip; do
            if [ -n "$instance_id" ]; then
                call_debug_ssm "coordinator" "$instance_id" "$public_ip"
            fi
        done <<< "$COORDINATOR_INSTANCES"
    else
        print_warning "No coordinator instances found"
    fi

    # Call debug-ssm on backend instances
    BACKEND_INSTANCES=$(get_instance_info "backend")
    if [ -n "$BACKEND_INSTANCES" ]; then
        while read -r instance_id public_ip; do
            if [ -n "$instance_id" ]; then
                call_debug_ssm "backend" "$instance_id" "$public_ip"
            fi
        done <<< "$BACKEND_INSTANCES"
    else
        print_warning "No backend instances found"
    fi
}

# Run main function
main "$@"

