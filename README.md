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
- **DA(Data Access)** : DA wrapper for openrank codebase (Located in `./da`)
- **Openrank-SDK** : Handles the build & run of RPC client for OpenRank (Located in `./openrank-sdk`)

## Getting Started

To get started with the OpenRank project, follow these steps:

1. Clone the repository: `git clone https://github.com/openrankprotocol/openrank.git`
2. Install `rustup` via `./init.sh`
3. Build the docker image: `./generate-docker-compose.sh`

## Dependencies

The OpenRank project relies on the following dependencies:

- libp2p
- tokio
- Rust

## License

The OpenRank project is licensed under MIT License.

## Contributing

Contributions to the OpenRank project are welcome. Please submit pull requests or issues to the repository.

## Contact

For any questions or support, please contact us at [hello@karma3labs.com](mailto:hello@karma3labs.com).

