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
pub struct Address {
    pub address: String,
}

#[derive(Serialize, FromRow)]
pub struct JobStatus {
    pub status: String,
    pub updated_at: chrono::NaiveDateTime,
}

pub async fn create_server(pool: PgPool) {
    info!("Starting API server initialization");

    // Разрешаем cors
    let cors = CorsLayer::new()
        .allow_origin(
            "http://127.0.0.1:5500"
                .parse::<http::HeaderValue>()
                .unwrap(),
        )
        .allow_methods(Any)
        .allow_headers(Any);

    // Создаем роутер
    let app = Router::new()
        .route("/analyze", post(address_processing))
        .route("/jobs/{id}", get(get_job_status))
        .layer(cors)
        .with_state(pool);

    // Создаем TCP listener
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();
    info!(address = "127.0.0.1:8080", "API listener bound");

    // Запускаем сервер
    info!("API server is running");
    axum::serve(listener, app).await.unwrap();
}

pub async fn address_processing(
    State(pool): State<PgPool>,
    Json(payload): Json<Address>,
) -> impl IntoResponse {
    info!(
        address = %mask_addr(&payload.address),
        "Received address processing request"
    );

    let query = "INSERT INTO processing_data (address, day, status, created_at, updated_at)
                 SELECT $1, CURRENT_DATE, 'pending', NOW(), NOW()
                 WHERE NOT EXISTS (
                     SELECT 1 FROM processing_data WHERE address = $1
                 )
                 RETURNING id";

    match sqlx::query(query)
        .bind(&payload.address)
        .fetch_optional(&pool)
        .await
    {
        Ok(Some(row)) => {
            let id: i64 = row.get("id");
            info!(job_id = id, "Processing job created");
            Json(json!({ "status": "ok", "job_id": id }))
        }
        Ok(None) => {
            info!(
                address = %mask_addr(&payload.address),
                "Processing job skipped: address already exists"
            );
            Json(json!({ "status": "ok", "message": "address already exists" }))
        }
        Err(e) => {
            error!(error = %e, "Failed to create processing job");
            Json(json!({ "status": "error", "message": e.to_string() }))
        }
    }
}

pub async fn get_job_status(State(pool): State<PgPool>, Path(id): Path<i64>) -> impl IntoResponse {
    info!(job_id = id, "Received job status request");
    let query = "SELECT status, updated_at FROM processing_data WHERE id = $1";

    match sqlx::query_as::<_, JobStatus>(query)
        .bind(id)
        .fetch_optional(&pool)
        .await
    {
        Ok(Some(job_status)) => {
            info!(job_id = id, status = %job_status.status, "Job status returned");
            Json(job_status).into_response()
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
            error!(job_id = id, error = %e, "Failed to fetch job status");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}
