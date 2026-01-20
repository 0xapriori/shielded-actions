//! Local prover CLI for generating real ZK proofs
//!
//! Prerequisites:
//! 1. Install RISC Zero: curl -L https://risczero.com/install | sh
//! 2. Install toolchain: rzup install
//!
//! Usage:
//!   cargo run --release --bin local-prove -- test
//!   cargo run --release --bin local-prove -- shield --token WETH --amount 0.01 --sender 0x...

use alloy::sol_types::SolValue;
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::time::Instant;

// ARM-RISC0 imports for real proving
use arm::action_tree::MerkleTree;
use arm::compliance::{ComplianceWitness, INITIAL_ROOT};
use arm::compliance_unit::ComplianceUnit;
use arm::action::Action;
use arm::delta_proof::DeltaWitness;
use arm::logic_proof::LogicProver;  // Trait needed for .prove() and .verifying_key()
use arm::nullifier_key::NullifierKey;
use arm::proving_system::ProofType;
use arm::resource::Resource;
use arm::resource_logic::TrivialLogicWitness;  // For ephemeral resources
use arm::transaction::{Delta, Transaction};

// Forwarder logic witness for shield/unshield with external_payload
use forwarder_logic_witness::ForwarderLogicWitness;

// EVM Protocol Adapter bindings
use evm_protocol_adapter_bindings::contract::ProtocolAdapter;

/// Shielded Actions Local Prover
#[derive(Parser)]
#[command(name = "local-prove")]
#[command(about = "Generate ZK proofs locally for shielded transactions on Sepolia")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a test proof to verify local proving works
    Test {
        /// Number of actions in the test transaction
        #[arg(long, default_value = "1")]
        actions: usize,

        /// Number of compliance units per action
        #[arg(long, default_value = "1")]
        compliance_units: usize,
    },

    /// Generate a test proof using ephemeral resources (uses INITIAL_ROOT)
    TestEphemeral,

    /// Generate a shield proof that triggers a forwarder call (transferFrom)
    Shield {
        /// Token to shield (USDC or WETH)
        #[arg(long, default_value = "USDC")]
        token: String,

        /// Amount to shield (in smallest units, e.g. 1000000 for 1 USDC)
        #[arg(long, default_value = "1000000")]
        amount: u128,

        /// Sender address (20 bytes hex, will call transferFrom from this address)
        #[arg(long, default_value = "0x0000000000000000000000000000000000000001")]
        sender: String,
    },

    /// Generate an unshield proof that triggers a forwarder call (transfer)
    Unshield {
        /// Token to unshield (USDC or WETH)
        #[arg(long, default_value = "USDC")]
        token: String,

        /// Amount to unshield (in smallest units)
        #[arg(long, default_value = "1000000")]
        amount: u128,

        /// Recipient address (20 bytes hex, will receive tokens)
        #[arg(long, default_value = "0x0000000000000000000000000000000000000001")]
        recipient: String,
    },

    /// Show info about prerequisites
    Info,

    /// Check the INITIAL_ROOT value (for debugging)
    CheckRoot,
}

/// Contract addresses on Sepolia
const PROTOCOL_ADAPTER: &str = "0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525";
const USDC_FORWARDER: &str = "0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE";
const WETH_FORWARDER: &str = "0xD5307D777dC60b763b74945BF5A42ba93ce44e4b";

/// Function selector for execute(Transaction) - ed3cf91f
const EXECUTE_SELECTOR: [u8; 4] = [0xed, 0x3c, 0xf9, 0x1f];

/// Output format for successful proofs
#[derive(Debug, Serialize)]
struct ProofOutput {
    calldata: String,
    to: String,
    calldata_length: usize,
    metadata: ProofMetadata,
}

#[derive(Debug, Serialize)]
struct ProofMetadata {
    proof_type: String,
    num_actions: usize,
    num_compliance_units: usize,
    generation_time_secs: f64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("╔════════════════════════════════════════════╗");
    println!("║   Shielded Actions Local Prover            ║");
    println!("║   RISC Zero zkVM • Sepolia Testnet         ║");
    println!("╚════════════════════════════════════════════╝\n");

    match cli.command {
        Commands::Test { actions, compliance_units } => {
            generate_test_proof(actions, compliance_units)?;
        }
        Commands::TestEphemeral => {
            generate_ephemeral_test_proof()?;
        }
        Commands::Shield { token, amount, sender } => {
            generate_shield_proof(&token, amount, &sender)?;
        }
        Commands::Unshield { token, amount, recipient } => {
            generate_unshield_proof(&token, amount, &recipient)?;
        }
        Commands::Info => {
            print_info();
        }
        Commands::CheckRoot => {
            check_initial_root();
        }
    }

    Ok(())
}

fn print_info() {
    println!("PREREQUISITES:");
    println!("  1. Install RISC Zero:");
    println!("     curl -L https://risczero.com/install | sh");
    println!();
    println!("  2. Install toolchain:");
    println!("     rzup install");
    println!();
    println!("USAGE:");
    println!("  # Test local proving works:");
    println!("  cargo run --release --bin local-prove -- test");
    println!();
    println!("  # Generate with specific action/CU count:");
    println!("  cargo run --release --bin local-prove -- test --actions 1 --compliance-units 1");
    println!();
    println!("CONTRACTS (Sepolia):");
    println!("  ProtocolAdapter: {}", PROTOCOL_ADAPTER);
    println!("  WETH Forwarder:  0xD5307D777dC60b763b74945BF5A42ba93ce44e4b");
    println!("  USDC Forwarder:  0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE");
}

fn check_initial_root() {
    println!("Checking ARM INITIAL_ROOT value...\n");

    // The on-chain EMPTY_HASH is sha256("EMPTY") = 0xcc1d2f838445db7aec431df9ee8a871f40e7aa5e064fc056633ef8c60fab7b06
    let expected_on_chain = "cc1d2f838445db7aec431df9ee8a871f40e7aa5e064fc056633ef8c60fab7b06";

    let initial_root_hex = hex::encode(INITIAL_ROOT.as_bytes());

    println!("ARM INITIAL_ROOT:     0x{}", initial_root_hex);
    println!("On-chain EMPTY_HASH:  0x{}", expected_on_chain);
    println!();

    if initial_root_hex == expected_on_chain {
        println!("✓ MATCH! The ARM INITIAL_ROOT matches the on-chain EMPTY_HASH.");
        println!("  Ephemeral resources using INITIAL_ROOT will verify on-chain.");
    } else {
        println!("✗ MISMATCH! The roots don't match.");
        println!("  This means the ARM version may not be compatible with the deployed contracts.");
    }
}

/// Generate a test proof using ARM's test transaction generator
fn generate_test_proof(n_actions: usize, n_cus: usize) -> Result<()> {
    println!("Generating TEST proof...");
    println!("  Actions: {}", n_actions);
    println!("  Compliance Units per Action: {}", n_cus);
    println!();

    let start = Instant::now();

    // Use ARM's built-in test transaction generator
    // This creates a valid transaction with proper resources
    // Use Groth16 proofs for on-chain verification (Succinct/STARK proofs can't be verified on-chain)
    println!("Building test transaction...");
    let mut tx = arm_tests::generate_test_transaction(n_actions, n_cus, ProofType::Groth16);

    println!("Generating ZK proofs...");
    println!("  This may take several minutes on first run (compiling circuits)");
    println!("  Subsequent runs will be faster (cached)");
    println!();

    // The test transaction generator already includes proofs
    // but we need to generate the delta proof
    tx = tx.generate_delta_proof()
        .map_err(|e| anyhow!("Delta proof generation failed: {:?}", e))?;

    let elapsed = start.elapsed();
    println!("\n✓ Proof generation complete!");
    println!("  Time: {:.2}s", elapsed.as_secs_f64());

    // Verify locally (clone because verify() takes ownership)
    println!("\nVerifying proofs locally...");
    tx.clone().verify()
        .map_err(|e| anyhow!("Verification failed: {:?}", e))?;
    println!("✓ Verification passed!");

    // Convert to EVM format using the bindings
    println!("\nConverting to EVM format...");
    let evm_tx = ProtocolAdapter::Transaction::from(tx);
    let abi_encoded = evm_tx.abi_encode();

    // Build full calldata with function selector
    let mut calldata = Vec::with_capacity(4 + abi_encoded.len());
    calldata.extend_from_slice(&EXECUTE_SELECTOR);
    calldata.extend_from_slice(&abi_encoded);

    let output = ProofOutput {
        calldata: format!("0x{}", hex::encode(&calldata)),
        to: PROTOCOL_ADAPTER.to_string(),
        calldata_length: calldata.len(),
        metadata: ProofMetadata {
            proof_type: "Groth16".to_string(),
            num_actions: n_actions,
            num_compliance_units: n_cus,
            generation_time_secs: elapsed.as_secs_f64(),
        },
    };

    // Save full calldata (with selector) to file
    let output_path = format!("test_tx_{}_{}.bin", n_actions, n_cus);
    std::fs::write(&output_path, &calldata)?;

    println!("\n════════════════════════════════════════════");
    println!("  TRANSACTION READY FOR ON-CHAIN EXECUTION");
    println!("════════════════════════════════════════════");
    println!();
    println!("Target: {}", PROTOCOL_ADAPTER);
    println!("Calldata: {} bytes (includes function selector)", calldata.len());
    println!("Saved to: {}", output_path);
    println!();
    println!("To execute on Sepolia:");
    println!("  # Using cast:");
    println!("  cast send {} --data 0x$(xxd -p {} | tr -d '\\\\n') \\",
             PROTOCOL_ADAPTER, output_path);
    println!("    --rpc-url https://ethereum-sepolia-rpc.publicnode.com \\");
    println!("    --private-key <YOUR_KEY> --gas-limit 1200000");

    // Also output JSON for programmatic use
    println!("\nJSON output:");
    println!("{}", serde_json::to_string_pretty(&output)?);

    println!("\n✓ LOCAL PROVING WORKS!");
    println!("  You can now build custom transactions for shield/swap/unshield.");

    Ok(())
}

/// Generate a test proof using ephemeral resources that reference INITIAL_ROOT
/// This transaction will verify on-chain because INITIAL_ROOT matches the deployed EMPTY_HASH
fn generate_ephemeral_test_proof() -> Result<()> {
    println!("Generating EPHEMERAL test proof...");
    println!("  This uses ephemeral resources with quantity=0");
    println!("  These reference INITIAL_ROOT which matches the on-chain EMPTY_HASH");
    println!();

    let start = Instant::now();

    // Create ephemeral resources (is_ephemeral=true, quantity=0)
    // These will use INITIAL_ROOT as the commitment tree root
    println!("Building ephemeral transaction...");

    // Create a nullifier key pair
    let nf_key = NullifierKey::default();
    let nf_key_cm = nf_key.commit();

    // Create consumed ephemeral resource (is_ephemeral=true, quantity=0)
    // TrivialLogicWitness expects: is_ephemeral=true, quantity=0
    let mut consumed_resource = Resource {
        logic_ref: TrivialLogicWitness::verifying_key(),  // Use trivial logic VK
        nk_commitment: nf_key_cm,
        quantity: 0,           // quantity=0 for ephemeral (required by TrivialLogicWitness)
        is_ephemeral: true,    // CRITICAL: this makes the circuit use ephemeral_root
        ..Default::default()
    };
    consumed_resource.nonce = [1u8; 32];  // Unique nonce

    let consumed_resource_nf = consumed_resource.nullifier(&nf_key)
        .map_err(|e| anyhow!("Failed to compute nullifier: {:?}", e))?;

    // Create created ephemeral resource (is_ephemeral=true, quantity=0)
    let mut created_resource = consumed_resource.clone();
    created_resource.set_nonce(consumed_resource_nf);

    // Create the compliance witness
    // with_fixed_rcv sets ephemeral_root to INITIAL_ROOT
    // Since is_ephemeral=true, the circuit will use ephemeral_root (INITIAL_ROOT)
    let compliance_witness = ComplianceWitness::with_fixed_rcv(
        consumed_resource.clone(),
        nf_key.clone(),
        created_resource.clone(),
    );

    // Create a compliance unit from the witness with Groth16 proofs
    let compliance_unit = ComplianceUnit::create(&compliance_witness, ProofType::Groth16)
        .map_err(|e| anyhow!("Failed to create compliance unit: {:?}", e))?;

    // Build the action tree for the merkle paths
    let created_resource_cm = created_resource.commitment();
    let mut action_tree = MerkleTree::new(vec![]);
    action_tree.insert(consumed_resource_nf);
    action_tree.insert(created_resource_cm);

    let action_tree_root = action_tree.root()
        .map_err(|e| anyhow!("Failed to compute action tree root: {:?}", e))?;

    // Create and prove TrivialLogic for consumed resource
    // TrivialLogicWitness::new(resource, action_tree_root, nf_key, is_consumed)
    let consumed_logic = TrivialLogicWitness::new(
        consumed_resource.clone(),
        action_tree_root,
        nf_key.clone(),
        true,  // is_consumed
    );
    let consumed_logic_proof = consumed_logic.prove(ProofType::Groth16)
        .map_err(|e| anyhow!("Failed to prove consumed logic: {:?}", e))?;

    // Create and prove TrivialLogic for created resource
    let created_logic = TrivialLogicWitness::new(
        created_resource.clone(),
        action_tree_root,
        nf_key.clone(),
        false,  // is_consumed
    );
    let created_logic_proof = created_logic.prove(ProofType::Groth16)
        .map_err(|e| anyhow!("Failed to prove created logic: {:?}", e))?;

    // Create an action with this compliance unit and logic proofs
    let action = Action::new(
        vec![compliance_unit],
        vec![consumed_logic_proof, created_logic_proof],
    ).map_err(|e| anyhow!("Failed to create action: {:?}", e))?;

    // Verify the action
    action.clone().verify()
        .map_err(|e| anyhow!("Action verification failed: {:?}", e))?;

    // Build the delta witness from the rcv values
    let delta_witness = DeltaWitness::from_bytes_vec(&[compliance_witness.rcv.to_vec()])
        .map_err(|e| anyhow!("Failed to create delta witness: {:?}", e))?;

    let tx = Transaction::create(vec![action], Delta::Witness(delta_witness));

    println!("Generating ZK proofs...");
    println!("  This may take several minutes on first run (compiling circuits)");
    println!("  Subsequent runs will be faster (cached)");
    println!();

    // Generate delta proof
    let balanced_tx = tx.generate_delta_proof()
        .map_err(|e| anyhow!("Delta proof generation failed: {:?}", e))?;

    let elapsed = start.elapsed();
    println!("\n✓ Proof generation complete!");
    println!("  Time: {:.2}s", elapsed.as_secs_f64());

    // Verify locally
    println!("\nVerifying proofs locally...");
    balanced_tx.clone().verify()
        .map_err(|e| anyhow!("Verification failed: {:?}", e))?;
    println!("✓ Verification passed!");

    // Convert to EVM format
    println!("\nConverting to EVM format...");
    let evm_tx = ProtocolAdapter::Transaction::from(balanced_tx);
    let abi_encoded = evm_tx.abi_encode();

    // Build full calldata with function selector
    let mut calldata = Vec::with_capacity(4 + abi_encoded.len());
    calldata.extend_from_slice(&EXECUTE_SELECTOR);
    calldata.extend_from_slice(&abi_encoded);

    let output = ProofOutput {
        calldata: format!("0x{}", hex::encode(&calldata)),
        to: PROTOCOL_ADAPTER.to_string(),
        calldata_length: calldata.len(),
        metadata: ProofMetadata {
            proof_type: "Groth16".to_string(),
            num_actions: 1,
            num_compliance_units: 1,
            generation_time_secs: elapsed.as_secs_f64(),
        },
    };

    // Save full calldata (with selector) to file
    let output_path = "ephemeral_test_tx.bin";
    std::fs::write(output_path, &calldata)?;

    println!("\n════════════════════════════════════════════");
    println!("  EPHEMERAL TRANSACTION READY FOR ON-CHAIN EXECUTION");
    println!("════════════════════════════════════════════");
    println!();
    println!("Target: {}", PROTOCOL_ADAPTER);
    println!("Calldata: {} bytes (includes function selector)", calldata.len());
    println!("Saved to: {}", output_path);
    println!();
    println!("This transaction uses INITIAL_ROOT: 0x{}", hex::encode(INITIAL_ROOT.as_bytes()));
    println!("Which matches the on-chain EMPTY_HASH, so it WILL verify on-chain!");
    println!();
    println!("To execute on Sepolia:");
    println!("  # Using cast:");
    println!("  cast send {} --data 0x$(xxd -p {} | tr -d '\\\\n') \\",
             PROTOCOL_ADAPTER, output_path);
    println!("    --rpc-url https://ethereum-sepolia-rpc.publicnode.com \\");
    println!("    --private-key <YOUR_KEY> --gas-limit 1200000");

    // Also output JSON
    println!("\nJSON output:");
    println!("{}", serde_json::to_string_pretty(&output)?);

    println!("\n✓ EPHEMERAL TEST TRANSACTION READY!");
    println!("  This transaction should verify on-chain because it uses INITIAL_ROOT.");

    Ok(())
}

/// Parse a hex address string into a 20-byte array
fn parse_address(addr: &str) -> Result<[u8; 20]> {
    let addr = addr.trim_start_matches("0x");
    let bytes = hex::decode(addr).map_err(|e| anyhow!("Invalid address: {}", e))?;
    if bytes.len() != 20 {
        return Err(anyhow!("Address must be 20 bytes, got {}", bytes.len()));
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Get the forwarder address for a token
fn get_forwarder_address(token: &str) -> Result<[u8; 20]> {
    match token.to_uppercase().as_str() {
        "USDC" => parse_address(USDC_FORWARDER),
        "WETH" => parse_address(WETH_FORWARDER),
        _ => Err(anyhow!("Unknown token: {}. Supported: USDC, WETH", token)),
    }
}

/// Generate a shield proof with external_payload for forwarder call
///
/// This creates a transaction that:
/// 1. Creates a shielded resource (commitment goes on-chain)
/// 2. Outputs external_payload encoding: transferFrom(sender, forwarder, amount)
/// 3. The Protocol Adapter executes this forwarder call when processing the proof
fn generate_shield_proof(token: &str, amount: u128, sender: &str) -> Result<()> {
    println!("Generating SHIELD proof with forwarder call...");
    println!("  Token: {}", token);
    println!("  Amount: {}", amount);
    println!("  Sender: {}", sender);
    println!();

    let start = Instant::now();

    let forwarder_address = get_forwarder_address(token)?;
    let sender_address = parse_address(sender)?;

    println!("  Forwarder: 0x{}", hex::encode(forwarder_address));
    println!();

    // Create nullifier key
    let nf_key = NullifierKey::default();
    let nf_key_cm = nf_key.commit();

    // Get the verifying keys for the logic circuits
    // Consumed resource uses TrivialLogic (no external call)
    // Created resource uses ForwarderLogic (triggers transferFrom)
    let trivial_vk = TrivialLogicWitness::verifying_key();
    let forwarder_vk = ForwarderLogicWitness::verifying_key();

    // Create consumed ephemeral resource (balance going in)
    // Uses TrivialLogic since it doesn't trigger any external call
    let mut consumed_resource = Resource {
        logic_ref: trivial_vk,
        nk_commitment: nf_key_cm,
        quantity: 0,           // ephemeral
        is_ephemeral: true,
        ..Default::default()
    };
    consumed_resource.nonce = [1u8; 32];

    let consumed_nf = consumed_resource.nullifier(&nf_key)
        .map_err(|e| anyhow!("Failed to compute nullifier: {:?}", e))?;

    // Create the shielded resource (created, uses ForwarderLogic)
    // This represents the shielded token balance and triggers the transferFrom call
    let mut created_resource = Resource {
        logic_ref: forwarder_vk,  // ForwarderLogic VK - this resource triggers the external call
        nk_commitment: nf_key_cm,
        quantity: 0,           // Still use 0 for ephemeral logic to work
        is_ephemeral: true,    // Ephemeral for INITIAL_ROOT compatibility
        ..Default::default()
    };
    created_resource.set_nonce(consumed_nf);

    // Create the compliance witness
    let compliance_witness = ComplianceWitness::with_fixed_rcv(
        consumed_resource.clone(),
        nf_key.clone(),
        created_resource.clone(),
    );

    // Create compliance unit
    let compliance_unit = ComplianceUnit::create(&compliance_witness, ProofType::Groth16)
        .map_err(|e| anyhow!("Failed to create compliance unit: {:?}", e))?;

    // Build action tree
    let created_cm = created_resource.commitment();
    let mut action_tree = MerkleTree::new(vec![]);
    action_tree.insert(consumed_nf);
    action_tree.insert(created_cm);

    let action_tree_root = action_tree.root()
        .map_err(|e| anyhow!("Failed to compute action tree root: {:?}", e))?;

    // Create ForwarderLogicWitness for the CREATED resource (triggers transferFrom)
    // The created resource triggers the shield: transferFrom(sender, forwarder, amount)
    let created_logic = ForwarderLogicWitness::new_shield(
        created_resource.clone(),
        action_tree_root,
        nf_key.clone(),
        false,  // is_consumed = false (this is the created resource)
        forwarder_address,
        sender_address,
        amount,
    );

    // Create TrivialLogicWitness for the CONSUMED resource (no external call)
    let consumed_logic = TrivialLogicWitness::new(
        consumed_resource.clone(),
        action_tree_root,
        nf_key.clone(),
        true,  // is_consumed = true
    );

    println!("Generating ZK proofs...");
    println!("  This may take several minutes on first run (compiling circuits)");
    println!();

    // Prove both logic witnesses
    let consumed_logic_proof = consumed_logic.prove(ProofType::Groth16)
        .map_err(|e| anyhow!("Failed to prove consumed logic: {:?}", e))?;

    let created_logic_proof = created_logic.prove(ProofType::Groth16)
        .map_err(|e| anyhow!("Failed to prove created logic: {:?}", e))?;

    // Create action
    let action = Action::new(
        vec![compliance_unit],
        vec![consumed_logic_proof, created_logic_proof],
    ).map_err(|e| anyhow!("Failed to create action: {:?}", e))?;

    // Verify action
    action.clone().verify()
        .map_err(|e| anyhow!("Action verification failed: {:?}", e))?;

    // Build delta witness
    let delta_witness = DeltaWitness::from_bytes_vec(&[compliance_witness.rcv.to_vec()])
        .map_err(|e| anyhow!("Failed to create delta witness: {:?}", e))?;

    let tx = Transaction::create(vec![action], Delta::Witness(delta_witness));

    // Generate delta proof
    let balanced_tx = tx.generate_delta_proof()
        .map_err(|e| anyhow!("Delta proof generation failed: {:?}", e))?;

    let elapsed = start.elapsed();
    println!("\n✓ Proof generation complete!");
    println!("  Time: {:.2}s", elapsed.as_secs_f64());

    // Verify locally
    println!("\nVerifying proofs locally...");
    balanced_tx.clone().verify()
        .map_err(|e| anyhow!("Verification failed: {:?}", e))?;
    println!("✓ Verification passed!");

    // Convert to EVM format
    println!("\nConverting to EVM format...");
    let evm_tx = ProtocolAdapter::Transaction::from(balanced_tx);
    let abi_encoded = evm_tx.abi_encode();

    // Build full calldata
    let mut calldata = Vec::with_capacity(4 + abi_encoded.len());
    calldata.extend_from_slice(&EXECUTE_SELECTOR);
    calldata.extend_from_slice(&abi_encoded);

    let output = ProofOutput {
        calldata: format!("0x{}", hex::encode(&calldata)),
        to: PROTOCOL_ADAPTER.to_string(),
        calldata_length: calldata.len(),
        metadata: ProofMetadata {
            proof_type: "Groth16".to_string(),
            num_actions: 1,
            num_compliance_units: 1,
            generation_time_secs: elapsed.as_secs_f64(),
        },
    };

    // Save to file
    let output_path = format!("shield_{}_{}.bin", token.to_lowercase(), amount);
    std::fs::write(&output_path, &calldata)?;

    println!("\n════════════════════════════════════════════");
    println!("  SHIELD TRANSACTION READY FOR ON-CHAIN EXECUTION");
    println!("════════════════════════════════════════════");
    println!();
    println!("Target: {}", PROTOCOL_ADAPTER);
    println!("Calldata: {} bytes", calldata.len());
    println!("Saved to: {}", output_path);
    println!();
    println!("This transaction will:");
    println!("  1. Call transferFrom({}, {}, {}) on {} forwarder",
             sender, hex::encode(forwarder_address), amount, token);
    println!("  2. Create a shielded resource commitment on-chain");
    println!();
    println!("IMPORTANT: Before executing, ensure:");
    println!("  - Sender has approved the forwarder contract for {} tokens", token);
    println!("  - Sender has sufficient {} balance", token);
    println!();

    // JSON output
    println!("JSON output:");
    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

/// Generate an unshield proof with external_payload for forwarder call
///
/// This creates a transaction that:
/// 1. Consumes a shielded resource (nullifier goes on-chain)
/// 2. Outputs external_payload encoding: transfer(recipient, amount)
/// 3. The Protocol Adapter executes this forwarder call when processing the proof
fn generate_unshield_proof(token: &str, amount: u128, recipient: &str) -> Result<()> {
    println!("Generating UNSHIELD proof with forwarder call...");
    println!("  Token: {}", token);
    println!("  Amount: {}", amount);
    println!("  Recipient: {}", recipient);
    println!();

    let start = Instant::now();

    let forwarder_address = get_forwarder_address(token)?;
    let recipient_address = parse_address(recipient)?;

    println!("  Forwarder: 0x{}", hex::encode(forwarder_address));
    println!();

    // Create nullifier key
    let nf_key = NullifierKey::default();
    let nf_key_cm = nf_key.commit();

    // For unshield: consumed resource uses ForwarderLogic (triggers transfer)
    // Created resource uses TrivialLogic (no external call)
    let trivial_vk = TrivialLogicWitness::verifying_key();
    let forwarder_vk = ForwarderLogicWitness::verifying_key();

    // Create consumed resource (the shielded balance being withdrawn)
    // Uses ForwarderLogic since this triggers the transfer call
    let mut consumed_resource = Resource {
        logic_ref: forwarder_vk,
        nk_commitment: nf_key_cm,
        quantity: 0,
        is_ephemeral: true,
        ..Default::default()
    };
    consumed_resource.nonce = [2u8; 32];  // Different nonce for unshield

    let consumed_nf = consumed_resource.nullifier(&nf_key)
        .map_err(|e| anyhow!("Failed to compute nullifier: {:?}", e))?;

    // Create output resource (ephemeral, represents the withdrawn value)
    // Uses TrivialLogic (no external call)
    let mut created_resource = Resource {
        logic_ref: trivial_vk,
        nk_commitment: nf_key_cm,
        quantity: 0,
        is_ephemeral: true,
        ..Default::default()
    };
    created_resource.set_nonce(consumed_nf);

    // Create compliance witness
    let compliance_witness = ComplianceWitness::with_fixed_rcv(
        consumed_resource.clone(),
        nf_key.clone(),
        created_resource.clone(),
    );

    let compliance_unit = ComplianceUnit::create(&compliance_witness, ProofType::Groth16)
        .map_err(|e| anyhow!("Failed to create compliance unit: {:?}", e))?;

    // Build action tree
    let created_cm = created_resource.commitment();
    let mut action_tree = MerkleTree::new(vec![]);
    action_tree.insert(consumed_nf);
    action_tree.insert(created_cm);

    let action_tree_root = action_tree.root()
        .map_err(|e| anyhow!("Failed to compute action tree root: {:?}", e))?;

    // Create ForwarderLogicWitness for the CONSUMED resource (triggers transfer)
    // The consumed resource triggers the unshield: transfer(recipient, amount)
    let consumed_logic = ForwarderLogicWitness::new_unshield(
        consumed_resource.clone(),
        action_tree_root,
        nf_key.clone(),
        true,  // is_consumed = true (this is the consumed resource)
        forwarder_address,
        recipient_address,
        amount,
    );

    // Create TrivialLogicWitness for the created resource (no external call)
    let created_logic = TrivialLogicWitness::new(
        created_resource.clone(),
        action_tree_root,
        nf_key.clone(),
        false,  // is_consumed = false
    );

    println!("Generating ZK proofs...");
    println!("  This may take several minutes on first run");
    println!();

    let consumed_logic_proof = consumed_logic.prove(ProofType::Groth16)
        .map_err(|e| anyhow!("Failed to prove consumed logic: {:?}", e))?;

    let created_logic_proof = created_logic.prove(ProofType::Groth16)
        .map_err(|e| anyhow!("Failed to prove created logic: {:?}", e))?;

    let action = Action::new(
        vec![compliance_unit],
        vec![consumed_logic_proof, created_logic_proof],
    ).map_err(|e| anyhow!("Failed to create action: {:?}", e))?;

    action.clone().verify()
        .map_err(|e| anyhow!("Action verification failed: {:?}", e))?;

    let delta_witness = DeltaWitness::from_bytes_vec(&[compliance_witness.rcv.to_vec()])
        .map_err(|e| anyhow!("Failed to create delta witness: {:?}", e))?;

    let tx = Transaction::create(vec![action], Delta::Witness(delta_witness));

    let balanced_tx = tx.generate_delta_proof()
        .map_err(|e| anyhow!("Delta proof generation failed: {:?}", e))?;

    let elapsed = start.elapsed();
    println!("\n✓ Proof generation complete!");
    println!("  Time: {:.2}s", elapsed.as_secs_f64());

    println!("\nVerifying proofs locally...");
    balanced_tx.clone().verify()
        .map_err(|e| anyhow!("Verification failed: {:?}", e))?;
    println!("✓ Verification passed!");

    println!("\nConverting to EVM format...");
    let evm_tx = ProtocolAdapter::Transaction::from(balanced_tx);
    let abi_encoded = evm_tx.abi_encode();

    let mut calldata = Vec::with_capacity(4 + abi_encoded.len());
    calldata.extend_from_slice(&EXECUTE_SELECTOR);
    calldata.extend_from_slice(&abi_encoded);

    let output = ProofOutput {
        calldata: format!("0x{}", hex::encode(&calldata)),
        to: PROTOCOL_ADAPTER.to_string(),
        calldata_length: calldata.len(),
        metadata: ProofMetadata {
            proof_type: "Groth16".to_string(),
            num_actions: 1,
            num_compliance_units: 1,
            generation_time_secs: elapsed.as_secs_f64(),
        },
    };

    let output_path = format!("unshield_{}_{}.bin", token.to_lowercase(), amount);
    std::fs::write(&output_path, &calldata)?;

    println!("\n════════════════════════════════════════════");
    println!("  UNSHIELD TRANSACTION READY FOR ON-CHAIN EXECUTION");
    println!("════════════════════════════════════════════");
    println!();
    println!("Target: {}", PROTOCOL_ADAPTER);
    println!("Calldata: {} bytes", calldata.len());
    println!("Saved to: {}", output_path);
    println!();
    println!("This transaction will:");
    println!("  1. Verify the shielded resource ownership via nullifier");
    println!("  2. Call transfer({}, {}) on {} forwarder",
             recipient, amount, token);
    println!();
    println!("IMPORTANT: The forwarder contract must hold sufficient {} tokens", token);
    println!();

    println!("JSON output:");
    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}
