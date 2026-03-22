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
use sqlx::{FromRow, PgPool, Row};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};

use crate::logging::mask_addr;

#[derive(Debug, Deserialize, Serialize)]
pub struct AddressProcessing {
    pub address: String,
    pub requested_hours: i16,
    #[serde(rename = "txLimit")]
    pub tx_limit: i16,
}

#[derive(Serialize, FromRow)]
pub struct JobInfo {
    pub status: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub total_transactions: i64,
    pub processed_transactions: i64,
    pub remaining_transactions: i64,
}

pub async fn create_server(pool: PgPool) -> Result<()> {
    info!("Starting API server initialization");

    // Разрешаем cors
    let cors = CorsLayer::new()
        .allow_origin("http://127.0.0.1:5500".parse::<http::HeaderValue>()?)
        .allow_methods(Any)
        .allow_headers(Any);

    // Создаем роутер
    let app = Router::new()
        .route("/analyze", post(address_processing))
        .route("/jobs/{id}", get(get_job_info))
        .layer(cors)
        .with_state(pool);

    // Создаем TCP listener
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    info!(address = "127.0.0.1:8080", "API listener bound");

    // Запускаем сервер
    info!("API server is running");
    axum::serve(listener, app).await?;

    Ok(())
}

pub async fn address_processing(
    State(pool): State<PgPool>,
    Json(payload): Json<AddressProcessing>,
) -> impl IntoResponse {
    info!(
        address = %mask_addr(&payload.address),
        "Received address processing request"
    );

    let requested_hours = payload.requested_hours;
    let tx_limit = payload.tx_limit;

    let query = "INSERT INTO processing_data (address, status, created_at, updated_at, tx_limit, requested_hours)
                 SELECT $1, 'pending', NOW(), NOW(), $2, $3
                 WHERE NOT EXISTS (
                     SELECT 1 FROM processing_data WHERE address = $1
                 )
                 RETURNING id";

    match sqlx::query(query)
        .bind(&payload.address)
        .bind(tx_limit)
        .bind(requested_hours)
        .fetch_optional(&pool)
        .await
    {
        Ok(Some(row)) => {
            let id: i64 = row.get("id");
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

pub async fn get_job_info(State(pool): State<PgPool>, Path(id): Path<i64>) -> impl IntoResponse {
    info!(job_id = id, "Received job info request");
    let query = "
        SELECT
            pd.status,
            pd.updated_at,
            COUNT(s.signature)::bigint AS total_transactions,
            COUNT(*) FILTER (WHERE s.is_processed = TRUE)::bigint AS processed_transactions,
            (
                COUNT(s.signature) - COUNT(*) FILTER (WHERE s.is_processed = TRUE)
            )::bigint AS remaining_transactions
        FROM processing_data pd
        LEFT JOIN signatures s
            ON s.owner_address = pd.address
        WHERE pd.id = $1
        GROUP BY pd.status, pd.updated_at
    ";

    match sqlx::query_as::<_, JobInfo>(query)
        .bind(id)
        .fetch_optional(&pool)
        .await
    {
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
