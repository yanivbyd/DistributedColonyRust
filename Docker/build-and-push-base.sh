#!/bin/bash

# Build and push BASE Docker image for DistributedColony
# This script builds the base Docker image (with dependencies) and pushes it to AWS ECR
# The base image should be rebuilt only when Cargo.toml files change

set -e
start_time=$(date +%s)

# Configuration
AWS_REGION=${AWS_REGION:-"eu-west-1"}
ECR_BASE_REPOSITORY=${ECR_BASE_REPOSITORY:-"distributed-colony-base"}
BASE_IMAGE_TAG=${BASE_IMAGE_TAG:-"latest"}
AWS_ACCOUNT_ID=$(aws sts get-caller-identity --query Account --output text)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
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

# Check if AWS CLI is installed
if ! command -v aws &> /dev/null; then
    print_error "AWS CLI is not installed!"
    print_status "Please install AWS CLI first:"
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
    exit 1
fi

print_status "Using AWS Account: $AWS_ACCOUNT_ID"
print_status "Using AWS Region: $AWS_REGION"
print_status "Using Base ECR Repository: $ECR_BASE_REPOSITORY"
print_status "Using Base Image Tag: $BASE_IMAGE_TAG"

# Construct full image URIs
ECR_BASE_URI="$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com/$ECR_BASE_REPOSITORY:$BASE_IMAGE_TAG"
ECR_BASE_CACHE_URI="$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com/$ECR_BASE_REPOSITORY:cache"

print_status "Full Base ECR URI: $ECR_BASE_URI"
print_status "Base Cache URI: $ECR_BASE_CACHE_URI"

# Function to create ECR repository if it doesn't exist
create_ecr_repository() {
    local repo_name=$1
    print_status "Checking if ECR repository '$repo_name' exists..."
    if ! aws ecr describe-repositories --repository-names $repo_name --region $AWS_REGION > /dev/null 2>&1; then
        print_warning "ECR repository '$repo_name' does not exist. Creating it..."
        aws ecr create-repository \
            --repository-name $repo_name \
            --region $AWS_REGION \
            --image-scanning-configuration scanOnPush=true \
            --encryption-configuration encryptionType=AES256
        print_status "ECR repository '$repo_name' created successfully"

        aws ecr put-lifecycle-policy \
            --repository-name "$repo_name" \
            --lifecycle-policy-text file://<(\
cat <<'EOF'
{
  "rules": [
    {
      "rulePriority": 1,
      "description": "Expire images older than 30 days",
      "selection": {
        "tagStatus": "any",
        "countType": "sinceImagePushed",
        "countUnit": "days",
        "countNumber": 30
      },
      "action": {
        "type": "expire"
      }
    }
  ]
}
EOF
)
    else
        print_status "ECR repository '$repo_name' already exists"
    fi
}

# Create ECR repository
create_ecr_repository $ECR_BASE_REPOSITORY

# Authenticate Docker with ECR
print_status "Authenticating Docker with ECR..."
aws ecr get-login-password --region $AWS_REGION | docker login --username AWS --password-stdin $AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com

# Ensure Buildx is available
print_status "Ensuring Docker Buildx is available..."
if ! docker buildx version > /dev/null 2>&1; then
    print_error "Docker Buildx is not available. Please update Docker to a version that includes Buildx."
    exit 1
fi

# Create/use a builder (idempotent)
docker buildx create --use >/dev/null 2>&1 || true

# Build base image locally first (for fast local colony builds)
# Using cargo-chef, this builds all dependencies which will be cached
print_status "Building BASE image locally for linux/amd64 (using cargo-chef)..."
docker build \
  -t distributed-colony-base:latest \
  -f Dockerfile.base \
  ..

# Tag and push to ECR (for remote builds/CI, but not needed for local colony builds)
print_status "Tagging and pushing BASE image to ECR..."
docker tag distributed-colony-base:latest $ECR_BASE_URI
docker push $ECR_BASE_URI

print_status "Base image successfully built and pushed to ECR!"
print_status "Base Image URI: $ECR_BASE_URI"

# Display image information
print_status "Base image details:"
aws ecr describe-images \
    --repository-name $ECR_BASE_REPOSITORY \
    --image-ids imageTag=$BASE_IMAGE_TAG \
    --region $AWS_REGION \
    --query 'imageDetails[0].{Size:imageSizeInBytes,PushedAt:imagePushedAt,Digest:imageDigest}' \
    --output table

print_status "Base image build and push completed successfully!"

end_time=$(date +%s)
elapsed=$(( end_time - start_time ))
minutes=$(( elapsed / 60 ))
seconds=$(( elapsed % 60 ))
if [ $minutes -gt 0 ]; then
    print_status "Total duration: ${minutes}m ${seconds}s"
else
    print_status "Total duration: ${seconds}s"
fi

