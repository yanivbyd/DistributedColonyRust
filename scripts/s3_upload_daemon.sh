#!/bin/bash
set -euo pipefail

# S3 Upload Daemon
# Periodically scans output/s3 directory and uploads files to S3 buckets

# Default configuration
S3_UPLOAD_INTERVAL=${S3_UPLOAD_INTERVAL:-60}
S3_ROOT_DIR=${S3_ROOT_DIR:-output/s3}
AWS_REGION=${AWS_REGION:-eu-west-1}
AWS_PROFILE=${AWS_PROFILE:-}
FILE_STABILITY_SECONDS=${FILE_STABILITY_SECONDS:-2}

# Parse command-line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -i|--interval)
            S3_UPLOAD_INTERVAL="$2"
            shift 2
            ;;
        -d|--directory)
            S3_ROOT_DIR="$2"
            shift 2
            ;;
        -r|--region)
            AWS_REGION="$2"
            shift 2
            ;;
        -p|--profile)
            AWS_PROFILE="$2"
            shift 2
            ;;
        -s|--stability)
            FILE_STABILITY_SECONDS="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  -i, --interval SECONDS    Scan interval (default: 60)"
            echo "  -d, --directory DIR       Root directory (default: output/s3)"
            echo "  -r, --region REGION       AWS region (default: eu-west-1)"
            echo "  -p, --profile PROFILE     AWS CLI profile (localhost only)"
            echo "  -s, --stability SECONDS   Stability check duration (default: 2)"
            echo "  -h, --help                Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use -h or --help for usage information"
            exit 1
            ;;
    esac
done

# Logging function
log() {
    local level=$1
    shift
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] [$level] $*"
}

# Check if AWS CLI is installed
check_aws_cli() {
    if ! command -v aws &> /dev/null; then
        log "ERROR" "AWS CLI is not installed. Please install it:"
        echo "  macOS: brew install awscli"
        echo "  Linux: See https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html"
        exit 1
    fi
}

# Detect if running on EC2
is_ec2_instance() {
    local instance_id
    instance_id=$(curl -s --max-time 2 --connect-timeout 2 http://169.254.169.254/latest/meta-data/instance-id 2>/dev/null || echo "")
    if [[ -n "$instance_id" ]]; then
        return 0  # true
    else
        return 1  # false
    fi
}

# Validate AWS access
validate_aws_access() {
    local aws_cmd="aws"
    if [[ -n "$AWS_PROFILE" ]]; then
        aws_cmd="aws --profile $AWS_PROFILE"
    fi
    
    if ! $aws_cmd sts get-caller-identity &>/dev/null; then
        if is_ec2_instance; then
            log "ERROR" "Failed to validate AWS access via IAM role. Check instance role permissions."
        else
            log "ERROR" "Failed to validate AWS access. Please configure credentials:"
            echo "  Run: aws configure"
            if [[ -n "$AWS_PROFILE" ]]; then
                echo "  Or: aws configure --profile $AWS_PROFILE"
            fi
        fi
        exit 1
    fi
    
    local identity
    identity=$($aws_cmd sts get-caller-identity --output json 2>/dev/null)
    log "INFO" "AWS access validated: $(echo "$identity" | grep -o '"Arn": "[^"]*' | cut -d'"' -f4 || echo "unknown")"
}

# Get file modification time (works on both macOS and Linux)
get_file_mtime() {
    local file_path=$1
    if [[ "$(uname)" == "Darwin" ]]; then
        stat -f %m "$file_path" 2>/dev/null || echo "0"
    else
        stat -c %Y "$file_path" 2>/dev/null || echo "0"
    fi
}

# Get file size (works on both macOS and Linux)
get_file_size() {
    local file_path=$1
    if [[ "$(uname)" == "Darwin" ]]; then
        stat -f %z "$file_path" 2>/dev/null || echo "0"
    else
        stat -c %s "$file_path" 2>/dev/null || echo "0"
    fi
}

# Check if file is stable (not modified recently)
is_file_stable() {
    local file_path=$1
    local current_time
    local file_mtime
    local age_seconds
    
    current_time=$(date +%s)
    file_mtime=$(get_file_mtime "$file_path")
    
    if [[ "$file_mtime" == "0" ]]; then
        return 1  # File doesn't exist or can't be stat'd
    fi
    
    age_seconds=$((current_time - file_mtime))
    
    # File is stable if it hasn't been modified for at least FILE_STABILITY_SECONDS
    if [[ $age_seconds -ge $FILE_STABILITY_SECONDS ]]; then
        return 0  # true - file is stable
    else
        return 1  # false - file is still being written
    fi
}

# Check if file exists in S3 and compare size
file_exists_in_s3() {
    local bucket=$1
    local key=$2
    local local_size=$3
    local aws_cmd="aws"
    
    if [[ -n "$AWS_PROFILE" ]]; then
        aws_cmd="aws --profile $AWS_PROFILE"
    fi
    
    # Use head-object to check existence and get size
    local head_output
    head_output=$($aws_cmd s3api head-object --bucket "$bucket" --key "$key" --region "$AWS_REGION" 2>/dev/null || echo "")
    
    if [[ -z "$head_output" ]]; then
        return 1  # File doesn't exist
    fi
    
    # Extract ContentLength from JSON output
    local s3_size
    s3_size=$(echo "$head_output" | grep -o '"ContentLength": [0-9]*' | grep -o '[0-9]*' || echo "0")
    
    # Compare sizes
    if [[ "$s3_size" == "$local_size" ]]; then
        return 0  # File exists and size matches
    else
        return 1  # File exists but size differs (re-upload needed)
    fi
}

# Upload file to S3
upload_file_to_s3() {
    local file_path=$1
    local bucket=$2
    local key=$3
    local aws_cmd="aws"
    
    if [[ -n "$AWS_PROFILE" ]]; then
        aws_cmd="aws --profile $AWS_PROFILE"
    fi
    
    # Upload with --only-show-errors to suppress normal output
    # Capture stderr to get error messages
    local error_output
    error_output=$($aws_cmd s3 cp "$file_path" "s3://$bucket/$key" --region "$AWS_REGION" --only-show-errors 2>&1)
    local exit_code=$?
    
    if [[ $exit_code -eq 0 ]]; then
        return 0
    else
        # Output error to stderr so caller can capture it
        echo "$error_output" >&2
        return 1
    fi
}

# Extract bucket name and S3 key from file path
extract_bucket_and_key() {
    local file_path=$1
    local root_dir=$2
    
    # Remove root directory prefix
    local relative_path="${file_path#$root_dir/}"
    
    # If file is directly in root (no subdirectory), return empty to indicate skip
    if [[ "$relative_path" != */* ]]; then
        echo "|"
        return
    fi
    
    # First directory component is the bucket name
    local bucket
    bucket=$(echo "$relative_path" | cut -d'/' -f1)

    # Map local directory name to actual S3 bucket name
    if [[ "$bucket" == "distributed_colony" ]]; then
        bucket="distributed-colony"
    fi
    
    # Rest of the path is the S3 key
    local key
    key=$(echo "$relative_path" | cut -d'/' -f2-)
    
    echo "$bucket|$key"
}

# Main scan and upload function
scan_and_upload() {
    local aws_cmd="aws"
    if [[ -n "$AWS_PROFILE" ]]; then
        aws_cmd="aws --profile $AWS_PROFILE"
    fi
    
    # Check if root directory exists
    if [[ ! -d "$S3_ROOT_DIR" ]]; then
        log "WARN" "Root directory does not exist: $S3_ROOT_DIR"
        return
    fi
    
    # Find all files recursively
    local files_processed=0
    local files_uploaded=0
    local files_skipped=0
    local files_errors=0
    
    while IFS= read -r -d '' file_path; do
        files_processed=$((files_processed + 1))
        
        # Check if file is stable
        if ! is_file_stable "$file_path"; then
            log "WARN" "File still being written, skipping: $file_path"
            files_skipped=$((files_skipped + 1))
            continue
        fi
        
        # Extract bucket and key
        local bucket_key
        bucket_key=$(extract_bucket_and_key "$file_path" "$S3_ROOT_DIR")
        local bucket
        bucket=$(echo "$bucket_key" | cut -d'|' -f1)
        local key
        key=$(echo "$bucket_key" | cut -d'|' -f2)
        
        if [[ -z "$bucket" || -z "$key" ]]; then
            # File is directly in root directory, skip it (e.g., .DS_Store)
            files_skipped=$((files_skipped + 1))
            continue
        fi
        
        # Skip files where bucket name starts with a dot (e.g., .DS_Store)
        if [[ "$bucket" == .* ]]; then
            files_skipped=$((files_skipped + 1))
            continue
        fi
        
        # Skip files where key (filename) starts with a dot (e.g., .DS_Store in subdirectories)
        # Extract just the filename (last component of the key)
        local filename
        filename=$(basename "$key")
        if [[ "$filename" == .* ]]; then
            files_skipped=$((files_skipped + 1))
            continue
        fi
        
        # Get local file size
        local local_size
        local_size=$(get_file_size "$file_path")
        
        # Check if file already exists in S3
        if file_exists_in_s3 "$bucket" "$key" "$local_size"; then
            log "INFO" "File already exists in S3 (bucket=$bucket, key=$key, size matches), deleting local: $file_path"
            if rm -f "$file_path"; then
                files_skipped=$((files_skipped + 1))
            else
                log "WARN" "Failed to delete local file: $file_path"
                files_errors=$((files_errors + 1))
            fi
            continue
        fi
        
        # Upload file to S3
        log "INFO" "Uploading: $file_path -> s3://$bucket/$key"
        local upload_error
        upload_error=$(upload_file_to_s3 "$file_path" "$bucket" "$key" 2>&1)
        local upload_exit_code=$?
        
        if [[ $upload_exit_code -eq 0 ]]; then
            # Delete local file after successful upload
            if rm -f "$file_path"; then
                log "INFO" "Successfully uploaded to bucket=$bucket, key=$key and deleted: $file_path"
                files_uploaded=$((files_uploaded + 1))
            else
                log "WARN" "Upload succeeded (bucket=$bucket, key=$key) but failed to delete local file: $file_path"
                files_errors=$((files_errors + 1))
            fi
        else
            log "ERROR" "Failed to upload: $file_path to bucket=$bucket, key=$key: $upload_error"
            files_errors=$((files_errors + 1))
        fi
    done < <(find "$S3_ROOT_DIR" -type f -print0 2>/dev/null || true)
    
    if [[ $files_processed -gt 0 ]]; then
        log "INFO" "Scan complete: processed=$files_processed uploaded=$files_uploaded skipped=$files_skipped errors=$files_errors"
    fi
}

# Main loop
main() {
    log "INFO" "Starting S3 upload daemon"
    log "INFO" "Configuration: interval=${S3_UPLOAD_INTERVAL}s, root_dir=$S3_ROOT_DIR, region=$AWS_REGION, stability=${FILE_STABILITY_SECONDS}s"
    
    # Check AWS CLI
    check_aws_cli
    
    # Detect environment
    if is_ec2_instance; then
        log "INFO" "Detected EC2 environment, using IAM role"
        export AWS_PROFILE=""  # Clear profile on EC2
    else
        log "INFO" "Detected localhost environment"
        if [[ -n "$AWS_PROFILE" ]]; then
            log "INFO" "Using AWS profile: $AWS_PROFILE"
        fi
    fi
    
    # Validate AWS access
    validate_aws_access
    
    # Main loop
    while true; do
        scan_and_upload || log "WARN" "Scan encountered errors, continuing..."
        sleep "$S3_UPLOAD_INTERVAL"
    done
}

# Run main function
main
