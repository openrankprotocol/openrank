// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract JobManager {
    // Roles and whitelists
    mapping(address => bool) public blockBuilders;
    mapping(address => bool) public computers;
    mapping(address => bool) public verifiers;

    // Struct to store job details
    struct Job {
        address blockBuilder;
        address computer;
        address verifier;
        bytes32 commitment;
        bool isCommitted;
        bool isVerfierVoted;
        bool isValid;

        bytes32 jobId;
        bytes32 jobRunRequestTxHash;
        bytes32 jobRunAssignmentTxHash;
        bytes32 createCommitmentTxHash;
        bytes32 jobVerificationTxHash;
    }

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
        bytes20 from;
        bytes20 to;
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
        bytes20 assigned_computer;
        bytes20 assigned_verifier;
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

    // Store jobs with JobId as the key
    mapping(bytes32 => Job) public jobs;

    // Store OpenrankTx with txHash as the key
    mapping(bytes32 => OpenrankTx) public txs;

    // Events
    event JobRunRequested(bytes32 indexed jobId, address blockBuilder, bytes32 txHash);
    event JobAssigned(bytes32 indexed jobId, address computer, address verifier);
    event JobCommitted(bytes32 indexed jobId, bytes32 commitment);
    event JobVerified(bytes32 indexed jobId, bool isVerified, address verifier);

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
    function sendJobRunRequest(bytes32 jobId, address _blockBuilder, OpenrankTx calldata transaction, bytes calldata signature) external {
        require(jobs[jobId].blockBuilder == address(0), "Job already assigned to a block builder");
        require(blockBuilders[_blockBuilder], "Assigned block builder is not whitelisted");

        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);
        address signer = recoverSigner(txHash, signature);
        
        // check if signer is whitelisted user
        // require(signer == user, "Invalid user signature");

        // check the transaction kind & jobId
        require(transaction.kind == TxKind.JobRunRequest, "Invalid transaction kind");

        JobRunRequest memory jobRunRequest = abi.decode(transaction.body, (JobRunRequest));

        require(jobRunRequest.job_id == jobId, "Invalid Job Id");

        // save Job in storage
        jobs[jobId] = Job({
            blockBuilder: _blockBuilder,
            computer: address(0),
            verifier: address(0),
            commitment: bytes32(0),
            isCommitted: false,
            isVerfierVoted: false,
            isValid: false,
            
            jobId: jobId,
            jobRunRequestTxHash: bytes32(0),
            jobRunAssignmentTxHash: bytes32(0),
            createCommitmentTxHash: bytes32(0),
            jobVerificationTxHash: bytes32(0)
        });


        // save TX in storage
        txs[txHash] = transaction;

        emit JobRunRequested(jobId, _blockBuilder, txHash);
    }

    // Block Builder sends JobAssignment to Computer, with signature validation
    function submitJobAssignment(bytes32 jobId, address _computer, address _verifier, OpenrankTx calldata transaction, bytes calldata signature) external onlyBlockBuilder {
        require(jobs[jobId].computer == address(0), "Job already assigned to a computer");
        require(computers[_computer], "Assigned computer is not whitelisted");
        require(verifiers[_verifier], "Verifier is not whitelisted");
        
        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);
        address signer = recoverSigner(txHash, signature);
        require(blockBuilders[signer], "Invalid Block Builder signature");

        // check the transaction kind & jobRunRequestTxHash
        require(transaction.kind == TxKind.JobRunAssignment, "Invalid transaction kind");

        JobRunAssignment memory jobRunAssignment = abi.decode(transaction.body, (JobRunAssignment));

        require(jobRunAssignment.jobRunRequestTxHash == txHash, "Invalid Job run request tx hash");

        // save Job in storage
        jobs[jobId].computer = _computer;
        jobs[jobId].verifier = _verifier;
        jobs[jobId].jobRunAssignmentTxHash = txHash;

        // save TX in storage
        txs[txHash] = transaction;

        emit JobAssigned(jobId, _computer, _verifier);
    }

    // Computer submits a CreateCommitment with signature validation
    function submitCreateCommitment(bytes32 jobId, bytes32 _commitment, OpenrankTx calldata transaction, bytes calldata signature) external onlyComputer {
        require(jobs[jobId].computer != address(0), "Job not assigned");
        require(!jobs[jobId].isCommitted, "Commitment already submitted");

        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);
        address signer = recoverSigner(txHash, signature);
        require(computers[signer], "Invalid Computer signature");

        // check the transaction kind & jobRunAssignmentTxHash
        require(transaction.kind == TxKind.CreateCommitment, "Invalid transaction kind");

        CreateCommitment memory createCommitment = abi.decode(transaction.body, (CreateCommitment));
        
        require(createCommitment.jobRunAssignmentTxHash == jobs[jobId].jobRunAssignmentTxHash, "Invalid Job run assignment tx hash");

        // save Job in storage
        jobs[jobId].commitment = _commitment;
        jobs[jobId].isCommitted = true;

        jobs[jobId].createCommitmentTxHash = txHash;

        // save TX in storage
        txs[txHash] = transaction;

        emit JobCommitted(jobId, _commitment);
    }

    // Verifier submit JobVerification result with signature validation
    function submitJobVerification(bytes32 jobId, OpenrankTx calldata transaction, bytes calldata signature) external onlyVerifier{
        require(jobs[jobId].isCommitted, "Commitment not submitted");

        // construct tx hash from transaction and check the signature
        bytes32 txHash = getTxHash(transaction);
        address signer = recoverSigner(txHash, signature);
        require(verifiers[signer], "Invalid Verifier signature");

        require(jobs[jobId].verifier == signer, "Verifier not part of this job");

        // check the transaction kind & jobRunAssignmentTxHash
        require(transaction.kind == TxKind.JobVerification, "Invalid transaction kind");

        JobVerification memory jobVerification = abi.decode(transaction.body, (JobVerification));

        require(jobVerification.jobRunAssignmentTxHash == jobs[jobId].jobRunAssignmentTxHash, "Invalid Job run assignment tx hash");

        // save Job in storage
        bool isValid = jobVerification.verificationResult;
        jobs[jobId].isValid = isValid;
        jobs[jobId].isVerfierVoted = true;

        jobs[jobId].jobVerificationTxHash = txHash;

        // save TX in storage
        txs[txHash] = transaction;

        emit JobVerified(jobId, isValid, signer);
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
