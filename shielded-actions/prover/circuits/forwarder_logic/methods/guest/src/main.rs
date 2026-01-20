//! Forwarder Logic Guest Program
//!
//! This RISC Zero guest program proves that a resource satisfies the forwarder logic constraints.
//! The output includes external_payload for triggering ERC20 forwarder calls.

use arm::{
    error::ArmError,
    logic_instance::{AppData, ExpirableBlob, LogicInstance},
    nullifier_key::NullifierKey,
    resource::Resource,
    resource_logic::LogicCircuit,
    utils::bytes_to_words,
};
use risc0_zkvm::guest::env;
use risc0_zkvm::sha::Digest;
use serde::{Deserialize, Serialize};

/// Deletion criterion: Never delete (persists after transaction)
const DELETION_CRITERION_NEVER: u32 = 1;

/// Forwarder Logic Witness
///
/// This witness enables resources to trigger ERC20 forwarder calls when consumed/created.
/// The external_payload encodes: (forwarderAddress, calldata, expectedOutput)
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ForwarderLogicWitness {
    /// The resource being consumed or created
    pub resource: Resource,
    /// The action tree root for merkle path verification
    pub action_tree_root: Digest,
    /// Whether this is a consumed (true) or created (false) resource
    pub is_consumed: bool,
    /// The nullifier key for computing tags
    pub nf_key: NullifierKey,
    /// Forwarder contract address (20 bytes)
    pub forwarder_address: [u8; 20],
    /// Call data to send to forwarder (includes function selector)
    pub call_data: Vec<u8>,
    /// Expected output from forwarder call
    pub expected_output: Vec<u8>,
    /// Whether to include external_payload (only for specific resource roles)
    pub include_external_call: bool,
}

impl LogicCircuit for ForwarderLogicWitness {
    fn constrain(&self) -> Result<LogicInstance, ArmError> {
        // Compute the resource tag (nullifier for consumed, commitment for created)
        let tag = self.resource.tag(self.is_consumed, &self.nf_key)?;

        // For ephemeral resources, quantity must be 0
        if self.resource.is_ephemeral {
            assert_eq!(self.resource.quantity, 0, "Ephemeral resources must have quantity=0");
        }

        // Build external_payload if this resource should trigger a forwarder call
        let external_payload = if self.include_external_call && !self.call_data.is_empty() {
            // Encode as: abi.encode(forwarderAddress, calldata, expectedOutput)
            let blob_data = self.encode_forwarder_call();

            vec![ExpirableBlob {
                blob: bytes_to_words(&blob_data),
                deletion_criterion: DELETION_CRITERION_NEVER,
            }]
        } else {
            vec![]
        };

        let app_data = AppData {
            resource_payload: vec![],
            discovery_payload: vec![],
            external_payload,
            application_payload: vec![],
        };

        Ok(LogicInstance {
            tag,
            is_consumed: self.is_consumed,
            root: self.action_tree_root,
            app_data,
        })
    }
}

impl ForwarderLogicWitness {
    /// Encode the forwarder call as ABI-encoded tuple: (address, bytes, bytes)
    fn encode_forwarder_call(&self) -> Vec<u8> {
        // ABI encode: (address forwarder, bytes calldata, bytes expectedOutput)
        // This matches the format expected by ProtocolAdapter._executeForwarderCall

        let mut result = Vec::new();

        // Address (padded to 32 bytes)
        result.extend_from_slice(&[0u8; 12]);
        result.extend_from_slice(&self.forwarder_address);

        // Offset to call_data (96 = 0x60)
        result.extend_from_slice(&[0u8; 31]);
        result.push(0x60);

        // Offset to expected_output (96 + 32 + call_data.len() padded to 32)
        let call_data_slot_size = 32 + ((self.call_data.len() + 31) / 32) * 32;
        let expected_offset = 96 + call_data_slot_size;
        let mut offset_bytes = [0u8; 32];
        offset_bytes[24..].copy_from_slice(&(expected_offset as u64).to_be_bytes());
        result.extend_from_slice(&offset_bytes);

        // call_data (length + data)
        let mut len_bytes = [0u8; 32];
        len_bytes[24..].copy_from_slice(&(self.call_data.len() as u64).to_be_bytes());
        result.extend_from_slice(&len_bytes);
        result.extend_from_slice(&self.call_data);
        // Pad to 32 bytes
        let padding = (32 - (self.call_data.len() % 32)) % 32;
        result.extend(core::iter::repeat(0u8).take(padding));

        // expected_output (length + data)
        let mut len_bytes = [0u8; 32];
        len_bytes[24..].copy_from_slice(&(self.expected_output.len() as u64).to_be_bytes());
        result.extend_from_slice(&len_bytes);
        result.extend_from_slice(&self.expected_output);
        // Pad to 32 bytes
        let padding = (32 - (self.expected_output.len() % 32)) % 32;
        result.extend(core::iter::repeat(0u8).take(padding));

        result
    }
}

fn main() {
    // Read the witness from the host
    let witness: ForwarderLogicWitness = env::read();

    // Execute the logic constraints
    // This computes the LogicInstance including external_payload
    let instance = witness.constrain().expect("Logic constraints failed");

    // Commit the instance as public output
    // This includes tag, is_consumed, root, and app_data (with external_payload)
    env::commit(&instance);
}
