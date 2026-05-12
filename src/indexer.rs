use crate::{
    AppState, logging,
    types::{ClaimedJob, JobInfo},
};

use anyhow::Result;
use bigdecimal::{ToPrimitive, Zero};
use std::time::Instant;
use tracing::{Instrument, debug, info, warn};

pub async fn process_claimed_job(app_state: &AppState, worker_id: u32, claimed_job: ClaimedJob) {
    let started = Instant::now();
    let job_id = claimed_job.job_id;
    let address = claimed_job.address;
    let requested_hours = claimed_job.requested_hours;
    let tx_limit = claimed_job.tx_limit;

    let processing_result: Result<()> = async {
        fetch_signatures(
            app_state,
            &address,
            tx_limit.to_usize().unwrap_or(1000),
            requested_hours,
        )
        .await?;
        process_unprocessed_signatures(app_state, &address).await?;
        Ok(())
    }
    .await;

    match processing_result {
        Ok(()) => {
            if let Err(err) = app_state
                .database
                .update_processing_status_by_job_id(job_id, "ready")
                .await
            {
                warn!(
                    %err,
                    job_id,
                    worker_id,
                    "Failed to update processing status to ready"
                );
            }

            info!(
                elapsed_ms = started.elapsed().as_millis(),
                worker_id, job_id, "Indexer finished for {}", &address
            );
        }
        Err(err) => {
            warn!(%err, worker_id, job_id, "Indexer failed for {}", &address);
            if let Err(status_err) = app_state
                .database
                .update_processing_status_by_job_id(job_id, "error")
                .await
            {
                warn!(
                    %status_err,
                    job_id,
                    worker_id,
                    "Failed to update processing status to error"
                );
            }
        }
    }
}

pub async fn process_pending_job_once(
    app_state: &AppState,
    worker_id: u32,
) -> Result<Option<JobInfo>> {
    let Some(claimed_job) = app_state.database.claim_pending_job(worker_id).await? else {
        return Ok(None);
    };

    let job_id = claimed_job.job_id;
    process_claimed_job(app_state, worker_id, claimed_job).await;

    app_state.database.get_job_info(job_id).await
}

async fn fetch_signatures(
    app_state: &AppState,
    address: &str,
    tx_limit: usize,
    requested_hours: i16,
) -> Result<()> {
    let database = &app_state.database;
    let helius_api = &app_state.helius_api;
    let masked_address = logging::mask_addr(address);
    let run_span = tracing::info_span!("indexer_run", address = %masked_address);

    async {
        let mut cur_last_signature: Option<String> = None;
        let mut sum: usize = 0;
        info!("Fetching signatures started");

        let sync_started = Instant::now();
        loop {
            debug!(before = ?cur_last_signature, "Fetching signatures page");
            let page_started = Instant::now();
            let signatures_page = helius_api
                .get_signatures(address, cur_last_signature, requested_hours)
                .await?;

            let res_len = signatures_page.response.result.len();
            sum += res_len;

            info!(
                raw_page_len = signatures_page.raw_count,
                page_len = res_len,
                total = sum,
                reached_cutoff = signatures_page.reached_cutoff,
                elapsed_ms = page_started.elapsed().as_millis(),
                "Signatures page received"
            );

            let inserted = database
                .write_signatures(&signatures_page.response, address)
                .await?;
            debug!(inserted, "Signatures saved");

            if signatures_page.reached_cutoff
                || signatures_page.last_signature.is_none()
                || signatures_page.raw_count < 1000
                || sum >= tx_limit
            {
                info!("No more signatures available");
                break;
            }

            cur_last_signature = signatures_page.last_signature;
        }

        info!(
            total = sum,
            elapsed_ms = sync_started.elapsed().as_millis(),
            "Signature sync finished"
        );

        Ok(())
    }
    .instrument(run_span)
    .await
}

async fn process_unprocessed_signatures(app_state: &AppState, address: &str) -> Result<()> {
    let database = &app_state.database;
    let helius_api = &app_state.helius_api;

    loop {
        let signatures = database.get_unprocessed_signatures(address, 100).await?;
        info!(count = signatures.len(), "Fetched unprocessed signatures");

        if signatures.len().is_zero() {
            info!("No more signatures available");
            break;
        }

        let tx_fetch_started = Instant::now();
        let transaction_batch = helius_api.get_transaction(&signatures).await?;
        info!(
            count = transaction_batch.transactions.len(),
            failed = transaction_batch.failed_signatures.len(),
            elapsed_ms = tx_fetch_started.elapsed().as_millis(),
            "Transactions fetched"
        );

        let save_started = Instant::now();
        let save_stats = database
            .save_transaction_data(&transaction_batch.transactions, address)
            .await?;
        let marked_processed = database
            .mark_signatures_processed(address, &transaction_batch.processed_signatures)
            .await?;

        info!(
            transactions_saved = save_stats.transactions,
            token_transfers_saved = save_stats.token_transfers,
            signatures_marked_processed = marked_processed,
            elapsed_ms = save_started.elapsed().as_millis(),
            "Transaction data saved"
        );

        if !transaction_batch.errors.is_empty() {
            warn!(
                errors = transaction_batch.errors.len(),
                "Some transaction requests failed in this batch"
            );
            return Err(anyhow::anyhow!(
                "{} transaction request(s) failed",
                transaction_batch.errors.len()
            ));
        }
    }

    Ok(())
}
