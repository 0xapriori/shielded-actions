use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, error};

mod prover;

use prover::{ProverService, ProofResponse};

#[derive(Clone)]
struct AppState {
    prover: Arc<RwLock<ProverService>>,
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
    };

    // CORS configuration
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/prove/shield", post(prove_shield))
        .route("/api/prove/swap", post(prove_swap))
        .route("/api/prove/unshield", post(prove_unshield))
        .route("/api/status/:proof_id", get(proof_status))
        .layer(cors)
        .with_state(state);

    // Get port from environment
    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".to_string());
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

// Shield proof request
#[derive(Debug, Deserialize)]
struct ShieldProofRequest {
    token: String,
    amount: String,
    sender: String,
    nullifier_key: String,
}

async fn prove_shield(
    State(state): State<AppState>,
    Json(req): Json<ShieldProofRequest>,
) -> Result<Json<ProofResponse>, (StatusCode, String)> {
    info!("Shield proof request: {:?}", req);

    let prover = state.prover.read().await;

    match prover.create_shield_proof(&req.token, &req.amount, &req.sender, &req.nullifier_key).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Shield proof failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

// Swap proof request
#[derive(Debug, Deserialize)]
struct SwapProofRequest {
    input_resource: serde_json::Value,
    output_token: String,
    nullifier_key: String,
    min_amount_out: String,
}

async fn prove_swap(
    State(state): State<AppState>,
    Json(req): Json<SwapProofRequest>,
) -> Result<Json<ProofResponse>, (StatusCode, String)> {
    info!("Swap proof request: {:?}", req);

    let prover = state.prover.read().await;

    match prover.create_swap_proof(&req.input_resource, &req.output_token, &req.nullifier_key, &req.min_amount_out).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Swap proof failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

// Unshield proof request
#[derive(Debug, Deserialize)]
struct UnshieldProofRequest {
    resource: serde_json::Value,
    recipient: String,
    nullifier_key: String,
}

async fn prove_unshield(
    State(state): State<AppState>,
    Json(req): Json<UnshieldProofRequest>,
) -> Result<Json<ProofResponse>, (StatusCode, String)> {
    info!("Unshield proof request: {:?}", req);

    let prover = state.prover.read().await;

    match prover.create_unshield_proof(&req.resource, &req.recipient, &req.nullifier_key).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Unshield proof failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    }
}

// Proof status check
async fn proof_status(
    State(state): State<AppState>,
    axum::extract::Path(proof_id): axum::extract::Path<String>,
) -> Result<Json<ProofResponse>, (StatusCode, String)> {
    let prover = state.prover.read().await;

    match prover.get_proof_status(&proof_id).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Proof status check failed: {}", e);
            Err((StatusCode::NOT_FOUND, e.to_string()))
        }
    }
}
