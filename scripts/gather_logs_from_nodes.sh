#!/bin/bash

# Script to gather application logs from DistributedColony spot instances
# This script finds all coordinator and backend instances and copies their logs

set -e

# Configuration (can be overridden via environment variables or arguments)
AWS_REGION=${AWS_REGION:-"eu-west-1"}
KEY_PATH=${KEY_PATH:-"CDK/distributed-colony-key.pem"}
# Get workspace root for default LOG_DIR if not set
SCRIPT_DIR_TMP="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT_TMP="$(cd "$SCRIPT_DIR_TMP/.." && pwd)"
LOG_DIR=${LOG_DIR:-"${WORKSPACE_ROOT_TMP}/logs"}
COORDINATOR_PORT=${COORDINATOR_PORT:-8083}
BACKEND_PORT=${BACKEND_PORT:-8082}
LOG_FILE=${LOG_FILE:-""}  # Optional: if provided, will append to this log file

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get script directory
SCRIPT_DIR="${SCRIPT_DIR_TMP:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)}"

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

# Expand and resolve KEY_PATH
if [[ "$KEY_PATH" =~ ^~ ]]; then
    EXPANDED_KEY_PATH="${KEY_PATH/#~/$HOME}"
elif [[ "$KEY_PATH" =~ ^/ ]]; then
    EXPANDED_KEY_PATH="$KEY_PATH"
else
    # Relative path - resolve from workspace root
    EXPANDED_KEY_PATH="${WORKSPACE_ROOT_TMP}/${KEY_PATH}"
fi

# Main function
main() {
    print_step "Gathering logs from DistributedColony instances..."
    log_output "AWS Region: $AWS_REGION"
    log_output "SSH Key: $EXPANDED_KEY_PATH"
    log_output ""

    # Create logs directory
    mkdir -p "$LOG_DIR"
    LOG_TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
    INSTANCE_LOG_DIR="${LOG_DIR}/${LOG_TIMESTAMP}"
    mkdir -p "$INSTANCE_LOG_DIR"

    print_status "Logs will be saved to: $INSTANCE_LOG_DIR"
    log_output "Instance log directory: $INSTANCE_LOG_DIR"
    log_output ""

    # Export INSTANCE_LOG_DIR for use in functions
    export INSTANCE_LOG_DIR

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

    # Function to copy logs from an instance
    copy_logs_from_instance() {
        local instance_id=$1
        local public_ip=$2
        local instance_type=$3
        
        print_status "Copying logs from ${instance_type} instance ${instance_id} (${public_ip})..."
        
        if [ -z "$public_ip" ] || [ "$public_ip" == "None" ]; then
            print_warning "Instance ${instance_id} has no public IP, skipping..."
            return
        fi
        
        # Set up local log directory
        LOCAL_LOG_DIR="${INSTANCE_LOG_DIR}/${instance_type}_${instance_id}"
        mkdir -p "$LOCAL_LOG_DIR"
        
        # Remote log directory (as defined in user-data-builder.ts)
        REMOTE_LOG_DIR="/data/distributed-colony/output/logs"
        
        # Expected log file names (based on user-data-builder.ts lines 62-65)
        if [ "$instance_type" == "coordinator" ]; then
            EXPECTED_LOG_FILE="coordinator_${COORDINATOR_PORT}.log"
        else
            EXPECTED_LOG_FILE="be_${BACKEND_PORT}.log"
        fi
        EXPECTED_LOG_PATH="${REMOTE_LOG_DIR}/${EXPECTED_LOG_FILE}"
        
        log_output "  Remote log directory: $REMOTE_LOG_DIR"
        log_output "  Expected log file: $EXPECTED_LOG_FILE"
        log_output "  Full remote path: $EXPECTED_LOG_PATH"
        
        # Use scp to copy all logs from the log directory
        if [ -f "$EXPANDED_KEY_PATH" ]; then
            # First, try to copy the entire logs directory (catches all log files)
            log_output "  Attempting to copy entire logs directory..."
            if [ -n "$LOG_FILE" ]; then
                SCP_OUTPUT=$(scp -o StrictHostKeyChecking=no \
                    -o ConnectTimeout=10 \
                    -r \
                    -i "$EXPANDED_KEY_PATH" \
                    "ec2-user@${public_ip}:${REMOTE_LOG_DIR}/" \
                    "${LOCAL_LOG_DIR}/" 2>&1)
                SCP_EXIT=$?
                echo "$SCP_OUTPUT" | tee -a "$LOG_FILE" > /dev/null
            else
                scp -o StrictHostKeyChecking=no \
                    -o ConnectTimeout=10 \
                    -r \
                    -i "$EXPANDED_KEY_PATH" \
                    "ec2-user@${public_ip}:${REMOTE_LOG_DIR}/" \
                    "${LOCAL_LOG_DIR}/" 2>&1
                SCP_EXIT=$?
            fi
            
            if [ $SCP_EXIT -eq 0 ]; then
                print_status "Successfully copied entire logs directory from ${instance_id}"
                log_output "  Logs saved to: ${LOCAL_LOG_DIR}/"
            else
                # Try copying the specific expected log file as fallback
                log_output "  Directory copy failed, trying specific log file..."
                if [ -n "$LOG_FILE" ]; then
                    SCP_OUTPUT=$(scp -o StrictHostKeyChecking=no \
                        -o ConnectTimeout=10 \
                        -i "$EXPANDED_KEY_PATH" \
                        "ec2-user@${public_ip}:${EXPECTED_LOG_PATH}" \
                        "${LOCAL_LOG_DIR}/" 2>&1)
                    SCP_EXIT=$?
                    echo "$SCP_OUTPUT" | tee -a "$LOG_FILE" > /dev/null
                else
                    scp -o StrictHostKeyChecking=no \
                        -o ConnectTimeout=10 \
                        -i "$EXPANDED_KEY_PATH" \
                        "ec2-user@${public_ip}:${EXPECTED_LOG_PATH}" \
                        "${LOCAL_LOG_DIR}/" 2>&1
                    SCP_EXIT=$?
                fi
                
                if [ $SCP_EXIT -eq 0 ]; then
                    print_status "Successfully copied log file from ${instance_id}"
                    log_output "  Log file saved to: ${LOCAL_LOG_DIR}/${EXPECTED_LOG_FILE}"
                else
                    print_warning "Failed to copy logs from ${instance_id} (file may not exist yet or instance not ready)"
                    log_output "  WARNING: Could not copy logs - instance may still be starting or logs not yet created"
                fi
            fi
        else
            print_warning "SSH key not found, skipping log copy from ${instance_id}"
            log_output "  WARNING: SSH key not found at $EXPANDED_KEY_PATH"
        fi
    }

    # Copy logs from coordinator instances
    print_status "Finding coordinator instances..."
    log_output "Querying AWS for all coordinator instances..."
    COORDINATOR_INSTANCES=$(get_instance_info "coordinator")
    if [ -n "$COORDINATOR_INSTANCES" ]; then
        COORDINATOR_COUNT=$(echo "$COORDINATOR_INSTANCES" | grep -v "^$" | wc -l | tr -d ' ')
        log_output "Found $COORDINATOR_COUNT coordinator instance(s)"
        while read -r instance_id public_ip; do
            if [ -n "$instance_id" ]; then
                log_output "  - Coordinator: $instance_id ($public_ip)"
                copy_logs_from_instance "$instance_id" "$public_ip" "coordinator"
            fi
        done <<< "$COORDINATOR_INSTANCES"
    else
        print_warning "No coordinator instances found"
        log_output "WARNING: No coordinator instances found"
    fi

    log_output ""

    # Copy logs from backend instances
    print_status "Finding backend instances..."
    log_output "Querying AWS for all backend instances..."
    BACKEND_INSTANCES=$(get_instance_info "backend")
    if [ -n "$BACKEND_INSTANCES" ]; then
        BACKEND_COUNT=$(echo "$BACKEND_INSTANCES" | grep -v "^$" | wc -l | tr -d ' ')
        log_output "Found $BACKEND_COUNT backend instance(s)"
        while read -r instance_id public_ip; do
            if [ -n "$instance_id" ]; then
                log_output "  - Backend: $instance_id ($public_ip)"
                copy_logs_from_instance "$instance_id" "$public_ip" "backend"
            fi
        done <<< "$BACKEND_INSTANCES"
    else
        print_warning "No backend instances found"
        log_output "WARNING: No backend instances found"
    fi

    log_output ""
    print_status "Log gathering completed!"
    log_output "Logs saved to: $INSTANCE_LOG_DIR"
    
    # Export the directory path for caller
    echo "$INSTANCE_LOG_DIR"
}

# Run main function
main "$@"

