use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use tracing::{info, warn};

/// Proof response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofResponse {
    pub proof_id: String,
    pub status: String,
    pub proof: Option<ProofData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofData {
    pub journal: String,
    pub seal: String,
    pub image_id: String,
}

/// Prover service that interfaces with Bonsai/Boundless
pub struct ProverService {
    // Bonsai API configuration
    bonsai_api_key: Option<String>,
    bonsai_api_url: Option<String>,

    // In-memory proof cache (for demo purposes)
    proofs: HashMap<String, ProofResponse>,

    // Use mock mode if no API key is set
    mock_mode: bool,
}

impl ProverService {
    pub fn new() -> Result<Self> {
        let bonsai_api_key = std::env::var("BONSAI_API_KEY").ok();
        let bonsai_api_url = std::env::var("BONSAI_API_URL").ok();

        let mock_mode = bonsai_api_key.is_none();

        if mock_mode {
            warn!("No BONSAI_API_KEY found, running in mock mode");
        } else {
            info!("Bonsai API configured, real proofs enabled");
        }

        Ok(Self {
            bonsai_api_key,
            bonsai_api_url,
            proofs: HashMap::new(),
            mock_mode,
        })
    }

    /// Create a shield proof
    pub async fn create_shield_proof(
        &self,
        token: &str,
        amount: &str,
        sender: &str,
        nullifier_key: &str,
    ) -> Result<ProofResponse> {
        let proof_id = self.generate_proof_id("shield", &[token, amount, sender]);

        if self.mock_mode {
            return self.create_mock_proof(proof_id, "shield", serde_json::json!({
                "token": token,
                "amount": amount,
                "sender": sender,
                "nullifier_key_commitment": self.hash_nullifier_key(nullifier_key),
            }));
        }

        // Real Bonsai proof generation
        self.submit_bonsai_proof(proof_id, "shield", serde_json::json!({
            "token": token,
            "amount": amount,
            "sender": sender,
            "nullifier_key": nullifier_key,
        })).await
    }

    /// Create a swap proof
    pub async fn create_swap_proof(
        &self,
        input_resource: &serde_json::Value,
        output_token: &str,
        nullifier_key: &str,
        min_amount_out: &str,
    ) -> Result<ProofResponse> {
        let proof_id = self.generate_proof_id("swap", &[output_token, min_amount_out]);

        if self.mock_mode {
            return self.create_mock_proof(proof_id, "swap", serde_json::json!({
                "input_resource": input_resource,
                "output_token": output_token,
                "min_amount_out": min_amount_out,
                "nullifier_key_commitment": self.hash_nullifier_key(nullifier_key),
            }));
        }

        // Real Bonsai proof generation
        self.submit_bonsai_proof(proof_id, "swap", serde_json::json!({
            "input_resource": input_resource,
            "output_token": output_token,
            "nullifier_key": nullifier_key,
            "min_amount_out": min_amount_out,
        })).await
    }

    /// Create an unshield proof
    pub async fn create_unshield_proof(
        &self,
        resource: &serde_json::Value,
        recipient: &str,
        nullifier_key: &str,
    ) -> Result<ProofResponse> {
        let proof_id = self.generate_proof_id("unshield", &[recipient]);

        if self.mock_mode {
            return self.create_mock_proof(proof_id, "unshield", serde_json::json!({
                "resource": resource,
                "recipient": recipient,
                "nullifier_key_commitment": self.hash_nullifier_key(nullifier_key),
            }));
        }

        // Real Bonsai proof generation
        self.submit_bonsai_proof(proof_id, "unshield", serde_json::json!({
            "resource": resource,
            "recipient": recipient,
            "nullifier_key": nullifier_key,
        })).await
    }

    /// Get proof status
    pub async fn get_proof_status(&self, proof_id: &str) -> Result<ProofResponse> {
        if let Some(proof) = self.proofs.get(proof_id) {
            return Ok(proof.clone());
        }

        // Check Bonsai status if not in cache
        if !self.mock_mode {
            return self.check_bonsai_status(proof_id).await;
        }

        Err(anyhow!("Proof not found: {}", proof_id))
    }

    // Helper functions

    fn generate_proof_id(&self, proof_type: &str, inputs: &[&str]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(proof_type.as_bytes());
        for input in inputs {
            hasher.update(input.as_bytes());
        }
        hasher.update(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_le_bytes());

        hex::encode(hasher.finalize())[..16].to_string()
    }

    fn hash_nullifier_key(&self, key: &str) -> String {
        let key_bytes = hex::decode(key.trim_start_matches("0x")).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(&key_bytes);
        hex::encode(hasher.finalize())
    }

    fn create_mock_proof(
        &self,
        proof_id: String,
        proof_type: &str,
        journal_data: serde_json::Value,
    ) -> Result<ProofResponse> {
        info!("Creating mock {} proof: {}", proof_type, proof_id);

        // Generate mock proof data
        let journal = serde_json::to_string(&journal_data)?;
        let journal_hex = hex::encode(journal.as_bytes());

        // Mock seal (in real implementation, this would be the ZK proof)
        let mut seal_hasher = Sha256::new();
        seal_hasher.update(journal.as_bytes());
        seal_hasher.update(proof_id.as_bytes());
        let seal = hex::encode(seal_hasher.finalize());

        // Mock image ID (would be the actual guest program ID)
        let image_id = "mock_shielded_actions_guest_v1";

        Ok(ProofResponse {
            proof_id,
            status: "completed".to_string(),
            proof: Some(ProofData {
                journal: journal_hex,
                seal,
                image_id: image_id.to_string(),
            }),
        })
    }

    async fn submit_bonsai_proof(
        &self,
        proof_id: String,
        proof_type: &str,
        input_data: serde_json::Value,
    ) -> Result<ProofResponse> {
        let api_key = self.bonsai_api_key.as_ref()
            .ok_or_else(|| anyhow!("Bonsai API key not configured"))?;
        let api_url = self.bonsai_api_url.as_ref()
            .ok_or_else(|| anyhow!("Bonsai API URL not configured"))?;

        info!("Submitting {} proof to Bonsai: {}", proof_type, proof_id);

        // In a full implementation, we would:
        // 1. Upload the guest program (ELF) if not already uploaded
        // 2. Upload the input data
        // 3. Create a session
        // 4. Poll for completion
        // 5. Download the receipt

        // For now, use the Bonsai SDK directly
        let client = bonsai_sdk::blocking::Client::from_parts(
            api_url.clone(),
            api_key.clone(),
            risc0_zkvm::VERSION,
        )?;

        // Serialize input
        let input_bytes = serde_json::to_vec(&input_data)?;

        // Upload input
        let input_id = client.upload_input(input_bytes)?;
        info!("Uploaded input: {}", input_id);

        // For a real implementation, we'd need to:
        // 1. Compile a guest program that verifies the shielded action constraints
        // 2. Upload that program
        // 3. Create a session with the program and input

        // Since we don't have the guest program compiled, return a pending status
        Ok(ProofResponse {
            proof_id,
            status: "pending".to_string(),
            proof: None,
        })
    }

    async fn check_bonsai_status(&self, proof_id: &str) -> Result<ProofResponse> {
        // In a full implementation, we would poll the Bonsai session status
        Err(anyhow!("Proof not found: {}", proof_id))
    }
}
