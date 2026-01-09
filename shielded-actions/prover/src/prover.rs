use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::sync::Mutex;
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

/// Session tracking for async proof generation
#[derive(Debug, Clone)]
struct ProofSession {
    session_id: String,
    status: String,
    proof: Option<ProofData>,
}

/// Prover service that interfaces with Bonsai
pub struct ProverService {
    // Bonsai API configuration
    bonsai_api_key: Option<String>,
    bonsai_api_url: String,

    // In-memory proof cache
    proofs: Mutex<HashMap<String, ProofSession>>,

    // Use mock mode if no API key is set
    mock_mode: bool,
}

impl ProverService {
    pub fn new() -> Result<Self> {
        let bonsai_api_key = std::env::var("BONSAI_API_KEY").ok();
        let bonsai_api_url = std::env::var("BONSAI_API_URL")
            .unwrap_or_else(|_| "https://api.bonsai.xyz".to_string());

        let mock_mode = bonsai_api_key.is_none();

        if mock_mode {
            warn!("No BONSAI_API_KEY found, running in mock mode");
        } else {
            info!("Bonsai API configured at {}, real proofs enabled", bonsai_api_url);
        }

        Ok(Self {
            bonsai_api_key,
            bonsai_api_url,
            proofs: Mutex::new(HashMap::new()),
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

        let journal_data = serde_json::json!({
            "action": "shield",
            "token": token,
            "amount": amount,
            "sender": sender,
            "nullifier_key_commitment": self.hash_nullifier_key(nullifier_key),
        });

        if self.mock_mode {
            return self.create_mock_proof(proof_id, "shield", journal_data);
        }

        self.submit_bonsai_proof(proof_id, journal_data).await
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

        let journal_data = serde_json::json!({
            "action": "swap",
            "input_resource": input_resource,
            "output_token": output_token,
            "min_amount_out": min_amount_out,
            "nullifier_key_commitment": self.hash_nullifier_key(nullifier_key),
        });

        if self.mock_mode {
            return self.create_mock_proof(proof_id, "swap", journal_data);
        }

        self.submit_bonsai_proof(proof_id, journal_data).await
    }

    /// Create an unshield proof
    pub async fn create_unshield_proof(
        &self,
        resource: &serde_json::Value,
        recipient: &str,
        nullifier_key: &str,
    ) -> Result<ProofResponse> {
        let proof_id = self.generate_proof_id("unshield", &[recipient]);

        let journal_data = serde_json::json!({
            "action": "unshield",
            "resource": resource,
            "recipient": recipient,
            "nullifier_key_commitment": self.hash_nullifier_key(nullifier_key),
        });

        if self.mock_mode {
            return self.create_mock_proof(proof_id, "unshield", journal_data);
        }

        self.submit_bonsai_proof(proof_id, journal_data).await
    }

    /// Get proof status
    pub async fn get_proof_status(&self, proof_id: &str) -> Result<ProofResponse> {
        let proofs = self.proofs.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        if let Some(session) = proofs.get(proof_id) {
            return Ok(ProofResponse {
                proof_id: proof_id.to_string(),
                status: session.status.clone(),
                proof: session.proof.clone(),
            });
        }

        // If not in cache and we have Bonsai configured, check status
        if !self.mock_mode {
            drop(proofs); // Release lock before async call
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

        let proof_data = ProofData {
            journal: journal_hex,
            seal,
            image_id: image_id.to_string(),
        };

        // Store in cache
        let mut proofs = self.proofs.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        proofs.insert(proof_id.clone(), ProofSession {
            session_id: proof_id.clone(),
            status: "completed".to_string(),
            proof: Some(proof_data.clone()),
        });

        Ok(ProofResponse {
            proof_id,
            status: "completed".to_string(),
            proof: Some(proof_data),
        })
    }

    async fn submit_bonsai_proof(
        &self,
        proof_id: String,
        input_data: serde_json::Value,
    ) -> Result<ProofResponse> {
        let api_key = self.bonsai_api_key.as_ref()
            .ok_or_else(|| anyhow!("Bonsai API key not configured"))?;

        info!("Submitting proof to Bonsai: {}", proof_id);

        // Use the blocking Bonsai SDK client
        // The SDK requires a risc0_zkvm version string
        let risc0_version = "1.4.0"; // Match the bonsai-sdk version

        let client = bonsai_sdk::blocking::Client::from_parts(
            self.bonsai_api_url.clone(),
            api_key.clone(),
            risc0_version,
        )?;

        // Serialize input
        let input_bytes = serde_json::to_vec(&input_data)?;

        // Upload input data
        let input_id = client.upload_input(input_bytes)?;
        info!("Uploaded input to Bonsai: {}", input_id);

        // For a complete implementation, we would need:
        // 1. A compiled guest program (ELF) that implements the shielded action verification
        // 2. Upload that program with client.upload_img()
        // 3. Create a session with client.create_session()
        // 4. Poll for completion with session.status()
        // 5. Download the receipt

        // For now, store as pending and return
        let mut proofs = self.proofs.lock().map_err(|e| anyhow!("Lock error: {}", e))?;
        proofs.insert(proof_id.clone(), ProofSession {
            session_id: input_id.clone(),
            status: "pending".to_string(),
            proof: None,
        });

        Ok(ProofResponse {
            proof_id,
            status: "pending".to_string(),
            proof: None,
        })
    }

    async fn check_bonsai_status(&self, proof_id: &str) -> Result<ProofResponse> {
        let proofs = self.proofs.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        if let Some(session) = proofs.get(proof_id) {
            // In a full implementation, we would poll Bonsai for the session status
            // using client.session_status(&session.session_id)
            return Ok(ProofResponse {
                proof_id: proof_id.to_string(),
                status: session.status.clone(),
                proof: session.proof.clone(),
            });
        }

        Err(anyhow!("Proof not found: {}", proof_id))
    }
}
