# Shielded Actions: Privacy-Preserving Swaps via Anoma Protocol Adapter

## Technical Specification Document

**Version:** 1.0  
**Date:** January 8, 2026  
**Status:** Draft for Implementation

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Background & Context](#2-background--context)
3. [System Architecture](#3-system-architecture)
4. [Core Components](#4-core-components)
5. [User Flow: Shielded Swap](#5-user-flow-shielded-swap)
6. [Smart Contract Specifications](#6-smart-contract-specifications)
7. [DEX Integration](#7-dex-integration)
8. [Relayer System](#8-relayer-system)
9. [Proof Generation](#9-proof-generation)
10. [Security Considerations](#10-security-considerations)
11. [Implementation Roadmap](#11-implementation-roadmap)
12. [Appendices](#appendices)

---

## 1. Executive Summary

### 1.1 Objective

Build a privacy-preserving swap system ("Shielded Actions") that enables users to:
1. **Shield** ERC20 tokens into the Anoma Protocol Adapter (converting them to private resources)
2. **Swap** tokens via Uniswap V2/V3 or CoWSwap while maintaining privacy
3. **Unshield** (or keep shielded) the resulting tokens

### 1.2 Key Innovation

This system breaks the on-chain link between a user's deposit and withdrawal addresses while leveraging existing DEX liquidity. Unlike traditional mixers that only provide transfer privacy, Shielded Actions enables **privacy-preserving DeFi interactions**.

### 1.3 Core Dependencies

| Component | Address/Location | Purpose |
|-----------|------------------|---------|
| Anoma Protocol Adapter | `0x46E622226F93Ed52C584F3f66135CD06AF01c86c` (Ethereum Mainnet) | Shielded state management |
| Uniswap V3 SwapRouter02 | `0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45` | DEX swaps |
| CoWSwap GPv2Settlement | `0x9008D19f58AAbD9eD0D60971565AA8510560ab41` | Intent-based swaps |
| RISC Zero Verifier | See [RISC Zero docs](https://dev.risczero.com/api/blockchain-integration/contracts/verifier) | ZK proof verification |

---

## 2. Background & Context

### 2.1 Anoma Protocol Adapter (PA)

The Anoma Protocol Adapter is a smart contract that implements the **Anoma Resource Machine (ARM)** on EVM-compatible chains. Key concepts:

**Resources**: Immutable units of state that can be created and consumed exactly once. Unlike smart contract storage (mutable), resources track state changes via:
- **Commitments**: Hash stored in Merkle tree when resource is created
- **Nullifiers**: Unique identifier revealed when resource is consumed

**Privacy Model**: Resources use a commitment/nullifier scheme (inspired by Zcash) that hides:
- Which commitment corresponds to which nullifier
- The contents of resources (when using encrypted data)

**Forwarder Contracts**: Custom contracts that enable the PA to interact with external contracts (like DEXs). They:
- Act as an escrow for token deposits
- Execute calls to target contracts (Uniswap, etc.)
- Create corresponding resources in the PA

### 2.2 Current PA Capabilities

The PA (as of contract deployment Nov 2025) is **settlement-only**, meaning:
- It processes fully-evaluated transactions
- No on-chain intent matching (must be done off-chain by solvers)
- All proofs must be generated client-side before submission

### 2.3 Key Limitation

**Single Forwarder Call Per Block**: The current PA design uses a singleton "calldata carrier" resource. When consumed in a transaction, subsequent transactions in the same block cannot use it. This will be improved in future versions.

---

## 3. System Architecture

### 3.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              USER LAYER                                  │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                  │
│  │   Web UI    │    │  CLI Tool   │    │   SDK       │                  │
│  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘                  │
│         │                  │                  │                          │
│         └──────────────────┼──────────────────┘                          │
│                            ▼                                             │
│                   ┌────────────────┐                                     │
│                   │  Proof Client  │ (RISC Zero zkVM)                    │
│                   └────────┬───────┘                                     │
└────────────────────────────┼────────────────────────────────────────────┘
                             │
┌────────────────────────────┼────────────────────────────────────────────┐
│                            ▼              RELAYER LAYER                  │
│                   ┌────────────────┐                                     │
│                   │ Intent Pool    │                                     │
│                   └────────┬───────┘                                     │
│                            ▼                                             │
│                   ┌────────────────┐                                     │
│                   │    Solver      │ (Matches intents, routes swaps)     │
│                   └────────┬───────┘                                     │
│                            ▼                                             │
│                   ┌────────────────┐                                     │
│                   │    Relayer     │ (Submits transactions)              │
│                   └────────┬───────┘                                     │
└────────────────────────────┼────────────────────────────────────────────┘
                             │
┌────────────────────────────┼────────────────────────────────────────────┐
│                            ▼              CONTRACT LAYER                 │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                   ANOMA PROTOCOL ADAPTER                         │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐           │    │
│  │  │ Commitment   │  │  Nullifier   │  │    Blob      │           │    │
│  │  │ Accumulator  │  │    Set       │  │   Storage    │           │    │
│  │  └──────────────┘  └──────────────┘  └──────────────┘           │    │
│  └─────────────────────────────┬───────────────────────────────────┘    │
│                                │                                         │
│                                ▼                                         │
│                     ┌──────────────────┐                                 │
│                     │ Forwarder (ERC20)│ ◄──── Custom per token          │
│                     └────────┬─────────┘                                 │
│                              │                                           │
│              ┌───────────────┼───────────────┐                           │
│              ▼               ▼               ▼                           │
│     ┌────────────┐   ┌────────────┐   ┌────────────┐                    │
│     │ Uniswap V2 │   │ Uniswap V3 │   │  CoWSwap   │                    │
│     └────────────┘   └────────────┘   └────────────┘                    │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Component Interaction Flow

```
User                Proof Client        Intent Pool      Solver/Relayer    PA + Forwarder      DEX
  │                      │                   │                │                  │              │
  │─────Shield Tx───────►│                   │                │                  │              │
  │                      │───Generate ZK────►│                │                  │              │
  │                      │◄──Proofs─────────►│                │                  │              │
  │                      │                   │───Intent───────►│                 │              │
  │                      │                   │                │───Match + Route─►│              │
  │                      │                   │                │                  │───Swap──────►│
  │                      │                   │                │                  │◄──Tokens─────│
  │                      │                   │                │◄─Settlement──────│              │
  │◄────Confirmation─────│                   │◄───Status──────│                  │              │
```

---

## 4. Core Components

### 4.1 Resources

**ERC20 Wrapped Resource**

A resource representing shielded ERC20 tokens:

```rust
struct ERC20Resource {
    // Public components (visible in commitment)
    kind: bytes32,           // Hash of (logic_ref, label_ref) - identifies token type
    quantity: u256,          // Amount of tokens
    
    // Private components (hidden)
    owner_npk: bytes32,      // Nullifier public key (derives nullifier on consumption)
    nonce: bytes32,          // Random value for uniqueness
    
    // Metadata
    token_address: address,  // Original ERC20 contract
    ephemeral: bool,         // If true, quantity must be 0 (padding resource)
}
```

**Calldata Carrier Resource**

A singleton resource that carries external call data:

```rust
struct CalldataCarrierResource {
    kind: bytes32,                    // Must match forwarder's expected kind
    forwarder_address: address,       // The forwarder contract
    input: bytes,                     // Call input data
    output: bytes,                    // Expected call output
    owner_npk: bytes32,               // Universal identity (consumable by anyone)
}
```

### 4.2 Forwarder Contract Interface

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface IForwarder {
    /// @notice Returns the calldata carrier resource kind this forwarder uses
    function calldataCarrierResourceKind() external view returns (bytes32);
    
    /// @notice Forwards a call to the target contract
    /// @param input The calldata to forward
    /// @return output The return data from the target
    function forwardCall(bytes calldata input) external returns (bytes memory output);
    
    /// @notice Returns the target contract address
    function target() external view returns (address);
}
```

### 4.3 Protocol Adapter Interface (Relevant Methods)

```solidity
interface IProtocolAdapter {
    struct Transaction {
        Action[] actions;
        bytes deltaProof;
    }
    
    struct Action {
        bytes32[] nullifiers;           // Consumed resource nullifiers
        bytes32[] commitments;          // Created resource commitments
        ResourceForwarderCalldataPair[] forwarderCalls;
        bytes[] logicProofs;
        bytes[] complianceProofs;
        bytes appData;                  // Additional data for resource logic
    }
    
    struct ResourceForwarderCalldataPair {
        Resource carrier;               // Calldata carrier resource
        ForwarderCalldata call;         // Forwarder call details
    }
    
    struct ForwarderCalldata {
        address untrustedForwarderContract;
        bytes input;
        bytes output;
    }
    
    /// @notice Execute a settlement transaction
    function execute(Transaction calldata tx) external;
}
```

---

## 5. User Flow: Shielded Swap

### 5.1 Complete Flow

**Phase 1: Shield (Deposit)**

1. User approves ERC20 token transfer to the Forwarder contract
2. User generates a secret note (random bytes) and computes commitment
3. User calls the shielding transaction:
   - Forwarder receives tokens via `transferFrom`
   - PA creates a resource commitment for the shielded amount
4. User stores secret note securely (required for withdrawal)

**Phase 2: Swap (Private)**

1. User creates a swap intent specifying:
   - Input: Their shielded resource (proven by nullifier knowledge)
   - Output: Desired token/amount (new shielded resource)
2. User generates ZK proofs client-side:
   - Resource logic proof (proves ownership)
   - Compliance proof (proves input/output balance)
   - Delta proof (proves conservation)
3. User submits intent to intent pool
4. Solver matches intent and routes through DEX
5. Relayer submits settlement transaction:
   - Consumes user's input resource (reveals nullifier)
   - Unshields tokens to forwarder
   - Forwarder executes swap on DEX
   - Shields output tokens to new resource (new commitment)

**Phase 3: Unshield (Withdraw) - Optional**

1. User proves ownership of shielded resource
2. User specifies withdrawal address
3. Relayer submits withdrawal transaction:
   - Consumes shielded resource
   - Forwarder transfers tokens to user's address

### 5.2 Detailed Swap Transaction Structure

```
┌──────────────────────────────────────────────────────────────────────────┐
│                           SWAP TRANSACTION                                │
├──────────────────────────────────────────────────────────────────────────┤
│ ACTION 1: Consume Input Resource + Unshield                              │
│ ├─ Nullifiers: [user_input_resource_nullifier]                           │
│ ├─ Commitments: [calldata_carrier_commitment]                            │
│ ├─ Forwarder Call:                                                       │
│ │   ├─ Target: ERC20ForwarderIn                                          │
│ │   ├─ Input: transferFrom(escrow, forwarder, amount)                    │
│ │   └─ Output: true                                                      │
│ └─ Proofs: [logic_proof, compliance_proof]                               │
├──────────────────────────────────────────────────────────────────────────┤
│ ACTION 2: Execute Swap                                                    │
│ ├─ Nullifiers: [calldata_carrier_nullifier_1]                            │
│ ├─ Commitments: [calldata_carrier_commitment_2]                          │
│ ├─ Forwarder Call:                                                       │
│ │   ├─ Target: UniswapForwarder                                          │
│ │   ├─ Input: exactInputSingle(tokenIn, tokenOut, fee, amount, ...)     │
│ │   └─ Output: amountOut                                                 │
│ └─ Proofs: [logic_proof, compliance_proof]                               │
├──────────────────────────────────────────────────────────────────────────┤
│ ACTION 3: Shield Output + Create User Resource                           │
│ ├─ Nullifiers: [calldata_carrier_nullifier_2]                            │
│ ├─ Commitments: [user_output_resource_commitment]                        │
│ ├─ Forwarder Call:                                                       │
│ │   ├─ Target: ERC20ForwarderOut                                         │
│ │   ├─ Input: transfer(escrow, amountOut)                                │
│ │   └─ Output: true                                                      │
│ └─ Proofs: [logic_proof, compliance_proof]                               │
├──────────────────────────────────────────────────────────────────────────┤
│ DELTA PROOF: Proves overall transaction balance                          │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## 6. Smart Contract Specifications

### 6.1 ERC20 Forwarder Contract

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

/**
 * @title ERC20Forwarder
 * @notice Forwarder contract for wrapping ERC20 tokens into Anoma resources
 * @dev Only callable by the Protocol Adapter
 */
contract ERC20Forwarder is Ownable {
    using SafeERC20 for IERC20;
    
    /// @notice The ERC20 token this forwarder handles
    IERC20 public immutable token;
    
    /// @notice The calldata carrier resource kind
    bytes32 public immutable calldataCarrierResourceKind;
    
    /// @notice Protocol Adapter address (set as owner)
    address public immutable protocolAdapter;
    
    event Deposited(address indexed from, uint256 amount, bytes32 commitment);
    event Withdrawn(address indexed to, uint256 amount, bytes32 nullifier);
    
    constructor(
        address _protocolAdapter,
        address _token,
        bytes32 _logicRef
    ) Ownable(_protocolAdapter) {
        protocolAdapter = _protocolAdapter;
        token = IERC20(_token);
        
        // Compute kind: hash(logicRef, labelRef) where labelRef = hash(address(this))
        bytes32 labelRef = sha256(abi.encode(address(this)));
        calldataCarrierResourceKind = sha256(abi.encode(_logicRef, labelRef));
    }
    
    /**
     * @notice Forward a call - only callable by Protocol Adapter
     * @param input The encoded function call
     * @return output The call result
     */
    function forwardCall(bytes calldata input) external onlyOwner returns (bytes memory output) {
        // Decode the function selector
        bytes4 selector = bytes4(input[:4]);
        
        if (selector == IERC20.transferFrom.selector) {
            // Deposit: transferFrom(from, to, amount)
            (address from, address to, uint256 amount) = abi.decode(input[4:], (address, address, uint256));
            require(to == address(this), "Invalid recipient");
            token.safeTransferFrom(from, address(this), amount);
            output = abi.encode(true);
            
        } else if (selector == IERC20.transfer.selector) {
            // Withdraw: transfer(to, amount)
            (address to, uint256 amount) = abi.decode(input[4:], (address, uint256));
            token.safeTransfer(to, amount);
            output = abi.encode(true);
            
        } else {
            revert("Unsupported selector");
        }
    }
    
    /**
     * @notice Get the resource kind for this forwarder
     */
    function getCalldataCarrierResourceKind() external view returns (bytes32) {
        return calldataCarrierResourceKind;
    }
}
```

### 6.2 Uniswap V3 Swap Forwarder

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

interface ISwapRouter02 {
    struct ExactInputSingleParams {
        address tokenIn;
        address tokenOut;
        uint24 fee;
        address recipient;
        uint256 amountIn;
        uint256 amountOutMinimum;
        uint160 sqrtPriceLimitX96;
    }
    
    function exactInputSingle(ExactInputSingleParams calldata params) 
        external payable returns (uint256 amountOut);
}

/**
 * @title UniswapV3Forwarder
 * @notice Forwarder contract for executing Uniswap V3 swaps via the Protocol Adapter
 */
contract UniswapV3Forwarder is Ownable {
    using SafeERC20 for IERC20;
    
    ISwapRouter02 public constant SWAP_ROUTER = 
        ISwapRouter02(0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45);
    
    bytes32 public immutable calldataCarrierResourceKind;
    
    constructor(address _protocolAdapter, bytes32 _logicRef) Ownable(_protocolAdapter) {
        bytes32 labelRef = sha256(abi.encode(address(this)));
        calldataCarrierResourceKind = sha256(abi.encode(_logicRef, labelRef));
    }
    
    /**
     * @notice Execute a swap via Uniswap V3
     * @param input Encoded ExactInputSingleParams
     * @return output The amount of tokens received
     */
    function forwardCall(bytes calldata input) external onlyOwner returns (bytes memory output) {
        // Decode swap parameters
        ISwapRouter02.ExactInputSingleParams memory params = 
            abi.decode(input, (ISwapRouter02.ExactInputSingleParams));
        
        // Ensure recipient is this contract (for re-shielding)
        require(params.recipient == address(this), "Invalid recipient");
        
        // Approve router to spend input tokens
        IERC20(params.tokenIn).safeApprove(address(SWAP_ROUTER), params.amountIn);
        
        // Execute swap
        uint256 amountOut = SWAP_ROUTER.exactInputSingle(params);
        
        // Clear approval
        IERC20(params.tokenIn).safeApprove(address(SWAP_ROUTER), 0);
        
        output = abi.encode(amountOut);
    }
    
    /**
     * @notice Rescue tokens (only callable by Protocol Adapter in emergency)
     */
    function rescueTokens(address token, address to, uint256 amount) external onlyOwner {
        IERC20(token).safeTransfer(to, amount);
    }
}
```

### 6.3 CoWSwap Integration Forwarder

```solidity
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

interface IGPv2Settlement {
    function setPreSignature(bytes calldata orderUid, bool signed) external;
    function invalidateOrder(bytes calldata orderUid) external;
}

interface IGPv2VaultRelayer {
    // Standard ERC20 approval target for CoWSwap
}

/**
 * @title CoWSwapForwarder
 * @notice Forwarder for CoWSwap integration - uses pre-signing for smart contracts
 * @dev CoWSwap flow: pre-sign order -> solver fills -> settlement executes
 */
contract CoWSwapForwarder is Ownable {
    using SafeERC20 for IERC20;
    
    IGPv2Settlement public constant SETTLEMENT = 
        IGPv2Settlement(0x9008D19f58AAbD9eD0D60971565AA8510560ab41);
    
    address public constant VAULT_RELAYER = 
        0xC92E8bdf79f0507f65a392b0ab4667716BFE0110;
    
    bytes32 public immutable calldataCarrierResourceKind;
    
    // Track pre-signed orders
    mapping(bytes32 => bool) public preSignedOrders;
    
    event OrderPreSigned(bytes32 indexed orderUid);
    event OrderInvalidated(bytes32 indexed orderUid);
    
    constructor(address _protocolAdapter, bytes32 _logicRef) Ownable(_protocolAdapter) {
        bytes32 labelRef = sha256(abi.encode(address(this)));
        calldataCarrierResourceKind = sha256(abi.encode(_logicRef, labelRef));
    }
    
    /**
     * @notice Forward calls for CoWSwap operations
     * @param input Encoded function call (preSign, invalidate, or approve)
     */
    function forwardCall(bytes calldata input) external onlyOwner returns (bytes memory output) {
        bytes4 selector = bytes4(input[:4]);
        
        if (selector == this.preSignOrder.selector) {
            // Pre-sign an order
            (bytes memory orderUid, address sellToken, uint256 sellAmount) = 
                abi.decode(input[4:], (bytes, address, uint256));
            
            // Approve vault relayer to spend tokens
            IERC20(sellToken).safeApprove(VAULT_RELAYER, sellAmount);
            
            // Pre-sign the order
            SETTLEMENT.setPreSignature(orderUid, true);
            preSignedOrders[keccak256(orderUid)] = true;
            
            emit OrderPreSigned(keccak256(orderUid));
            output = abi.encode(true);
            
        } else if (selector == this.invalidateOrder.selector) {
            // Cancel a pre-signed order
            bytes memory orderUid = abi.decode(input[4:], (bytes));
            SETTLEMENT.invalidateOrder(orderUid);
            preSignedOrders[keccak256(orderUid)] = false;
            
            emit OrderInvalidated(keccak256(orderUid));
            output = abi.encode(true);
            
        } else {
            revert("Unsupported selector");
        }
    }
    
    // Selectors for encoding
    function preSignOrder(bytes memory, address, uint256) external pure {}
    function invalidateOrder(bytes memory) external pure {}
}
```

---

## 7. DEX Integration

### 7.1 Uniswap V2 Integration

**Router Address:** `0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D`

```solidity
interface IUniswapV2Router02 {
    function swapExactTokensForTokens(
        uint amountIn,
        uint amountOutMin,
        address[] calldata path,
        address to,
        uint deadline
    ) external returns (uint[] memory amounts);
    
    function getAmountsOut(uint amountIn, address[] calldata path) 
        external view returns (uint[] memory amounts);
}
```

**Forwarder Implementation Notes:**
- Use `swapExactTokensForTokens` for exact input swaps
- Always set `to` address to the forwarder contract
- Query `getAmountsOut` off-chain for quote estimation

### 7.2 Uniswap V3 Integration

**Router Address:** `0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45` (SwapRouter02)

**Key Functions:**
- `exactInputSingle`: Single-hop swaps with exact input
- `exactInput`: Multi-hop swaps with exact input
- `exactOutputSingle`: Single-hop swaps with exact output
- `exactOutput`: Multi-hop swaps with exact output

**Fee Tiers:**
- 0.01% (100) - Stable pairs
- 0.05% (500) - Stable pairs
- 0.30% (3000) - Standard pairs
- 1.00% (10000) - Exotic pairs

### 7.3 CoWSwap Integration

**Settlement Contract:** `0x9008D19f58AAbD9eD0D60971565AA8510560ab41`
**Vault Relayer:** `0xC92E8bdf79f0507f65a392b0ab4667716BFE0110`

**Integration Approach:**

CoWSwap is fundamentally different from Uniswap - it's an **intent-based** DEX where:
1. Users sign orders off-chain
2. Solvers compete to fill orders
3. Best execution is guaranteed through auction

**For Smart Contracts (Forwarders):**
- Use `setPreSignature(orderUid, true)` to pre-sign orders
- CoWSwap solver will execute when economically optimal
- Output tokens sent to forwarder for re-shielding

**Order Flow:**
```
1. User intent → 2. Generate CoW order → 3. Pre-sign via forwarder →
4. Submit to CoW API → 5. Solver fills → 6. Tokens arrive at forwarder →
7. Re-shield to user's new commitment
```

---

## 8. Relayer System

### 8.1 Relayer Architecture

Relayers are essential for privacy because:
1. **Gas Payment**: Users can withdraw to fresh addresses with no ETH
2. **Transaction Submission**: Hides the sender's identity
3. **Intent Matching**: Combines user intents into efficient batches

```
┌──────────────────────────────────────────────────────────────────┐
│                        RELAYER NODE                               │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────┐      ┌──────────────┐      ┌──────────────┐   │
│  │ Intent Pool  │ ───► │   Solver     │ ───► │  TX Builder  │   │
│  │   Monitor    │      │   Engine     │      │              │   │
│  └──────────────┘      └──────────────┘      └──────┬───────┘   │
│                                                      │           │
│  ┌──────────────┐      ┌──────────────┐             │           │
│  │ Price Oracle │      │ DEX Router   │ ◄───────────┤           │
│  │   (quotes)   │      │  (optimal)   │             │           │
│  └──────────────┘      └──────────────┘             │           │
│                                                      ▼           │
│                                              ┌──────────────┐   │
│                                              │  Gas Manager │   │
│                                              │ (hot wallet) │   │
│                                              └──────┬───────┘   │
└─────────────────────────────────────────────────────┼───────────┘
                                                      │
                                                      ▼
                                            ┌──────────────────┐
                                            │ Ethereum Network │
                                            └──────────────────┘
```

### 8.2 Relayer Fee Model

```solidity
struct RelayerFee {
    uint256 baseFeeGwei;         // Base gas price to use
    uint256 priorityFeeGwei;     // Priority fee for fast inclusion
    uint256 serviceFeePercent;   // Relayer service fee (e.g., 0.1%)
    uint256 minServiceFee;       // Minimum fee in output token
}
```

**Fee Deduction:**
- Fee is deducted from output tokens before re-shielding
- User specifies `amountOutMinimum` accounting for fees
- Relayer's address receives fee via separate transfer

### 8.3 Relayer Registration

```solidity
interface IRelayerRegistry {
    struct Relayer {
        address relayerAddress;
        string endpoint;           // API endpoint URL
        uint256 stake;             // Staked amount for slashing
        uint256 serviceFee;        // Fee percentage (basis points)
        bool active;
    }
    
    function register(string calldata endpoint) external payable;
    function stake() external payable;
    function unstake(uint256 amount) external;
    function slash(address relayer, uint256 amount, bytes calldata evidence) external;
}
```

### 8.4 Relayer API Specification

**Endpoints:**

```
POST /api/v1/intent
  - Submit a shielded swap intent
  - Body: { intent, proofs, signature }
  - Returns: { intentId, estimatedExecution }

GET /api/v1/intent/{intentId}
  - Query intent status
  - Returns: { status, txHash, settledAmount }

GET /api/v1/quote
  - Get a quote for a swap
  - Query: tokenIn, tokenOut, amountIn
  - Returns: { amountOut, fee, route }

GET /api/v1/status
  - Relayer health check
  - Returns: { healthy, queueDepth, gasPrice }
```

---

## 9. Proof Generation

### 9.1 RISC Zero Integration

The Protocol Adapter uses [RISC Zero](https://risczero.com/) for ZK proof generation:

**Proof Types:**

1. **Resource Logic Proofs**: Verify resource validity
   - Ownership (correct nullifier key)
   - Kind matching (correct token type)
   - Quantity bounds

2. **Compliance Proofs**: Verify action validity
   - Input resource exists (Merkle proof of commitment)
   - Output resource correctly computed
   - Nullifier correctly derived

3. **Delta Proofs**: Verify transaction balance
   - Sum of inputs equals sum of outputs
   - Uses elliptic curve arithmetic (secp256k1)

### 9.2 Proof Generation Flow

```rust
// Pseudo-code for client-side proof generation

use risc0_zkvm::{prove, Receipt};

struct ShieldedSwapWitness {
    // Private inputs
    input_resource: Resource,
    input_nullifier_key: [u8; 32],
    merkle_path: Vec<[u8; 32]>,
    
    // Public inputs
    merkle_root: [u8; 32],
    nullifier: [u8; 32],
    output_commitment: [u8; 32],
}

fn generate_proof(witness: ShieldedSwapWitness) -> Receipt {
    let env = risc0_zkvm::ExecutorEnv::builder()
        .write(&witness)
        .build()
        .unwrap();
    
    let prover = risc0_zkvm::default_prover();
    let receipt = prover.prove(env, RESOURCE_LOGIC_ELF).unwrap();
    
    receipt
}
```

### 9.3 Verification on EVM

```solidity
// Verification uses RISC Zero's on-chain verifier
interface IRiscZeroVerifier {
    function verify(
        bytes calldata seal,
        bytes32 imageId,
        bytes32 journalDigest
    ) external view returns (bool);
}

// Called by Protocol Adapter during settlement
function verifyProof(
    bytes calldata proof,
    bytes32 expectedImageId,
    bytes32 publicInputsHash
) internal view returns (bool) {
    return riscZeroVerifier.verify(proof, expectedImageId, publicInputsHash);
}
```

---

## 10. Security Considerations

### 10.1 Attack Vectors & Mitigations

| Attack | Description | Mitigation |
|--------|-------------|------------|
| Front-running | MEV bots observe intent, execute first | Use CoWSwap for MEV protection; set tight slippage |
| Replay attack | Reuse old transaction proofs | Nullifiers prevent double-spend; unique nonces |
| Forwarder exploit | Malicious forwarder drains funds | PA only calls registered forwarders; audit all forwarders |
| Relayer censorship | Relayer refuses to submit tx | Multiple relayers; fallback direct submission |
| Timing analysis | Correlate deposit/withdraw timing | Encourage users to wait; anonymity set growth |

### 10.2 Smart Contract Security

**Required Audits:**
- Forwarder contracts (all variants)
- Relayer registry
- Any modifications to PA interaction

**Best Practices:**
- Use OpenZeppelin contracts where possible
- Implement reentrancy guards
- Use SafeERC20 for token transfers
- Validate all external call return values

### 10.3 Privacy Guarantees

**What IS hidden:**
- Link between deposit and withdrawal addresses
- Specific amounts (when using fixed denominations)
- Swap details (input/output linkage)

**What is NOT hidden:**
- Total value entering/exiting the system
- Timing patterns (mitigate with delays)
- Forwarder contract interactions (visible on-chain)

### 10.4 Compliance Considerations

- Implement voluntary compliance tool (like Tornado Cash compliance)
- Allow users to generate proofs of source
- Consider geographic restrictions per jurisdiction
- Implement OFAC screening on relayer side

---

## 11. Implementation Roadmap

### Phase 1: Core Infrastructure (Weeks 1-4)

**Deliverables:**
1. ERC20Forwarder contract (single token: USDC)
2. Basic relayer node (single instance)
3. CLI tool for shielding/unshielding
4. Integration tests on Sepolia

**Milestones:**
- [ ] Deploy ERC20Forwarder for USDC on Sepolia
- [ ] Implement proof generation client (Rust)
- [ ] Basic relayer with intent queue
- [ ] End-to-end shield/unshield flow

### Phase 2: Uniswap Integration (Weeks 5-8)

**Deliverables:**
1. UniswapV3Forwarder contract
2. Quote service (price estimation)
3. Swap routing logic
4. Extended CLI for swaps

**Milestones:**
- [ ] Deploy UniswapV3Forwarder on Sepolia
- [ ] Implement swap intent format
- [ ] Add Uniswap quote integration
- [ ] End-to-end shielded swap flow

### Phase 3: Production Hardening (Weeks 9-12)

**Deliverables:**
1. Multiple token support (ETH, WETH, USDT, DAI)
2. Relayer registry with staking
3. Web UI (basic)
4. Mainnet deployment

**Milestones:**
- [ ] Security audit completion
- [ ] Multi-token forwarder deployment
- [ ] Relayer network (3+ nodes)
- [ ] Mainnet launch (beta)

### Phase 4: Advanced Features (Weeks 13-16)

**Deliverables:**
1. CoWSwap integration
2. Multi-hop routing
3. Aggregator comparison (best execution)
4. Mobile-friendly UI

**Milestones:**
- [ ] CoWSwapForwarder deployment
- [ ] Cross-DEX routing engine
- [ ] UI polish and UX improvements
- [ ] Public launch

---

## Appendices

### A. Contract Addresses (Ethereum Mainnet)

| Contract | Address |
|----------|---------|
| Anoma Protocol Adapter | `0x46E622226F93Ed52C584F3f66135CD06AF01c86c` |
| Uniswap V3 SwapRouter02 | `0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45` |
| Uniswap V2 Router | `0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D` |
| CoWSwap Settlement | `0x9008D19f58AAbD9eD0D60971565AA8510560ab41` |
| CoWSwap Vault Relayer | `0xC92E8bdf79f0507f65a392b0ab4667716BFE0110` |
| WETH | `0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2` |
| USDC | `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48` |
| DAI | `0x6B175474E89094C44Da98b954EedeAC495271d0F` |

### B. References

1. [Anoma Specs - EVM Protocol Adapter](https://specs.anoma.net/main/arch/integrations/adapters/evm.html)
2. [Anoma Protocol Adapter GitHub](https://github.com/anoma/evm-protocol-adapter)
3. [EthResearch - Anoma PA Announcement](https://ethresear.ch/t/the-anoma-protocol-adapter-is-live-on-ethereum/23466)
4. [Uniswap V3 Docs](https://docs.uniswap.org/contracts/v3/overview)
5. [CoWSwap Documentation](https://docs.cow.fi/)
6. [RISC Zero Documentation](https://dev.risczero.com/)

### C. Glossary

| Term | Definition |
|------|------------|
| **Resource** | Immutable unit of state in Anoma; created and consumed exactly once |
| **Commitment** | Hash of resource data; stored in Merkle tree to prove existence |
| **Nullifier** | Unique identifier revealed when consuming a resource; prevents double-spend |
| **Forwarder** | Smart contract enabling PA to interact with external contracts |
| **Calldata Carrier** | Resource carrying external call data for forwarder execution |
| **Intent** | User's desired action expressed as constraints (not explicit execution path) |
| **Solver** | Off-chain entity matching intents and computing optimal execution |
| **Relayer** | Entity submitting transactions on behalf of users (gas abstraction) |
| **Shield** | Convert public ERC20 tokens to private Anoma resources |
| **Unshield** | Convert private Anoma resources back to public ERC20 tokens |

### D. Error Codes

| Code | Description |
|------|-------------|
| `SA001` | Invalid proof - verification failed |
| `SA002` | Nullifier already spent |
| `SA003` | Commitment not found in tree |
| `SA004` | Forwarder call output mismatch |
| `SA005` | Insufficient output amount (slippage exceeded) |
| `SA006` | Invalid calldata carrier kind |
| `SA007` | Relayer not registered |
| `SA008` | Intent expired |
| `SA009` | Unsupported token |
| `SA010` | Rate limit exceeded |

---

## Implementation Notes for Claude Code

### Getting Started

1. **Clone the Anoma PA repo:**
   ```bash
   git clone https://github.com/anoma/evm-protocol-adapter.git
   cd evm-protocol-adapter
   ```

2. **Study the existing forwarder base:**
   - `contracts/src/forwarders/ForwarderBase.sol`
   - `contracts/src/Types.sol` (for resource types)

3. **Begin with ERC20Forwarder:**
   - Implement the simplest shield/unshield flow first
   - Test on Sepolia before adding swap logic

4. **Proof generation:**
   - The `bindings/` folder has Rust code for type conversion
   - RISC Zero proving requires their toolchain (see their docs)

### Key Implementation Decisions

1. **Single vs. Multi-Token Forwarders:**
   - Start with one forwarder per token (simpler)
   - Later optimize with factory pattern

2. **Relayer Architecture:**
   - Start with single relayer (your own)
   - Add registry once flow is stable

3. **Denomination Strategy:**
   - Fixed denominations improve anonymity (like Tornado)
   - Variable amounts provide flexibility
   - Recommend: Start with variable, add fixed later

### Testing Strategy

1. **Unit Tests:** Each forwarder function
2. **Integration Tests:** Full shield → swap → unshield flow
3. **Fork Tests:** Test against mainnet state on Sepolia fork
4. **Fuzz Tests:** Edge cases for proof verification

---

*End of Specification*
