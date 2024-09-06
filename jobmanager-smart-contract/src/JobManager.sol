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
        address[] verifiers;
        bytes32 commitment;
        bool isCommitted;
        bool[] verifierVotes;
        bool isValid;
    }

    // Store jobs with txHash as the key
    mapping(bytes32 => Job) public jobs;

    // Events
    event JobAssigned(bytes32 indexed txHash, address computer, address[] verifiers);
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

    // Block Builder assigns a job with signature validation
    function assignJob(bytes32 txHash, address _computer, address[] memory _verifiers, bytes memory signature) external {

    }

    // Computer submits a job commitment with signature validation
    function submitCommitment(bytes32 txHash, bytes32 _commitment, bytes memory signature) external onlyComputer {

    }

    // Verifiers submit their verification votes with signature validation
    function submitVerification(bytes32 txHash, bool isValid, bytes memory signature) external {
        
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
