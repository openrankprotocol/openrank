# OpenRank

An implementation of reputation computer network


## Introduction

OpenRank is a decentralized platform for allowing the users build & use the reputation system. The project utilizes a peer-to-peer network, built using the libp2p library, to enable communication and data sharing between nodes.  
Visit our [site](https://openrank.com/) and read our [documentation](https://docs.openrank.com/) to learn more about the OpenRank protocol.

## Components

The OpenRank project consists of several components:

- **Sequencer** : Handles the sequencing of events and data publication to the network. (Located in `./sequencer`)
- **Block Builder** : Builds and publishes blocks to the network. (Located in `./block-builder`)
- **Computer** : Performs computations and generates results. (Located in `./computer`)
- **Verifier** : Responsible for verifying the integrity of data and ensuring the correctness of computations. (Located in `./verifier`)
- **Common** : Includes algorithm & data structure needed for the project.  (Located in `./common`)
- **DA(Data Availability)** : DA interface for openrank codebase (Located in `./da`)
- **Openrank-SDK** : Handles the build & run of RPC client for OpenRank (Located in `./openrank-sdk`)

## Getting Started

### Environment Setup
- Rust
- Git
- Docker

1. Install `rustup` via `./init.sh`
2. Install `git` via `https://git-scm.com/downloads`
3. Make sure [Docker](https://docker.com) is installed.

### Compiling
1. Clone the repository: `git clone https://github.com/openrankprotocol/openrank.git`
2. Build the docker image: `./generate-docker-compose.sh`

## License

The OpenRank project is licensed under MIT License.

## Contributing

Contributions to the OpenRank project are welcome. Please submit pull requests or issues to the repository.

## Contact

For any questions or support, please contact us at [hello@karma3labs.com](mailto:hello@karma3labs.com).

