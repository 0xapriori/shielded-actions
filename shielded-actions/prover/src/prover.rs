use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tracing::{info, warn};

// For proof ID generation
use sha2::{Sha256, Digest};

/// Get Docker binary path, checking common locations on macOS/Linux
fn get_docker_path() -> Option<String> {
    // Common Docker locations
    let paths = [
        "/Applications/Docker.app/Contents/Resources/bin/docker",
        "/usr/local/bin/docker",
        "/usr/bin/docker",
        "/opt/homebrew/bin/docker",
    ];

    for path in paths {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    // Try PATH as fallback
    if std::process::Command::new("docker")
        .args(["--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some("docker".to_string());
    }

    None
}

/// Check if Docker is available and running
fn is_docker_available() -> bool {
    if let Some(docker) = get_docker_path() {
        std::process::Command::new(&docker)
            .args(["info"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        false
    }
}

/// Get PATH with Docker added
fn get_path_with_docker() -> String {
    let current_path = std::env::var("PATH").unwrap_or_default();

    // Add common Docker locations to PATH
    let docker_paths = [
        "/Applications/Docker.app/Contents/Resources/bin",
        "/usr/local/bin",
        "/opt/homebrew/bin",
    ];

    let mut new_path = docker_paths.join(":");
    if !current_path.is_empty() {
        new_path.push(':');
        new_path.push_str(&current_path);
    }

    new_path
}

/// Convert human-readable token amount to smallest units
/// USDC has 6 decimals, WETH has 18 decimals
fn parse_token_amount(amount: &str, token: &str) -> Result<u128> {
    let decimals: u32 = match token.to_uppercase().as_str() {
        "USDC" => 6,
        "WETH" => 18,
        _ => 18, // Default to 18 decimals
    };

    // Try parsing as u128 first (already in smallest units)
    if let Ok(val) = amount.parse::<u128>() {
        // If it's a large number, assume it's already in smallest units
        if val > 1_000_000_000 {
            return Ok(val);
        }
    }

    // Parse as float and convert to smallest units
    let amount_str = amount.trim();

    // Handle amounts like ".1" by prepending "0"
    let normalized = if amount_str.starts_with('.') {
        format!("0{}", amount_str)
    } else {
        amount_str.to_string()
    };

    let float_val: f64 = normalized.parse()
        .map_err(|e| anyhow!("Invalid amount '{}': {}", amount, e))?;

    // Convert to smallest units
    let multiplier = 10u128.pow(decimals);
    let smallest_units = (float_val * multiplier as f64).round() as u128;

    info!("Parsed amount '{}' for {} -> {} smallest units ({} decimals)",
          amount, token, smallest_units, decimals);

    Ok(smallest_units)
}

/// Proof response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofResponse {
    pub proof_id: String,
    pub status: String,
    pub proof: Option<ProofData>,
    /// Full calldata with function selector for on-chain execution
    pub calldata: Option<String>,
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

    // Use real ARM proving (requires Docker for Groth16)
    use_real_arm: bool,
}

impl ProverService {
    pub fn new() -> Result<Self> {
        let bonsai_api_key = std::env::var("BONSAI_API_KEY").ok();
        let bonsai_api_url = std::env::var("BONSAI_API_URL")
            .unwrap_or_else(|_| "https://api.bonsai.xyz".to_string());

        // USE_REAL_ARM=1 enables real ARM-RISC0 proving (requires Docker)
        let use_real_arm = std::env::var("USE_REAL_ARM")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        let mock_mode = bonsai_api_key.is_none() && !use_real_arm;

        if use_real_arm {
            info!("Real ARM-RISC0 proving enabled (requires Docker for Groth16)");
        } else if mock_mode {
            warn!("Running in mock mode. Set USE_REAL_ARM=1 for real proofs or BONSAI_API_KEY for Bonsai");
        } else {
            info!("Bonsai API configured at {}, real proofs enabled", bonsai_api_url);
        }

        Ok(Self {
            bonsai_api_key,
            bonsai_api_url,
            proofs: Mutex::new(HashMap::new()),
            mock_mode,
            use_real_arm,
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

        // Use real ARM proving with forwarder logic if enabled
        if self.use_real_arm {
            // Parse amount, handling both decimal strings like "0.1" and raw u128 values
            let amount_u128 = parse_token_amount(amount, token)?;
            return self.create_shield_proof_with_forwarder(proof_id, token, amount_u128, sender);
        }

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

        // Use real ARM proving if enabled
        if self.use_real_arm {
            return self.create_real_ephemeral_proof(proof_id);
        }

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

        // Use real ARM proving with forwarder logic if enabled
        if self.use_real_arm {
            // Extract token and amount from resource
            let token = resource.get("token")
                .and_then(|v| v.as_str())
                .unwrap_or("USDC");
            let amount: u128 = resource.get("amount")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            return self.create_unshield_proof_with_forwarder(proof_id, token, amount, recipient);
        }

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
                calldata: None,
            });
        }

        // If not in cache and we have Bonsai configured, check status
        if !self.mock_mode && !self.use_real_arm {
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
            calldata: None, // Mock mode doesn't produce real calldata
        })
    }

    /// Load a pre-generated proof from disk or generate by calling the local-prove binary
    /// The proof uses INITIAL_ROOT and is valid for on-chain execution
    ///
    /// NOTE: The pre-generated proof has a fixed nullifier. Once used on-chain, it cannot be
    /// reused (PreExistingNullifier error). To generate a fresh proof with a new nullifier,
    /// Docker must be running for Groth16 proof generation.
    pub fn create_real_ephemeral_proof(&self, proof_id: String) -> Result<ProofResponse> {
        info!("Looking for pre-generated proof or calling local-prove...");

        // Try to load pre-generated calldata from file
        // This file is generated by: cargo run --release --bin local-prove -- test-ephemeral
        let proof_file = "ephemeral_test_tx.bin";

        if let Ok(calldata) = std::fs::read(proof_file) {
            let calldata_hex = format!("0x{}", hex::encode(&calldata));
            info!("Loaded pre-generated proof: {} bytes", calldata.len());
            warn!("NOTE: Pre-generated proof has fixed nullifier. If 'PreExistingNullifier' error occurs, generate fresh proof with Docker.");

            return Ok(ProofResponse {
                proof_id,
                status: "completed".to_string(),
                proof: Some(ProofData {
                    journal: "ephemeral_proof_pregenerated".to_string(),
                    seal: hex::encode(&calldata[..64.min(calldata.len())]),
                    image_id: "arm_trivial_logic_v0.13.0".to_string(),
                }),
                calldata: Some(calldata_hex),
            });
        }

        // Check if Docker is available before trying to generate
        if !is_docker_available() {
            return Err(anyhow!(
                "Docker not available. Please ensure Docker Desktop is running. \
                 Proof generation requires Docker for Groth16 proving."
            ));
        }

        // Generate proof using local-prove with Docker-aware PATH
        info!("Generating fresh ephemeral proof with Docker (this will take ~8 minutes)...");

        let output = std::process::Command::new("cargo")
            .args(["run", "--release", "--bin", "local-prove", "--", "test-ephemeral"])
            .env("PATH", get_path_with_docker())
            .current_dir(std::env::current_dir().unwrap_or_default())
            .output();

        match output {
            Ok(out) => {
                if out.status.success() {
                    // Try to load the generated file
                    if let Ok(calldata) = std::fs::read(proof_file) {
                        let calldata_hex = format!("0x{}", hex::encode(&calldata));
                        info!("Generated proof: {} bytes", calldata.len());

                        return Ok(ProofResponse {
                            proof_id,
                            status: "completed".to_string(),
                            proof: Some(ProofData {
                                journal: "ephemeral_proof_generated".to_string(),
                                seal: hex::encode(&calldata[..64.min(calldata.len())]),
                                image_id: "arm_trivial_logic_v0.13.0".to_string(),
                            }),
                            calldata: Some(calldata_hex),
                        });
                    }
                }
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(anyhow!("local-prove failed: {}", stderr))
            }
            Err(e) => {
                Err(anyhow!("Failed to run local-prove: {}. Generate proof manually with: cargo run --release --bin local-prove -- test-ephemeral", e))
            }
        }
    }

    /// Generate a shield proof with forwarder call for real token transfers
    /// This proof includes external_payload that triggers transferFrom on the forwarder contract
    pub fn create_shield_proof_with_forwarder(
        &self,
        proof_id: String,
        token: &str,
        amount: u128,
        sender: &str,
    ) -> Result<ProofResponse> {
        info!("Generating shield proof with forwarder call: token={}, amount={}, sender={}", token, amount, sender);

        // Check if we have a pre-generated proof for this exact parameters
        let proof_file = format!("shield_{}_{}.bin", token.to_lowercase(), amount);

        if let Ok(calldata) = std::fs::read(&proof_file) {
            let calldata_hex = format!("0x{}", hex::encode(&calldata));
            info!("Loaded pre-generated shield proof: {} bytes", calldata.len());
            warn!("NOTE: Pre-generated proof has fixed nullifier. For production, generate fresh proof.");

            return Ok(ProofResponse {
                proof_id,
                status: "completed".to_string(),
                proof: Some(ProofData {
                    journal: format!("shield_{}_{}", token, amount),
                    seal: hex::encode(&calldata[..64.min(calldata.len())]),
                    image_id: "forwarder_logic_v0.1.0".to_string(),
                }),
                calldata: Some(calldata_hex),
            });
        }

        // Check if Docker is available
        if !is_docker_available() {
            return Err(anyhow!(
                "Docker not available. Please ensure Docker Desktop is running. \
                 Proof generation requires Docker for Groth16 proving."
            ));
        }

        // Generate proof using local-prove with Docker-aware PATH
        info!("Generating fresh proof with Docker (this will take ~7 minutes)...");

        let output = std::process::Command::new("cargo")
            .args([
                "run", "--release", "--bin", "local-prove", "--",
                "shield",
                "--token", token,
                "--amount", &amount.to_string(),
                "--sender", sender,
            ])
            .env("PATH", get_path_with_docker())
            .current_dir(std::env::current_dir().unwrap_or_default())
            .output();

        match output {
            Ok(out) => {
                if out.status.success() {
                    if let Ok(calldata) = std::fs::read(&proof_file) {
                        let calldata_hex = format!("0x{}", hex::encode(&calldata));
                        info!("Generated shield proof: {} bytes", calldata.len());

                        return Ok(ProofResponse {
                            proof_id,
                            status: "completed".to_string(),
                            proof: Some(ProofData {
                                journal: format!("shield_{}_{}", token, amount),
                                seal: hex::encode(&calldata[..64.min(calldata.len())]),
                                image_id: "forwarder_logic_v0.1.0".to_string(),
                            }),
                            calldata: Some(calldata_hex),
                        });
                    }
                }
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(anyhow!("local-prove shield failed: {}", stderr))
            }
            Err(e) => {
                Err(anyhow!("Failed to run local-prove: {}", e))
            }
        }
    }

    /// Generate an unshield proof with forwarder call for real token transfers
    /// This proof includes external_payload that triggers transfer on the forwarder contract
    pub fn create_unshield_proof_with_forwarder(
        &self,
        proof_id: String,
        token: &str,
        amount: u128,
        recipient: &str,
    ) -> Result<ProofResponse> {
        info!("Generating unshield proof with forwarder call: token={}, amount={}, recipient={}", token, amount, recipient);

        let proof_file = format!("unshield_{}_{}.bin", token.to_lowercase(), amount);

        if let Ok(calldata) = std::fs::read(&proof_file) {
            let calldata_hex = format!("0x{}", hex::encode(&calldata));
            info!("Loaded pre-generated unshield proof: {} bytes", calldata.len());

            return Ok(ProofResponse {
                proof_id,
                status: "completed".to_string(),
                proof: Some(ProofData {
                    journal: format!("unshield_{}_{}", token, amount),
                    seal: hex::encode(&calldata[..64.min(calldata.len())]),
                    image_id: "forwarder_logic_v0.1.0".to_string(),
                }),
                calldata: Some(calldata_hex),
            });
        }

        // Check if Docker is available
        if !is_docker_available() {
            return Err(anyhow!(
                "Docker not available. Please ensure Docker Desktop is running. \
                 Proof generation requires Docker for Groth16 proving."
            ));
        }

        // Generate proof using local-prove with Docker-aware PATH
        info!("Generating fresh unshield proof with Docker (this will take ~7 minutes)...");

        let output = std::process::Command::new("cargo")
            .args([
                "run", "--release", "--bin", "local-prove", "--",
                "unshield",
                "--token", token,
                "--amount", &amount.to_string(),
                "--recipient", recipient,
            ])
            .env("PATH", get_path_with_docker())
            .current_dir(std::env::current_dir().unwrap_or_default())
            .output();

        match output {
            Ok(out) => {
                if out.status.success() {
                    if let Ok(calldata) = std::fs::read(&proof_file) {
                        let calldata_hex = format!("0x{}", hex::encode(&calldata));
                        info!("Generated unshield proof: {} bytes", calldata.len());

                        return Ok(ProofResponse {
                            proof_id,
                            status: "completed".to_string(),
                            proof: Some(ProofData {
                                journal: format!("unshield_{}_{}", token, amount),
                                seal: hex::encode(&calldata[..64.min(calldata.len())]),
                                image_id: "forwarder_logic_v0.1.0".to_string(),
                            }),
                            calldata: Some(calldata_hex),
                        });
                    }
                }
                let stderr = String::from_utf8_lossy(&out.stderr);
                Err(anyhow!("local-prove unshield failed: {}", stderr))
            }
            Err(e) => {
                Err(anyhow!("Failed to run local-prove: {}", e))
            }
        }
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
            calldata: None,
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
                calldata: None,
            });
        }

        Err(anyhow!("Proof not found: {}", proof_id))
    }
}
