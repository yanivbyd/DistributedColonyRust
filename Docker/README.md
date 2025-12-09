# DistributedColony Docker Deployment

This directory contains Docker configuration for deploying DistributedColony nodes to AWS spot instances.

## Overview

The Docker setup uses a two-stage build system:
1. **Base Image** (`distributed-colony-base`): Pre-compiles all Rust dependencies using cargo-chef
2. **Colony Image** (`distributed-colony`): Contains only the compiled Rust binaries in a minimal distroless runtime

This separation allows for fast iterative builds while maintaining minimal production images.

## Files

- `Dockerfile.base` - Multi-stage Docker build that pre-compiles all Rust dependencies using cargo-chef
- `Dockerfile` - Multi-stage Docker build that compiles the application and creates a minimal runtime image
- `build-and-push-base.sh` - Script to build and push the base Docker image to AWS ECR (slow, run when dependencies change)
- `build-and-push-colony.sh` - Script to build and push the application Docker image to AWS ECR (fast, run for code changes)
- `.dockerignore` - Files to exclude from Docker context
- `README.md` - This documentation file

## Prerequisites

1. Docker installed locally
2. Docker BuildKit enabled (included in modern Docker installations)
3. AWS CLI configured with appropriate permissions
4. AWS ECR repositories will be created automatically if they don't exist

## Quick Start

### 1. Build and Push Base Image (One Time or When Dependencies Change)

```bash
cd Docker
chmod +x build-and-push-base.sh
./build-and-push-base.sh
```

This will:
- Build the base image locally (tagged as `distributed-colony-base:latest`)
- Push it to ECR as `${accountId}.dkr.ecr.${region}.amazonaws.com/distributed-colony-base:latest`
- Take 5-15 minutes (only needed when `Cargo.toml` or `Cargo.lock` changes)

### 2. Build and Push Colony Image (Fast Iteration)

```bash
chmod +x build-and-push-colony.sh
./build-and-push-colony.sh
```

This will:
- Check that the base image exists locally (will fail if not found - never pulls from ECR)
- Build the colony image using the local base image with Docker BuildKit cache mounts
- Push it to ECR as `${accountId}.dkr.ecr.${region}.amazonaws.com/distributed-colony:latest`
- Take 30 seconds - 2 minutes (run after every code change)

### 3. Deploy with CDK

```bash
cd ../CDK
npm install
npm run deploy
```

## Docker Image Details

### Base Image (`distributed-colony-base`)

- **Purpose**: Pre-compiles all Rust dependencies
- **Size**: Large (~500MB - 1GB)
- **Contents**: Rust toolchain, compiled dependencies (with full Cargo metadata), build environment
- **Rebuild**: Only when `Cargo.toml` or `Cargo.lock` changes
- **Note**: Dependency artifacts are NOT stripped to preserve Cargo metadata

### Colony Image (`distributed-colony`)

- **Purpose**: Production runtime image
- **Size**: Very small (~20-50MB)
- **Contents**: 
  - Compiled `backend` binary (stripped)
  - Compiled `coordinator` binary (stripped)
  - Minimal distroless runtime (C runtime + CA certificates only)
- **Rebuild**: Every code change
- **Build Speed**: Fast (< 2 minutes) using Docker BuildKit cache mounts

## Environment Variables

### Build Scripts

- `AWS_REGION`: AWS region (default: eu-west-1)
- `ECR_REPOSITORY`: Colony repository name (default: distributed-colony)
- `ECR_BASE_REPOSITORY`: Base repository name (default: distributed-colony-base)
- `IMAGE_TAG`: Colony image tag (default: latest)
- `BASE_IMAGE_TAG`: Base image tag (default: latest)

### Container Runtime

The container supports the following environment variables (for reference, distroless doesn't use them):

- `SERVICE_TYPE` - Either "backend" or "coordinator" (not used, override CMD instead)
- `COORDINATOR_HOST` - Hostname/IP of the coordinator (default: localhost)
- `COORDINATOR_PORT` - Port of the coordinator (default: 8083)
- `BACKEND_HOST` - Bind address for backend (default: 0.0.0.0)
- `BACKEND_PORT` - Port for the backend service (default: 8082)
- `BUILD_VERSION` - Build version string

## Running Containers

### Run Backend

```bash
docker run -p 8082:8082 \
  <accountId>.dkr.ecr.<region>.amazonaws.com/distributed-colony:latest \
  /usr/local/bin/backend 0.0.0.0 8082 aws
```

### Run Coordinator

```bash
docker run -p 8083:8083 \
  <accountId>.dkr.ecr.<region>.amazonaws.com/distributed-colony:latest \
  /usr/local/bin/coordinator aws
```

## Build Workflow

### Initial Setup

1. Build base image once: `./build-and-push-base.sh`
2. Build colony image: `./build-and-push-colony.sh`
3. Deploy with CDK

### Iterative Development

1. Make code changes
2. Build colony image: `./build-and-push-colony.sh` (fast, ~30s-2min)
3. Deploy with CDK

### When Dependencies Change

1. Update `Cargo.toml` or `Cargo.lock`
2. Rebuild base image: `./build-and-push-base.sh` (slow, ~5-15min)
3. Build colony image: `./build-and-push-colony.sh` (fast)
4. Deploy with CDK

## Important Notes

- **Never Pulls from ECR**: The colony build script will fail if the base image doesn't exist locally. This ensures fast builds by using local images only.
- **AMD64 Only**: All builds target `linux/amd64` platform only (for EC2 compatibility).
- **Minimal Runtime**: The final colony image uses distroless/cc, which has no shell or package manager for security.
- **Fast Builds**: Uses Docker BuildKit cache mounts to persist compiled dependencies across builds, ensuring fast iteration even when Cargo's incremental compilation invalidates its cache.
- **GUI Crate Excluded**: The workspace manifest explicitly excludes the GUI crate to avoid platform-specific issues.

## Troubleshooting

### Base Image Not Found

If you see: `Base image 'distributed-colony-base:latest' not found locally!`

**Solution**: Run `./build-and-push-base.sh` first.

### AWS Credentials Invalid

If you see: `AWS CLI is not configured or credentials are invalid`

**Solution**: Run `aws configure` and provide your AWS credentials.

### Docker BuildKit Not Available

If you see build errors related to cache mounts:

**Solution**: Ensure Docker BuildKit is enabled. Modern Docker installations include it by default. You can enable it explicitly with `DOCKER_BUILDKIT=1`.

### Build Too Slow

If colony builds are taking longer than 2 minutes:

**Solution**: 
- Ensure the base image was built correctly
- Check that Docker BuildKit cache mounts are working (look for cache mount messages in build output)
- Verify that `/app/target` is being copied from the base image before cache mounts
