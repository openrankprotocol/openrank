# Start from the latest official Rust image
FROM rust:latest as builder

# Set the working directory
WORKDIR /app

# Copy the entire workspace
COPY . .

# Build the verifier binary
RUN cargo build --release --package openrank-verifier

# Check if the binary is built successfully
RUN test -f /app/target/release/openrank-verifier

# Start a new stage from debian:bullseye-slim
FROM debian:bullseye-slim

# Copy the build artifact from the build stage
COPY --from=builder /app/target/release/openrank-verifier /usr/local/bin/verifier

# Set the startup command to run the binary
CMD ["verifier"]
