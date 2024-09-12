// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

// import "https://github.com/hamdiallam/solidity-rlp/blob/master/contracts/RLPReader.sol";
import "./RLPReader.sol";
import "./RLPEncode.sol";

contract JobManager {
    using RLPReader for bytes;
    using RLPReader for RLPReader.RLPItem;
    using RLPReader for RLPReader.Iterator;

    using RLPEncode for bytes;
    using RLPEncode for uint;

    // Roles and whitelists
    mapping(address => bool) public blockBuilders;
    mapping(address => bool) public computers;
    mapping(address => bool) public verifiers;

    // enum to store TX kind
    enum TxKind { 
        TrustUpdate,
        SeedUpdate,
        JobRunRequest,
        JobRunAssignment,
        CreateScores,
        CreateCommitment,
        JobVerification,
        ProposedBlock,
        FinalisedBlock
    }

    // struct to store TX
    struct OpenrankTx {
        uint64 nonce;
        bytes from; // assume it's address(bytes20)
        bytes to; // assume it's address(bytes20)
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

    struct JobRunRequest {
        uint64 domain_id;
        uint32 block_height;
        bytes32 job_id;
    }

    struct JobRunAssignment {
        bytes32 job_run_request_tx_hash;
        bytes20 assigned_compute_node;
        bytes20 assigned_verifier_node;
    }

    struct CreateCommitment {
        bytes32 job_run_assignment_tx_hash;
        bytes32 lt_root_hash;
        bytes32 compute_root_hash;
        bytes32[] scores_tx_hashes;
        bytes32[] new_trust_tx_hashes;
        bytes32[] new_seed_tx_hashes;
    }

    struct JobVerification {
        bytes32 job_run_assignment_tx_hash;
        bool verification_result;
    }

    // Store OpenrankTx with txHash as the key
    mapping(bytes32 => OpenrankTx) public txs;
    mapping(bytes32 => bool) public hasTx;

    // Events
    event JobRunRequested(bytes32 txHash, address blockBuilder);
    event JobAssigned(bytes32 txHash, address computer, address verifier);
    event JobCommitted(bytes32 txHash);
    event JobVerified(bytes32 txHash, bool isVerified, address verifier);

    // Modifier for whitelisted Block Builder
    modifier onlyBlockBuilder() {
        require(blockBuilders[msg.sender], "Not authorized as Block Builder");
        _;
    }

    // Modifier for whitelisted Computer
    modifier onlyComputer() {
        require(computers[msg.sender], "Not authorized as Computer");
        _;
    }

    // Modifier for whitelisted Verifier
    modifier onlyVerifier() {
        require(verifiers[msg.sender], "Not authorized as Verifier");
        _;
    }

    // Initialize the contract with whitelisted addresses
    constructor(address[] memory _blockBuilders, address[] memory _computers, address[] memory _verifiers) {
        for (uint256 i = 0; i < _blockBuilders.length; i++) {
            blockBuilders[_blockBuilders[i]] = true;
        }

        for (uint256 i = 0; i < _computers.length; i++) {
            computers[_computers[i]] = true;
        }

        for (uint256 i = 0; i < _verifiers.length; i++) {
            verifiers[_verifiers[i]] = true;
        }
    }

    // User sends JobRunRequest to Block Builder
    function sendJobRunRequest(OpenrankTx memory transaction) external {
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);

        Signature memory sig = transaction.signature;
        bytes memory signature = abi.encodePacked(sig.r, sig.s, sig.r_id);
        address signer = recoverSigner(txHash, signature);
        
        // check if signer is whitelisted user
        // require(signer == user, "Invalid user signature");

        // check the transaction kind & jobId
        require(transaction.kind == TxKind.JobRunRequest, "Invalid transaction kind");

        address _blockBuilder = address(convertBytesToBytes20(transaction.to));
        require(blockBuilders[_blockBuilder], "Assigned block builder is not whitelisted");

        JobRunRequest memory jobRunRequest = decodeJobRunRequest(transaction.body);

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobRunRequested(txHash, _blockBuilder);
    }

    // Convert bytes to bytes20
    function convertBytesToBytes20(bytes memory input) public pure returns (bytes20) {
        require(input.length == 20, "Input must be 20 bytes long");
        bytes20 result;
        assembly {
            result := mload(add(input, 0x20))
        }
        return result;
    }

    // Block Builder sends JobAssignment to Computer, with signature validation
    function submitJobAssignment(OpenrankTx calldata transaction) external onlyBlockBuilder {        
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);

        Signature memory sig = transaction.signature;
        bytes memory signature = abi.encodePacked(sig.r, sig.s, sig.r_id);
        address signer = recoverSigner(txHash, signature);
        require(blockBuilders[signer], "Invalid Block Builder signature");

        // check the transaction kind & jobRunRequestTxHash
        require(transaction.kind == TxKind.JobRunAssignment, "Invalid transaction kind");

        JobRunAssignment memory jobRunAssignment = decodeJobRunAssignment(transaction.body);

        require(hasTx[jobRunAssignment.job_run_request_tx_hash], "Invalid Job run request tx hash");

        address _computer = address(jobRunAssignment.assigned_compute_node);
        address _verifier = address(jobRunAssignment.assigned_verifier_node);
        require(computers[_computer], "Assigned computer is not whitelisted");
        require(verifiers[_verifier], "Verifier is not whitelisted");

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobAssigned(txHash, _computer, _verifier);
    }

    // Computer submits a CreateCommitment with signature validation
    function submitCreateCommitment(OpenrankTx calldata transaction) external onlyComputer {
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);

        Signature memory sig = transaction.signature;
        bytes memory signature = abi.encodePacked(sig.r, sig.s, sig.r_id);
        address signer = recoverSigner(txHash, signature);
        require(computers[signer], "Invalid Computer signature");

        // check the transaction kind & jobRunAssignmentTxHash
        require(transaction.kind == TxKind.CreateCommitment, "Invalid transaction kind");

        CreateCommitment memory createCommitment = decodeCreateCommitment(transaction.body);
        
        require(hasTx[createCommitment.job_run_assignment_tx_hash], "Invalid Job run assignment tx hash");

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobCommitted(txHash);
    }

    // Verifier submit JobVerification result with signature validation
    function submitJobVerification(OpenrankTx calldata transaction) external onlyVerifier{
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);

        Signature memory sig = transaction.signature;
        bytes memory signature = abi.encodePacked(sig.r, sig.s, sig.r_id);
        address signer = recoverSigner(txHash, signature);
        require(verifiers[signer], "Invalid Verifier signature");

        // check the transaction kind & jobRunAssignmentTxHash
        require(transaction.kind == TxKind.JobVerification, "Invalid transaction kind");

        JobVerification memory jobVerification = decodeJobVerification(transaction.body);

        require(hasTx[jobVerification.job_run_assignment_tx_hash], "Invalid Job run assignment tx hash");

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobVerified(txHash, jobVerification.verification_result, signer);
    }

    // Recover signer from the provided hash and signature
    function recoverSigner(bytes32 messageHash, bytes memory signature) internal pure returns (address) {
        (uint8 v, bytes32 r, bytes32 s) = splitSignature(signature);
        return ecrecover(messageHash, v, r, s);
    }

    // Helper function to split the signature into `r`, `s`, and `v`
    function splitSignature(bytes memory sig) internal pure returns (uint8 v, bytes32 r, bytes32 s) {
        require(sig.length == 65, "Invalid signature length");
        assembly {
            r := mload(add(sig, 32))
            s := mload(add(sig, 64))
            v := byte(0, mload(add(sig, 96)))
        }
        // Since OpenRankTX signature uses `recovery_id`(0 - 3), we need to convert it to valid `v`(27 or 28)
        v = v + 27;
        return (v, r, s);
    }

    // Helper function to get the transaction hash from the OpenrankTx
    function getTxHash(OpenrankTx memory transaction) internal pure returns (bytes32) {
        bytes[] memory _from = new bytes[](1);
        _from[0] = RLPEncode.encodeBytes(transaction.from);

        bytes[] memory _to = new bytes[](1);
        _to[0] = RLPEncode.encodeBytes(transaction.to);

        return keccak256(abi.encodePacked(transaction.nonce, RLPEncode.encodeList(_from), RLPEncode.encodeList(_to), RLPEncode.encodeUint(uint(transaction.kind)), transaction.body));
    }

    // Decode RLP back into the JobRunRequest struct
    function decodeJobRunRequest(bytes memory rlpData) public pure returns (JobRunRequest memory) {
        RLPReader.RLPItem[] memory items = rlpData.toRlpItem().toList();
        return JobRunRequest({
            domain_id: uint64(items[0].toUint()),
            block_height: uint32(items[1].toUint()),
            job_id: bytes32(items[2].toList()[0].toBytes())
        });
    }

    // Decode RLP back into the JobRunAssignment struct
    function decodeJobRunAssignment(bytes memory rlpData) public pure returns (JobRunAssignment memory) {
        RLPReader.RLPItem[] memory items = rlpData.toRlpItem().toList();
        return JobRunAssignment({
            job_run_request_tx_hash: bytes32(items[0].toList()[0].toBytes()),
            assigned_compute_node: bytes20(items[1].toList()[0].toBytes()),
            assigned_verifier_node: bytes20(items[2].toList()[0].toBytes())
        });
    }

    // Decode RLP back into the CreateCommitment struct
    function decodeCreateCommitment(bytes memory rlpData) public pure returns (CreateCommitment memory) {
        RLPReader.RLPItem[] memory items = rlpData.toRlpItem().toList();

        // Decode bytes32[] field
        RLPReader.RLPItem[] memory scoresTxHashes = items[3].toList();
        bytes32[] memory scores_tx_hashes = new bytes32[](scoresTxHashes.length);
        for (uint i = 0; i < scoresTxHashes.length; i++) {
            scores_tx_hashes[i] = bytes32(scoresTxHashes[i].toList()[0].toBytes());
        }

        // Decode bytes32[] field
        RLPReader.RLPItem[] memory newTrustTxHashes = items[4].toList();
        bytes32[] memory new_trust_tx_hashes = new bytes32[](newTrustTxHashes.length);
        for (uint i = 0; i < newTrustTxHashes.length; i++) {
            new_trust_tx_hashes[i] = bytes32(newTrustTxHashes[i].toList()[0].toBytes());
        }

        // Decode bytes32[] field
        RLPReader.RLPItem[] memory newSeedTxHashes = items[5].toList();
        bytes32[] memory new_seed_tx_hashes = new bytes32[](newSeedTxHashes.length);
        for (uint i = 0; i < newSeedTxHashes.length; i++) {
            new_seed_tx_hashes[i] = bytes32(newSeedTxHashes[i].toList()[0].toBytes());
        }

        return CreateCommitment({
            job_run_assignment_tx_hash: bytes32(items[0].toList()[0].toBytes()),
            lt_root_hash: bytes32(items[1].toList()[0].toBytes()),
            compute_root_hash: bytes32(items[2].toList()[0].toBytes()),
            scores_tx_hashes: scores_tx_hashes,
            new_trust_tx_hashes: new_trust_tx_hashes,
            new_seed_tx_hashes: new_seed_tx_hashes
        });
    }

    // Decode RLP back into the JobVerification struct
    function decodeJobVerification(bytes memory rlpData) public pure returns (JobVerification memory) {
        RLPReader.RLPItem[] memory items = rlpData.toRlpItem().toList();
        return JobVerification({
            job_run_assignment_tx_hash: bytes32(items[0].toList()[0].toBytes()),
            verification_result: items[1].toBoolean()
        });
    }
}
