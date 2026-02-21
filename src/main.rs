use anyhow::{Ok, Result};
use futures::future::ok;
use std::time::Instant;
use tokio::time::{Duration, sleep};
use tracing::{debug, info, warn};

mod requests;
use requests::*;

mod database;
use database::*;

mod api;
use api::*;

mod logging;
mod telemetry;

struct AppState {
    database: Database,
    helius_api: HeliusApi,
}

#[tokio::main]
async fn main() -> Result<()> {
    telemetry::init()?;

    let app_state = AppState {
        database: Database::new_pool().await?,
        helius_api: HeliusApi::new(),
    };

    let started = Instant::now();

    create(app_state.database.pool.clone()).await;
    // TODO: начать трекать запросы с фронта, адресс записывать в бд

    // TODO: брать адрес с бд
    fetching_signatures(&app_state, "airsent").await?;
    fetched_unprocessed_signatures(&app_state, "airsent").await?;

    info!(
        elapsed_ms = started.elapsed().as_millis(),
        "Indexer finished"
    );

    Ok(())
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
