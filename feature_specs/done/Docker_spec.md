# Docker Build System Specification

## Acknowledgment
Acknowledged by Yaniv

## Overview

This specification defines a two-stage Docker build system for DistributedColony that separates dependency compilation from application binary creation, enabling fast iterative builds while maintaining minimal production images.

## Architecture

### Two-Image Strategy

1. **Base Image** (`distributed-colony-base`)
   - **Purpose**: Pre-compiles all Rust dependencies using cargo-chef
   - **Size**: Large (~500MB - 1GB) - contains Rust toolchain, compiled dependencies, and build artifacts
   - **Rebuild Frequency**: Only when `Cargo.toml` or `Cargo.lock` changes
   - **Build Time**: Slow (5-15 minutes) - compiles all dependencies from scratch
   - **Storage**: Stored in ECR for reference, but never pulled during colony builds

2. **Colony Image** (`distributed-colony`)
   - **Purpose**: Contains only the compiled Rust binaries in a minimal runtime
   - **Size**: Very small (~20-50MB) - distroless base + stripped binaries only
   - **Rebuild Frequency**: Every code change
   - **Build Time**: Very fast (30 seconds - 2 minutes) - reuses base image's compiled dependencies
   - **Storage**: Stored in ECR and used by CDK for deployment

## Design Principles

0. **Use Cargo-Chef with Docker BuildKit Cache Mounts**: Use cargo-chef for dependency compilation in base image, and Docker BuildKit cache mounts in colony image to persist compiled dependencies across builds. This ensures fast builds even when Cargo's incremental compilation invalidates its cache.
1. **AMD64 Platform Only**: All builds must target `linux/amd64` platform only. No multi-arch support.
2. **Never Pull from ECR**: All builds use local images only. If base image doesn't exist locally, build fails with clear error message.
3. **Fast Iteration**: Colony image builds must be fast by reusing compiled dependencies from base image.
4. **Minimal Production Image**: Final colony image contains only runtime binaries, no build tools or dependencies.
5. **CDK Compatibility**: Colony image URI format must match CDK expectations: `${accountId}.dkr.ecr.${region}.amazonaws.com/distributed-colony:latest`
6. **Build Artifact Reuse**: Leverage Docker layer caching and Cargo incremental compilation for maximum speed.

## File Structure

```
Docker/
├── Dockerfile.base          # Multi-stage build for base image (cargo-chef)
├── Dockerfile               # Multi-stage build for colony image (minimal runtime)
├── build-and-push-base.sh  # Script to build and push base image (slow)
├── build-and-push-colony.sh # Script to build and push colony image (fast)
├── .dockerignore           # Files to exclude from Docker context
└── README.md               # Usage documentation
```

## Base Image Build (`Dockerfile.base`)

### Stages

1. **Chef Stage** (`FROM lukemathwalker/cargo-chef:latest-rust-1.87-alpine AS chef`)
   - Target platform: `linux/amd64` (explicitly specified in build command)
   - Install build dependencies (musl-dev, openssl-dev, pkgconfig, git)
   - Set working directory to `/app`
   - Set `CARGO_TARGET_DIR=/app/target` for consistency

2. **Planner Stage** (`FROM chef AS planner`)
   - Copy `Cargo.toml` and `Cargo.lock` (root workspace files)
   - Copy individual crate `Cargo.toml` files (backend, coordinator, shared)
   - **CRITICAL: Ensure GUI crate is explicitly excluded in Cargo.toml workspace manifest:**
     - Workspace must have: `exclude = ["crates/gui"]` or use `members = [...]` without GUI
     - This prevents Cargo from loading GUI manifest and invalidating dependency layers
   - Create minimal source stubs for all crates (so cargo-chef can parse dependencies)
   - Run `cargo chef prepare --recipe-path recipe.json` to generate dependency recipe
   - Output: `recipe.json` containing all dependency information

3. **Builder Stage** (`FROM chef AS builder`)
   - Copy `recipe.json` from planner stage
   - Set `RUSTFLAGS="-C link-arg=-s"` to strip symbols from final binaries (not dependencies)
   - Run `cargo chef cook --release --recipe-path recipe.json` to compile all dependencies
   - This stage compiles only dependencies, not application code
   - **CRITICAL: Do NOT strip `.rlib` files or dependency artifacts** - this corrupts Cargo metadata and breaks incremental compilation
   - Final image contains: Rust toolchain, compiled dependencies (with full metadata) in `/app/target`, build environment

### Key Features

- Uses cargo-chef for efficient dependency caching
- Separates dependency analysis (planner) from compilation (builder)
- Excludes GUI crate to avoid platform-specific issues (must be explicit in Cargo.toml)
- All dependencies pre-compiled and ready for reuse
- **CRITICAL: Dependency artifacts (`.rlib`, `.o` files) are NOT stripped** - only final binaries are stripped in colony image
- Uses `RUSTFLAGS="-C link-arg=-s"` for symbol stripping (affects final binaries only)
- Target directory preserved for incremental builds with full Cargo metadata
- When `Cargo.toml` or `Cargo.lock` changes, only the recipe and dependency compilation layers are invalidated
- Recipe generation is fast (only parses manifests, doesn't compile)
- Dependency compilation is cached separately from application code

### Cargo-Chef Workflow

The Dockerfile uses cargo-chef's three-stage approach:

1. **Chef stage**: Base image with cargo-chef tool installed
2. **Planner stage**: Analyzes `Cargo.toml` files and generates `recipe.json` (fast, cached unless manifests change)
3. **Builder stage**: Compiles dependencies based on recipe (slow, cached unless recipe changes)

This ensures that:
- When only source code changes, the base image's dependency layers remain cached
- When `Cargo.toml` or `Cargo.lock` changes, only the recipe and dependency compilation need to be regenerated
- Dependency compilation is completely separate from application code compilation

## Colony Image Build (`Dockerfile`)

### Stages

1. **Builder Stage** (`FROM distributed-colony-base:latest AS builder`)
   - Target platform: `linux/amd64` (explicitly specified in build command)
   - Uses local base image (never pulls from ECR)
   - **CRITICAL: Copy `/app/target` from base image FIRST** before using cache mounts:
     - `COPY --from=distributed-colony-base:latest /app/target /app/target`
     - This populates the cache mount with pre-compiled dependencies from base image
     - BuildKit cache mounts do NOT include base image filesystem, so explicit copy is required
   - Copies source code (`crates/`, `Cargo.toml`, `Cargo.lock`)
   - Excludes GUI from workspace (matches base image, must be explicit in Cargo.toml)
   - **Uses Docker BuildKit cache mount** to persist compiled dependencies:
     - `--mount=type=cache,target=/app/target` - Compiled artifacts cache (populated from base image)
   - Builds binaries: `cargo build --release --bin backend --bin coordinator --offline`
   - Strips **only final binaries**: `strip target/release/backend target/release/coordinator`
   - Uses `RUSTFLAGS="-C link-arg=-s"` for symbol stripping (affects final binaries only)

2. **Runtime Stage** (`FROM gcr.io/distroless/cc AS runtime`)
   - Target platform: `linux/amd64` (inherited from builder stage)
   - Minimal distroless image (C runtime + CA certificates only)
   - Copies stripped binaries from builder stage
   - Sets environment variables (for reference, not used by distroless)
   - Exposes ports 8082 (backend) and 8083 (coordinator)
   - Default CMD: `["/usr/local/bin/backend", "0.0.0.0", "8082", "aws"]`

### Key Features

- **Copies `/app/target` from base image** before using cache mounts to ensure dependency reuse
- **Uses Docker BuildKit cache mount** for `/app/target` to persist compiled dependencies across builds
- Even if Cargo's incremental compilation invalidates its cache, Docker cache mounts preserve compiled artifacts
- Uses `--offline` flag to prevent network fetches (registry/git caches from base image)
- Minimal runtime image (distroless/cc)
- Both binaries included (backend and coordinator)
- **Only final binaries are stripped** - dependency artifacts preserve full Cargo metadata
- Fast builds (< 2 minutes) by reusing cached dependencies from Docker cache mounts

## Build Scripts

### `build-and-push-base.sh`

**Purpose**: Build and push base image to ECR (slow operation)

**Behavior**:
1. Validate AWS CLI configuration
2. Get AWS account ID and region (default: eu-west-1)
3. Construct ECR URI: `${accountId}.dkr.ecr.${region}.amazonaws.com/distributed-colony-base:latest`
4. Create ECR repository if it doesn't exist (with lifecycle policy)
5. Authenticate Docker with ECR
6. Build base image locally using `Dockerfile.base`:
   - Tag as `distributed-colony-base:latest` (for local use)
   - Use `docker buildx build --platform linux/amd64 --load`
   - Build from parent directory (context: `..`)
7. Tag and push to ECR:
   - Tag local image with ECR URI
   - Push to ECR
8. Print timing information and success message

**Configuration**:
- `AWS_REGION` (default: eu-west-1)
- `ECR_BASE_REPOSITORY` (default: distributed-colony-base)
- `BASE_IMAGE_TAG` (default: latest)

**Error Handling**:
- Exit if AWS CLI not installed
- Exit if AWS credentials invalid
- Exit if Docker buildx not available
- Exit if build fails

**Output**:
- Colored status messages (INFO, WARNING, ERROR)
- Build and push timing
- Final ECR URI

### `build-and-push-colony.sh`

**Purpose**: Build and push colony image to ECR (fast operation)

**Behavior**:
1. Validate AWS CLI configuration
2. Get AWS account ID and region (default: eu-west-1)
3. Construct ECR URIs:
   - Colony: `${accountId}.dkr.ecr.${region}.amazonaws.com/distributed-colony:latest`
4. **Check for local base image**:
   - Check if `distributed-colony-base:latest` exists locally
   - If not found, print error and exit (NEVER pull from ECR)
   - Error message: "Base image 'distributed-colony-base:latest' not found locally. Please build it first: ./build-and-push-base.sh"
5. Create ECR repository if it doesn't exist (with lifecycle policy)
6. Authenticate Docker with ECR
7. Build colony image locally:
   - Use `DOCKER_BUILDKIT=1 docker build --platform linux/amd64` (BuildKit required for cache mounts)
   - Pass `BASE_IMAGE=distributed-colony-base:latest` as build arg
   - Tag with ECR URI
   - Build from parent directory (context: `..`)
   - BuildKit cache mounts will automatically persist compiled dependencies across builds
8. Push colony image to ECR
9. Print timing information and success message

**Configuration**:
- `AWS_REGION` (default: eu-west-1)
- `ECR_REPOSITORY` (default: distributed-colony)
- `IMAGE_TAG` (default: latest)
- `BUILD_VERSION` (auto-generated timestamp)

**Error Handling**:
- Exit if AWS CLI not installed
- Exit if AWS credentials invalid
- Exit if Docker buildx not available
- Exit if base image not found locally (NEVER pull from ECR)
- Exit if build fails

**Output**:
- Colored status messages (INFO, WARNING, ERROR)
- Build and push timing
- Final ECR URI
- Reminder to deploy CDK stack

## ECR Repository Configuration

### Base Repository (`distributed-colony-base`)

- **Lifecycle Policy**: Expire images older than 30 days
- **Scanning**: Enabled on push
- **Encryption**: AES256
- **Tags**: `latest` (and optionally versioned tags)

### Colony Repository (`distributed-colony`)

- **Lifecycle Policy**: Expire images older than 30 days
- **Scanning**: Enabled on push
- **Encryption**: AES256
- **Tags**: `latest` (used by CDK)

## CDK Integration

### Image URI Format

CDK expects the colony image at:
```
${accountId}.dkr.ecr.${region}.amazonaws.com/distributed-colony:latest
```

This matches the output of `build-and-push-colony.sh`.

### User Data

CDK's `UserDataBuilder` pulls the image from ECR and runs:
- Backend: `docker run ... /usr/local/bin/backend <host> <port> aws`
- Coordinator: `docker run ... /usr/local/bin/coordinator aws`

The colony image supports both commands via CMD override.

## Build Workflow

### Initial Setup (One Time)

```bash
# 1. Build and push base image (slow, ~5-15 minutes)
cd Docker
./build-and-push-base.sh

# Base image is now:
# - Available locally as: distributed-colony-base:latest
# - Available in ECR as: <account>.dkr.ecr.<region>.amazonaws.com/distributed-colony-base:latest
```

### Iterative Development (Fast)

```bash
# 2. Make code changes to Rust source files

# 3. Build and push colony image (fast, ~30 seconds - 2 minutes)
./build-and-push-colony.sh

# Colony image is now:
# - Available in ECR as: <account>.dkr.ecr.<region>.amazonaws.com/distributed-colony:latest
# - Ready for CDK deployment
```

### When Dependencies Change

```bash
# If Cargo.toml or Cargo.lock changes, rebuild base image
./build-and-push-base.sh

# Then continue with colony builds as normal
./build-and-push-colony.sh
```

## Performance Characteristics

### Base Image Build
- **Time**: 5-15 minutes (first build or after dependency changes)
- **Network**: Minimal (only ECR push)
- **Disk**: ~1-2GB (image size)
- **CPU**: High (compiling all dependencies)

### Colony Image Build
- **Time**: 30 seconds - 2 minutes (typical)
- **Network**: Minimal (only ECR push)
- **Disk**: ~20-50MB (final image size)
- **CPU**: Low (only compiles application code, reuses dependencies)

## Error Scenarios

### Base Image Not Found Locally

**Scenario**: Running `build-and-push-colony.sh` without base image

**Behavior**:
```
[ERROR] Base image 'distributed-colony-base:latest' not found locally!
[INFO] Please build the base image first:
[INFO]   ./build-and-push-base.sh
```

**Action**: User must run `build-and-push-base.sh` first

### AWS Credentials Invalid

**Scenario**: AWS CLI not configured or credentials expired

**Behavior**:
```
[ERROR] AWS CLI is not configured or credentials are invalid
[INFO] Please run: aws configure
```

**Action**: User must configure AWS CLI

### Build Failure

**Scenario**: Docker build fails (compilation error, etc.)

**Behavior**: Script exits with error code, Docker error message displayed

**Action**: User must fix the issue and retry

## Environment Variables

### Build Scripts

- `AWS_REGION`: AWS region (default: eu-west-1)
- `ECR_REPOSITORY`: Colony repository name (default: distributed-colony)
- `ECR_BASE_REPOSITORY`: Base repository name (default: distributed-colony-base)
- `IMAGE_TAG`: Colony image tag (default: latest)
- `BASE_IMAGE_TAG`: Base image tag (default: latest)
- `BUILD_VERSION`: Build version string (auto-generated timestamp in colony script)

### Container Runtime

- `SERVICE_TYPE`: Service type (backend/coordinator) - for reference only
- `COORDINATOR_HOST`: Coordinator hostname (default: localhost)
- `COORDINATOR_PORT`: Coordinator port (default: 8083)
- `BACKEND_HOST`: Backend bind address (default: 0.0.0.0)
- `BACKEND_PORT`: Backend port (default: 8082)
- `BUILD_VERSION`: Build version string

## Security Considerations

1. **Distroless Runtime**: Minimal attack surface (no shell, no package manager)
2. **Non-root User**: Not applicable (distroless doesn't support users, but runs as non-root by default)
3. **Image Scanning**: Enabled on ECR push
4. **Encryption**: ECR repositories use AES256 encryption
5. **Lifecycle Policies**: Automatic cleanup of old images

## Future Enhancements

1. **Version Tagging**: Support semantic versioning for images
2. **Build Cache**: Use ECR as build cache backend (optional)
3. **CI/CD Integration**: GitHub Actions workflow for automated builds
4. **Image Signing**: Sign images with Cosign for supply chain security

Note: Multi-arch support is explicitly excluded per design principle #1 (AMD64 only).

## Testing Checklist

- [ ] Base image builds successfully from clean state
- [ ] Base image pushes to ECR successfully
- [ ] Colony image builds successfully with base image present
- [ ] Colony image fails with clear error if base image missing
- [ ] Colony image pushes to ECR successfully
- [ ] Colony image size is minimal (<50MB)
- [ ] CDK can pull and deploy colony image
- [ ] Backend binary runs correctly in container
- [ ] Coordinator binary runs correctly in container
- [ ] Build times are acceptable (colony < 2 minutes)
- [ ] Layer caching works (subsequent builds are faster)
- [ ] ECR repositories created with correct policies
- [ ] Lifecycle policies expire old images correctly

## Critical Implementation Requirements

The following three fixes are **REQUIRED** for the implementation to meet the requirements:

### Fix 1: Copy Base Image Target Directory Before Cache Mounts

**Problem**: BuildKit cache mounts do NOT include the base image filesystem. Without explicitly copying `/app/target` from the base image, the colony build will not reuse pre-compiled dependencies.

**Solution**: Add this line at the start of the colony builder stage (before any cache mounts):
```dockerfile
COPY --from=distributed-colony-base:latest /app/target /app/target
```

This ensures the base-image dependency artifacts populate the cache mount and are reused.

### Fix 2: Do NOT Strip Dependency Artifacts in Base Image

**Problem**: Stripping `.rlib` files or dependency artifacts in the base image can:
- Corrupt Cargo metadata
- Break incremental compilation
- Force full recompilation in the colony image

**Solution**: 
- **Base Image**: Do NOT strip `.rlib`, `.o`, or any dependency artifacts
- **Colony Image**: Strip ONLY the final binaries (backend and coordinator)

### Fix 3: Explicitly Exclude GUI Crate in Workspace Manifest

**Problem**: If GUI crate is not explicitly excluded in `Cargo.toml`, Cargo may still load the GUI manifest, invalidating dependency layers.

**Solution**: Ensure `Cargo.toml` workspace manifest explicitly excludes GUI:
```toml
[workspace]
members = ["crates/backend", "crates/coordinator", "crates/shared"]
exclude = ["crates/gui"]
```

Or use `members = [...]` without including `crates/gui`.

