use anyhow::Result;
use bigdecimal::{BigDecimal, FromPrimitive};
use std::time::Instant;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::QueryBuilder;
use tracing::{debug, info, instrument};

use super::{RpcResponse, TokenTransferChange, TransactionInfo, TransactionResult};
use crate::logging::mask_addr;

pub struct Database {
    pool: PgPool,
}

pub struct SaveStats {
    pub transactions: u64,
    pub token_transfers: u64,
}

impl Database {
    #[instrument]
    pub async fn new_pool() -> Result<Self> {
        let url = dotenvy::var("DATABASE_URL").expect("database_url не найден в .env");
        let started = Instant::now();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;
        info!(
            elapsed_ms = started.elapsed().as_millis(),
            "Database pool created"
        );

        Ok(Database { pool })
    }

    /// Получает пачку подписей, которые еще не были обработаны (is_processed = false),
    /// и сразу помечает их как обработанные (is_processed = true).
    /// Использует атомарное обновление (UPDATE ... RETURNING) для корректной работы в многопоточной среде.
    #[instrument(skip(self), fields(address = %mask_addr(address), limit))]
    pub async fn get_and_mark_unprocessed_signatures(
        &self,
        address: &str,
        limit: i64,
    ) -> Result<Vec<String>> {
        let started = Instant::now();
        let signatures = sqlx::query!(
            r#"
            WITH selected_signatures AS (
                SELECT signature
                FROM signatures
                WHERE owner_address = $1 AND is_processed = FALSE
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            UPDATE signatures
            SET is_processed = TRUE
            FROM selected_signatures
            WHERE signatures.signature = selected_signatures.signature
            RETURNING signatures.signature
            "#,
            address, // $1
            limit    // $2
        )
        .fetch_all(&self.pool)
        .await?;

        let result: Vec<String> = signatures.into_iter().map(|rec| rec.signature).collect();
        debug!(
            count = result.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "Fetched and marked unprocessed signatures"
        );
        Ok(result)
    }

    #[instrument(skip(self, signatures), fields(address = %mask_addr(adress), input_count = signatures.result.len()))]
    pub async fn write_signatures(&self, signatures: &RpcResponse, adress: &str) -> Result<u64> {
        let started = Instant::now();
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "INSERT INTO signatures 
            (owner_address, signature, block_time)",
        );

        let signatures_iter = signatures.result.iter();

        query_builder.push_values(signatures_iter, |mut b, signature| {
            b.push_bind(&adress)
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

    #[instrument(skip(self, transaction_info), fields(address = %mask_addr(tracked_owner), input_count = transaction_info.len()))]
    pub async fn write_transaction_info(
        &self,
        transaction_info: &Vec<TransactionResult>,
        tracked_owner: &str,
    ) -> Result<u64> {
        let started = Instant::now();
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "INSERT INTO transactions
            (owner_address, signature, slot, block_time, err, fee, compute_units, num_signers, num_instructions)",
        );

        let transaction_iter = transaction_info.iter();

        query_builder.push_values(transaction_iter, |mut b, tx| {
            let signature = tx
                .result
                .transaction
                .signatures
                .first()
                .map(String::as_str)
                .unwrap_or_default();
            let num_signers = tx.num_signers();
            let num_instructions = tx.num_instructions();

            b.push_bind(&tracked_owner)
                .push_bind(signature)
                .push_bind(tx.result.slot)
                .push_bind(tx.result.block_time)
                .push_bind(&tx.result.meta.err)
                .push_bind(tx.result.meta.fee)
                .push_bind(tx.result.meta.compute_units_consumed)
                .push_bind(num_signers)
                .push_bind(num_instructions);
        });
        query_builder.push("ON CONFLICT (owner_address, signature) DO NOTHING");

        let query = query_builder.build();

        let result = query.execute(&self.pool).await?;
        let inserted = result.rows_affected();
        debug!(
            inserted,
            elapsed_ms = started.elapsed().as_millis(),
            "Transactions inserted"
        );

        Ok(inserted)
    }

    #[instrument(skip(self, transactions), fields(tracked_owner = %mask_addr(tracked_owner), input_count = transactions.len()))]
    pub async fn write_token_transfers(
        &self,
        transactions: &[TransactionResult],
        tracked_owner: &str,
    ) -> Result<u64> {
        let mut rows: Vec<(&TransactionInfo, &TokenTransferChange)> = Vec::new();

        for tx in transactions {
            for transfer in &tx.token_transfer_changes {
                rows.push((&tx.result, transfer));
            }
        }

        if rows.is_empty() {
            debug!("No token transfers to insert");
            return Ok(0);
        }
        let started = Instant::now();

        // Сохраняем распарсенные transfer-поля по схеме token_transfers
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "INSERT INTO token_transfers
            (tracked_owner, signature, source_owner, destination_owner, source_token_account, destination_token_account, token_mint, token_program, amount_raw, amount_ui, decimals, asset_type, transfer_type, direction, instruction_idx, inner_idx, authority, slot, block_time)",
        );

        query_builder.push_values(rows.iter(), |mut b, (tx, transfer)| {
            let signature = tx
                .transaction
                .signatures
                .first()
                .map(String::as_str)
                .unwrap_or_default();

            b.push_bind(&tracked_owner)
                .push_bind(signature)
                .push_bind(&transfer.source_owner)
                .push_bind(&transfer.destination_owner)
                .push_bind(&transfer.source_token_account)
                .push_bind(&transfer.destination_token_account)
                .push_bind(&transfer.token_mint)
                .push_bind(&transfer.token_program)
                .push_bind(BigDecimal::from(transfer.amount_raw))
                .push_bind(
                    transfer
                        .amount_ui
                        .and_then(BigDecimal::from_f64),
                )
                .push_bind(transfer.decimals.map(i32::from))
                .push_bind(&transfer.asset_type)
                .push_bind(&transfer.transfer_type)
                .push_bind(&transfer.direction)
                .push_bind(transfer.instruction_idx)
                .push_bind(transfer.inner_idx)
                .push_bind(&transfer.authority)
                .push_bind(i64::from(tx.slot))
                .push_bind(i64::from(tx.block_time));
        });
        query_builder.push("ON CONFLICT DO NOTHING");

        let query = query_builder.build();
        let result = query.execute(&self.pool).await?;
        let inserted = result.rows_affected();
        debug!(
            inserted,
            elapsed_ms = started.elapsed().as_millis(),
            "Token transfers inserted"
        );

        Ok(inserted)
    }

    #[instrument(skip(self, transaction_info), fields(address = %mask_addr(address), input_count = transaction_info.len()))]
    pub async fn save_transaction_data(
        &self,
        transaction_info: &Vec<TransactionResult>,
        address: &str,
    ) -> Result<SaveStats> {
        let started = Instant::now();
        let transactions = self
            .write_transaction_info(transaction_info, address)
            .await?;
        let token_transfers = self
            .write_token_transfers(transaction_info, address)
            .await?;
        info!(
            transactions,
            token_transfers,
            elapsed_ms = started.elapsed().as_millis(),
            "Transaction data saved"
        );

        Ok(SaveStats {
            transactions,
            token_transfers,
        })
    }
}
