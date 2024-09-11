// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract JobManager {
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

    struct JobRunRequest {
        uint64 domain_id;
        uint32 block_height;
        bytes32 job_id;
    }

    struct JobRunAssignment {
        bytes32 job_run_request_tx_hash;
        address assigned_compute_node;
        address assigned_verifier_node;
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
    function sendJobRunRequest(OpenrankTx memory transaction, JobRunRequest memory jobRunRequest) external {
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);

        Signature memory sig = transaction.signature;
        bytes memory signature = abi.encodePacked(sig.r, sig.s, sig.r_id);
        address signer = recoverSigner(txHash, signature);
        
        // check if signer is whitelisted user
        // require(signer == user, "Invalid user signature");

        // check the transaction kind & jobId
        require(transaction.kind == TxKind.JobRunRequest, "Invalid transaction kind");

        address _blockBuilder = transaction.to;
        require(blockBuilders[_blockBuilder], "Assigned block builder is not whitelisted");

        // TODO: should decode the body to JobRunRequest, or encode the `jobRunRequest` to check the tx body
        // JobRunRequest memory jobRunRequest = abi.decode(transaction.body, (JobRunRequest));

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobRunRequested(txHash, _blockBuilder);
    }

    // Block Builder sends JobAssignment to Computer, with signature validation
    function submitJobAssignment(OpenrankTx calldata transaction, JobRunAssignment memory jobRunAssignment) external onlyBlockBuilder {        
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);

        Signature memory sig = transaction.signature;
        bytes memory signature = abi.encodePacked(sig.r, sig.s, sig.r_id);
        address signer = recoverSigner(txHash, signature);
        require(blockBuilders[signer], "Invalid Block Builder signature");

        // check the transaction kind & jobRunRequestTxHash
        require(transaction.kind == TxKind.JobRunAssignment, "Invalid transaction kind");

        // TODO: should decode the body to JobRunAssignment, or encode the `jobRunAssignment` to check the tx body
        // JobRunAssignment memory jobRunAssignment = abi.decode(transaction.body, (JobRunAssignment));

        require(hasTx[jobRunAssignment.job_run_request_tx_hash], "Invalid Job run request tx hash");

        address _computer = jobRunAssignment.assigned_compute_node;
        address _verifier = jobRunAssignment.assigned_verifier_node;
        require(computers[_computer], "Assigned computer is not whitelisted");
        require(verifiers[_verifier], "Verifier is not whitelisted");

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobAssigned(txHash, _computer, _verifier);
    }

    // Computer submits a CreateCommitment with signature validation
    function submitCreateCommitment(OpenrankTx calldata transaction, CreateCommitment memory createCommitment) external onlyComputer {
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);

        Signature memory sig = transaction.signature;
        bytes memory signature = abi.encodePacked(sig.r, sig.s, sig.r_id);
        address signer = recoverSigner(txHash, signature);
        require(computers[signer], "Invalid Computer signature");

        // check the transaction kind & jobRunAssignmentTxHash
        require(transaction.kind == TxKind.CreateCommitment, "Invalid transaction kind");

        // TODO: should decode the body to CreateCommitment, or encode the `createCommitment` to check the tx body
        // CreateCommitment memory createCommitment = abi.decode(transaction.body, (CreateCommitment));
        
        require(hasTx[createCommitment.job_run_assignment_tx_hash], "Invalid Job run assignment tx hash");

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobCommitted(txHash);
    }

    // Verifier submit JobVerification result with signature validation
    function submitJobVerification(OpenrankTx calldata transaction, JobVerification memory jobVerification) external onlyVerifier{
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);

        Signature memory sig = transaction.signature;
        bytes memory signature = abi.encodePacked(sig.r, sig.s, sig.r_id);
        address signer = recoverSigner(txHash, signature);
        require(verifiers[signer], "Invalid Verifier signature");

        // check the transaction kind & jobRunAssignmentTxHash
        require(transaction.kind == TxKind.JobVerification, "Invalid transaction kind");

        // TODO: should decode the body to JobVerification, or encode the `jobVerification` to check the tx body
        // JobVerification memory jobVerification = abi.decode(transaction.body, (JobVerification));

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
        return (v, r, s);
    }

    // Helper function to get the transaction hash from the OpenrankTx
    function getTxHash(OpenrankTx memory transaction) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(transaction.nonce, transaction.from, transaction.to, transaction.kind, transaction.body));
    }
}
