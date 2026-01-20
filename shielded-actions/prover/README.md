# Shielded Actions Prover

This directory contains the Rust prover service for generating ZK proofs for shielded transactions on Sepolia.

## Current Status

**ZK Proof Verification: WORKING**
- The ephemeral proof generation works with ARM v0.13.0
- Proofs verify on-chain via the Protocol Adapter at `0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525`
- Uses `INITIAL_ROOT` which matches the on-chain `EMPTY_HASH`

**Token Operations: NOT YET IMPLEMENTED**
- Shield/swap/unshield require a custom logic circuit that outputs `external_payload`
- The current `TrivialLogicWitness` outputs empty `external_payload`
- Token transfers via forwarders aren't triggered without `external_payload`

## Architecture

### Proof Flow

```
User Request → Prover Service → ARM SDK → RISC Zero zkVM → Groth16 Proof
                                                              ↓
Protocol Adapter (Sepolia) ← EVM ABI Encode ← Transaction ← Proof
```

### Key Components

1. **local_prove.rs** - CLI for generating proofs locally
2. **prover.rs** - HTTP prover service (for backend integration)
3. **shield_logic.rs** - Custom logic witness (needs circuit compilation)

## Usage

### Prerequisites

```bash
# Install RISC Zero
curl -L https://risczero.com/install | sh
rzup install

# Ensure Docker is running (for Groth16 proofs)
docker info
```

### Generate Test Proof

```bash
# Ephemeral proof (verifies on-chain)
cargo run --release --bin local-prove -- test-ephemeral

# Check INITIAL_ROOT matches on-chain
cargo run --release --bin local-prove -- check-root
```

### Run Prover Service

```bash
# Set environment
export USE_REAL_ARM=1  # Enable real proofs (requires Docker)

# Run server
cargo run --release --bin shielded-prover
```

## Required Work for Full Token Operations

To enable real shield/swap/unshield with token transfers:

### 1. Create Shield Logic Circuit

Create a RISC Zero guest program that:
- Takes `ShieldLogicWitness` as input
- Outputs `LogicInstance` with populated `external_payload`
- Gets compiled to an ELF binary

Location: `circuits/shield_logic/methods/guest/src/main.rs`

```rust
use risc0_zkvm::guest::env;
use shield_logic::ShieldLogicWitness;

fn main() {
    let witness: ShieldLogicWitness = env::read();
    let instance = witness.constrain().unwrap();
    env::commit(&instance);
}
```

### 2. Compile the Circuit

```bash
cd circuits/shield_logic
cargo build --release
```

This produces:
- ELF binary (`methods/guest/target/riscv32im-risc0-zkvm-elf/release/shield-logic`)
- Image ID (verifying key)

### 3. Update Shield Operations

1. Use the new verifying key in resource creation
2. Build proofs using `ShieldLogicWitness`
3. Include proper `external_payload` encoding

### External Payload Format

For forwarder calls, encode as:
```solidity
abi.encode(forwarderAddress, input, expectedOutput)
```

Where `input` is:
- Shield: `transferFrom(sender, forwarder, amount)`
- Unshield: `transfer(recipient, amount)`
- Swap: `exactInputSingle(params)`

## Contracts (Sepolia)

| Contract | Address |
|----------|---------|
| Protocol Adapter | 0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525 |
| WETH Forwarder | 0xD5307D777dC60b763b74945BF5A42ba93ce44e4b |
| USDC Forwarder | 0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE |
| Uniswap Forwarder | 0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA |

## Testing On-Chain

```bash
# Test with cast (dry run)
cast call 0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525 \
  --data 0x$(xxd -p ephemeral_test_tx.bin | tr -d '\n') \
  --rpc-url https://ethereum-sepolia-rpc.publicnode.com

# Execute on-chain
cast send 0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525 \
  --data 0x$(xxd -p ephemeral_test_tx.bin | tr -d '\n') \
  --rpc-url https://ethereum-sepolia-rpc.publicnode.com \
  --private-key <YOUR_KEY> \
  --gas-limit 1500000
```

## Dependencies

- `arm` v0.13.0 - Anoma Resource Machine SDK
- `arm_tests` v0.13.0 - Test utilities
- `evm_protocol_adapter_bindings` - EVM ABI encoding
- `risc0-zkvm` - ZK proof generation
- `bonsai-sdk` - Remote proving (optional)
# Trigger redeploy
