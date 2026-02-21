use anyhow::Result;
use std::{sync::Arc, time::Instant};
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};
use tracing::{debug, info, warn};

mod requests;
use requests::*;

mod database;
use database::*;

mod api;
use api::*;

mod backoff;
use backoff::WorkerBackoff;

mod logging;
mod telemetry;

struct AppState {
    database: Database,
    helius_api: HeliusApi,
}

#[tokio::main]
async fn main() -> Result<()> {
    telemetry::init()?;

    let app_state = Arc::new(AppState {
        database: Database::new_pool().await?,
        helius_api: HeliusApi::new(),
    });

    // Ловит запросы с фронта (запускаем в отдельной задаче, чтобы не блокировать воркеров)
    let server_handle = tokio::spawn(create_server(app_state.database.pool.clone()));

    let worker_handles: Vec<JoinHandle<Result<()>>> = (1..5)
        .into_iter()
        .map(|worker_id| {
            let state = Arc::clone(&app_state);
            tokio::spawn(worker_loop(state, worker_id))
        })
        .collect();

    // Ждём завершения всех задач (сервер + воркеры)
    tokio::select! {
        res = server_handle => {
            warn!("API server exited: {:?}", res);
        }
        _ = futures::future::join_all(worker_handles) => {
            warn!("All workers exited");
        }
    }

    Ok(())
}

async fn worker_loop(app_state: Arc<AppState>, worker_id: u32) -> Result<()> {
    let mut worker_backoff = WorkerBackoff::new(200, 2000, 0.5);
    loop {
        let started = Instant::now();
        let claimed_job: ClaimedJob = loop {
            let claimed_job = app_state.database.claim_pending_job(worker_id).await;

            match claimed_job {
                Ok(Some(job)) => {
                    worker_backoff.reset();
                    break job;
                }
                Ok(None) => {
                    let delay = worker_backoff.step_and_get_sleep_duration();
                    sleep(delay).await;
                    continue;
                }
                Err(err) => {
                    warn!(%err, worker_id, "Failed to claim pending job");
                    let delay = worker_backoff.step_and_get_sleep_duration();
                    sleep(delay).await;
                    continue;
                }
            }
        };

        let job_id = claimed_job.job_id;
        let address = claimed_job.address;

        let processing_result: Result<()> = async {
            fetching_signatures(&app_state, &address).await?;
            fetched_unprocessed_signatures(&app_state, &address).await?;
            Ok(())
        }
        .await;

        match processing_result {
            Ok(_) => {
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
}

async fn fetching_signatures(app_state: &AppState, address: &str) -> Result<()> {
    let database = &app_state.database;
    let helius_api = &app_state.helius_api;

    let mut cur_last_signature: Option<String> = None;
    let mut sum: usize = 0;
    let masked_address = logging::mask_addr(address);
    let run_span = tracing::info_span!("indexer_run", address = %masked_address);
    let _run_guard = run_span.enter();
    info!("Fetching signatures started");

    let sync_started = Instant::now();
    loop {
        // Сбор всех подписей
        debug!(before = ?cur_last_signature, "Fetching signatures page");
        let page_started = Instant::now();
        let (response, last_signature) = helius_api
            .get_signatures(address, cur_last_signature)
            .await?;

        // может вернуть 0 транзакций и все упадет

        let res_len = response.result.len();
        sum += res_len;

        info!(
            page_len = res_len,
            total = sum,
            elapsed_ms = page_started.elapsed().as_millis(),
            "Signatures page received"
        );

        let inserted = database.write_signatures(&response, address).await?;
        debug!(inserted, "Signatures saved");

        // решить что как правильно поступать с проверкой; для тестов, не больше 2000
        if res_len < 1000 || sum >= 2000 {
            info!("No more signatures available");
            break;
        }

        cur_last_signature = Some(last_signature);

        sleep(Duration::from_millis(125)).await;
    }

    info!(
        total = sum,
        elapsed_ms = sync_started.elapsed().as_millis(),
        "Signature sync finished"
    );

    Ok(())
}

async fn fetched_unprocessed_signatures(app_state: &AppState, address: &str) -> Result<()> {
    let database = &app_state.database;
    let helius_api = &app_state.helius_api;

    loop {
        let signatures = database.get_unprocessed_signatures(address, 100).await?;
        info!(count = signatures.len(), "Fetched unprocessed signatures");

        if signatures.len() == 0 {
            info!("No more signatures available");
            break;
        }

        let tx_fetch_started = Instant::now();
        let transaction_batch = helius_api.get_transaction(signatures).await?;
        info!(
            count = transaction_batch.transactions.len(),
            failed = transaction_batch.failed_signatures.len(),
            elapsed_ms = tx_fetch_started.elapsed().as_millis(),
            "Transactions fetched"
        );

        if !transaction_batch.errors.is_empty() {
            warn!(
                errors = transaction_batch.errors.len(),
                "Some transaction requests failed in this batch"
            );
        }

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
    }

    Ok(())
}
