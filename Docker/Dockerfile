# Multi-stage Docker build for DistributedColony
# Optimized for minimal final image size using distroless runtime

ARG BASE_IMAGE=distributed-colony-base:latest

# Builder stage: Compile application binaries using pre-compiled dependencies
FROM ${BASE_IMAGE} AS builder

ARG BUILD_VERSION="unknown"
ENV BUILD_VERSION=${BUILD_VERSION}

# Ensure we use the same target directory and working directory as the base image
# This is critical for Cargo to recognize pre-compiled dependencies
WORKDIR /app
ENV CARGO_TARGET_DIR=/app/target

# Ensure OpenSSL development libraries and static libraries are available for linking
# The base image should have openssl-dev and openssl-libs-static, but we ensure it here too
RUN apk add --no-cache openssl-libs-static

# Add Rust build optimizations for smaller binary size
# -C link-arg=-s: Strip symbols from the final binary
ENV RUSTFLAGS="-C link-arg=-s"

# Copy Cargo files and crate manifests first to match base image structure exactly
COPY Cargo.toml Cargo.lock ./
COPY crates/backend/Cargo.toml crates/backend/Cargo.toml
COPY crates/coordinator/Cargo.toml crates/coordinator/Cargo.toml
COPY crates/shared/Cargo.toml crates/shared/Cargo.toml

# Exclude GUI from workspace (same as base image) to match the dependency set
# This must match the base image exactly
RUN sed -i '/crates\/gui/d' Cargo.toml

# Copy the full source code (this overwrites the stub files from base image)
COPY crates/ ./crates/

# Build the application binaries with release optimizations
# CRITICAL: Copy /app/target from base image into cache mount location
# BuildKit cache mounts do NOT include the base image filesystem
# We use a bind mount to access base image's target, then copy it into the cache mount
# This populates the cache mount with pre-compiled dependencies from base image
# Using --offline to ensure Cargo doesn't try to fetch (dependencies already in base image)
RUN --mount=type=cache,target=/app/target \
    --mount=type=bind,from=distributed-colony-base:latest,source=/app/target,target=/tmp/base-target,readonly \
    sh -c "if [ ! -d /app/target/release/deps ] || [ -z \"\$(ls -A /app/target/release/deps 2>/dev/null)\" ]; then echo 'Populating cache mount from base image...' && cp -a /tmp/base-target/* /app/target/ 2>/dev/null || true; fi" && \
    cargo build --release --bin backend --bin coordinator --offline

# Strip the binaries to further reduce size (additional to RUSTFLAGS)
# CRITICAL: Strip ONLY the final binaries, NOT dependency artifacts
# This removes debug symbols and other metadata from the final executables only
RUN strip /app/target/release/backend /app/target/release/coordinator || true

# Runtime stage: Minimal distroless image
# Using distroless/cc which provides only the C runtime library and CA certificates
# This results in a dramatically smaller image (~10-40 MB vs ~270 MB)
FROM gcr.io/distroless/cc AS runtime

ARG BUILD_VERSION="unknown"
ENV BUILD_VERSION=${BUILD_VERSION}

# Set default environment variables for compatibility
# Note: In distroless, shell scripts cannot be used, so these are for reference
# The actual binary selection must be done via CMD override
ENV SERVICE_TYPE=backend
ENV BACKEND_HOST=0.0.0.0
ENV RPC_PORT=8084
ENV HTTP_PORT=8085

# Copy both binaries from builder stage
# Both are needed since the image can run either backend or coordinator
COPY --from=builder /app/target/release/backend /usr/local/bin/backend
COPY --from=builder /app/target/release/coordinator /usr/local/bin/coordinator

# Expose ports (will be overridden by environment variables in deployment)
EXPOSE 8082 8083 8084 8085

# Default command: run backend with default arguments
# To run coordinator: docker run ... /usr/local/bin/coordinator 8082 8083 aws
# To run backend: docker run ... /usr/local/bin/backend <host> <rpc_port> <http_port> aws
# Note: Using CMD instead of ENTRYPOINT allows complete override
CMD ["/usr/local/bin/backend", "0.0.0.0", "8084", "8085", "aws"]
