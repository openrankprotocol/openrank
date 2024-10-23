# ComputeManager Smart Contract

## Deployment Instructions

1. **Navigate to the Project Directory**
   ```sh
   cd compute-manager-smart-contract
   ```

2. **Build the Smart Contract**
   ```sh
   forge build
   ```

3. **(Optional) Configure Environment**
   - Create a `.env` file by copying `.env.example` and add your private key.  
     This key is used for submitting the deployment transaction to the blockchain.

4. **(Optional) Update Addresses**
   - Modify the addresses of `submitters`, `computers`, and `verifiers` in `DeployComputeManager.s.sol` if needed.

5. **Simulate Deployment**
   ```sh
   forge script script/DeployComputeManager.s.sol
   ```
   - This simulation provides deployment guarantees and gas cost estimates.  
   - **Note**: Ensure the `.env` file is present.

6. **Deploy to Polygon Testnet**
   - To deploy on a different network (e.g., Ethereum mainnet), adjust the `rpc-url` accordingly.
   ```sh
   forge script script/DeployComputeManager.s.sol --rpc-url https://rpc-amoy.polygon.technology/ --broadcast --optimize --optimizer-runs 4000
   ```

7. **Update Configuration**
   - Copy the deployed contract address into `smart-contract-client/config.toml` for use in the smart contract client.

