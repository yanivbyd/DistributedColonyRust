# Multi-stage Docker build for DistributedColony
# Optimized for minimal final image size using a lightweight Alpine runtime

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
# NOTE: We always force a fresh link of the final binaries to ensure
# that no stale coordinator or backend executable can be reused from
# any previous target directory or base image artifact.
RUN rm -f /app/target/release/backend /app/target/release/coordinator || true && \
    cargo build --release --bin backend --bin coordinator

# Strip the binaries to further reduce size (additional to RUSTFLAGS)
# CRITICAL: Strip ONLY the final binaries, NOT dependency artifacts
# This removes debug symbols and other metadata from the final executables only
RUN strip /app/target/release/backend /app/target/release/coordinator || true

# Runtime stage: Minimal Alpine image (non-distroless)
# Using alpine which provides a small but fully functional Linux userspace (shell, package manager, etc.)
# This keeps the image small while allowing on-container debugging and tooling.
FROM alpine:3.20 AS runtime

ARG BUILD_VERSION="unknown"
ENV BUILD_VERSION=${BUILD_VERSION}

# Align runtime working directory with builder and volume mounts
# so that relative paths like "output/logs/..." resolve under /app,
# which is where /data/distributed-colony/output is mounted.
WORKDIR /app

# Install runtime dependencies (e.g., CA certificates for TLS)
RUN apk add --no-cache ca-certificates

# Set default environment variables for compatibility
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
