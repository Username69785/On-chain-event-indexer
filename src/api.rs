use axum::{
    Router,
    extract::{Json, State},
    response::IntoResponse,
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;

#[derive(Debug, Deserialize, Serialize)]
pub struct Address {
    pub address: String,
}

pub async fn create(pool: PgPool) {
    // Создаем роутер
    let app = Router::new()
        .route("/application", post(address_processing))
        .with_state(pool);

    // Создаем TCP listener
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();

    // Запускаем сервер
    axum::serve(listener, app).await.unwrap();
}

pub async fn address_processing(
    State(pool): State<PgPool>,
    Json(payload): Json<Address>,
) -> impl IntoResponse {
    let query = "INSERT INTO processing_data (address, day, status, created_at, updated_at) 
                 VALUES ($1, CURRENT_DATE, 'pending', NOW(), NOW())";

    match sqlx::query(query)
        .bind(&payload.address)
        .execute(&pool)
        .await
    {
        Ok(_) => Json(json!({ "status": "ok" })),
        Err(e) => {
            // В хорошем приложении стоило бы логировать ошибку
            Json(json!({ "status": "error", "message": e.to_string() }))
        }
    }
}
