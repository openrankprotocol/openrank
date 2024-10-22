# ComputeManager smart contract

## How to deploy
- Go to `compute-manager-smart-contract`
```
cd compute-manager-smart-contract
```

- Build the smart contract
```
forge build
```

- (Optional) Create `.env` file by copying the `.env.example` & add the prvate key
The wallet related to private key is used for submitting the deployment transaction to blockchain.

- (Optional) Update the addresses of `submitters`, `computers` and `verifiers` in `DeployComputeManager.s.sol` file, if you want

- Run the simulation before real deployment
```
forge script script/DeployComputeManager.s.sol
```
This simulation provides you with deployment guarantee + gas cost estimation.
NOTE: This simulation requires the `.env`.

- Run the following command to deploy the contract into Polygon testnet
If you wanna deploy the contract to other network(e.g. Ethereum mainnet), you need to change the `rpc-url` accordingly.
```
forge script script/DeployComputeManager.s.sol --rpc-url https://rpc-amoy.polygon.technology/ --broadcast --optimize --optimizer-runs 4000
```

- Copy the contract address into `smart-contract-client/config.toml` file for use in smart contract client
