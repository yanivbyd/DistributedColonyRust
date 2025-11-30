import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as iam from 'aws-cdk-lib/aws-iam';
import { Construct } from 'constructs';
import { UserDataBuilder, ColonyInstanceType } from './user-data-builder';

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
    const coordinatorPortNumber = Number(this.node.tryGetContext('coordinatorPort'));
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
    if (!Number.isFinite(coordinatorPortNumber)) {
      throw new Error("Context 'coordinatorPort' must be a number");
    }

    const vpc = ec2.Vpc.fromLookup(this, 'VPC', { isDefault: true });

    const securityGroup = new ec2.SecurityGroup(this, 'BackendSecurityGroup', {
      vpc,
      description: 'Security group for DistributedColony backend spot instances',
      allowAllOutbound: true,
    });
    securityGroup.addIngressRule(ec2.Peer.anyIpv4(), ec2.Port.tcp(sshPortNumber), 'Allow SSH');
    securityGroup.addIngressRule(ec2.Peer.anyIpv4(), ec2.Port.tcp(backendPortNumber), 'Allow backend traffic');
    securityGroup.addIngressRule(ec2.Peer.anyIpv4(), ec2.Port.tcp(coordinatorPortNumber), 'Allow coordinator traffic');
    securityGroup.addIngressRule(ec2.Peer.anyIpv4(), ec2.Port.tcp(8084), 'Allow HTTP server (coordinator and backend)');
    securityGroup.addIngressRule(ec2.Peer.anyIpv4(), ec2.Port.allIcmp(), 'Allow ICMP health checks');

    const accountId = cdk.Stack.of(this).account;
    const region = cdk.Stack.of(this).region;

    // EC2 instance role (for pulling from ECR, SSM, etc.)
    const instanceRole = new iam.Role(this, 'BackendInstanceRole', {
      assumedBy: new iam.ServicePrincipal('ec2.amazonaws.com'),
      description: 'Role for backend EC2 instances',
      managedPolicies: [
        iam.ManagedPolicy.fromAwsManagedPolicyName('AmazonSSMManagedInstanceCore'),
      ],
    });

    // Allow instances to write/read their SSM registration parameters
    instanceRole.addToPolicy(new iam.PolicyStatement({
      effect: iam.Effect.ALLOW,
      actions: [
        'ssm:PutParameter',
        'ssm:DeleteParameter',
        'ssm:GetParameter',
        'ssm:GetParameters',
        'ssm:GetParametersByPath',
      ],
      resources: [
        `arn:aws:ssm:${region}:${accountId}:parameter/colony/*`,
      ],
    }));

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

    // Allow instances to tag themselves as Faulty on failures
    instanceRole.addToPolicy(new iam.PolicyStatement({
      effect: iam.Effect.ALLOW,
      actions: [
        'ec2:CreateTags',
      ],
      resources: [
        `arn:aws:ec2:${region}:${accountId}:instance/*`,
      ],
      conditions: {
        StringEquals: {
          'ec2:Region': region,
        },
      },
    }));

    const instanceProfile = new iam.CfnInstanceProfile(this, 'BackendInstanceProfile', {
      roles: [instanceRole.roleName],
    });

    const ecrUri = `${accountId}.dkr.ecr.${region}.amazonaws.com/distributed-colony:latest`;

    const userDataBuilder = new UserDataBuilder(accountId, region);

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
        userData: cdk.Fn.base64(userDataBuilder.buildUserData(ColonyInstanceType.BACKEND, backendPortNumber)),
        tagSpecifications: [
          {
            resourceType: 'instance',
            tags: [
              { key: 'Name', value: 'distributed-colony-backend' },
              { key: 'Type', value: 'backend' },
              { key: 'Service', value: 'distributed-colony' },
            ],
          },
        ],
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

    // Coordinator user data (reuse same flow, different service/port)

    const coordinatorLt = new ec2.CfnLaunchTemplate(this, 'CoordinatorLaunchTemplate', {
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
        userData: cdk.Fn.base64(userDataBuilder.buildUserData(ColonyInstanceType.COORDINATOR, coordinatorPortNumber)),
        tagSpecifications: [
          {
            resourceType: 'instance',
            tags: [
              { key: 'Name', value: 'distributed-colony-coordinator' },
              { key: 'Type', value: 'coordinator' },
              { key: 'Service', value: 'distributed-colony' },
            ],
          },
        ],
      },
    });

    const coordinatorSpotFleet = new ec2.CfnSpotFleet(this, 'CoordinatorSpotFleet', {
      spotFleetRequestConfigData: {
        iamFleetRole: spotServiceLinkedRoleArn,
        targetCapacity: 1,
        type: 'maintain',
        excessCapacityTerminationPolicy: 'Default',
        spotPrice: config.maxPrice!,
        launchTemplateConfigs: [
          {
            launchTemplateSpecification: {
              launchTemplateId: coordinatorLt.attrLaunchTemplateId,
              version: coordinatorLt.attrLatestVersionNumber,
            },
          },
        ],
        terminateInstancesWithExpiration: true,
      },
    });

    new cdk.CfnOutput(this, 'CoordinatorSpotFleetId', {
      value: coordinatorSpotFleet.ref,
      description: 'The ID of the coordinator spot fleet',
    });
  }
}


