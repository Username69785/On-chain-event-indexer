pub mod jobs;
pub mod signatures;
pub mod transactions;

use jobs::Jobs;
use signatures::Signatures;
use transactions::Transactions;

use crate::backoff::WorkerBackoff;
use crate::requests::{RpcResponse, TransactionResult};
use crate::types::{ClaimedJob, JobInfo, SaveStats};

use anyhow::{Context, Result};
use sqlx::PgPool;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use std::time::Instant;
use tracing::{error, info, instrument, warn};

pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub struct Database {
    jobs: Jobs,
    signatures: Signatures,
    transactions: Transactions,
    pool: PgPool,
}

impl Database {
    #[instrument]
    pub async fn new() -> Result<Self> {
        let max_retries = 5;
        let mut retry_count = 0;
        let mut backoff = WorkerBackoff::new(500.0, 5_000.0, 2.0);

        let url = dotenvy::var("DATABASE_URL").context("DATABASE_URL is not set")?;
        let started = Instant::now();
        let pool = loop {
            match PgPoolOptions::new().max_connections(5).connect(&url).await {
                Ok(pool) => break pool,
                Err(err) if retry_count < max_retries => {
                    retry_count += 1;
                    let delay = backoff.step_and_get_sleep_duration();
                    warn!(
                        %err,
                        attempt = retry_count,
                        max_retries,
                        ?delay,
                        "Failed to connect to database; retrying"
                    );

                    tokio::time::sleep(delay).await;
                }
                Err(err) => {
                    error!(
                        %err,
                        attempts = retry_count + 1,
                        max_retries,
                        elapsed_ms = started.elapsed().as_millis(),
                        "Failed to connect to database after retry limit"
                    );
                    return Err(err).context(format!(
                        "failed to connect to database after {} attempts",
                        retry_count + 1
                    ));
                }
            }
        };

        info!(
            elapsed_ms = started.elapsed().as_millis(),
            "Database pool created"
        );

        Ok(Self::from_pool(pool))
    }

    #[instrument(skip(pool))]
    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            jobs: Jobs::new(pool.clone()),
            signatures: Signatures::new(pool.clone()),
            transactions: Transactions::new(pool.clone()),
            pool,
        }
    }

    pub async fn migrate(&self) -> Result<()> {
        MIGRATOR
            .run(&self.pool)
            .await
            .context("failed to run database migrations")?;
        Ok(())
    }

    pub async fn claim_pending_job(&self, worker_id: u32) -> Result<Option<ClaimedJob>> {
        self.jobs.claim_pending_job(worker_id).await
    }

    pub async fn update_processing_status_by_job_id(
        &self,
        job_id: i64,
        status: &str,
    ) -> Result<u64> {
        self.jobs
            .update_processing_status_by_job_id(job_id, status)
            .await
    }

    pub async fn get_unprocessed_signatures(
        &self,
        address: &str,
        limit: i64,
    ) -> Result<Vec<String>> {
        self.signatures
            .get_unprocessed_signatures(address, limit)
            .await
    }

    pub async fn mark_signatures_processed(
        &self,
        address: &str,
        signatures: &[String],
    ) -> Result<u64> {
        self.signatures
            .mark_signatures_processed(address, signatures)
            .await
    }

    pub async fn write_signatures(&self, signatures: &RpcResponse, address: &str) -> Result<u64> {
        self.signatures.write_signatures(signatures, address).await
    }

    pub async fn save_transaction_data(
        &self,
        transaction_info: &[TransactionResult],
        address: &str,
    ) -> Result<SaveStats> {
        self.transactions
            .save_transaction_data(transaction_info, address)
            .await
    }

    pub async fn get_job_info(&self, job_id: i64) -> Result<Option<JobInfo>> {
        self.jobs.get_job_info(job_id).await
    }

    pub async fn create_processing_job(
        &self,
        address: &str,
        tx_limit: i16,
        requested_hours: i16,
    ) -> Result<Option<i64>> {
        self.jobs
            .create_processing_job(address, tx_limit, requested_hours)
            .await
    }
}
