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
}
