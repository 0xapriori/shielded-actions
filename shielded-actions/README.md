# Shielded Actions - Privacy-Preserving Swaps on Ethereum

A complete implementation of privacy-preserving token swaps using the [Anoma Protocol Adapter](https://github.com/anoma/evm-protocol-adapter) on Ethereum. This system allows users to:

1. **Shield** ERC20 tokens (convert to private Anoma resources)
2. **Swap** tokens privately via Uniswap V3 without revealing trade details on-chain
3. **Unshield** tokens (convert back to standard ERC20)

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        User's Wallet                                 │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                     Protocol Adapter                                 │
│  - Verifies RISC Zero proofs                                        │
│  - Manages commitment tree & nullifier set                          │
│  - Executes forwarder calls                                         │
└─────────────────────────────────────────────────────────────────────┘
                                  │
                    ┌─────────────┼─────────────┐
                    ▼             ▼             ▼
            ┌───────────┐ ┌───────────┐ ┌───────────────┐
            │   WETH    │ │   USDC    │ │   Uniswap V3  │
            │ Forwarder │ │ Forwarder │ │   Forwarder   │
            └───────────┘ └───────────┘ └───────────────┘
                    │             │             │
                    ▼             ▼             ▼
            ┌───────────┐ ┌───────────┐ ┌───────────────┐
            │   WETH    │ │   USDC    │ │ Uniswap V3    │
            │  Contract │ │  Contract │ │ SwapRouter02  │
            └───────────┘ └───────────┘ └───────────────┘
```

## Deployed Contracts (Sepolia Testnet)

| Contract | Address | Etherscan |
|----------|---------|-----------|
| Protocol Adapter | `0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525` | [View](https://sepolia.etherscan.io/address/0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525#code) |
| WETH Forwarder | `0xD5307D777dC60b763b74945BF5A42ba93ce44e4b` | [View](https://sepolia.etherscan.io/address/0xD5307D777dC60b763b74945BF5A42ba93ce44e4b#code) |
| USDC Forwarder | `0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE` | [View](https://sepolia.etherscan.io/address/0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE#code) |
| Uniswap V3 Forwarder | `0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA` | [View](https://sepolia.etherscan.io/address/0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA#code) |

### External Dependencies (Sepolia)

| Contract | Address |
|----------|---------|
| RISC Zero Verifier Router | `0x925d8331ddc0a1F0d96E68CF073DFE1d92b69187` |
| Uniswap V3 SwapRouter02 | `0x3bFA4769FB09eefC5a80d6E87c3B9C650f7Ae48E` |
| WETH | `0x7b79995e5f793A07Bc00c21412e50Ecae098E7f9` |
| USDC | `0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238` |

## Live Demo

The frontend is deployed on GitHub Pages and connects to the Elixir backend for proof generation.

**Frontend**: [https://0xapriori.github.io/shielded-actions/](https://0xapriori.github.io/shielded-actions/)

## Running Locally

### Backend (Elixir - Proof Generation)

```bash
cd backend

# Install dependencies
mix deps.get

# Configure environment (optional)
cp .env.example .env
# Edit .env with your BONSAI_API_KEY if using remote proving

# Start the server
mix run --no-halt
```

The backend API will be available at [http://localhost:4000](http://localhost:4000).

### Frontend (React)

```bash
cd frontend
npm install --legacy-peer-deps
npm run dev
```

Open [http://localhost:3000](http://localhost:3000) to view the demo.

The frontend will automatically connect to the backend at `http://localhost:4000`. To use a different backend URL, set the `VITE_API_URL` environment variable.

## Prerequisites

- [Foundry](https://book.getfoundry.sh/getting-started/installation)
- [Node.js](https://nodejs.org/) >= 18 (for frontend)
- Sepolia ETH for gas ([Faucet](https://sepoliafaucet.com/))
- Sepolia WETH/USDC for testing

## Quick Start

### 1. Clone and Install

```bash
git clone <repository-url>
cd shielded-actions

# Install Foundry dependencies
forge install
```

### 2. Build

```bash
forge build
```

### 3. Run Tests

```bash
forge test -vv
```

### 4. Deploy (Optional - contracts already deployed on Sepolia)

```bash
# Create .env file
cp .env.example .env
# Edit .env with your values

# Deploy
source .env
forge script script/DeployShieldedActions.s.sol:DeployShieldedActions \
    --sig "run(address)" \
    --rpc-url $SEPOLIA_RPC_URL \
    --private-key $PRIVATE_KEY \
    --broadcast \
    --verify \
    --etherscan-api-key $ETHERSCAN_API_KEY \
    <EMERGENCY_STOP_CALLER_ADDRESS>
```

## How It Works

### Shielding (Deposit)

1. User approves the ERC20Forwarder to spend their tokens
2. User creates an Anoma transaction with:
   - A resource logic proof for the "shield" operation
   - External payload calling `transferFrom` on the forwarder
3. Protocol Adapter verifies proofs and executes the transfer
4. Tokens are held in escrow by the forwarder
5. User receives a private commitment in the Anoma state

### Shielded Swap

1. User creates an Anoma transaction that:
   - Consumes the shielded input token resource
   - Creates a shielded output token resource
   - Includes external payload for Uniswap swap
2. Protocol Adapter:
   - Verifies all proofs (compliance, logic, delta)
   - Executes the swap via UniswapV3Forwarder
   - Updates commitments and nullifiers
3. The swap happens atomically with privacy preserved

### Unshielding (Withdraw)

1. User creates an Anoma transaction consuming their shielded resource
2. External payload calls `transfer` on the forwarder
3. Protocol Adapter verifies proofs and releases tokens
4. User receives tokens at their specified address

## Contract Overview

### ERC20Forwarder

Handles shielding and unshielding of ERC20 tokens:

```solidity
// Shield: transferFrom(user, forwarder, amount)
// Unshield: transfer(recipient, amount)
function forwardCall(bytes32 logicRef, bytes calldata input) external returns (bytes memory);
```

### UniswapV3Forwarder

Executes Uniswap V3 swaps:

```solidity
// Exact input swap
function exactInputSingle(ExactInputSingleParams calldata params) external;

// Exact output swap
function exactOutputSingle(ExactOutputSingleParams calldata params) external;
```

### Protocol Adapter

The core Anoma contract that:
- Verifies RISC Zero proofs (compliance, logic, aggregation)
- Maintains the commitment tree (Merkle tree of created resources)
- Tracks nullifiers (spent resources)
- Executes external forwarder calls

## Testing

```bash
# Run all tests
forge test

# Run with verbosity
forge test -vvv

# Run specific test
forge test --match-test test_deposit_shielding

# Gas report
forge test --gas-report
```

## Security Considerations

1. **Forwarder Access Control**: Only the Protocol Adapter can call forwarders
2. **Proof Verification**: All operations require valid RISC Zero proofs
3. **Emergency Stop**: Owner can pause the Protocol Adapter
4. **Reentrancy Protection**: Protocol Adapter uses transient reentrancy guards

## Project Structure

```
shielded-actions/
├── src/
│   ├── ProtocolAdapter.sol          # Anoma Protocol Adapter
│   ├── Types.sol                    # Resource types
│   ├── forwarders/
│   │   ├── ERC20Forwarder.sol       # Shield/unshield ERC20 tokens
│   │   └── UniswapV3Forwarder.sol   # Execute Uniswap V3 swaps
│   ├── interfaces/
│   ├── libs/
│   └── state/
├── script/
│   └── DeployShieldedActions.s.sol  # Deployment script
├── test/
│   ├── ERC20Forwarder.t.sol
│   └── UniswapV3Forwarder.t.sol
├── frontend/                        # React frontend
│   ├── src/
│   │   ├── App.tsx                  # Main application
│   │   ├── api.ts                   # Backend API client
│   │   └── contracts.ts             # Contract addresses & ABIs
│   └── vite.config.ts
├── backend/                         # Elixir proof generation backend
│   ├── lib/
│   │   └── backend/
│   │       ├── router.ex            # API endpoints
│   │       ├── proof_service.ex     # Proof generation using Anoma SDK
│   │       └── resource_store.ex    # Resource tracking
│   └── mix.exs
└── foundry.toml
```

## Backend API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/api/info` | GET | API info and contract addresses |
| `/api/generate-keypair` | POST | Generate nullifier keypair |
| `/api/shield` | POST | Create shield transaction proof |
| `/api/swap` | POST | Create shielded swap transaction proof |
| `/api/unshield` | POST | Create unshield transaction proof |
| `/api/resources/:address` | GET | Get shielded resources for address |

## Proof Generation

Generating valid proofs for shielded transactions requires the Anoma SDK. Here are the options:

### Option 1: Anoma SDK (Recommended for Production)

The [Anoma SDK](https://github.com/anoma/anoma-sdk) (Elixir) provides full transaction construction and proof generation:

```elixir
# Add to mix.exs
{:anoma_sdk, github: "anoma/anoma-sdk"}

# Set environment variables
BONSAI_API_URL=...
BONSAI_API_KEY=...
PROTOCOL_ADAPTER_ADDRESS=0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525
```

### Option 2: Bonsai Remote Proving

[RISC Zero Bonsai](https://dev.risczero.com/litepaper) is a remote proving service that offloads proof generation. Useful when local proving is too resource-intensive.

### Option 3: Dev Mode (Testing Only)

For local development without real proofs:

```bash
RISC0_DEV_MODE=true
```

This generates mock proofs that pass verification in test environments but are not valid on mainnet.

### Integration Architecture

For a production frontend:

```
Frontend (React) ──API──> Backend (Elixir/Rust)
                              │
                              ├── Construct Anoma Transaction
                              ├── Generate RISC Zero Proofs (via Bonsai)
                              └── Submit to Protocol Adapter
```

## Resources

- [Anoma Protocol Adapter Spec](https://specs.anoma.net/main/arch/integrations/adapters/evm.html)
- [EVM Protocol Adapter GitHub](https://github.com/anoma/evm-protocol-adapter)
- [Anoma SDK](https://github.com/anoma/anoma-sdk)
- [ARM RISC0](https://github.com/anoma/arm-risc0) - Core resource machine implementation
- [RISC Zero](https://www.risczero.com/)
- [Uniswap V3 Docs](https://docs.uniswap.org/contracts/v3/overview)

## License

MIT
