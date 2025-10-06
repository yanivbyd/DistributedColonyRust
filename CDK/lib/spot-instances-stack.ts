import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as iam from 'aws-cdk-lib/aws-iam';
import { Construct } from 'constructs';

export interface SpotInstancesConfig {
  instanceType?: string;
  maxPrice?: string;
  keyPairName?: string;
  volumeSize?: number;
  volumeType?: string;
  sshPort?: number;
  backendPort?: number;
  targetCapacity?: number;
}

export class SpotInstancesStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    const config: SpotInstancesConfig = {
      instanceType: this.node.tryGetContext('instanceType'),
      maxPrice: this.node.tryGetContext('maxPrice'),
      keyPairName: this.node.tryGetContext('keyPairName'),
      volumeSize: this.node.tryGetContext('volumeSize'),
      volumeType: this.node.tryGetContext('volumeType'),
      sshPort: this.node.tryGetContext('sshPort'),
      backendPort: this.node.tryGetContext('backendPort'),
      targetCapacity: this.node.tryGetContext('targetCapacity'),
    };

    // Validate required context values (no in-code defaults per requirement)
    if (config.instanceType === undefined) {
      throw new Error("Missing required context 'instanceType' in cdk.json (context.instanceType)");
    }
    if (config.maxPrice === undefined) {
      throw new Error("Missing required context 'maxPrice' in cdk.json (context.maxPrice)");
    }
    if (config.keyPairName === undefined) {
      throw new Error("Missing required context 'keyPairName' in cdk.json (context.keyPairName)");
    }
    if (config.volumeSize === undefined) {
      throw new Error("Missing required context 'volumeSize' in cdk.json (context.volumeSize)");
    }
    if (config.volumeType === undefined) {
      throw new Error("Missing required context 'volumeType' in cdk.json (context.volumeType)");
    }
    if (config.sshPort === undefined) {
      throw new Error("Missing required context 'sshPort' in cdk.json (context.sshPort)");
    }
    if (config.backendPort === undefined) {
      throw new Error("Missing required context 'backendPort' in cdk.json (context.backendPort)");
    }
    if (config.targetCapacity === undefined) {
      throw new Error("Missing required context 'targetCapacity' in cdk.json (context.targetCapacity)");
    }

    const sshPortNumber = Number(config.sshPort);
    const backendPortNumber = Number(config.backendPort);
    const targetCapacityNumber = Number(config.targetCapacity);
    if (!Number.isFinite(sshPortNumber)) {
      throw new Error("Context 'sshPort' must be a number");
    }
    if (!Number.isFinite(backendPortNumber)) {
      throw new Error("Context 'backendPort' must be a number");
    }
    if (!Number.isFinite(targetCapacityNumber) || targetCapacityNumber < 0) {
      throw new Error("Context 'targetCapacity' must be a non-negative number");
    }

    const vpc = ec2.Vpc.fromLookup(this, 'VPC', { isDefault: true });

    const securityGroup = new ec2.SecurityGroup(this, 'BackendSecurityGroup', {
      vpc,
      description: 'Security group for DistributedColony backend spot instances',
      allowAllOutbound: true,
    });
    securityGroup.addIngressRule(ec2.Peer.anyIpv4(), ec2.Port.tcp(sshPortNumber), 'Allow SSH');
    securityGroup.addIngressRule(ec2.Peer.anyIpv4(), ec2.Port.tcp(backendPortNumber), 'Allow backend traffic');

    // EC2 instance role (for pulling from ECR, SSM, etc.)
    const instanceRole = new iam.Role(this, 'BackendInstanceRole', {
      assumedBy: new iam.ServicePrincipal('ec2.amazonaws.com'),
      description: 'Role for backend EC2 instances',
      managedPolicies: [
        iam.ManagedPolicy.fromAwsManagedPolicyName('AmazonSSMManagedInstanceCore'),
      ],
    });

    // ECR pull permissions
    instanceRole.addToPolicy(new iam.PolicyStatement({
      effect: iam.Effect.ALLOW,
      actions: [
        'ecr:GetAuthorizationToken',
        'ecr:BatchCheckLayerAvailability',
        'ecr:GetDownloadUrlForLayer',
        'ecr:BatchGetImage',
      ],
      resources: ['*'],
    }));

    const instanceProfile = new iam.CfnInstanceProfile(this, 'BackendInstanceProfile', {
      roles: [instanceRole.roleName],
    });

    const accountId = cdk.Stack.of(this).account;
    const region = cdk.Stack.of(this).region;
    const ecrUri = `${accountId}.dkr.ecr.${region}.amazonaws.com/distributed-colony:latest`;

    const userData = ec2.UserData.forLinux();
    userData.addCommands(
      '#!/bin/bash',
      'set -e',
      'yum update -y',
      'yum install -y docker',
      'systemctl start docker',
      'systemctl enable docker',
      'usermod -a -G docker ec2-user',
      'curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip"',
      'unzip -q awscliv2.zip',
      'sudo ./aws/install',
      'rm -rf awscliv2.zip aws/',
      `aws configure set region ${region}`,
      `aws ecr get-login-password --region ${region} | docker login --username AWS --password-stdin ${accountId}.dkr.ecr.${region}.amazonaws.com`,
      `docker pull ${ecrUri}`,
      `docker run -d --name distributed-colony -p ${backendPortNumber}:${backendPortNumber} -e SERVICE_TYPE=backend -e BACKEND_PORT=${backendPortNumber} ${ecrUri}`,
      'echo "Backend container started"',

      // Write a reload script
      "cat <<'EOF' > /home/ec2-user/reload-backend.sh",
      '#!/bin/bash',
      'set -e',
      `ECR_URI=${accountId}.dkr.ecr.${region}.amazonaws.com/distributed-colony:latest`,
      `BACKEND_PORT=${backendPortNumber}`,
      '',
      'echo "[INFO] Pulling latest Docker image..."',
      `aws ecr get-login-password --region ${region} | docker login --username AWS --password-stdin ${'${'}ECR_URI%/*}`,
      'docker pull $ECR_URI',
      '',
      'echo "[INFO] Stopping and removing existing container (if any)..."',
      'docker stop distributed-colony || true',
      'docker rm distributed-colony || true',
      '',
      'echo "[INFO] Starting new container..."',
      'docker run -d \\',
      '  --name distributed-colony \\',
      `  -p ${backendPortNumber}:${backendPortNumber} \\`,
      '  -e SERVICE_TYPE=backend \\',
      `  -e BACKEND_PORT=${backendPortNumber} \\`,
      '  $ECR_URI',
      'EOF',
      'chmod +x /home/ec2-user/reload-backend.sh'
    );

    // Resolve an Amazon Linux 2 AMI at synth-time for this region
    const amiId = ec2.MachineImage.latestAmazonLinux2().getImage(this).imageId;

    // Low-level launch template for use by Spot Fleet
    const lt = new ec2.CfnLaunchTemplate(this, 'BackendLaunchTemplate', {
      launchTemplateData: {
        imageId: amiId,
        instanceType: config.instanceType!,
        keyName: config.keyPairName!,
        iamInstanceProfile: { arn: instanceProfile.attrArn },
        securityGroupIds: [securityGroup.securityGroupId],
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
        userData: cdk.Fn.base64(userData.render()),
      },
    });

    // Use the AWS service-linked role for EC2 Spot Fleet. AWS will create it if missing on first use.
    const spotServiceLinkedRoleArn = `arn:aws:iam::${accountId}:role/aws-service-role/spotfleet.amazonaws.com/AWSServiceRoleForEC2SpotFleet`;

    const spotFleet = new ec2.CfnSpotFleet(this, 'BackendSpotFleet', {
      spotFleetRequestConfigData: {
        iamFleetRole: spotServiceLinkedRoleArn,
        targetCapacity: targetCapacityNumber,
        type: 'maintain',
        excessCapacityTerminationPolicy: 'Default',
        spotPrice: config.maxPrice!,
        launchTemplateConfigs: [
          {
            launchTemplateSpecification: {
              launchTemplateId: lt.attrLaunchTemplateId,
              version: lt.attrLatestVersionNumber,
            },
          },
        ],
        terminateInstancesWithExpiration: true,
      },
    });

    new cdk.CfnOutput(this, 'SpotFleetId', {
      value: spotFleet.ref,
      description: 'The ID of the backend spot fleet',
    });
  }
}


