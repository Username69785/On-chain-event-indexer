use crate::types::{ClaimedJob, JobInfo};
use anyhow::{Ok, Result};
use sqlx::postgres::PgPool;
use std::time::Instant;
use tracing::{debug, instrument};

pub struct Jobs {
    pool: PgPool,
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
                updated_at = NOW()
            WHERE id = $2
              AND status = 'indexing'
              AND (
                  $1 <> 'ready'
                  OR NOT EXISTS (
                      SELECT 1
                      FROM signatures s
                      WHERE s.owner_address = processing_data.address
                        AND s.block_time >= EXTRACT(EPOCH FROM (processing_data.created_at - processing_data.requested_hours * INTERVAL '1 hour'))::bigint
                        AND s.block_time <= EXTRACT(EPOCH FROM processing_data.created_at)::bigint
                        AND s.is_processed = FALSE
                  )
              )
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

    pub async fn get_job_info(&self, job_id: i64) -> Result<Option<JobInfo>> {
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
           AND s.block_time >= EXTRACT(EPOCH FROM (pd.created_at - pd.requested_hours * INTERVAL '1 hour'))::bigint
           AND s.block_time <= EXTRACT(EPOCH FROM pd.created_at)::bigint
        WHERE pd.id = $1
        GROUP BY pd.status, pd.updated_at
    ";

        let result = sqlx::query_as::<_, JobInfo>(query)
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(result)
    }

    pub async fn create_processing_job(
        &self,
        address: &str,
        tx_limit: i16,
        requested_hours: i16,
    ) -> Result<Option<i64>> {
        let query = "INSERT INTO processing_data (address, status, created_at, updated_at, tx_limit, requested_hours)
                VALUES ($1, 'pending', NOW(), NOW(), $2, $3)
                RETURNING id";

        let job_id: Option<i64> = sqlx::query_scalar(query)
            .bind(address)
            .bind(tx_limit)
            .bind(requested_hours)
            .fetch_optional(&self.pool)
            .await?;

        Ok(job_id)
    }
}
