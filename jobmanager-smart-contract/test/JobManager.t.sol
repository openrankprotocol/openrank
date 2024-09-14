// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {JobManager} from "../src/JobManager.sol";

contract JobManagerTest is Test {
    JobManager public jobManager;
    
    address public blockBuilder;    
    address public computer;
    address public verifier;

    function setUp() public {
        // Get the addresses from the default test wallets
        blockBuilder = vm.addr(1);
        address[] memory _blockBuilders = new address[](1);
        _blockBuilders[0] = blockBuilder;

        computer = vm.addr(2);
        address[] memory _computers = new address[](1);
        _computers[0] = computer;

        verifier = vm.addr(3);
        address[] memory _verifiers = new address[](1);
        _verifiers[0] = verifier;
        
        jobManager = new JobManager(_blockBuilders, _computers, _verifiers);
    }

    function test_decodeJobRunRequest() public view {
        // JobRunRequest::new(DomainHash::from(1), 2, Hash::default());
        bytes memory data = hex"e5c10102e1a00000000000000000000000000000000000000000000000000000000000000000";

        JobManager.JobRunRequest memory jobRunRequest = jobManager.decodeJobRunRequest(data);

        assertEq(jobRunRequest.domain_id, 1);
        assertEq(jobRunRequest.block_height, 2);
        assertEq(jobRunRequest.job_id, hex"0000000000000000000000000000000000000000000000000000000000000000");
    }

    function test_decodeJobRunAssignment() public view {
        // JobRunAssignment::new(TxHash::default(), Address::default(), Address::default());
        bytes memory data = hex"f84ee1a00000000000000000000000000000000000000000000000000000000000000000d5940000000000000000000000000000000000000000d5940000000000000000000000000000000000000000";

        JobManager.JobRunAssignment memory jobRunAssignment = jobManager.decodeJobRunAssignment(data);

        assertEq(jobRunAssignment.job_run_request_tx_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(jobRunAssignment.assigned_compute_node, hex"0000000000000000000000000000000000000000");
        assertEq(jobRunAssignment.assigned_verifier_node, hex"0000000000000000000000000000000000000000");
    }

    function test_decodeCreateCommitment() public view {
        // CreateCommitment::default();
        bytes memory data = hex"f869e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000c0c0c0";

        JobManager.CreateCommitment memory createCommitment = jobManager.decodeCreateCommitment(data);

        assertEq(createCommitment.job_run_assignment_tx_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(createCommitment.lt_root_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(createCommitment.compute_root_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");

        assertEq(createCommitment.scores_tx_hashes.length, 0);
        assertEq(createCommitment.new_trust_tx_hashes.length, 0);
        assertEq(createCommitment.new_seed_tx_hashes.length, 0);

        // CreateCommitment::default_with(TxHash::default(), Hash::default(), Hash::default(), vec![TxHash::default(), TxHash::default()]);
        bytes memory data1 = hex"f8aee1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000f844e1a00000000000000000000000000000000000000000000000000000000000000000e1a00000000000000000000000000000000000000000000000000000000000000000c0c0";

        JobManager.CreateCommitment memory createCommitment1 = jobManager.decodeCreateCommitment(data1);

        assertEq(createCommitment1.job_run_assignment_tx_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(createCommitment1.lt_root_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(createCommitment1.compute_root_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");

        assertEq(createCommitment1.scores_tx_hashes.length, 2);
        assertEq(createCommitment1.new_trust_tx_hashes.length, 0);
        assertEq(createCommitment1.new_seed_tx_hashes.length, 0);
    }

    function test_decodeJobVerification() public view {
        // JobVerification::default();
        bytes memory data = hex"e3e1a0000000000000000000000000000000000000000000000000000000000000000001";

        JobManager.JobVerification memory jobVerification = jobManager.decodeJobVerification(data);

        assertEq(jobVerification.job_run_assignment_tx_hash, hex"0000000000000000000000000000000000000000000000000000000000000000");
        assertEq(jobVerification.verification_result, true);
    }
}
