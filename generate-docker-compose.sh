#!/bin/bash

# Load environment variables from .env file
export $(grep -v '^#' .env | xargs)

# Default to 3 nodes if NODE_COUNT is not set
NODE_COUNT=${NODE_COUNT:-3}

# Starting ports
SEQUENCER_PORT=8000
BLOCK_BUILDER_PORT=9000
COMPUTER_PORT=10000
VERIFIER_START_PORT=11000

# Function to check if a port is available
function is_port_available() {
    ! nc -z localhost $1
}

# Function to find the next available port starting from a given port
function find_available_port() {
    local port=$1
    while ! is_port_available $port; do
        port=$((port + 1))
    done
    echo $port
}

# Start generating the docker-compose.yml file
cat <<EOF > docker-compose.yml

services:
EOF

# Add the sequencer service to docker-compose.yml
sequencer_port=$(find_available_port $SEQUENCER_PORT)
cat <<EOF >> docker-compose.yml
  sequencer:
    build:
      context: .
      dockerfile: sequencer/Dockerfile
    container_name: sequencer
    environment:
      - INSTANCE_NAME=sequencer
      - PORT=$sequencer_port
    ports:
      - "$sequencer_port:$sequencer_port"
    networks:
      - openrank-network
EOF

# Add the block-builder service to docker-compose.yml
block_builder_port=$(find_available_port $BLOCK_BUILDER_PORT)
cat <<EOF >> docker-compose.yml
  block-builder:
    build:
      context: .
      dockerfile: block-builder/Dockerfile
    container_name: block-builder
    environment:
      - INSTANCE_NAME=block-builder
      - PORT=$block_builder_port
    ports:
      - "$block_builder_port:$block_builder_port"
    networks:
      - openrank-network

EOF

# Append each verifier service to docker-compose.yml
for i in $(seq 1 $NODE_COUNT); do
    verifier_port=$(find_available_port $((VERIFIER_START_PORT + i)))
cat <<EOF >> docker-compose.yml
  verifier-$i:
    build:
      context: .
      dockerfile: verifier/Dockerfile
    container_name: verifier-$i
    environment:
      - INSTANCE_NAME=verifier-$i
      - PORT=$verifier_port
    ports:
      - "$verifier_port:$verifier_port"
    networks:
      - openrank-network

EOF
done

# Add the da service to docker-compose.yml
computer_port=$(find_available_port $COMPUTER_PORT)
cat <<EOF >> docker-compose.yml
  computer:
    build:
      context: .
      dockerfile: computer/Dockerfile
    container_name: computer
    environment:
      - INSTANCE_NAME=computer
      - PORT=$computer_port
    ports:
      - "$computer_port:$computer_port"
    networks:
      - openrank-network

networks:
    openrank-network:
        driver: bridge
EOF
