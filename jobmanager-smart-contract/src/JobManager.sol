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
    }

    struct JobRunRequest {
        uint64 domainHash;
        uint32 blockHeight;
        bytes32 job_id;
    }

    struct JobRunAssignment {
        bytes32 jobRunRequestTxHash;
        address assigned_computer;
        address assigned_verifier;
    }

    struct CreateCommitment {
        bytes32 jobRunAssignmentTxHash;
        bytes32 ltRootHash;
        bytes32 computeRootHash;
        bytes32[] scoresTxHashes;
    }

    struct JobVerification {
        bytes32 jobRunAssignmentTxHash;
        bool verificationResult;
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
    function sendJobRunRequest(OpenrankTx calldata transaction, bytes calldata signature) external {
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);
        address signer = recoverSigner(txHash, signature);
        
        // check if signer is whitelisted user
        // require(signer == user, "Invalid user signature");

        // check the transaction kind & jobId
        require(transaction.kind == TxKind.JobRunRequest, "Invalid transaction kind");

        address _blockBuilder = transaction.to;
        require(blockBuilders[_blockBuilder], "Assigned block builder is not whitelisted");

        JobRunRequest memory jobRunRequest = abi.decode(transaction.body, (JobRunRequest));

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobRunRequested(txHash, _blockBuilder);
    }

    // Block Builder sends JobAssignment to Computer, with signature validation
    function submitJobAssignment(OpenrankTx calldata transaction, bytes calldata signature) external onlyBlockBuilder {        
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);
        address signer = recoverSigner(txHash, signature);
        require(blockBuilders[signer], "Invalid Block Builder signature");

        // check the transaction kind & jobRunRequestTxHash
        require(transaction.kind == TxKind.JobRunAssignment, "Invalid transaction kind");

        JobRunAssignment memory jobRunAssignment = abi.decode(transaction.body, (JobRunAssignment));

        require(hasTx[jobRunAssignment.jobRunRequestTxHash], "Invalid Job run request tx hash");

        address _computer = jobRunAssignment.assigned_computer;
        address _verifier = jobRunAssignment.assigned_verifier;
        require(computers[_computer], "Assigned computer is not whitelisted");
        require(verifiers[_verifier], "Verifier is not whitelisted");

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobAssigned(txHash, _computer, _verifier);
    }

    // Computer submits a CreateCommitment with signature validation
    function submitCreateCommitment(OpenrankTx calldata transaction, bytes calldata signature) external onlyComputer {
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);
        address signer = recoverSigner(txHash, signature);
        require(computers[signer], "Invalid Computer signature");

        // check the transaction kind & jobRunAssignmentTxHash
        require(transaction.kind == TxKind.CreateCommitment, "Invalid transaction kind");

        CreateCommitment memory createCommitment = abi.decode(transaction.body, (CreateCommitment));
        
        require(hasTx[createCommitment.jobRunAssignmentTxHash], "Invalid Job run assignment tx hash");

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobCommitted(txHash);
    }

    // Verifier submit JobVerification result with signature validation
    function submitJobVerification(OpenrankTx calldata transaction, bytes calldata signature) external onlyVerifier{
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);
        address signer = recoverSigner(txHash, signature);
        require(verifiers[signer], "Invalid Verifier signature");

        // check the transaction kind & jobRunAssignmentTxHash
        require(transaction.kind == TxKind.JobVerification, "Invalid transaction kind");

        JobVerification memory jobVerification = abi.decode(transaction.body, (JobVerification));

        require(hasTx[jobVerification.jobRunAssignmentTxHash], "Invalid Job run assignment tx hash");

        // save TX in storage
        txs[txHash] = transaction;
        hasTx[txHash] = true;

        emit JobVerified(txHash, jobVerification.verificationResult, signer);
    }

    // Recover signer from the provided hash and signature
    function recoverSigner(bytes32 hash, bytes memory signature) internal pure returns (address) {
        bytes32 messageHash = keccak256(abi.encodePacked(hash));
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
    function getTxHash(OpenrankTx calldata transaction) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(transaction.nonce, transaction.from, transaction.to, transaction.kind, transaction.body));
    }
}
