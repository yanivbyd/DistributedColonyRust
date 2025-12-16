#!/bin/bash

# build-and-push-colony.sh: Build and push COLONY Docker image for DistributedColony
# This script builds the application Docker image and pushes it to AWS ECR
# The colony image depends on the base image, which should be built first

set -e

# Always run from the workspace root so Dockerfile/context paths are correct
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

start_time=$(date +%s)
BUILD_VERSION=$(date -u +"%Y-%m-%d %H:%M:%S UTC")
# Unique build tag for coordinator binary: <git-short-hash>-<UTC timestamp>
COORDINATOR_BUILD_TAG="$(git rev-parse --short HEAD 2>/dev/null || echo no-git)-$(date -u +%Y%m%d%H%M%S)"

# Configuration
AWS_REGION=${AWS_REGION:-"eu-west-1"}
ECR_REPOSITORY=${ECR_REPOSITORY:-"distributed-colony"}
ECR_BASE_REPOSITORY=${ECR_BASE_REPOSITORY:-"distributed-colony-base"}
IMAGE_TAG=${IMAGE_TAG:-"latest"}
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

# Helper function to print timing
print_timing() {
    local step_name=$1
    local start=$2
    local end=$(date +%s)
    local elapsed=$(( end - start ))
    local min=$(( elapsed / 60 ))
    local sec=$(( elapsed % 60 ))
    if [ $min -gt 0 ]; then
        print_status "[TIMING] $step_name: ${min}m ${sec}s"
    else
        print_status "[TIMING] $step_name: ${sec}s"
    fi
}

# Check if AWS CLI is installed
if ! command -v aws &> /dev/null; then
    print_error "AWS CLI is not installed!"
    print_status "Please install AWS CLI first:"
    exit 1
fi

# Check if AWS CLI is configured
step_start=$(date +%s)
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
print_timing "AWS CLI configuration check" $step_start

step_start=$(date +%s)
print_status "Using AWS Account: $AWS_ACCOUNT_ID"
print_status "Using AWS Region: $AWS_REGION"
print_status "Using ECR Repository: $ECR_REPOSITORY"
print_status "Using Base ECR Repository: $ECR_BASE_REPOSITORY"
print_status "Using Image Tag: $IMAGE_TAG"
print_status "Using Base Image Tag: $BASE_IMAGE_TAG"
print_status "Build version timestamp: $BUILD_VERSION"
print_timing "AWS account/region setup" $step_start

# Construct full image URIs
ECR_URI="$AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com/$ECR_REPOSITORY:$IMAGE_TAG"

print_status "Full ECR URI: $ECR_URI"

# Check if base image exists locally (NEVER pull from ECR)
step_start=$(date +%s)
print_status "Checking if base image exists locally..."
if ! docker image inspect distributed-colony-base:latest > /dev/null 2>&1; then
    print_error "Base image 'distributed-colony-base:latest' not found locally!"
    print_status "Please build the base image first:"
    print_status "  ./build-and-push-base.sh"
    exit 1
fi
print_status "Base image found locally (using local image, no network pull needed)"
print_timing "Base image check" $step_start

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
step_start=$(date +%s)
create_ecr_repository $ECR_REPOSITORY
print_timing "ECR repository check/creation" $step_start

# Authenticate Docker with ECR
step_start=$(date +%s)
print_status "Authenticating Docker with ECR..."
aws ecr get-login-password --region $AWS_REGION | docker login --username AWS --password-stdin $AWS_ACCOUNT_ID.dkr.ecr.$AWS_REGION.amazonaws.com
print_timing "Docker ECR authentication" $step_start

build_start=$(date +%s)
print_status "Building COLONY image for linux/amd64 (using local base image: distributed-colony-base:latest)..."
print_status "Docker build cache disabled for this build to guarantee fresh binaries"
DOCKER_BUILDKIT=1 docker build \
  --no-cache \
  --platform linux/amd64 \
  --build-arg BUILD_VERSION="$BUILD_VERSION" \
  --build-arg COORDINATOR_BUILD_TAG="$COORDINATOR_BUILD_TAG" \
  --build-arg BASE_IMAGE="distributed-colony-base:latest" \
  -t "$ECR_URI" \
  -f Docker/Dockerfile \
  .
build_end=$(date +%s)
build_elapsed=$(( build_end - build_start ))
build_min=$(( build_elapsed / 60 ))
build_sec=$(( build_elapsed % 60 ))
if [ $build_min -gt 0 ]; then
    print_status "[TIMING] Docker build: ${build_min}m ${build_sec}s"
else
    print_status "[TIMING] Docker build: ${build_sec}s"
fi

# Push colony image to ECR (only the final image, no cache operations)
push_start=$(date +%s)
print_status "Pushing COLONY image to ECR..."
docker push $ECR_URI
push_end=$(date +%s)
push_elapsed=$(( push_end - push_start ))
push_min=$(( push_elapsed / 60 ))
push_sec=$(( push_elapsed % 60 ))
if [ $push_min -gt 0 ]; then
    print_status "[TIMING] ECR push: ${push_min}m ${push_sec}s"
else
    print_status "[TIMING] ECR push: ${push_sec}s"
fi

# Get image size for verification
image_size=$(docker image inspect $ECR_URI --format='{{.Size}}' 2>/dev/null || echo "unknown")
if [ "$image_size" != "unknown" ]; then
    image_size_mb=$(( image_size / 1024 / 1024 ))
    print_status "Colony image size: ${image_size_mb}MB"
fi

print_status "Colony image successfully built and pushed to ECR!"
print_status "Image URI: $ECR_URI"

print_status "Build and push completed successfully!"
print_status "You can now deploy your CDK stack with: cd ../CDK && npm run deploy"

end_time=$(date +%s)
elapsed=$(( end_time - start_time ))
minutes=$(( elapsed / 60 ))
seconds=$(( elapsed % 60 ))
echo ""
print_status "=========================================="
if [ $minutes -gt 0 ]; then
    print_status "TOTAL DURATION: ${minutes}m ${seconds}s"
else
    print_status "TOTAL DURATION: ${seconds}s"
fi
print_status "=========================================="
