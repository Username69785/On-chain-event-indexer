use crate::logging::mask_addr;
use crate::requests::RpcResponse;

use anyhow::Result;
use sqlx::QueryBuilder;
use sqlx::postgres::PgPool;
use std::time::Instant;
use tracing::{debug, instrument};

pub struct Signatures {
    pub pool: PgPool,
}

impl Signatures {
    #[instrument]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Получает пачку подписей, которые еще не были обработаны (`is_processed` = false).
    /// Отметка как обработанных выполняется отдельным шагом после успешной обработки.
    #[instrument(skip(self), fields(address = %mask_addr(address), limit))]
    pub async fn get_unprocessed_signatures(
        &self,
        address: &str,
        limit: i64,
    ) -> Result<Vec<String>> {
        let started = Instant::now();
        let signatures = sqlx::query_scalar::<_, String>(
            "
            SELECT signature
            FROM signatures
            WHERE owner_address = $1 AND is_processed = FALSE
            ORDER BY block_time DESC
            LIMIT $2
            ",
        )
        .bind(address)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let result: Vec<String> = signatures;
        debug!(
            count = result.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "Fetched unprocessed signatures"
        );
        Ok(result)
    }

    #[instrument(skip(self, signatures), fields(address = %mask_addr(address), input_count = signatures.len()))]
    pub async fn mark_signatures_processed(
        &self,
        address: &str,
        signatures: &[String],
    ) -> Result<u64> {
        if signatures.is_empty() {
            return Ok(0);
        }

        let started = Instant::now();
        let result = sqlx::query(
            "
            UPDATE signatures
            SET is_processed = TRUE
            WHERE owner_address = $1
              AND signature = ANY($2)
              AND is_processed = FALSE
            ",
        )
        .bind(address)
        .bind(signatures)
        .execute(&self.pool)
        .await?;

        let updated = result.rows_affected();
        debug!(
            updated,
            elapsed_ms = started.elapsed().as_millis(),
            "Signatures marked as processed"
        );
        Ok(updated)
    }

    #[instrument(skip(self, signatures), fields(address = %mask_addr(adress), input_count = signatures.result.len()))]
    pub async fn write_signatures(&self, signatures: &RpcResponse, adress: &str) -> Result<u64> {
        if signatures.result.is_empty() {
            debug!("No signatures to insert");
            return Ok(0);
        }

        let started = Instant::now();
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "INSERT INTO signatures 
            (owner_address, signature, block_time)",
        );

        let signatures_iter = signatures.result.iter();

        query_builder.push_values(signatures_iter, |mut b, signature| {
            b.push_bind(adress)
                .push_bind(&signature.signature)
                .push_bind(signature.block_time);
        });
        query_builder.push("ON CONFLICT (signature) DO NOTHING");

        let query = query_builder.build();

        let result = query.execute(&self.pool).await?;
        let inserted = result.rows_affected();
        debug!(
            inserted,
            elapsed_ms = started.elapsed().as_millis(),
            "Signatures inserted"
        );

        Ok(inserted)
    }
}
