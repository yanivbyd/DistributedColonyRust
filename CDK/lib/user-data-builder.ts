import * as cdk from 'aws-cdk-lib';

export enum ColonyInstanceType {
  BACKEND = 'backend',
  COORDINATOR = 'coordinator'
}

export class UserDataBuilder {
  constructor(
    private accountId: string,
    private region: string
  ) {}

  buildUserData(instanceType: ColonyInstanceType, port: number): string {
    const reloadScriptPath = '/home/ec2-user/reload.sh';
    const commands = [
      '#!/bin/bash',
      'set -e',
      'set -o pipefail',
      'exec > >(tee -a /var/log/user-data.log) 2>&1',
      'echo "[INFO] Starting user-data script execution..."',
      ...this.buildSystemSetup(),
      ...this.buildDirectorySetup(),
      ...this.buildUsefulScripts(instanceType, port),
      // SSM registration is now handled by the Rust coordinator/backend code
      ...this.buildReloadScript(instanceType, port, reloadScriptPath),
      ...this.buildContainerStartup(reloadScriptPath),
      ...this.buildS3UploadDaemon(instanceType),
      'echo "[INFO] Startup completed successfully"',
    ];
    return commands.join('\n');
  }

  private buildSystemSetup(): string[] {
    return [
      'yum update -y',
      'yum install -y docker',
      'systemctl start docker',
      'systemctl enable docker',
      'usermod -a -G docker ec2-user',
      'curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip"',
      'unzip -q awscliv2.zip',
      'sudo ./aws/install',
      'rm -rf awscliv2.zip aws/',
      `aws configure set region ${this.region}`,
      `aws ecr get-login-password --region ${this.region} | docker login --username AWS --password-stdin ${this.accountId}.dkr.ecr.${this.region}.amazonaws.com`,
    ];
  }

  private buildDirectorySetup(): string[] {
    return [
      // Prepare host directories with root privileges so ec2-user can write
      'mkdir -p /data',
      'mkdir -p /data/distributed-colony',
      'mkdir -p /data/distributed-colony/output',
      'chmod 777 /data /data/distributed-colony /data/distributed-colony/output',
      // Ensure reload log exists and is writable by ec2-user
      `touch /var/log/reload.log`,
      `chown ec2-user:ec2-user /var/log/reload.log`,
      `chmod 664 /var/log/reload.log`,
    ];
  }

  private buildUsefulScripts(instanceType: ColonyInstanceType, port: number): string[] {
    const logFileName = instanceType === ColonyInstanceType.COORDINATOR
      ? `coordinator_${port}.log`
      : `be_${port}.log`;
    const logPath = `/data/distributed-colony/output/logs/${logFileName}`;
    const scripts = [
      `cat <<'EOF' > /home/ec2-user/scripts.txt`,
      '# Useful debugging scripts',
      '',
      '# To verify startup completion in cloud-init logs:',
      'sudo grep -F "Startup completed successfully" /var/log/cloud-init-output.log',
      '',
      '# Show the output of the startup script:',
      'sudo cat /var/log/cloud-init-output.log',
      '',
      '# Show the output of the reload script:',
      'sudo cat /var/log/reload.log',
      '',
      '# Show running containers:',
      'sudo docker ps -a',
      '',
      '# Show container logs (replace <container_id> with actual ID):',
      'sudo docker logs <container_id>',
      '',
    ];

    scripts.push('# View application logs:', `cat ${logPath}`);

    if (instanceType === ColonyInstanceType.COORDINATOR) {
      scripts.push(
        '',
        '# Trigger colony-start workflow:',
        'curl -X POST -i http://127.0.0.1:8083/colony-start',
      );
    } 
      
    scripts.push(
        '',
        '# Inspect current SSM registrations:',
        `curl -X GET -i http://127.0.0.1:${instanceType === ColonyInstanceType.COORDINATOR ? '8083' : '8085'}/debug-ssm`
    );
    

    scripts.push('EOF', 'chown ec2-user:ec2-user /home/ec2-user/scripts.txt');

    return scripts;
  }

  private buildSSMScripts(instanceType: ColonyInstanceType, port: number): string[] {
    return [
      // Create SSM registration script
      `cat <<'EOF' > /usr/local/bin/register-ssm.sh`,
      '#!/bin/bash',
      'set -e',
      'INSTANCE_ID=$(curl -s http://169.254.169.254/latest/meta-data/instance-id)',
      'LOCAL_IP=$(curl -s http://169.254.169.254/latest/meta-data/local-ipv4)',
      ...(instanceType === ColonyInstanceType.COORDINATOR
        ? [`aws ssm put-parameter --name "/colony/coordinator" --value "$LOCAL_IP:${port}" --type String --overwrite --region ${this.region}`]
        : [`aws ssm put-parameter --name "/colony/backends/$INSTANCE_ID" --value "$LOCAL_IP:${port}" --type String --overwrite --region ${this.region}`]
      ),
      'EOF',
      'chmod +x /usr/local/bin/register-ssm.sh',
      // Create SSM removal script
      `cat <<'EOF' > /usr/local/bin/remove-ssm.sh`,
      '#!/bin/bash',
      'set -e',
      'INSTANCE_ID=$(curl -s http://169.254.169.254/latest/meta-data/instance-id)',
      instanceType === ColonyInstanceType.COORDINATOR ?
        `aws ssm delete-parameter --name "/colony/coordinator" --region ${this.region} || true` :
        `aws ssm delete-parameter --name "/colony/backends/$INSTANCE_ID" --region ${this.region} || true`,
      'EOF',
      'chmod +x /usr/local/bin/remove-ssm.sh',
      // Create systemd service for shutdown cleanup
      `cat <<'EOF' > /etc/systemd/system/remove-ssm.service`,
      '[Unit]',
      'Description=Remove SSM entry on shutdown',
      'DefaultDependencies=no',
      'Before=shutdown.target',
      '',
      '[Service]',
      'Type=oneshot',
      'ExecStart=/usr/local/bin/remove-ssm.sh',
      '',
      '[Install]',
      'WantedBy=shutdown.target',
      'EOF',
      'systemctl enable remove-ssm.service',
      // Run Register SSM script
      'bash /usr/local/bin/register-ssm.sh',
    ];
  }

  private buildReloadScript(instanceType: ColonyInstanceType, port: number, scriptPath: string): string[] {
    const containerName = instanceType === ColonyInstanceType.COORDINATOR ? 'distributed-coordinator' : 'distributed-colony';
    return [
      `cat <<'EOF' > ${scriptPath}`,
      '#!/bin/bash',
      'set -euo pipefail',
      `ECR_URI=${this.accountId}.dkr.ecr.${this.region}.amazonaws.com/distributed-colony:latest`,
      `REGION=${this.region}`,
      'INSTANCE_ID=$(curl -s http://169.254.169.254/latest/meta-data/instance-id)',
      '',
      'tag_faulty() {',
      '  local REASON="$1"',
      '  echo "[ERROR] ${REASON}"',
      '  # Attempt to tag the instance as Faulty with a failure reason',
      '  aws ec2 create-tags \
        --resources "$INSTANCE_ID" \
        --tags Key=Status,Value=Faulty Key=FailureReason,Value="$REASON" \
        --region "$REGION" 1>/dev/null 2>&1 || true',
      '}',
      '',
      'fail() {',
      '  local REASON="$1"',
      '  tag_faulty "$REASON"',
      '  exit 1',
      '}',
      '',
      'echo "[INFO] Pulling latest Docker image..."',
      'aws ecr get-login-password --region "$REGION" | docker login --username AWS --password-stdin "${ECR_URI%/*}" || fail "ECR login failed"',
      'docker pull "$ECR_URI" || fail "Docker pull failed"',
      '',
      'echo "[INFO] Stopping and removing existing container if any..."',
      `docker stop ${containerName} 2>/dev/null || true`,
      `docker rm ${containerName} 2>/dev/null || true`,
      '',
      'echo "[INFO] Starting new container..."',
      ...this.buildDockerRunCommand(instanceType, port).map((line, index, array) => {
        // For the last line (the command), add failure handling
        if (index === array.length - 1) {
          return `${line} || fail "Docker run failed"`;
        }
        return line;
      }),
      'EOF',
      `chmod +x ${scriptPath}`,
    ];
  }

  private buildRuntimeRegistration(instanceType: ColonyInstanceType, port: number): string[] {
    return [
      // Set COORDINATOR environment variable for SSM registration
      instanceType === ColonyInstanceType.COORDINATOR ? 'export COORDINATOR=1' : 'export COORDINATOR=0',
      // Register instance in SSM before starting container
      'INSTANCE_ID=$(curl -s http://169.254.169.254/latest/meta-data/instance-id)',
      `REGION=${this.region}`,
      'LOCAL_IP=$(curl -s http://169.254.169.254/latest/meta-data/local-ipv4)',
      'echo "[INFO] Beginning SSM registration..." | tee -a /var/log/reload.log',
      'if [ "$COORDINATOR" = "1" ]; then',
      `  aws ssm put-parameter --name "/colony/coordinator" --value "$LOCAL_IP:${port}" --type String --overwrite --region ${this.region} | tee -a /var/log/reload.log || { echo "[ERROR] SSM registration failed (coordinator)" >> /var/log/reload.log; aws ec2 create-tags --resources "$INSTANCE_ID" --tags Key=Status,Value=Faulty Key=FailureReason,Value="SSM registration failed (coordinator)" --region "$REGION" 1>/dev/null 2>&1 || true; exit 1; } && echo "[INFO] SSM registration succeeded: /colony/coordinator=$LOCAL_IP:${port}" >> /var/log/reload.log`,
      'else',
      `  aws ssm put-parameter --name "/colony/backends/$INSTANCE_ID" --value "$LOCAL_IP:${port}" --type String --overwrite --region ${this.region} | tee -a /var/log/reload.log || { echo "[ERROR] SSM registration failed (backend)" >> /var/log/reload.log; aws ec2 create-tags --resources "$INSTANCE_ID" --tags Key=Status,Value=Faulty Key=FailureReason,Value="SSM registration failed (backend)" --region "$REGION" 1>/dev/null 2>&1 || true; exit 1; } && echo "[INFO] SSM registration succeeded: /colony/backends/$INSTANCE_ID=$LOCAL_IP:${port}" >> /var/log/reload.log`,
      'fi',
    ];
  }

  private buildContainerStartup(scriptPath: string): string[] {
    return [
      'echo "[INFO] Starting container via reload script..."',
      `if ! su - ec2-user -c '/home/ec2-user/reload.sh >> /var/log/reload.log 2>&1'; then`,
      '  echo "[ERROR] Failed to start container. Checking reload.log..."',
      '  cat /var/log/reload.log || true',
      '  echo "[ERROR] Container startup failed - see /var/log/reload.log for details"',
      '  exit 1',
      'fi',
      'cat /var/log/reload.log',
      'echo "[INFO] Container startup completed"',
    ];
  }

  private buildDockerRunCommand(instanceType: ColonyInstanceType, port: number): string[] {
    // Port assignments per instance type:
    // Coordinator: RPC_PORT=8082, HTTP_PORT=8083
    // Backend: RPC_PORT=8084, HTTP_PORT=8085
    const rpcPort = port; // port parameter is the RPC port
    const httpPort = instanceType === ColonyInstanceType.COORDINATOR ? 8083 : 8085;
    const containerName = instanceType === ColonyInstanceType.COORDINATOR ? 'distributed-coordinator' : 'distributed-colony';
    const serviceType = instanceType === ColonyInstanceType.COORDINATOR ? 'coordinator' : 'backend';
    
    // Get the local IP for backend instances (coordinator uses 127.0.0.1)
    const hostname = instanceType === ColonyInstanceType.COORDINATOR ? '127.0.0.1' : '$(curl -s http://169.254.169.254/latest/meta-data/local-ipv4)';
    
    const baseCommand: string[] = [];

    // Set environment variables for both coordinator and backend
    baseCommand.push(`RPC_PORT=${rpcPort}`);
    baseCommand.push(`HTTP_PORT=${httpPort}`);

    baseCommand.push(
      'docker run -d \\',
      `  --name ${containerName} \\`,
      '  --network host \\',
      '  -v /data/distributed-colony/output:/app/output \\',
      `  -e SERVICE_TYPE=${serviceType} \\`,
      `  -e RPC_PORT=${rpcPort} \\`,
      `  -e HTTP_PORT=${httpPort} \\`,
      `  -e AWS_DEFAULT_REGION=${this.region} \\`,
      `  -e AWS_REGION=${this.region} \\`,
      '  -e DEPLOYMENT_MODE=aws \\',
    );
    
    if (instanceType === ColonyInstanceType.BACKEND) {
      baseCommand.push(`  -e BACKEND_HOST=${hostname} \\`);
    }
    
    // Build command arguments based on instance type
    // In AWS mode, both coordinator and backend read ports from environment variables
    // Coordinator: reads RPC_PORT and HTTP_PORT from env vars
    // Backend: reads BACKEND_HOST, RPC_PORT, and HTTP_PORT from env vars
    if (instanceType === ColonyInstanceType.COORDINATOR) {
      // AWS mode: pass only "aws" as argument, ports come from env vars
      baseCommand.push(`  "$ECR_URI" /usr/local/bin/coordinator aws`);
    } else {
      // AWS mode: pass only "aws" as argument, hostname and ports come from env vars
      baseCommand.push(`  "$ECR_URI" /usr/local/bin/backend aws`);
    }
    
    return baseCommand;
  }

  private buildS3UploadDaemon(instanceType: ColonyInstanceType): string[] {
    // Only install on coordinator since that's where files are written
    if (instanceType !== ColonyInstanceType.COORDINATOR) {
      return [];
    }

    const daemonScriptPath = '/usr/local/bin/s3_upload_daemon.sh';
    const daemonLogPath = '/var/log/s3_upload_daemon.log';
    const s3RootDir = '/data/distributed-colony/output/s3';

    return [
      // Install S3 upload daemon script
      `cat <<'S3DAEMON_EOF' > ${daemonScriptPath}`,
      '#!/bin/bash',
      'set -euo pipefail',
      '',
      '# S3 Upload Daemon',
      '# Periodically scans output/s3 directory and uploads files to S3 buckets',
      '',
      '# Default configuration',
      'S3_UPLOAD_INTERVAL=${S3_UPLOAD_INTERVAL:-60}',
      `S3_ROOT_DIR=${s3RootDir}`,
      `AWS_REGION=${this.region}`,
      'AWS_PROFILE=${AWS_PROFILE:-}',
      'FILE_STABILITY_SECONDS=${FILE_STABILITY_SECONDS:-2}',
      '',
      '# Logging function',
      'log() {',
      '    local level=$1',
      '    shift',
      '    echo "[$(date +\'%Y-%m-%d %H:%M:%S\')] [$level] $*"',
      '}',
      '',
      '# Check if AWS CLI is installed',
      'check_aws_cli() {',
      '    if ! command -v aws &> /dev/null; then',
      '        log "ERROR" "AWS CLI is not installed"',
      '        exit 1',
      '    fi',
      '}',
      '',
      '# Detect if running on EC2',
      'is_ec2_instance() {',
      '    local instance_id',
      '    instance_id=$(curl -s --max-time 2 --connect-timeout 2 http://169.254.169.254/latest/meta-data/instance-id 2>/dev/null || echo "")',
      '    if [[ -n "$instance_id" ]]; then',
      '        return 0',
      '    else',
      '        return 1',
      '    fi',
      '}',
      '',
      '# Validate AWS access',
      'validate_aws_access() {',
      '    if ! aws sts get-caller-identity &>/dev/null; then',
      '        log "ERROR" "Failed to validate AWS access via IAM role. Check instance role permissions."',
      '        exit 1',
      '    fi',
      '    local identity',
      '    identity=$(aws sts get-caller-identity --output json 2>/dev/null)',
      '    log "INFO" "AWS access validated: $(echo "$identity" | grep -o \'"Arn": "[^"]*\' | cut -d\'"\' -f4 || echo "unknown")"',
      '}',
      '',
      '# Get file modification time (Linux)',
      'get_file_mtime() {',
      '    local file_path=$1',
      '    stat -c %Y "$file_path" 2>/dev/null || echo "0"',
      '}',
      '',
      '# Get file size (Linux)',
      'get_file_size() {',
      '    local file_path=$1',
      '    stat -c %s "$file_path" 2>/dev/null || echo "0"',
      '}',
      '',
      '# Check if file is stable (not modified recently)',
      'is_file_stable() {',
      '    local file_path=$1',
      '    local current_time',
      '    local file_mtime',
      '    local age_seconds',
      '    ',
      '    current_time=$(date +%s)',
      '    file_mtime=$(get_file_mtime "$file_path")',
      '    ',
      '    if [[ "$file_mtime" == "0" ]]; then',
      '        return 1',
      '    fi',
      '    ',
      '    age_seconds=$((current_time - file_mtime))',
      '    ',
      '    if [[ $age_seconds -ge $FILE_STABILITY_SECONDS ]]; then',
      '        return 0',
      '    else',
      '        return 1',
      '    fi',
      '}',
      '',
      '# Check if file exists in S3 and compare size',
      'file_exists_in_s3() {',
      '    local bucket=$1',
      '    local key=$2',
      '    local local_size=$3',
      '    ',
      '    local head_output',
      '    head_output=$(aws s3api head-object --bucket "$bucket" --key "$key" --region "$AWS_REGION" 2>/dev/null || echo "")',
      '    ',
      '    if [[ -z "$head_output" ]]; then',
      '        return 1',
      '    fi',
      '    ',
      '    local s3_size',
      '    s3_size=$(echo "$head_output" | grep -o \'"ContentLength": [0-9]*\' | grep -o \'[0-9]*\' || echo "0")',
      '    ',
      '    if [[ "$s3_size" == "$local_size" ]]; then',
      '        return 0',
      '    else',
      '        return 1',
      '    fi',
      '}',
      '',
      '# Upload file to S3',
      'upload_file_to_s3() {',
      '    local file_path=$1',
      '    local bucket=$2',
      '    local key=$3',
      '    ',
      '    # Upload with --only-show-errors to suppress normal output',
      '    # Capture stderr to get error messages',
      '    local error_output',
      '    error_output=$(aws s3 cp "$file_path" "s3://$bucket/$key" --region "$AWS_REGION" --only-show-errors 2>&1)',
      '    local exit_code=$?',
      '    ',
      '    if [[ $exit_code -eq 0 ]]; then',
      '        return 0',
      '    else',
      '        # Output error to stderr so caller can capture it',
      '        echo "$error_output" >&2',
      '        return 1',
      '    fi',
      '}',
      '',
      '# Extract bucket name and S3 key from file path',
      'extract_bucket_and_key() {',
      '    local file_path=$1',
      '    local root_dir=$2',
      '    ',
      '    local relative_path="${file_path#$root_dir/}"',
      '    ',
      '    # If file is directly in root (no subdirectory), return empty to indicate skip',
      '    if [[ "$relative_path" != */* ]]; then',
      '        echo "|"',
      '        return',
      '    fi',
      '    ',
      '    # First directory component is the bucket name',
      '    local bucket',
      '    bucket=$(echo "$relative_path" | cut -d\'/\' -f1)',
      '    ',
      '    # Map local directory name to actual S3 bucket name',
      '    if [[ "$bucket" == "distributed_colony" ]]; then',
      '        bucket="distributed-colony"',
      '    fi',
      '    ',
      '    # Rest of the path is the S3 key',
      '    local key',
      '    key=$(echo "$relative_path" | cut -d\'/\' -f2-)',
      '    ',
      '    echo "$bucket|$key"',
      '}',
      '',
      '# Main scan and upload function',
      'scan_and_upload() {',
      '    if [[ ! -d "$S3_ROOT_DIR" ]]; then',
      '        log "WARN" "Root directory does not exist: $S3_ROOT_DIR"',
      '        return',
      '    fi',
      '    ',
      '    local files_processed=0',
      '    local files_uploaded=0',
      '    local files_skipped=0',
      '    local files_errors=0',
      '    ',
      '    while IFS= read -r -d \'\' file_path; do',
      '        files_processed=$((files_processed + 1))',
      '        ',
      '        if ! is_file_stable "$file_path"; then',
      '            log "WARN" "File still being written, skipping: $file_path"',
      '            files_skipped=$((files_skipped + 1))',
      '            continue',
      '        fi',
      '        ',
      '        local bucket_key',
      '        bucket_key=$(extract_bucket_and_key "$file_path" "$S3_ROOT_DIR")',
      '        local bucket',
      '        bucket=$(echo "$bucket_key" | cut -d\'|\' -f1)',
      '        local key',
      '        key=$(echo "$bucket_key" | cut -d\'|\' -f2)',
      '        ',
      '        if [[ -z "$bucket" || -z "$key" ]]; then',
      '            log "ERROR" "Failed to extract bucket/key from: $file_path"',
      '            files_errors=$((files_errors + 1))',
      '            continue',
      '        fi',
      '        ',
      '        # Skip files where bucket name starts with a dot (e.g., .DS_Store)',
      '        if [[ "$bucket" == .* ]]; then',
      '            files_skipped=$((files_skipped + 1))',
      '            continue',
      '        fi',
      '        ',
      '        # Skip files where key (filename) starts with a dot (e.g., .DS_Store in subdirectories)',
      '        # Extract just the filename (last component of the key)',
      '        local filename',
      '        filename=$(basename "$key")',
      '        if [[ "$filename" == .* ]]; then',
      '            files_skipped=$((files_skipped + 1))',
      '            continue',
      '        fi',
      '        ',
      '        local local_size',
      '        local_size=$(get_file_size "$file_path")',
      '        ',
      '        if file_exists_in_s3 "$bucket" "$key" "$local_size"; then',
      '            log "INFO" "File already exists in S3 (bucket=$bucket, key=$key, size matches), deleting local: $file_path"',
      '            if rm -f "$file_path"; then',
      '                files_skipped=$((files_skipped + 1))',
      '            else',
      '                log "WARN" "Failed to delete local file: $file_path"',
      '                files_errors=$((files_errors + 1))',
      '            fi',
      '            continue',
      '        fi',
      '        ',
      '        log "INFO" "Uploading: $file_path -> s3://$bucket/$key"',
      '        local upload_error',
      '        upload_error=$(upload_file_to_s3 "$file_path" "$bucket" "$key" 2>&1)',
      '        local upload_exit_code=$?',
      '        ',
      '        if [[ $upload_exit_code -eq 0 ]]; then',
      '            if rm -f "$file_path"; then',
      '                log "INFO" "Successfully uploaded to bucket=$bucket, key=$key and deleted: $file_path"',
      '                files_uploaded=$((files_uploaded + 1))',
      '            else',
      '                log "WARN" "Upload succeeded (bucket=$bucket, key=$key) but failed to delete local file: $file_path"',
      '                files_errors=$((files_errors + 1))',
      '            fi',
      '        else',
      '            log "ERROR" "Failed to upload: $file_path to bucket=$bucket, key=$key: $upload_error"',
      '            files_errors=$((files_errors + 1))',
      '        fi',
      '    done < <(find "$S3_ROOT_DIR" -type f -print0 2>/dev/null || true)',
      '    ',
      '    if [[ $files_processed -gt 0 ]]; then',
      '        log "INFO" "Scan complete: processed=$files_processed uploaded=$files_uploaded skipped=$files_skipped errors=$files_errors"',
      '    fi',
      '}',
      '',
      '# Main loop',
      'main() {',
      '    log "INFO" "Starting S3 upload daemon"',
      '    log "INFO" "Configuration: interval=${S3_UPLOAD_INTERVAL}s, root_dir=$S3_ROOT_DIR, region=$AWS_REGION, stability=${FILE_STABILITY_SECONDS}s"',
      '    ',
      '    check_aws_cli',
      '    ',
      '    if is_ec2_instance; then',
      '        log "INFO" "Detected EC2 environment, using IAM role"',
      '        export AWS_PROFILE=""',
      '    fi',
      '    ',
      '    validate_aws_access',
      '    ',
      '    while true; do',
      '        scan_and_upload',
      '        sleep "$S3_UPLOAD_INTERVAL"',
      '    done',
      '}',
      '',
      'main',
      'S3DAEMON_EOF',
      `chmod +x ${daemonScriptPath}`,
      // Create systemd service for S3 upload daemon
      `cat <<'EOF' > /etc/systemd/system/s3-upload-daemon.service`,
      '[Unit]',
      'Description=S3 Upload Daemon',
      'After=network.target',
      '',
      '[Service]',
      'Type=simple',
      // Use /bin/bash -c so we can safely redirect stdout/stderr into the log file
      // This avoids relying on newer systemd StandardOutput/StandardError=append: semantics
      `ExecStart=/bin/bash -c '${daemonScriptPath} >> ${daemonLogPath} 2>&1'`,
      'Restart=always',
      'RestartSec=10',
      'User=root',
      '',
      '[Install]',
      'WantedBy=multi-user.target',
      'EOF',
      'systemctl daemon-reload',
      'systemctl enable s3-upload-daemon.service',
      'systemctl start s3-upload-daemon.service',
      `echo "[INFO] S3 upload daemon installed and started (logs: ${daemonLogPath})"`,
    ];
  }
}
