//! Shield Logic - A custom resource logic circuit for ERC20 token shielding
//!
//! This circuit allows resources to include external payloads that trigger
//! forwarder contract calls when the proof is verified on-chain.
//!
//! For shield operations, the external_payload encodes:
//! - Forwarder address
//! - transferFrom(sender, forwarder, amount) call data
//! - Expected output (abi.encode(true))

use arm::error::ArmError;
use arm::logic_instance::{AppData, ExpirableBlob, LogicInstance};
use arm::nullifier_key::NullifierKey;
use arm::resource::Resource;
use arm::resource_logic::LogicCircuit;
use arm::utils::bytes_to_words;
use alloy::primitives::{Address, U256};
use alloy::sol_types::SolValue;
use risc0_zkvm::sha::Digest;
use serde::{Deserialize, Serialize};

/// Contract addresses on Sepolia
pub mod contracts {
    pub const USDC_FORWARDER: &str = "0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE";
    pub const WETH_FORWARDER: &str = "0xD5307D777dC60b763b74945BF5A42ba93ce44e4b";
    pub const UNISWAP_FORWARDER: &str = "0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA";
}

/// DeletionCriterion::Never = 1 (persists after transaction)
const DELETION_CRITERION_NEVER: u32 = 1;

/// Shield Logic Witness - witness data for shield operations
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ShieldLogicWitness {
    /// The resource being created (for shield) or consumed (for unshield)
    pub resource: Resource,
    /// The action tree root for merkle path verification
    pub action_tree_root: Digest,
    /// Whether this is a consumed (true) or created (false) resource
    pub is_consumed: bool,
    /// The nullifier key for computing tags
    pub nf_key: NullifierKey,
    /// The forwarder contract address
    pub forwarder_address: [u8; 20],
    /// The user/sender address for transfers
    pub user_address: [u8; 20],
    /// Amount to transfer (in token base units)
    pub amount: u128,
    /// True for shield (transferFrom), false for unshield (transfer)
    pub is_shield: bool,
}

impl LogicCircuit for ShieldLogicWitness {
    fn constrain(&self) -> Result<LogicInstance, ArmError> {
        // Compute the resource tag
        let tag = self.resource.tag(self.is_consumed, &self.nf_key)?;

        // Build the external payload for the forwarder call
        let external_payload = self.build_external_payload();

        // Build the app data with the external payload
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

impl ShieldLogicWitness {
    /// Create a new shield witness for depositing tokens
    pub fn new_shield(
        resource: Resource,
        action_tree_root: Digest,
        nf_key: NullifierKey,
        is_consumed: bool,
        forwarder_address: [u8; 20],
        user_address: [u8; 20],
        amount: u128,
    ) -> Self {
        Self {
            resource,
            action_tree_root,
            is_consumed,
            nf_key,
            forwarder_address,
            user_address,
            amount,
            is_shield: true,
        }
    }

    /// Create a new unshield witness for withdrawing tokens
    pub fn new_unshield(
        resource: Resource,
        action_tree_root: Digest,
        nf_key: NullifierKey,
        is_consumed: bool,
        forwarder_address: [u8; 20],
        recipient_address: [u8; 20],
        amount: u128,
    ) -> Self {
        Self {
            resource,
            action_tree_root,
            is_consumed,
            nf_key,
            forwarder_address,
            user_address: recipient_address,
            amount,
            is_shield: false,
        }
    }

    /// Build the external payload for the forwarder call
    /// Format: abi.encode(forwarderAddress, input, expectedOutput)
    fn build_external_payload(&self) -> Vec<ExpirableBlob> {
        // Only include external payload for created resources (not consumed)
        // The shield operation creates a new resource, the unshield consumes it
        if self.is_consumed && self.is_shield {
            // Consumed resources in shield don't need forwarder calls
            return vec![];
        }
        if !self.is_consumed && !self.is_shield {
            // Created resources in unshield don't need forwarder calls
            return vec![];
        }

        let forwarder = Address::from_slice(&self.forwarder_address);
        let user = Address::from_slice(&self.user_address);
        let amount = U256::from(self.amount);

        // Build the ERC20 call data
        let call_data = if self.is_shield {
            // transferFrom(from, to, amount) - shield deposits tokens TO the forwarder
            Self::encode_transfer_from(user, forwarder, amount)
        } else {
            // transfer(to, amount) - unshield withdraws tokens FROM the forwarder
            Self::encode_transfer(user, amount)
        };

        // Expected output: abi.encode(true) for successful transfers
        let expected_output = true.abi_encode();

        // Full blob: abi.encode(forwarderAddress, input, expectedOutput)
        let blob_data = (forwarder, call_data.clone(), expected_output.clone()).abi_encode();

        let blob = ExpirableBlob {
            blob: bytes_to_words(&blob_data),
            deletion_criterion: DELETION_CRITERION_NEVER,
        };

        vec![blob]
    }

    /// Encode transferFrom(from, to, amount) call
    fn encode_transfer_from(from: Address, to: Address, amount: U256) -> Vec<u8> {
        // Function selector for transferFrom(address,address,uint256)
        let selector: [u8; 4] = [0x23, 0xb8, 0x72, 0xdd];
        let mut data = selector.to_vec();
        data.extend_from_slice(&(from, to, amount).abi_encode());
        data
    }

    /// Encode transfer(to, amount) call
    fn encode_transfer(to: Address, amount: U256) -> Vec<u8> {
        // Function selector for transfer(address,uint256)
        let selector: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];
        let mut data = selector.to_vec();
        data.extend_from_slice(&(to, amount).abi_encode());
        data
    }
}

/// Helper to parse address from hex string
pub fn parse_address(s: &str) -> [u8; 20] {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).expect("Invalid hex address");
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    arr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_address() {
        let addr = parse_address("0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE");
        assert_eq!(addr.len(), 20);
    }

    #[test]
    fn test_encode_transfer_from() {
        let from = Address::from_slice(&parse_address("0x1234567890123456789012345678901234567890"));
        let to = Address::from_slice(&parse_address("0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE"));
        let amount = U256::from(1000000u64);

        let encoded = ShieldLogicWitness::encode_transfer_from(from, to, amount);
        assert_eq!(&encoded[..4], &[0x23, 0xb8, 0x72, 0xdd]);
    }
}
