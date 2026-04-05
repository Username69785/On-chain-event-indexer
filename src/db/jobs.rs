use anyhow::Result;
use sqlx::FromRow;
use sqlx::postgres::PgPool;
use std::time::Instant;
use tracing::{debug, instrument};

pub struct Jobs {
    pool: PgPool,
}

#[derive(Debug, FromRow)]
pub struct ClaimedJob {
    pub job_id: i64,
    pub address: String,
    pub requested_hours: i16,
    pub tx_limit: i16,
}

impl Jobs {
    #[instrument]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[instrument(skip(self), fields(worker_id))]
    pub async fn claim_pending_job(&self, worker_id: u32) -> Result<Option<ClaimedJob>> {
        let started = Instant::now();
        let worker_id = i32::try_from(worker_id).unwrap_or(i32::MAX);

        let claimed_job = sqlx::query_as::<_, ClaimedJob>(
            "
            WITH next_job AS (
                SELECT id
                FROM processing_data pd
                WHERE status = 'pending'
                ORDER BY created_at ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            UPDATE processing_data pd
            SET status     = 'indexing',
                worker_id  = $1,
                updated_at = now()
            FROM next_job
            WHERE pd.id = next_job.id
            RETURNING pd.id AS job_id, pd.address, pd.requested_hours, pd.tx_limit
            ",
        )
        .bind(worker_id)
        .fetch_optional(&self.pool)
        .await?;

        debug!(
            job_id = claimed_job.as_ref().map(|job| job.job_id),
            elapsed_ms = started.elapsed().as_millis(),
            "claim_pending_job"
        );

        Ok(claimed_job)
    }

    #[instrument(skip(self), fields(job_id, status))]
    pub async fn update_processing_status_by_job_id(
        &self,
        job_id: i64,
        status: &str,
    ) -> Result<u64> {
        let started = Instant::now();
        let result = sqlx::query(
            "
            UPDATE processing_data
            SET status     = $1,
                updated_at = now()
            WHERE id = $2
              AND status = 'indexing'
            ",
        )
        .bind(status)
        .bind(job_id)
        .execute(&self.pool)
        .await?;

        let updated = result.rows_affected();
        debug!(
            updated,
            elapsed_ms = started.elapsed().as_millis(),
            "Processing status updated"
        );

        Ok(updated)
    }
}
