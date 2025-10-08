# Multi-stage Docker build for DistributedColony
# Stage 1: Build the Rust application
FROM rust:1.87-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    pkgconfig \
    git

# Set working directory
WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Build the application in release mode
RUN cargo build --release --bin backend --bin coordinator

# Stage 2: Create minimal runtime image
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    libgcc \
    procps \
    && rm -rf /var/cache/apk/*

# Create non-root user
RUN addgroup -g 1001 -S appgroup && \
    adduser -u 1001 -S appuser -G appgroup

# Set working directory
WORKDIR /app

# Copy binaries from builder stage
COPY --from=builder /app/target/release/backend /usr/local/bin/
COPY --from=builder /app/target/release/coordinator /usr/local/bin/

# Create startup script
RUN echo '#!/bin/sh' > /usr/local/bin/start.sh && \
    echo 'set -e' >> /usr/local/bin/start.sh && \
    echo '' >> /usr/local/bin/start.sh && \
    echo '# Default values' >> /usr/local/bin/start.sh && \
    echo 'SERVICE_TYPE=${SERVICE_TYPE:-"backend"}' >> /usr/local/bin/start.sh && \
    echo 'COORDINATOR_HOST=${COORDINATOR_HOST:-"localhost"}' >> /usr/local/bin/start.sh && \
    echo 'COORDINATOR_PORT=${COORDINATOR_PORT:-"8083"}' >> /usr/local/bin/start.sh && \
    echo 'BACKEND_HOST=${BACKEND_HOST:-"0.0.0.0"}' >> /usr/local/bin/start.sh && \
    echo 'BACKEND_PORT=${BACKEND_PORT:-"8082"}' >> /usr/local/bin/start.sh && \
    echo 'DEPLOYMENT_MODE=${DEPLOYMENT_MODE:-"localhost"}' >> /usr/local/bin/start.sh && \
    echo '' >> /usr/local/bin/start.sh && \
    echo 'case "$SERVICE_TYPE" in' >> /usr/local/bin/start.sh && \
    echo '    "backend")' >> /usr/local/bin/start.sh && \
    echo '        echo "Starting backend service on $BACKEND_HOST:$BACKEND_PORT in $DEPLOYMENT_MODE mode"' >> /usr/local/bin/start.sh && \
    echo '        # Backend needs hostname, port, and deployment_mode as arguments' >> /usr/local/bin/start.sh && \
    echo '        exec /usr/local/bin/backend $BACKEND_HOST $BACKEND_PORT $DEPLOYMENT_MODE' >> /usr/local/bin/start.sh && \
    echo '        ;;' >> /usr/local/bin/start.sh && \
    echo '    "coordinator")' >> /usr/local/bin/start.sh && \
    echo '        echo "Starting coordinator service on port $COORDINATOR_PORT"' >> /usr/local/bin/start.sh && \
    echo '        exec /usr/local/bin/coordinator' >> /usr/local/bin/start.sh && \
    echo '        ;;' >> /usr/local/bin/start.sh && \
    echo '    *)' >> /usr/local/bin/start.sh && \
    echo '        echo "Unknown service type: $SERVICE_TYPE"' >> /usr/local/bin/start.sh && \
    echo '        echo "Valid options: backend, coordinator"' >> /usr/local/bin/start.sh && \
    echo '        exit 1' >> /usr/local/bin/start.sh && \
    echo '        ;;' >> /usr/local/bin/start.sh && \
    echo 'esac' >> /usr/local/bin/start.sh

# Make startup script executable
RUN chmod +x /usr/local/bin/start.sh

# Create health check script
RUN echo '#!/bin/sh' > /usr/local/bin/healthcheck.sh && \
    echo '# Simple health check - check if the process is running' >> /usr/local/bin/healthcheck.sh && \
    echo 'if [ "$SERVICE_TYPE" = "backend" ]; then' >> /usr/local/bin/healthcheck.sh && \
    echo '    # Check if backend process is running' >> /usr/local/bin/healthcheck.sh && \
    echo '    pgrep -f "/usr/local/bin/backend" > /dev/null || exit 1' >> /usr/local/bin/healthcheck.sh && \
    echo 'elif [ "$SERVICE_TYPE" = "coordinator" ]; then' >> /usr/local/bin/healthcheck.sh && \
    echo '    # Check if coordinator process is running' >> /usr/local/bin/healthcheck.sh && \
    echo '    pgrep -f "/usr/local/bin/coordinator" > /dev/null || exit 1' >> /usr/local/bin/healthcheck.sh && \
    echo 'fi' >> /usr/local/bin/healthcheck.sh && \
    echo 'exit 0' >> /usr/local/bin/healthcheck.sh

# Make health check script executable
RUN chmod +x /usr/local/bin/healthcheck.sh

# Prepare writable log directory and change ownership to non-root user
RUN mkdir -p /app/output/logs \
    && chown -R appuser:appgroup /usr/local/bin/backend /usr/local/bin/coordinator /usr/local/bin/start.sh /usr/local/bin/healthcheck.sh /app

# Switch to non-root user
USER appuser

# Expose ports (will be overridden by environment variables)
EXPOSE 8082 8083

# Add health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD /usr/local/bin/healthcheck.sh

# Set default environment variables
ENV SERVICE_TYPE=backend
ENV COORDINATOR_HOST=localhost
ENV COORDINATOR_PORT=8083
ENV BACKEND_HOST=0.0.0.0
ENV BACKEND_PORT=8082
ENV DEPLOYMENT_MODE=localhost

# Start the application
CMD ["/usr/local/bin/start.sh"]
