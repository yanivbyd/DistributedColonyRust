#!/bin/bash

# Build and push Docker image for DistributedColony
# This script builds the Docker image and pushes it to AWS ECR

set -e

# Configuration
AWS_REGION=${AWS_REGION:-"eu-west-1"}
ECR_REPOSITORY=${ECR_REPOSITORY:-"distributed-colony"}
IMAGE_TAG=${IMAGE_TAG:-"latest"}
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
print_status "Using ECR Repository: $ECR_REPOSITORY"
print_status "Using Image Tag: $IMAGE_TAG"

# Construct full image URI
ECR_URI="$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com/$ECR_REPOSITORY:$IMAGE_TAG"

print_status "Full ECR URI: $ECR_URI"

# Check if ECR repository exists, create if it doesn't
print_status "Checking if ECR repository exists..."
if ! aws ecr describe-repositories --repository-names $ECR_REPOSITORY --region $AWS_REGION > /dev/null 2>&1; then
    print_warning "ECR repository '$ECR_REPOSITORY' does not exist. Creating it..."
    aws ecr create-repository \
        --repository-name $ECR_REPOSITORY \
        --region $AWS_REGION \
        --image-scanning-configuration scanOnPush=true \
        --encryption-configuration encryptionType=AES256
    print_status "ECR repository created successfully"

	aws ecr put-lifecycle-policy \
		--repository-name "$ECR_REPOSITORY" \
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
    print_status "ECR repository already exists"
fi

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

# Build and push a multi-arch image (amd64 for EC2, arm64 for Apple Silicon)
print_status "Building and pushing multi-arch image (linux/amd64, linux/arm64)..."
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t $ECR_URI \
  -f Dockerfile \
  .. \
  --push

print_status "Docker image successfully built and pushed to ECR!"
print_status "Image URI: $ECR_URI"

# Display image information
print_status "Image details:"
aws ecr describe-images \
    --repository-name $ECR_REPOSITORY \
    --image-ids imageTag=$IMAGE_TAG \
    --region $AWS_REGION \
    --query 'imageDetails[0].{Size:imageSizeInBytes,PushedAt:imagePushedAt,Digest:imageDigest}' \
    --output table

print_status "Build and push completed successfully!"
print_status "You can now deploy your CDK stack with: cd ../CDK && npm run deploy"
