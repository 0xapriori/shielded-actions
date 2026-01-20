# Build stage - use nightly for edition2024 support
FROM rust:latest AS build

WORKDIR /app

# Install rustup and switch to nightly
RUN rustup default nightly

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Copy everything needed for the build
COPY Cargo.toml Cargo.lock* ./
COPY circuits ./circuits
COPY src ./src

# Build the application
RUN cargo build --release --bin shielded-prover

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary
COPY --from=build /app/target/release/shielded-prover /app/shielded-prover

# Set environment
ENV PORT=3001

# Run the prover service
CMD ["/app/shielded-prover"]
