use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

mod prover;

use prover::{ProverService, ProofResponse};

#[derive(Clone)]
struct AppState {
    prover: Arc<RwLock<ProverService>>,
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
) -> Result<Json<ProofResponse>, AppError> {
    info!("Shield proof request: {:?}", req);

    let prover = state.prover.read().await;

    let response = prover
        .create_shield_proof(&req.token, &req.amount, &req.sender, &req.nullifier_key)
        .await?;

    Ok(Json(response))
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
) -> Result<Json<ProofResponse>, AppError> {
    info!("Swap proof request: {:?}", req);

    let prover = state.prover.read().await;

    let response = prover
        .create_swap_proof(&req.input_resource, &req.output_token, &req.nullifier_key, &req.min_amount_out)
        .await?;

    Ok(Json(response))
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
) -> Result<Json<ProofResponse>, AppError> {
    info!("Unshield proof request: {:?}", req);

    let prover = state.prover.read().await;

    let response = prover
        .create_unshield_proof(&req.resource, &req.recipient, &req.nullifier_key)
        .await?;

    Ok(Json(response))
}

// Proof status check
async fn proof_status(
    State(state): State<AppState>,
    Path(proof_id): Path<String>,
) -> Result<Json<ProofResponse>, AppError> {
    let prover = state.prover.read().await;

    let response = prover.get_proof_status(&proof_id).await?;

    Ok(Json(response))
}
