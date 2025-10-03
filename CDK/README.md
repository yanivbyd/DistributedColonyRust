# DistributedColonyCloud

Cloud infrastructure and deployment configurations for [DistributedColonyRust](https://github.com/yanivbyd/DistributedColonyRust).

This repository contains Infrastructure as Code (IaC) for deploying the DistributedColony game to various cloud providers.

## Overview

DistributedColonyRust is a distributed colony simulation game built with Rust. This repository provides the cloud deployment infrastructure to run the game at scale.

## Cloud Providers

### AWS

AWS deployment uses CDK (Cloud Development Kit) to provision infrastructure.

#### Prerequisites

- Node.js 18+ and npm
- AWS CLI configured with credentials
- AWS CDK CLI: `npm install -g aws-cdk`

#### Setup AWS CLI

**Install AWS CLI** (macOS):
```bash
brew install awscli
```

For other operating systems, see [AWS CLI Installation Guide](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html).

**Configure AWS Credentials**:
```bash
aws configure
```

You'll need:
- AWS Access Key ID (from IAM Console)
- AWS Secret Access Key
- Default region: `eu-west-1`
- Output format: `json`

Your credentials will be stored in `~/.aws/credentials` and configuration in `~/.aws/config`.

#### Quick Start

```bash
cd aws
npm install
cdk bootstrap  # First time only
cdk deploy
```

#### Shutdown and Stop All Charges

To completely shut down and remove all AWS resources (stops all charges):

```bash
cd aws
npm run shutdown
# or
cdk destroy --force
```

⚠️ **Warning**: This will permanently delete the spot instance and all data on it.

#### Architecture

- **Spot Instance**: Cost-optimized EC2 spot instance for running the game server
- **Security Group**: Configured for game traffic
- **IAM Role**: Permissions for the instance

#### Configuration

See `aws/README.md` for detailed AWS deployment instructions.

## Contributing

Contributions are welcome! Please feel free to submit pull requests or open issues.

## License

Same as [DistributedColonyRust](https://github.com/yanivbyd/DistributedColonyRust)
