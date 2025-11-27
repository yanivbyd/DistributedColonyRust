#!/bin/bash

# Complete deployment script for DistributedColony
# This script builds the Docker image, pushes it to ECR, and deploys the CDK stack

set -e
BUILD_VERSION=$(date -u +"%Y-%m-%d %H:%M:%S UTC")

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

# Configuration
AWS_REGION=${AWS_REGION:-"eu-west-1"}
ECR_REPOSITORY=${ECR_REPOSITORY:-"distributed-colony"}
IMAGE_TAG=${IMAGE_TAG:-"latest"}
CDK_CONTEXT_FILE=${CDK_CONTEXT_FILE:-"../CDK/cdk.context.json"}

print_step "Starting DistributedColony deployment (version $BUILD_VERSION)..."

# Check if AWS CLI is installed
if ! command -v aws &> /dev/null; then
    print_error "AWS CLI is not installed!"
    print_status "Please install AWS CLI first:"
    print_status "  macOS: brew install awscli"
    print_status "  Linux: curl 'https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip' -o 'awscliv2.zip' && unzip awscliv2.zip && sudo ./aws/install"
    print_status "  Windows: Download from https://aws.amazon.com/cli/"
    exit 1
fi

# Check if AWS CLI is configured
print_status "Checking AWS CLI configuration..."
if ! aws sts get-caller-identity > /dev/null 2>&1; then
    print_error "AWS CLI is not configured or credentials are invalid"
    echo ""
    print_status "Please run the following command to configure AWS CLI:"
    print_status "  aws configure"
    echo ""
    print_status "You'll need to provide:"
    print_status "  - AWS Access Key ID"
    print_status "  - AWS Secret Access Key"
    print_status "  - Default region (eu-west-1 for Dublin)"
    print_status "  - Default output format (json)"
    echo ""
    print_status "If you don't have AWS credentials, you can:"
    print_status "  1. Create them in AWS Console > IAM > Users > Security credentials"
    print_status "  2. Or use AWS SSO if your organization uses it"
    print_status "  3. Or use instance roles if running on EC2"
    echo ""
    print_status "After running 'aws configure', try this script again."
    exit 1
fi

# Check if we're in the right directory
if [ ! -f "Dockerfile" ]; then
    print_error "Dockerfile not found. Please run this script from the Docker directory."
    exit 1
fi

# Check if CDK directory exists
if [ ! -d "../CDK" ]; then
    print_error "CDK directory not found. Please ensure the CDK directory exists."
    exit 1
fi

# Step 1: Build and push Docker image
print_step "Step 1: Building and pushing Docker image..."
BUILD_VERSION="$BUILD_VERSION" ./build-and-push.sh

if [ $? -ne 0 ]; then
    print_error "Docker build and push failed!"
    exit 1
fi

print_status "Docker image built and pushed successfully!"

# Step 2: Deploy CDK stack
print_step "Step 2: Deploying CDK infrastructure..."

cd ../CDK

# Check if CDK is installed
if ! command -v cdk &> /dev/null; then
    print_warning "CDK CLI not found. Installing dependencies..."
    npm install
fi

# Bootstrap CDK if needed
print_status "Bootstrapping CDK (if needed)..."
cdk bootstrap

# Deploy the stacks
print_status "Deploying coordinator stack..."
cdk deploy DistributedColonyCoordinator --require-approval never

print_status "Deploying backend stack..."
cdk deploy DistributedColonyBackend --require-approval never

print_status "CDK deployment completed successfully!"

# Step 3: Display connection information
print_step "Step 3: Getting deployment information..."

print_status "Getting coordinator IP address..."
COORDINATOR_IP=$(aws ec2 describe-spot-fleet-instances --spot-fleet-request-id $(aws cloudformation describe-stacks --stack-name DistributedColonyCoordinator --query 'Stacks[0].Outputs[?OutputKey==`CoordinatorSpotFleetId`].OutputValue' --output text) --query 'ActiveInstances[0].InstanceId' --output text | xargs -I {} aws ec2 describe-instances --instance-ids {} --query 'Reservations[0].Instances[0].PublicIpAddress' --output text)

print_status "Getting backend IP addresses..."
BACKEND_IPS=$(aws ec2 describe-spot-fleet-instances --spot-fleet-request-id $(aws cloudformation describe-stacks --stack-name DistributedColonyBackend --query 'Stacks[0].Outputs[?OutputKey==`SpotFleetId`].OutputValue' --output text) --query 'ActiveInstances[].InstanceId' --output text | xargs -I {} aws ec2 describe-instances --instance-ids {} --query 'Reservations[].Instances[].PublicIpAddress' --output text)

print_step "Deployment Summary:"
echo "=========================================="
echo "Coordinator IP: $COORDINATOR_IP"
echo "Backend IPs: $BACKEND_IPS"
echo "=========================================="
echo ""
echo "You can now connect to your DistributedColony deployment!"
echo ""
echo "To check coordinator status:"
echo "curl http://$COORDINATOR_IP:8083/health"
echo ""
echo "To check backend status:"
for ip in $BACKEND_IPS; do
    echo "curl http://$ip:8082/health"
done

print_status "Deployment completed successfully!"
