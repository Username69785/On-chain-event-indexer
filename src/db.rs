pub mod jobs;
pub mod signatures;
pub mod transactions;

pub use jobs::*;
pub use signatures::*;
pub use transactions::*;

use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use std::time::Instant;
use tracing::{info, instrument};

pub struct Database {
    pub jobs: Jobs,
    pub signatures: Signatures,
    pub transactions: Transactions,
}

impl Database {
    #[instrument]
    pub async fn new() -> Result<Self> {
        let url = dotenvy::var("DATABASE_URL")?;
        let started = Instant::now();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        info!(
            elapsed_ms = started.elapsed().as_millis(),
            "Database pool created"
        );

        Ok(Self {
            jobs: Jobs::new(pool.clone()),
            signatures: Signatures::new(pool.clone()),
            transactions: Transactions::new(pool),
        })
    }
}
