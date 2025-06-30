FROM rust:1.75 as builder

WORKDIR /app

# Copy all sensor code
COPY . .

# Build the sensor
RUN cargo build --release --bin jackbot-sensor

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/jackbot-sensor /app/jackbot-sensor

# Environment variables
ENV RUST_LOG=info
ENV REDIS_URL=redis://host.docker.internal:6379

# Run the sensor
CMD ["./jackbot-sensor", "binance", "--redis", "redis://host.docker.internal:6379"]