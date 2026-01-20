use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

mod prover;

use prover::{ProverService, ProofResponse};

/// Job status for async proof generation
#[derive(Clone, serde::Serialize)]
struct JobStatus {
    job_id: String,
    status: String, // "pending", "generating", "completed", "failed"
    proof: Option<ProofResponse>,
    error: Option<String>,
    created_at: u64,
}

#[derive(Clone)]
struct AppState {
    prover: Arc<RwLock<ProverService>>,
    jobs: Arc<RwLock<HashMap<String, JobStatus>>>,
}

// Custom error type for proper axum responses
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": self.0.to_string()})),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

fn get_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn generate_job_id() -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(get_timestamp().to_le_bytes());
    hasher.update(rand::random::<[u8; 16]>());
    hex::encode(&hasher.finalize()[..8])
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize prover service
    let prover = ProverService::new()?;
    let state = AppState {
        prover: Arc::new(RwLock::new(prover)),
        jobs: Arc::new(RwLock::new(HashMap::new())),
    };

    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router with async job pattern
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/info", get(api_info))
        .route("/api/generate-keypair", post(generate_keypair))
        // Async endpoints - return job_id immediately
        .route("/api/shield", post(start_shield_job))
        .route("/api/swap", post(start_swap_job))
        .route("/api/unshield", post(start_unshield_job))
        // Job status polling
        .route("/api/job/{job_id}", get(get_job_status))
        // Legacy sync endpoints (for backwards compat with backend)
        .route("/api/prove/shield", post(prove_shield_sync))
        .route("/api/prove/swap", post(prove_swap_sync))
        .route("/api/prove/unshield", post(prove_unshield_sync))
        .layer(cors)
        .with_state(state);

    // Get port from environment
    let port = std::env::var("PORT").unwrap_or_else(|_| "3002".to_string());
    let addr = format!("0.0.0.0:{}", port);

    info!("Starting Shielded Prover service on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// Health check endpoint
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "shielded-prover"
    }))
}

// API info endpoint
async fn api_info() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "Shielded Actions Prover",
        "version": "0.2.0",
        "network": "sepolia",
        "contracts": {
            "protocol_adapter": "0x08c3bdc46B115cDc71Df076d9De96EeEBaa98525",
            "usdc_forwarder": "0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE",
            "weth_forwarder": "0xD5307D777dC60b763b74945BF5A42ba93ce44e4b",
            "uniswap_forwarder": "0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA"
        },
        "features": {
            "async_proofs": true,
            "polling_endpoint": "/api/job/:job_id"
        }
    }))
}

// Generate keypair endpoint
async fn generate_keypair() -> Json<serde_json::Value> {
    use sha2::{Sha256, Digest};

    let mut hasher = Sha256::new();
    hasher.update(get_timestamp().to_le_bytes());
    hasher.update(rand::random::<[u8; 32]>());
    let private_key = hex::encode(hasher.finalize());

    let mut pub_hasher = Sha256::new();
    pub_hasher.update(hex::decode(&private_key).unwrap_or_default());
    let public_key = hex::encode(pub_hasher.finalize());

    Json(serde_json::json!({
        "private_key": private_key,
        "public_key": public_key
    }))
}

// ============== ASYNC JOB ENDPOINTS ==============

#[derive(Debug, Deserialize)]
struct ShieldProofRequest {
    token: String,
    amount: String,
    sender: String,
    nullifier_key: String,
}

// Start a shield proof job asynchronously
async fn start_shield_job(
    State(state): State<AppState>,
    Json(req): Json<ShieldProofRequest>,
) -> Json<serde_json::Value> {
    let job_id = generate_job_id();
    info!("Starting shield job {}: {:?}", job_id, req);

    // Create pending job
    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(job_id.clone(), JobStatus {
            job_id: job_id.clone(),
            status: "pending".to_string(),
            proof: None,
            error: None,
            created_at: get_timestamp(),
        });
    }

    // Spawn background task to generate proof
    let state_clone = state.clone();
    let job_id_clone = job_id.clone();
    let req_token = req.token.clone();
    let req_amount = req.amount.clone();
    let req_sender = req.sender.clone();
    let req_nullifier = req.nullifier_key.clone();

    tokio::spawn(async move {
        // Update status to generating
        {
            let mut jobs = state_clone.jobs.write().await;
            if let Some(job) = jobs.get_mut(&job_id_clone) {
                job.status = "generating".to_string();
            }
        }

        // Generate the proof
        let prover = state_clone.prover.read().await;
        let result = prover
            .create_shield_proof(&req_token, &req_amount, &req_sender, &req_nullifier)
            .await;

        // Update job with result
        let mut jobs = state_clone.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id_clone) {
            match result {
                Ok(proof) => {
                    job.status = "completed".to_string();
                    job.proof = Some(proof);
                }
                Err(e) => {
                    job.status = "failed".to_string();
                    job.error = Some(e.to_string());
                }
            }
        }
    });

    // Return immediately with job ID
    Json(serde_json::json!({
        "job_id": job_id,
        "status": "pending",
        "message": "Proof generation started. Poll /api/job/{} for status.".replace("{}", &job_id)
    }))
}

#[derive(Debug, Deserialize)]
struct SwapProofRequest {
    input_resource: serde_json::Value,
    output_token: String,
    nullifier_key: String,
    min_amount_out: String,
}

async fn start_swap_job(
    State(state): State<AppState>,
    Json(req): Json<SwapProofRequest>,
) -> Json<serde_json::Value> {
    let job_id = generate_job_id();
    info!("Starting swap job {}: {:?}", job_id, req);

    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(job_id.clone(), JobStatus {
            job_id: job_id.clone(),
            status: "pending".to_string(),
            proof: None,
            error: None,
            created_at: get_timestamp(),
        });
    }

    let state_clone = state.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        {
            let mut jobs = state_clone.jobs.write().await;
            if let Some(job) = jobs.get_mut(&job_id_clone) {
                job.status = "generating".to_string();
            }
        }

        let prover = state_clone.prover.read().await;
        let result = prover
            .create_swap_proof(&req.input_resource, &req.output_token, &req.nullifier_key, &req.min_amount_out)
            .await;

        let mut jobs = state_clone.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id_clone) {
            match result {
                Ok(proof) => {
                    job.status = "completed".to_string();
                    job.proof = Some(proof);
                }
                Err(e) => {
                    job.status = "failed".to_string();
                    job.error = Some(e.to_string());
                }
            }
        }
    });

    Json(serde_json::json!({
        "job_id": job_id,
        "status": "pending"
    }))
}

#[derive(Debug, Deserialize)]
struct UnshieldProofRequest {
    resource: serde_json::Value,
    recipient: String,
    nullifier_key: String,
}

async fn start_unshield_job(
    State(state): State<AppState>,
    Json(req): Json<UnshieldProofRequest>,
) -> Json<serde_json::Value> {
    let job_id = generate_job_id();
    info!("Starting unshield job {}: {:?}", job_id, req);

    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(job_id.clone(), JobStatus {
            job_id: job_id.clone(),
            status: "pending".to_string(),
            proof: None,
            error: None,
            created_at: get_timestamp(),
        });
    }

    let state_clone = state.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        {
            let mut jobs = state_clone.jobs.write().await;
            if let Some(job) = jobs.get_mut(&job_id_clone) {
                job.status = "generating".to_string();
            }
        }

        let prover = state_clone.prover.read().await;
        let result = prover
            .create_unshield_proof(&req.resource, &req.recipient, &req.nullifier_key)
            .await;

        let mut jobs = state_clone.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id_clone) {
            match result {
                Ok(proof) => {
                    job.status = "completed".to_string();
                    job.proof = Some(proof);
                }
                Err(e) => {
                    job.status = "failed".to_string();
                    job.error = Some(e.to_string());
                }
            }
        }
    });

    Json(serde_json::json!({
        "job_id": job_id,
        "status": "pending"
    }))
}

// Get job status
async fn get_job_status(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let jobs = state.jobs.read().await;

    if let Some(job) = jobs.get(&job_id) {
        let mut response = serde_json::json!({
            "job_id": job.job_id,
            "status": job.status,
        });

        if let Some(proof) = &job.proof {
            // Include the calldata when proof is ready
            response["calldata"] = serde_json::json!(proof.calldata);
            response["proof_id"] = serde_json::json!(proof.proof_id);

            // Build the full response the frontend expects
            if let Some(calldata) = &proof.calldata {
                response["result"] = serde_json::json!({
                    "transaction": proof.proof_id,
                    "resource_commitment": format!("0x{}", proof.proof_id),
                    "calldata": calldata,
                    "forwarder_call": {
                        "data": calldata
                    }
                });
            }
        }

        if let Some(error) = &job.error {
            response["error"] = serde_json::json!(error);
        }

        Ok(Json(response))
    } else {
        Err(anyhow::anyhow!("Job not found: {}", job_id).into())
    }
}

// ============== SYNC ENDPOINTS (for backend compatibility) ==============

async fn prove_shield_sync(
    State(state): State<AppState>,
    Json(req): Json<ShieldProofRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!("Shield proof request (sync): {:?}", req);

    let prover = state.prover.read().await;
    let response = prover
        .create_shield_proof(&req.token, &req.amount, &req.sender, &req.nullifier_key)
        .await?;

    let forwarder = match req.token.to_uppercase().as_str() {
        "USDC" => "0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE",
        "WETH" => "0xD5307D777dC60b763b74945BF5A42ba93ce44e4b",
        _ => "0x0000000000000000000000000000000000000000",
    };

    let resource = serde_json::json!({
        "logic_ref": response.proof.as_ref().map(|p| &p.image_id).unwrap_or(&"".to_string()),
        "label_ref": format!("0x{}", hex::encode(req.token.as_bytes())),
        "quantity": req.amount.parse::<u64>().unwrap_or(0),
        "value_ref": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "is_ephemeral": true,
        "nonce": format!("0x{}", response.proof_id),
        "nk_commitment": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "rand_seed": "0x0000000000000000000000000000000000000000000000000000000000000000"
    });

    Ok(Json(serde_json::json!({
        "transaction": response.proof_id,
        "resource_commitment": format!("0x{}", response.proof_id),
        "resource": resource,
        "forwarder_call": {
            "to": forwarder,
            "data": response.calldata.as_ref().map(|c| c.clone()).unwrap_or_default()
        },
        "calldata": response.calldata
    })))
}

async fn prove_swap_sync(
    State(state): State<AppState>,
    Json(req): Json<SwapProofRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!("Swap proof request (sync): {:?}", req);

    let prover = state.prover.read().await;
    let response = prover
        .create_swap_proof(&req.input_resource, &req.output_token, &req.nullifier_key, &req.min_amount_out)
        .await?;

    let new_resource = serde_json::json!({
        "logic_ref": response.proof.as_ref().map(|p| &p.image_id).unwrap_or(&"".to_string()),
        "label_ref": format!("0x{}", hex::encode(req.output_token.as_bytes())),
        "quantity": req.min_amount_out.parse::<u64>().unwrap_or(0),
        "value_ref": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "is_ephemeral": true,
        "nonce": format!("0x{}", response.proof_id),
        "nk_commitment": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "rand_seed": "0x0000000000000000000000000000000000000000000000000000000000000000"
    });

    Ok(Json(serde_json::json!({
        "transaction": response.proof_id,
        "nullifier": format!("0x{}", response.proof_id),
        "new_resource_commitment": format!("0x{}", response.proof_id),
        "new_resource": new_resource,
        "uniswap_call": {
            "to": "0x9335Fa4A31E552378Ed29b94704c52b5635cd1AA",
            "data": response.calldata.as_ref().map(|c| c.clone()).unwrap_or_default()
        },
        "calldata": response.calldata
    })))
}

async fn prove_unshield_sync(
    State(state): State<AppState>,
    Json(req): Json<UnshieldProofRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    info!("Unshield proof request (sync): {:?}", req);

    let prover = state.prover.read().await;
    let response = prover
        .create_unshield_proof(&req.resource, &req.recipient, &req.nullifier_key)
        .await?;

    let token = req.resource.get("label_ref")
        .and_then(|v| v.as_str())
        .map(|s| String::from_utf8_lossy(&hex::decode(s.trim_start_matches("0x")).unwrap_or_default()).to_string())
        .unwrap_or_else(|| "USDC".to_string());

    let forwarder = match token.to_uppercase().as_str() {
        "USDC" => "0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE",
        "WETH" => "0xD5307D777dC60b763b74945BF5A42ba93ce44e4b",
        _ => "0x5256b82cB889f8845570b3a2f1C2af7d2F1567fE",
    };

    Ok(Json(serde_json::json!({
        "transaction": response.proof_id,
        "nullifier": format!("0x{}", response.proof_id),
        "forwarder_call": {
            "to": forwarder,
            "data": response.calldata.as_ref().map(|c| c.clone()).unwrap_or_default()
        },
        "calldata": response.calldata
    })))
}
