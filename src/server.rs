use std::sync::Arc;

use anyhow::Result;
use axum::{
    Router,
    extract::{Json, Path, State},
    http,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};

use crate::AppState;
use crate::logging::mask_addr;

#[derive(Debug, Deserialize, Serialize)]
pub struct AddressProcessing {
    pub address: String,
    pub requested_hours: i16,
    #[serde(rename = "txLimit")]
    pub tx_limit: i16,
}

pub async fn create_server(
    app_state: Arc<AppState>,
    bind: SocketAddr,
    cors_allowed_origins: Vec<String>,
) -> Result<()> {
    info!("Starting API server initialization");

    info!(?cors_allowed_origins, "Configured CORS allowed origins");

    let allowed_origins = cors_allowed_origins
        .iter()
        .map(|origin| str::parse::<http::HeaderValue>(origin))
        .collect::<Result<Vec<_>, _>>()?;

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/analyze", post(address_processing))
        .route("/jobs/{id}", get(get_job_info))
        .layer(cors)
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    info!(address = bind.to_string(), "API listener bound");

    info!("API server is running");
    axum::serve(listener, app).await?;

    Ok(())
}

pub async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

pub async fn address_processing(
    State(app_state): State<Arc<AppState>>,
    Json(payload): Json<AddressProcessing>,
) -> impl IntoResponse {
    info!(
        address = %mask_addr(&payload.address),
        "Received address processing request"
    );

    let result = app_state
        .database
        .create_processing_job(&payload.address, payload.tx_limit, payload.requested_hours)
        .await;

    match result {
        Ok(Some(id)) => {
            info!(job_id = id, "Processing job created");
            Json(json!({ "status": "ok", "job_id": id })).into_response()
        }
        Ok(None) => {
            info!(
                address = %mask_addr(&payload.address),
                "Processing job skipped: address already exists"
            );
            Json(json!({ "status": "ok", "message": "address already exists" })).into_response()
        }
        Err(e) => {
            error!(error = %e, "Failed to create processing job");
            Json(json!({ "status": "error", "message": e.to_string() })).into_response()
        }
    }
}

pub async fn get_job_info(
    State(app_state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    info!(job_id = id, "Received job info request");
    let result = app_state.database.get_job_info(id).await;

    match result {
        Ok(Some(job_info)) => {
            info!(
                job_id = id,
                status = %job_info.status,
                total_transactions = job_info.total_transactions,
                processed_transactions = job_info.processed_transactions,
                remaining_transactions = job_info.remaining_transactions,
                "Job info returned"
            );
            Json(job_info).into_response()
        }
        Ok(None) => {
            warn!(job_id = id, "Job not found");
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(json!({ "error": "Job not found" })),
            )
                .into_response()
        }
        Err(e) => {
            error!(job_id = id, error = %e, "Failed to fetch job info");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}
