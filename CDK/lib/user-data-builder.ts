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
      ...this.buildSystemSetup(),
      ...this.buildDirectorySetup(),
      ...this.buildUsefulScripts(),
      ...this.buildSSMScripts(instanceType, port),
      ...this.buildReloadScript(instanceType, port, reloadScriptPath),
      ...this.buildContainerStartup(reloadScriptPath),
      ...this.buildRuntimeRegistration(instanceType, port),
      'echo "Startup completed successfully"',
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

  private buildUsefulScripts(): string[] {
    return [
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
      '# View application logs:',
      'cat /data/distributed-colony/output/logs/be_8082.log',
      'EOF',
      'chown ec2-user:ec2-user /home/ec2-user/scripts.txt',
    ];
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
      'docker stop distributed-colony 2>/dev/null || true',
      'docker rm distributed-colony 2>/dev/null || true',
      '',
      'echo "[INFO] Starting new container..."',
      ...this.buildDockerRunCommand(instanceType, port).map(line => {
        // For the last line (the image), add failure handling
        if (line.trim() === '"$ECR_URI"') {
          return '  "$ECR_URI" || fail "Docker run failed"';
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
      instanceType === ColonyInstanceType.COORDINATOR ?
        `  aws ssm put-parameter --name "/colony/coordinator" --value "$LOCAL_IP:${port}" --type String --overwrite --region ${this.region} | tee -a /var/log/reload.log || { echo "[ERROR] SSM registration failed (coordinator)" >> /var/log/reload.log; aws ec2 create-tags --resources "$INSTANCE_ID" --tags Key=Status,Value=Faulty Key=FailureReason,Value="SSM registration failed (coordinator)" --region "$REGION" 1>/dev/null 2>&1 || true; exit 1; } && echo "[INFO] SSM registration succeeded: /colony/coordinator=$LOCAL_IP:${port}" >> /var/log/reload.log` :
        `  aws ssm put-parameter --name "/colony/backends/$INSTANCE_ID" --value "$LOCAL_IP:${port}" --type String --overwrite --region ${this.region} | tee -a /var/log/reload.log || { echo "[ERROR] SSM registration failed (backend)" >> /var/log/reload.log; aws ec2 create-tags --resources "$INSTANCE_ID" --tags Key=Status,Value=Faulty Key=FailureReason,Value="SSM registration failed (backend)" --region "$REGION" 1>/dev/null 2>&1 || true; exit 1; } && echo "[INFO] SSM registration succeeded: /colony/backends/$INSTANCE_ID=$LOCAL_IP:${port}" >> /var/log/reload.log`,
      'else',
      instanceType === ColonyInstanceType.COORDINATOR ?
        `  aws ssm put-parameter --name "/colony/backends/$INSTANCE_ID" --value "$LOCAL_IP:${port}" --type String --overwrite --region ${this.region} | tee -a /var/log/reload.log || { echo "[ERROR] SSM registration failed (backend)" >> /var/log/reload.log; aws ec2 create-tags --resources "$INSTANCE_ID" --tags Key=Status,Value=Faulty Key=FailureReason,Value="SSM registration failed (backend)" --region "$REGION" 1>/dev/null 2>&1 || true; exit 1; } && echo "[INFO] SSM registration succeeded: /colony/backends/$INSTANCE_ID=$LOCAL_IP:${port}" >> /var/log/reload.log` :
        `  aws ssm put-parameter --name "/colony/coordinator" --value "$LOCAL_IP:${port}" --type String --overwrite --region ${this.region} | tee -a /var/log/reload.log || { echo "[ERROR] SSM registration failed (coordinator)" >> /var/log/reload.log; aws ec2 create-tags --resources "$INSTANCE_ID" --tags Key=Status,Value=Faulty Key=FailureReason,Value="SSM registration failed (coordinator)" --region "$REGION" 1>/dev/null 2>&1 || true; exit 1; } && echo "[INFO] SSM registration succeeded: /colony/coordinator=$LOCAL_IP:${port}" >> /var/log/reload.log`,
      'fi',
    ];
  }

  private buildContainerStartup(scriptPath: string): string[] {
    return [
      `su - ec2-user -c '/home/ec2-user/reload.sh >> /var/log/reload.log 2>&1'`,
      'cat /var/log/reload.log',
      'echo "Container started"',
    ];
  }

  private buildDockerRunCommand(instanceType: ColonyInstanceType, port: number): string[] {
    const containerName = instanceType === ColonyInstanceType.COORDINATOR ? 'distributed-coordinator' : 'distributed-colony';
    const serviceType = instanceType === ColonyInstanceType.COORDINATOR ? 'coordinator' : 'backend';
    const portEnvVar = instanceType === ColonyInstanceType.COORDINATOR ? 'COORDINATOR_PORT' : 'BACKEND_PORT';
    
    // Get the local IP for backend instances (coordinator uses 127.0.0.1)
    const hostname = instanceType === ColonyInstanceType.COORDINATOR ? '127.0.0.1' : '$(curl -s http://169.254.169.254/latest/meta-data/local-ipv4)';
    
    const baseCommand = [
      'docker run -d \\',
      `  --name ${containerName} \\`,
      `  -p ${port}:${port} \\`,
      '  -v /data/distributed-colony/output:/app/output \\',
      `  -e SERVICE_TYPE=${serviceType} \\`,
      `  -e ${portEnvVar}=${port} \\`,
      '  -e DEPLOYMENT_MODE=aws \\',
    ];
    
    if (instanceType === ColonyInstanceType.BACKEND) {
      baseCommand.push(`  -e BACKEND_HOST=${hostname} \\`);
    }
    
    baseCommand.push('  "$ECR_URI"');
    
    return baseCommand;
  }
}
