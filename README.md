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

### Development Guide

To generate fresh keypair, run openrank-sdk command `generate-keypair`:
`cargo run -p openrank-sdk generate-keypair`

The output of this command will be a newly generated secret key and an address associated with this key:
```
SIGNING_KEY: fd0c684affeb0d4c8286917f71ad3bef81dc50cd2c1f83930e806d3a32833267
ADDRESS:     7880ffa45868ef04dd942ff1a9580ba70d18ec87
```

These secret_key should be generated for BlockBuilder, Computer and Verifier Node and placed in `.env` file for each node separately. `.env` file should be created inside the root folder of the crate, e.g. for BlockBuilder, it should be placed inside `./block-builder`.

Then, each `config.toml` file should be updated to specify the correct address for each whitelisted participant, whether its block builder, user or compute/verifier node.

Finally, trust and seed owners in each config file should be updated to specify the address of the user of OpenRank SDK, or more precisely the signer of outside TXs.

To run the nodes in dev mode:
- Block Builder: navigate to `./block-builder`, run `RUST_LOG=info cargo run --release`
- Computer: navigate to `./computer`, run `RUST_LOG=info cargo run --release`
- Verifier: navigate to `./verifier`, run `RUST_LOG=info cargo run --release`

## License

The OpenRank project is licensed under MIT License.

## Contributing

Contributions to the OpenRank project are welcome. Please submit pull requests or issues to the repository.

## Contact

For any questions or support, please contact us at [hello@karma3labs.com](mailto:hello@karma3labs.com).
