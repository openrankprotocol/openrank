// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract ComputeManager {
    struct Signature {
        bytes32 s;
        bytes32 r;
        uint8 r_id;
    }

    // Whitelisted addresses
    mapping(address => bool) public submitters;
    mapping(address => bool) public computers;
    mapping(address => bool) public verifiers;

    // computeAssignTxHash => computeCommitmentTxHash => computeRootHash
    mapping(bytes32 => mapping(bytes32 => bytes32)) public computeRootHashes;
    // [computeAssignTxHash | computeCommitmentTxHash | computeVerificationTxHash] => bool
    mapping(bytes32 => bool) public hasTx;

    // Events
    event ComputeCommitted(bytes32 txHash, address indexed computer);
    event ComputeVerified(bytes32 txHash, address indexed verifier);

    modifier onlySubmitter {
        require(submitters[msg.sender], "Only submitters can call this function");
        _;
    }

    // Initialize the contract with whitelisted addresses
    constructor(address[] memory _submitters, address[] memory _computers, address[] memory _verifiers) {
        for (uint256 i = 0; i < _submitters.length; i++) {
            submitters[_submitters[i]] = true;
        }

        for (uint256 i = 0; i < _computers.length; i++) {
            computers[_computers[i]] = true;
        }

        for (uint256 i = 0; i < _verifiers.length; i++) {
            verifiers[_verifiers[i]] = true;
        }
    }

    // Computer submits a ComputeCommitment txHash with computeRootHash
    function submitComputeCommitment(
        bytes32 computeAssignTxHash,
        bytes32 computeCommitTxHash,
        bytes32 computeRootHash,
        Signature calldata sig
    ) external onlySubmitter {
        address signer = recoverSigner(computeCommitTxHash, sig);
        require(computers[signer], "Computer not whitelisted");

        // save `computeRootHash`
        computeRootHashes[computeAssignTxHash][
            computeCommitTxHash
        ] = computeRootHash;

        hasTx[computeAssignTxHash] = true;
        hasTx[computeCommitTxHash] = true;

        emit ComputeCommitted(computeCommitTxHash, signer);
    }

    // Verifier submits ComputeVerification txHash with signature
    function submitComputeVerification(
        bytes32 computeVerifyTxHash,
        bytes32 computeAssignTxHash,
        Signature calldata sig
    ) external onlySubmitter {
        address signer = recoverSigner(computeVerifyTxHash, sig);
        require(verifiers[signer], "Verifier not whitelisted");

        require(
            hasTx[computeAssignTxHash],
            "Matching ComputeAssignment TX missing"
        );

        hasTx[computeVerifyTxHash] = true;

        emit ComputeVerified(computeVerifyTxHash, signer);
    }

    // Recover signer from the provided hash and signature
    function recoverSigner(
        bytes32 messageHash,
        Signature calldata signature
    ) internal pure returns (address) {
        (uint8 v, bytes32 r, bytes32 s) = (
            signature.r_id + 27,
            signature.r,
            signature.s
        );
        address signer = ecrecover(messageHash, v, r, s);
        require(signer != address(0), "Invalid signature");
        return signer;
    }
}
