// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

contract JobManager {
    // Roles and whitelists
    address public blockBuilder;
    address public computer;
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
    }

    // Store jobs with txHash as the key
    mapping(bytes32 => Job) public jobs;

    // Events
    event JobAssigned(bytes32 indexed txHash, address computer, address verifier);
    event JobCommitted(bytes32 indexed txHash, bytes32 commitment);
    event JobVerified(bytes32 indexed txHash, bool isVerified, address verifier);

    // Modifier for whitelisted Block Builder
    modifier onlyBlockBuilder() {
        require(msg.sender == blockBuilder, "Not authorized as Block Builder");
        _;
    }

    // Modifier for whitelisted Computer
    modifier onlyComputer() {
        require(msg.sender == computer, "Not authorized as Computer");
        _;
    }

    // Modifier for whitelisted Verifier
    modifier onlyVerifier() {
        require(verifiers[msg.sender], "Not authorized as Verifier");
        _;
    }

    // Initialize the contract with whitelisted addresses
    constructor(address _blockBuilder, address _computer, address[] memory _verifiers) {
        require(_blockBuilder != address(0), "Invalid Block Builder address");
        require(_computer != address(0), "Invalid Computer address");
        blockBuilder = _blockBuilder;
        computer = _computer;

        for (uint256 i = 0; i < _verifiers.length; i++) {
            verifiers[_verifiers[i]] = true;
        }
    }

    // Block Builder sends JobAssignment to Computer, with signature validation
    function submitJobAssignment(bytes32 txHash, address _computer, address _verifier, bytes calldata signature) external onlyBlockBuilder {
        require(jobs[txHash].blockBuilder == address(0), "Job already exists");
        require(_computer == computer, "Assigned computer is not whitelisted");

        // Verify the signature
        address signer = recoverSigner(txHash, signature);
        require(signer == blockBuilder, "Invalid Block Builder signature");

        require(verifiers[_verifier], "Verifier is not whitelisted");

        jobs[txHash] = Job({
            blockBuilder: msg.sender,
            computer: _computer,
            verifier: _verifier,
            commitment: bytes32(0),
            isCommitted: false,
            isVerfierVoted: false,
            isValid: false
        });

        emit JobAssigned(txHash, _computer, _verifier);
    }

    // Computer submits a CreateCommitment with signature validation
    function submitCreateCommitment(bytes32 txHash, bytes32 _commitment, bytes calldata signature) external onlyComputer {
        require(jobs[txHash].blockBuilder != address(0), "Job not assigned");
        require(!jobs[txHash].isCommitted, "Commitment already submitted");

        // Verify the signature
        address signer = recoverSigner(txHash, signature);
        require(signer == computer, "Invalid Computer signature");

        jobs[txHash].commitment = _commitment;
        jobs[txHash].isCommitted = true;

        emit JobCommitted(txHash, _commitment);
    }

    // Verifier submit JobVerification result with signature validation
    function submitJobVerification(bytes32 txHash, bool isValid, bytes calldata signature) external onlyVerifier{
        require(jobs[txHash].isCommitted, "Commitment not submitted");

        // Verify the signature
        address signer = recoverSigner(txHash, signature);
        require(verifiers[signer], "Invalid Verifier signature");

        require(jobs[txHash].verifier == signer, "Verifier not part of this job");

        jobs[txHash].isValid = isValid;
        jobs[txHash].isVerfierVoted = true;

        emit JobVerified(txHash, isValid, signer);
    }

    // Recover signer from the provided hash and signature
    function recoverSigner(bytes32 hash, bytes memory signature) internal pure returns (address) {
        bytes32 messageHash = prefixed(hash);
        (uint8 v, bytes32 r, bytes32 s) = splitSignature(signature);
        return ecrecover(messageHash, v, r, s);
    }

    // Helper function to prefix the hash with "\x19Ethereum Signed Message:\n32" to match with the standard
    function prefixed(bytes32 hash) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked("\x19Ethereum Signed Message:\n32", hash));
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
}
