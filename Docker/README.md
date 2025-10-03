# DistributedColony Docker Deployment

This directory contains Docker configuration for deploying DistributedColony nodes to AWS spot instances.

## Overview

The Docker setup creates a containerized version of the DistributedColony application that includes both the backend and coordinator components. This allows for easy deployment to AWS spot instances without the need to compile Rust code on the target machines.

## Files

- `Dockerfile` - Multi-stage Docker build that compiles the Rust application and creates a minimal runtime image
- `build-and-push.sh` - Script to build and push the Docker image to AWS ECR
- `README.md` - This documentation file

## Prerequisites

1. Docker installed locally
2. AWS CLI configured with appropriate permissions
3. AWS ECR repository created for the Docker images

## Quick Start

### 1. Create ECR Repository

```bash
aws ecr create-repository --repository-name distributed-colony --region us-east-1
```

### 2. Build and Push Docker Image

```bash
# Make the script executable
chmod +x build-and-push.sh

# Build and push the image
./build-and-push.sh
```

### 3. Deploy with CDK

The CDK stack has been updated to use the Docker image instead of compiling on the instance. Deploy with:

```bash
cd ../CDK
npm install
npm run deploy
```

## Docker Image Details

The Docker image is built using a multi-stage approach:

1. **Build Stage**: Uses the official Rust image to compile the application
2. **Runtime Stage**: Uses a minimal Alpine Linux image with only the necessary runtime dependencies

### Image Contents

- Compiled `backend` binary
- Compiled `coordinator` binary
- Runtime dependencies (glibc, etc.)
- Startup script to run the appropriate service

### Environment Variables

The container supports the following environment variables:

- `SERVICE_TYPE` - Either "backend" or "coordinator" to determine which service to run
- `COORDINATOR_HOST` - Hostname/IP of the coordinator (for backend instances)
- `COORDINATOR_PORT` - Port of the coordinator (for backend instances)
- `BACKEND_PORT` - Port for the backend service
- `COORDINATOR_PORT` - Port for the coordinator service

## Deployment Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Coordinator   │    │    Backend 1    │    │    Backend N    │
│   (Spot Instance)│    │  (Spot Instance)│    │  (Spot Instance)│
│                 │    │                 │    │                 │
│  Docker Image   │    │  Docker Image   │    │  Docker Image   │
│  - coordinator  │    │  - backend      │    │  - backend      │
│                 │    │                 │    │                 │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

## Cost Optimization

Using Docker images provides several cost benefits:

1. **Faster Startup**: No compilation time on spot instances
2. **Reduced Instance Time**: Instances can start serving traffic immediately
3. **Consistent Environment**: Same runtime environment across all instances
4. **Smaller Instances**: Can use smaller instance types since no compilation is needed

## Monitoring and Logs

The Docker containers are configured to:

- Log to stdout/stderr for CloudWatch integration
- Include health checks for container orchestration
- Support graceful shutdowns for spot instance termination

## Troubleshooting

### Common Issues

1. **ECR Authentication**: Ensure AWS CLI is configured and you have ECR permissions
2. **Image Pull Errors**: Check that the ECR repository exists and the image was pushed successfully
3. **Service Startup**: Verify environment variables are set correctly for each service type

### Debugging

To debug container issues:

```bash
# Check container logs
docker logs <container_id>

# Execute shell in running container
docker exec -it <container_id> /bin/sh

# Check if binaries are present
docker exec -it <container_id> ls -la /usr/local/bin/
```

## Security Considerations

- The Docker image runs as a non-root user
- Only necessary dependencies are included in the runtime image
- ECR repository should be configured with appropriate access policies
- Consider using ECR image scanning for vulnerability detection
