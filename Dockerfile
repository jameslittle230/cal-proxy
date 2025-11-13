FROM --platform=linux/amd64 rust:1.91-slim AS builder

WORKDIR /usr/src/app
COPY . .

# Install OpenSSL development packages
RUN apt-get update && apt-get install -y openssl libssl-dev pkg-config

# Build the application
RUN cargo build --release

# Runtime stage
FROM --platform=linux/amd64 debian:bookworm-slim

# Install dependencies
RUN apt-get update && apt-get install -y \
    openssl \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY --from=builder /usr/src/app/target/release/cal-proxy /usr/local/bin/cal-proxy

# Set environment variables
ENV PORT=3000
ENV DEFAULT_URL="https://example.com/calendar"

# Expose the port
EXPOSE 3000

# Run the binary
CMD ["cal-proxy"]
