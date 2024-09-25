// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract JobManager {
    // Roles and whitelists
    mapping(address => bool) public verifiers;

    struct Signature {
        bytes32 s;
        bytes32 r;
        uint8 r_id;
    }

    mapping(bytes32 => bytes32) public computeRootHashes;
    mapping(bytes32 => bool) public hasTxHash;

    // Events
    event JobCommitted(bytes32 txHash);
    event JobVerified(bytes32 txHash, address indexed verifier);

    // Initialize the contract with whitelisted addresses
    constructor(address[] memory _verifiers) {
        for (uint256 i = 0; i < _verifiers.length; i++) {
            verifiers[_verifiers[i]] = true;
        }
    }

    // Computer submits a CreateCommitment txHash with computeRootHash
    function submitCreateCommitment(bytes32 txHash, bytes32 computeRootHash) external {
        // save `computeRootHash`
        computeRootHashes[txHash] = computeRootHash;
        hasTxHash[txHash] = true;

        emit JobCommitted(txHash);
    }

    // Verifier submits JobVerification txHash with signature
    function submitJobVerification(bytes32 txHash, Signature memory sig) external {
        address signer = recoverSigner(txHash, sig);
        require(verifiers[signer], "Verifier not whitelisted");

        // save `txHash` as seen
        hasTxHash[txHash] = true;

        emit JobVerified(txHash, signer);
    }

    // Recover signer from the provided hash and signature
    function recoverSigner(bytes32 messageHash, Signature memory signature) internal pure returns (address) {
        (uint8 v, bytes32 r, bytes32 s) = (signature.r_id + 27, signature.r, signature.s);
        address signer = ecrecover(messageHash, v, r, s);
        require(signer != address(0), "Invalid signature");
        return signer;
    }
}
