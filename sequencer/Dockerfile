# Start from the latest official Rust image
FROM rust:latest as builder

# Set the working directory
WORKDIR /app

# Copy the entire workspace
COPY . .

# Build the sequencer binary
RUN cargo build --release --package openrank-sequencer

# Check if the binary is built successfully
RUN test -f /app/target/release/openrank-sequencer

# Start a new stage from debian:bullseye-slim
FROM debian:bullseye-slim

# Copy the build artifact from the build stage
COPY --from=builder /app/target/release/openrank-sequencer /usr/local/bin/sequencer

# Set the startup command to run the binary
CMD ["sequencer"]
