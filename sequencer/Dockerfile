# Start from the latest official Rust image
FROM rust:1.78.0 as builder
ENV RUST_LOG=info

# Set the working directory
WORKDIR /app

# Copy the entire workspace
COPY . .

# Build the sequencer binary
RUN cargo build --release --package openrank-sequencer

# Check if the binary is built successfully
RUN test -f /app/target/release/openrank-sequencer

# Start a new stage from debian:bullseye-slim
FROM ubuntu:22.04
RUN apt-get update && apt -y install curl build-essential && apt-cache policy libc6 && apt-get install libc6 && apt install -y librust-openssl-dev libssl-dev

# Copy the build artifact from the build stage
COPY --from=builder /app/target/release/openrank-sequencer /usr/local/bin/sequencer

# Set the startup command to run the binary
CMD ["sequencer", "/dns/sequencer/tcp/8000"]
