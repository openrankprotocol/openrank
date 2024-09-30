// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import "./RLPReader.sol";
import "./RLPEncode.sol";

contract ComputeManager {
    using RLPReader for bytes;
    using RLPReader for RLPReader.RLPItem;
    using RLPReader for RLPReader.Iterator;

    using RLPEncode for bytes;
    using RLPEncode for uint;

    // enum to store TX kind
    enum TxKind {
        TrustUpdate,
        SeedUpdate,
        ComputeRequest,
        ComputeAssignment,
        ComputeScores,
        ComputeCommitment,
        ComputeVerification,
        ProposedBlock,
        FinalisedBlock
    }

    // struct to store TX
    struct OpenrankTx {
        uint64 nonce;
        address from;
        address to;
        TxKind kind;
        bytes body;
        Signature signature;
        uint64 sequence_number;
    }

    struct Signature {
        bytes32 s;
        bytes32 r;
        uint8 r_id;
    }

    struct ComputeVerification {
        bytes32 compute_assignment_tx_hash;
        bool verification_result;
    }

    // Whitelisted addresses
    mapping(address => bool) public computers;
    mapping(address => bool) public verifiers;

    // computeAssignTxHash => computeCommitmentTxHash => computeRootHash
    mapping(bytes32 => mapping(bytes32 => bytes32)) public computeRootHashes;
    // [computeAssignTxHash | computeCommitmentTxHash | computeVerificationTxHash] => bool
    mapping(bytes32 => bool) public hasTx;

    // Events
    event ComputeCommitted(bytes32 txHash, address indexed computer);
    event ComputeVerified(bytes32 txHash, address indexed verifier);

    // Initialize the contract with whitelisted addresses
    constructor(address[] memory _computers, address[] memory _verifiers) {
        for (uint256 i = 0; i < _computers.length; i++) {
            computers[_computers[i]] = true;
        }

        for (uint256 i = 0; i < _verifiers.length; i++) {
            verifiers[_verifiers[i]] = true;
        }
    }

    // Computer submits a CreateCommitment txHash with computeRootHash
    function submitCreateCommitment(
        bytes32 computeAssignTxHash,
        bytes32 computeCommitTxHash,
        bytes32 computeRootHash,
        Signature memory sig
    ) external {
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
    function submitComputeVerification(OpenrankTx calldata _tx) external {
        // construct tx hash from transaction and check the signature
        bytes32 computeVerifyTxHash = getTxHash(_tx);

        Signature memory sig = _tx.signature;
        address signer = recoverSigner(computeVerifyTxHash, sig);
        require(verifiers[signer], "Verifier not whitelisted");

        // check the transaction kind & computeAssignmentTxHash
        require(
            _tx.kind == TxKind.ComputeVerification,
            "Expected ComputeVerification TX"
        );

        ComputeVerification
            memory computeVerification = decodeComputeVerification(_tx.body);

        require(
            hasTx[computeVerification.compute_assignment_tx_hash],
            "Matching ComputeAssignment TX missing"
        );

        // save TX in storage
        hasTx[computeVerifyTxHash] = true;

        emit ComputeVerified(computeVerifyTxHash, signer);
    }

    // Helper function to get the transaction hash from the OpenrankTx
    function getTxHash(
        OpenrankTx memory transaction
    ) internal pure returns (bytes32) {
        bytes[] memory _from = new bytes[](1);
        _from[0] = RLPEncode.encodeBytes(abi.encodePacked(transaction.from));

        bytes[] memory _to = new bytes[](1);
        _to[0] = RLPEncode.encodeBytes(abi.encodePacked(transaction.to));

        return
            keccak256(
                abi.encodePacked(
                    transaction.nonce,
                    RLPEncode.encodeList(_from),
                    RLPEncode.encodeList(_to),
                    RLPEncode.encodeUint(uint(transaction.kind)),
                    transaction.body
                )
            );
    }

    // Recover signer from the provided hash and signature
    function recoverSigner(
        bytes32 messageHash,
        Signature memory signature
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

    // Decode RLP bytes back into the `ComputeVerification` struct
    function decodeComputeVerification(
        bytes memory rlpData
    ) public pure returns (ComputeVerification memory) {
        RLPReader.RLPItem[] memory items = rlpData.toRlpItem().toList();
        return
            ComputeVerification({
                compute_assignment_tx_hash: bytes32(
                    items[0].toList()[0].toBytes()
                ),
                verification_result: items[1].toBoolean()
            });
    }
}
