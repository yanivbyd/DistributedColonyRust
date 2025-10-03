#!/usr/bin/env node
import 'source-map-support/register';
import * as cdk from 'aws-cdk-lib';
import { DistributedColonyStack } from '../lib/spot-instance-stack';
import { CoordinatorStack } from '../lib/coordinator-stack';

const app = new cdk.App();

// Get configuration from context or environment variables
const env = {
  account: process.env.CDK_DEFAULT_ACCOUNT,
  region: process.env.CDK_DEFAULT_REGION || 'eu-west-1',
};

// Deploy coordinator first (backend instances depend on it)
const coordinatorStack = new CoordinatorStack(app, 'DistributedColonyCoordinator', {
  env,
  description: 'Distributed Colony coordinator deployment',
  tags: {
    Project: 'DistributedColony',
    Environment: 'production',
    ManagedBy: 'CDK',
    Component: 'Coordinator',
  },
});

// Deploy backend instances
const backendStack = new DistributedColonyStack(app, 'DistributedColonyBackend', {
  env,
  description: 'Distributed Colony backend deployment',
  tags: {
    Project: 'DistributedColony',
    Environment: 'production',
    ManagedBy: 'CDK',
    Component: 'Backend',
  },
});

// Add dependency to ensure coordinator is deployed first
backendStack.addDependency(coordinatorStack);

