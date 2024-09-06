// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {JobManager} from "../src/JobManager.sol";

contract JobManagerScript is Script {
    JobManager public jobManager;
    address public blockBuilder;
    address public computer;
    address public verifier1;
    address public verifier2;
    address public verifier3;

    function setUp() public {
        // Get the addresses from the default test wallets
        blockBuilder = vm.addr(1);
        computer = vm.addr(2);
        verifier1 = vm.addr(3);
        verifier2 = vm.addr(4);
        verifier3 = vm.addr(5);
    }

    function run() public {
        vm.startBroadcast();

        address[] memory _verifiers = new address[](3);
        _verifiers[0] = verifier1;
        _verifiers[1] = verifier2;
        _verifiers[2] = verifier3;
        jobManager = new JobManager(blockBuilder, computer, _verifiers);

        vm.stopBroadcast();
    }
}
