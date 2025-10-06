#!/usr/bin/env node
import 'source-map-support/register';
import * as cdk from 'aws-cdk-lib';
import { SpotInstancesStack } from '../lib/spot-instances-stack';

const app = new cdk.App();

// Get configuration from context or environment variables
const env = {
  account: process.env.CDK_DEFAULT_ACCOUNT,
  region: process.env.CDK_DEFAULT_REGION || 'eu-west-1',
};

// Deploy single stack for backend spot instances
new SpotInstancesStack(app, 'DistributedColonySpotInstances', {
  env,
  description: 'Distributed Colony backend spot instances',
  tags: {
    Project: 'DistributedColony',
    Environment: 'production',
    ManagedBy: 'CDK',
    Component: 'Backend',
  },
});

