use alloy::sol;

// Codegen from ABI file to interact with the contract.
sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    ComputeManager,
    "../out/ComputeManager.sol/ComputeManager.json"
);
