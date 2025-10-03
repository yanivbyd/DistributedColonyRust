import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as iam from 'aws-cdk-lib/aws-iam';
import { Construct } from 'constructs';

export interface CoordinatorConfig {
  instanceType?: string;
  maxPrice?: string;
  keyPairName?: string;
  volumeSize?: number;
  volumeType?: string;
  coordinatorPort?: number;
  sshPort?: number;
  cpuType?: string;
  allocationStrategy?: string;
}

export class CoordinatorStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    // Configuration from context
    const config: CoordinatorConfig = {
      instanceType: this.node.tryGetContext('instanceType'),
      maxPrice: this.node.tryGetContext('maxPrice'),
      keyPairName: this.node.tryGetContext('keyPairName'),
      volumeSize: this.node.tryGetContext('volumeSize'),
      volumeType: this.node.tryGetContext('volumeType'),
      coordinatorPort: this.node.tryGetContext('coordinatorPort'),
      sshPort: this.node.tryGetContext('sshPort'),
      cpuType: this.node.tryGetContext('cpuType'),
      allocationStrategy: this.node.tryGetContext('allocationStrategy'),
    };

    // VPC - use default VPC or create a new one
    const vpc = ec2.Vpc.fromLookup(this, 'VPC', {
      isDefault: true,
    });

    // Security Group for Coordinator
    const securityGroup = new ec2.SecurityGroup(this, 'CoordinatorSecurityGroup', {
      vpc,
      description: 'Security group for DistributedColony coordinator',
      allowAllOutbound: true,
    });

    // Allow SSH access
    securityGroup.addIngressRule(
      ec2.Peer.anyIpv4(),
      ec2.Port.tcp(config.sshPort!),
      'Allow SSH access'
    );

    // Allow coordinator port
    securityGroup.addIngressRule(
      ec2.Peer.anyIpv4(),
      ec2.Port.tcp(config.coordinatorPort!),
      'Allow coordinator traffic'
    );

    // IAM Role for the coordinator instance
    const role = new iam.Role(this, 'CoordinatorInstanceRole', {
      assumedBy: new iam.ServicePrincipal('ec2.amazonaws.com'),
      description: 'IAM role for DistributedColony coordinator',
      managedPolicies: [
        iam.ManagedPolicy.fromAwsManagedPolicyName('AmazonSSMManagedInstanceCore'),
        iam.ManagedPolicy.fromAwsManagedPolicyName('CloudWatchAgentServerPolicy'),
      ],
    });

    // Add ECR permissions for pulling Docker images
    role.addToPolicy(new iam.PolicyStatement({
      effect: iam.Effect.ALLOW,
      actions: [
        'ecr:GetAuthorizationToken',
        'ecr:BatchCheckLayerAvailability',
        'ecr:GetDownloadUrlForLayer',
        'ecr:BatchGetImage',
      ],
      resources: ['*'],
    }));

    // Get AWS account ID for ECR URI
    const accountId = cdk.Stack.of(this).account;
    const region = cdk.Stack.of(this).region;
    const ecrUri = `${accountId}.dkr.ecr.${region}.amazonaws.com/distributed-colony:latest`;

    // User data script to set up Docker and run the coordinator container
    const userData = ec2.UserData.forLinux();
    userData.addCommands(
      '#!/bin/bash',
      'set -e',
      '',
      '# Update system',
      'yum update -y',
      '',
      '# Install Docker',
      'yum install -y docker',
      'systemctl start docker',
      'systemctl enable docker',
      'usermod -a -G docker ec2-user',
      '',
      '# Install AWS CLI v2',
      'curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip"',
      'unzip awscliv2.zip',
      'sudo ./aws/install',
      'rm -rf awscliv2.zip aws/',
      '',
      '# Configure AWS CLI to use instance role',
      'aws configure set region ' + region,
      '',
      '# Login to ECR',
      'aws ecr get-login-password --region ' + region + ' | docker login --username AWS --password-stdin ' + accountId + '.dkr.ecr.' + region + '.amazonaws.com',
      '',
      '# Pull the Docker image',
      'docker pull ' + ecrUri,
      '',
      '# Create systemd service for coordinator container',
      'cat > /etc/systemd/system/distributed-colony-coordinator.service << EOF',
      '[Unit]',
      'Description=DistributedColony Coordinator (Docker)',
      'After=docker.service',
      'Requires=docker.service',
      '',
      '[Service]',
      'Type=simple',
      'User=ec2-user',
      'ExecStartPre=-/usr/bin/docker stop distributed-colony-coordinator',
      'ExecStartPre=-/usr/bin/docker rm distributed-colony-coordinator',
      'ExecStart=/usr/bin/docker run --name distributed-colony-coordinator --rm -p ' + config.coordinatorPort + ':' + config.coordinatorPort + ' -e SERVICE_TYPE=coordinator -e COORDINATOR_PORT=' + config.coordinatorPort + ' ' + ecrUri,
      'ExecStop=/usr/bin/docker stop distributed-colony-coordinator',
      'Restart=always',
      'RestartSec=10',
      '',
      '[Install]',
      'WantedBy=multi-user.target',
      'EOF',
      '',
      '# Enable and start the service',
      'systemctl daemon-reload',
      'systemctl enable distributed-colony-coordinator.service',
      'systemctl start distributed-colony-coordinator.service',
      '',
      '# Signal completion',
      'echo "Coordinator Docker setup complete!"'
    );

    // Launch Template for Coordinator Spot Instance
    const launchTemplate = new ec2.LaunchTemplate(this, 'CoordinatorLaunchTemplate', {
      instanceType: new ec2.InstanceType(config.instanceType!),
      machineImage: ec2.MachineImage.latestAmazonLinux2023({
        cpuType: config.cpuType! as ec2.AmazonLinuxCpuType,
      }),
      securityGroup,
      role,
      userData,
      blockDevices: [
        {
          deviceName: '/dev/xvda',
          volume: ec2.BlockDeviceVolume.ebs(config.volumeSize!, {
            volumeType: config.volumeType! as ec2.EbsDeviceVolumeType,
            encrypted: true,
          }),
        },
      ],
      ...(config.keyPairName && { keyName: config.keyPairName }),
    });

    // Spot Instance Request for Coordinator
    const spotFleet = new ec2.CfnSpotFleet(this, 'CoordinatorSpotFleet', {
      spotFleetRequestConfigData: {
        iamFleetRole: role.roleArn,
        targetCapacity: 1,
        spotPrice: config.maxPrice,
        launchSpecifications: [
          {
            imageId: ec2.MachineImage.latestAmazonLinux2023({
              cpuType: config.cpuType! as ec2.AmazonLinuxCpuType,
            }).getImage(this).imageId,
            instanceType: config.instanceType!,
            keyName: config.keyPairName,
            securityGroups: [
              {
                groupId: securityGroup.securityGroupId,
              },
            ],
            iamInstanceProfile: {
              arn: role.roleArn,
            },
            userData: Buffer.from(userData.render()).toString('base64'),
            blockDeviceMappings: [
              {
                deviceName: '/dev/xvda',
                ebs: {
                  volumeSize: config.volumeSize!,
                  volumeType: config.volumeType!,
                  encrypted: true,
                },
              },
            ],
            subnetId: vpc.publicSubnets[0].subnetId,
          },
        ],
        allocationStrategy: config.allocationStrategy!,
        terminateInstancesWithExpiration: true,
        type: 'maintain',
      },
    });

    // Outputs
    new cdk.CfnOutput(this, 'CoordinatorSpotFleetId', {
      value: spotFleet.ref,
      description: 'The ID of the coordinator spot fleet',
    });

    new cdk.CfnOutput(this, 'CoordinatorSecurityGroupId', {
      value: securityGroup.securityGroupId,
      description: 'Coordinator Security Group ID',
    });

    new cdk.CfnOutput(this, 'CoordinatorPort', {
      value: config.coordinatorPort!.toString(),
      description: 'Coordinator port',
    });

    new cdk.CfnOutput(this, 'CoordinatorConnectionCommand', {
      value: `aws ec2 describe-spot-fleet-instances --spot-fleet-request-id ${spotFleet.ref} --query 'ActiveInstances[0].InstanceId' --output text | xargs -I {} aws ec2 describe-instances --instance-ids {} --query 'Reservations[0].Instances[0].PublicIpAddress' --output text`,
      description: 'Run this command to get the coordinator IP address',
    });
  }
}
