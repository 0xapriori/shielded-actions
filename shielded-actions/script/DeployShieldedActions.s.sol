// SPDX-License-Identifier: MIT
pragma solidity ^0.8.30;

import {Script, console} from "forge-std/Script.sol";

import {RiscZeroVerifierRouter} from "@risc0-ethereum/RiscZeroVerifierRouter.sol";

import {ProtocolAdapter} from "../src/ProtocolAdapter.sol";
import {Versioning} from "../src/libs/Versioning.sol";
import {ERC20Forwarder} from "../src/forwarders/ERC20Forwarder.sol";
import {UniswapV3Forwarder} from "../src/forwarders/UniswapV3Forwarder.sol";

/// @title DeployShieldedActions
/// @notice Deployment script for Shielded Actions on Sepolia testnet
/// @dev Deploys: Protocol Adapter + ERC20Forwarder (for WETH/USDC) + UniswapV3Forwarder
contract DeployShieldedActions is Script {
    // ============ Sepolia Addresses ============

    /// @notice RISC Zero Verifier Router on Sepolia
    /// @dev From https://dev.risczero.com/api/3.0/blockchain-integration/contracts/verifier
    address public constant RISC_ZERO_VERIFIER_ROUTER_SEPOLIA = 0x925d8331ddc0a1F0d96E68CF073DFE1d92b69187;

    /// @notice Uniswap V3 SwapRouter02 on Sepolia
    address public constant UNISWAP_V3_ROUTER_SEPOLIA = 0x3bFA4769FB09eefC5a80d6E87c3B9C650f7Ae48E;

    /// @notice WETH on Sepolia
    address public constant WETH_SEPOLIA = 0x7b79995e5f793A07Bc00c21412e50Ecae098E7f9;

    /// @notice USDC test token on Sepolia (Circle's official test USDC)
    /// @dev Alternative: 0x94a9D9AC8a22534E3FaCa9F4e7F2E2cf85d5E4C8
    address public constant USDC_SEPOLIA = 0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238;

    // ============ Deployed Addresses (filled after deployment) ============

    ProtocolAdapter public protocolAdapter;
    ERC20Forwarder public wethForwarder;
    ERC20Forwarder public usdcForwarder;
    UniswapV3Forwarder public uniswapForwarder;

    /// @notice Main deployment function
    /// @param emergencyStopCaller The address that can emergency stop the Protocol Adapter
    function run(address emergencyStopCaller) public {
        // Allow Sepolia (11155111) or local Anvil (31337)
        require(
            block.chainid == 11155111 || block.chainid == 31337,
            "This script is for Sepolia or local Anvil only"
        );

        console.log("=== Deploying Shielded Actions to Sepolia ===");
        console.log("Emergency stop caller:", emergencyStopCaller);

        vm.startBroadcast();

        // 1. Deploy Protocol Adapter
        console.log("\n1. Deploying Protocol Adapter...");
        protocolAdapter = new ProtocolAdapter({
            riscZeroVerifierRouter: RiscZeroVerifierRouter(RISC_ZERO_VERIFIER_ROUTER_SEPOLIA),
            riscZeroVerifierSelector: Versioning._RISC_ZERO_VERIFIER_SELECTOR,
            emergencyStopCaller: emergencyStopCaller
        });
        console.log("   Protocol Adapter deployed at:", address(protocolAdapter));

        // 2. Deploy WETH Forwarder
        console.log("\n2. Deploying WETH Forwarder...");
        wethForwarder = new ERC20Forwarder({
            _protocolAdapter: address(protocolAdapter),
            _token: WETH_SEPOLIA
        });
        console.log("   WETH Forwarder deployed at:", address(wethForwarder));

        // 3. Deploy USDC Forwarder
        console.log("\n3. Deploying USDC Forwarder...");
        usdcForwarder = new ERC20Forwarder({
            _protocolAdapter: address(protocolAdapter),
            _token: USDC_SEPOLIA
        });
        console.log("   USDC Forwarder deployed at:", address(usdcForwarder));

        // 4. Deploy Uniswap V3 Forwarder
        console.log("\n4. Deploying Uniswap V3 Forwarder...");
        uniswapForwarder = new UniswapV3Forwarder({
            _protocolAdapter: address(protocolAdapter),
            _swapRouter: UNISWAP_V3_ROUTER_SEPOLIA
        });
        console.log("   Uniswap V3 Forwarder deployed at:", address(uniswapForwarder));

        vm.stopBroadcast();

        // Print summary
        console.log("\n=== Deployment Summary ===");
        console.log("Protocol Adapter:     ", address(protocolAdapter));
        console.log("WETH Forwarder:       ", address(wethForwarder));
        console.log("USDC Forwarder:       ", address(usdcForwarder));
        console.log("Uniswap V3 Forwarder: ", address(uniswapForwarder));
        console.log("\n=== External Dependencies ===");
        console.log("RISC Zero Router:     ", RISC_ZERO_VERIFIER_ROUTER_SEPOLIA);
        console.log("Uniswap V3 Router:    ", UNISWAP_V3_ROUTER_SEPOLIA);
        console.log("WETH:                 ", WETH_SEPOLIA);
        console.log("USDC:                 ", USDC_SEPOLIA);
    }

}
