# Build stage - Using rust:latest (1.88) for Rust 2024 support
FROM rust:latest AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /usr/src/jackbot

# Set up cargo config for proper feature resolution
RUN mkdir -p .cargo
RUN echo '[unstable]' > .cargo/config.toml && \
    echo 'edition2024 = true' >> .cargo/config.toml

# Copy entire project
COPY . .

# For Rust 2024, we might need to use cargo with specific flags
ENV CARGO_RESOLVER_EDITION_SPECIFIC_FEATURES=1

# Build the specific examples we need
# Try with edition2024 enabled first, fallback to stable features
RUN cargo build --release --example public_trades_streams || \
    cargo +nightly build --release --example public_trades_streams || \
    (sed -i 's/edition = "2024"/edition = "2021"/g' */Cargo.toml && \
     cargo build --release --example public_trades_streams)

RUN cargo build --release --example order_books_l2_streams || \
    cargo +nightly build --release --example order_books_l2_streams || \
    (sed -i 's/edition = "2024"/edition = "2021"/g' */Cargo.toml && \
     cargo build --release --example order_books_l2_streams)

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binaries from builder
COPY --from=builder /usr/src/jackbot/target/release/examples/public_trades_streams /usr/local/bin/
COPY --from=builder /usr/src/jackbot/target/release/examples/order_books_l2_streams /usr/local/bin/

# Create non-root user
RUN useradd -m -u 1001 jackbot

# Switch to non-root user
USER jackbot

# Default to trades stream
CMD ["/usr/local/bin/public_trades_streams"]