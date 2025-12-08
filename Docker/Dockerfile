# Multi-stage Docker build for DistributedColony using cargo-chef
# Optimized for minimal final image size using distroless runtime

# Stage 1: Planner stage (cargo-chef prepare)
# Note: This stage is typically handled by the base image, but we reference it here for clarity
ARG BASE_IMAGE=distributed-colony-base:latest

# Stage 2: Builder stage (cargo-chef cook + cargo build --release)
# Uses the base image which already has dependencies pre-compiled via cargo-chef
FROM ${BASE_IMAGE} AS builder

ARG BUILD_VERSION="unknown"
ENV BUILD_VERSION=${BUILD_VERSION}

# Add Rust build optimizations for smaller binary size
# -C link-arg=-s: Strip symbols from the final binary
# Note: LTO cannot be enabled via RUSTFLAGS here due to conflict with embed-bitcode=no in base image
# LTO can be enabled in Cargo.toml [profile.release] section if desired (requires embed-bitcode=true)
ENV RUSTFLAGS="-C link-arg=-s"

# Copy the full source code
COPY crates/ ./crates/
COPY Cargo.toml Cargo.lock ./

# Exclude GUI from workspace (same as base image) to match the dependency set
# This ensures Cargo only tries to build what we have dependencies for
RUN sed -i '/crates\/gui/d' Cargo.toml

# Set source file timestamps to be older than compiled dependencies
# This prevents Cargo's incremental compilation from invalidating the dependency cache
# The base image has dependencies compiled, so we want Cargo to reuse them
RUN find crates/ -type f -name "*.rs" -exec touch -t 202001010000.00 {} \; 2>/dev/null || true

# Build the application binaries with release optimizations
# Cargo should reuse pre-compiled dependencies from /app/target in the base image
# The target directory from the base image contains all compiled dependencies
# Using --offline to ensure Cargo doesn't try to fetch (dependencies already compiled)
# Cargo's incremental compilation will only rebuild what actually changed
# LTO is enabled via Cargo.toml release profile settings if configured
RUN cargo build --release --bin backend --bin coordinator --offline

# Strip the binaries to further reduce size (additional to RUSTFLAGS)
# This removes debug symbols and other metadata
RUN strip /app/target/release/backend /app/target/release/coordinator || true

# Stage 3: Runtime stage (minimal distroless image)
# Using distroless/cc which provides only the C runtime library and CA certificates
# This results in a dramatically smaller image (~10-40 MB vs ~270 MB)
FROM gcr.io/distroless/cc AS runtime

ARG BUILD_VERSION="unknown"
ENV BUILD_VERSION=${BUILD_VERSION}

# Set default environment variables for compatibility
# Note: In distroless, shell scripts cannot be used, so these are for reference
# The actual binary selection must be done via CMD override
ENV SERVICE_TYPE=backend
ENV COORDINATOR_HOST=localhost
ENV COORDINATOR_PORT=8083
ENV BACKEND_HOST=0.0.0.0
ENV BACKEND_PORT=8082

# Copy both binaries from builder stage
# Both are needed since the image can run either backend or coordinator
COPY --from=builder /app/target/release/backend /usr/local/bin/backend
COPY --from=builder /app/target/release/coordinator /usr/local/bin/coordinator

# Expose ports (will be overridden by environment variables in deployment)
EXPOSE 8082 8083

# Default entrypoint: run backend with default arguments
# To run coordinator instead, override CMD: docker run ... /usr/local/bin/coordinator aws
# To run backend with custom args: docker run ... /usr/local/bin/backend <host> <port> aws
ENTRYPOINT ["/usr/local/bin/backend"]
CMD ["0.0.0.0", "8082", "aws"]
