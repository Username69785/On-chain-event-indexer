use anyhow::Result;
use std::time::Instant;
use tokio::time::{Duration, sleep};
use tracing::{debug, info};

mod requests;
use requests::*;

mod database;
use database::*;

mod logging;
mod telemetry;

#[tokio::main]
async fn main() -> Result<()> {
    telemetry::init()?;
    let started = Instant::now();
    let database = Database::new_pool().await?;
    let helius_api = HeliusApi::new();

    let mut cur_last_signature: Option<String> = None;
    let mut sum: usize = 0;
    let address = "Ckn17KaYABk3gTdgHxZtDwQpCwnKHyyXzTX6SDH7ma44";
    let masked_address = logging::mask_addr(address);
    let run_span = tracing::info_span!("indexer_run", address = %masked_address);
    let _run_guard = run_span.enter();
    info!("Indexer started");

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

        // решить что как правильно поступать с проверкой
        if res_len < 1000 {
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

    loop {
        let signatures = database
            .get_and_mark_unprocessed_signatures(address, 100)
            .await?;
        info!(count = signatures.len(), "Fetched unprocessed signatures");

        if signatures.len() == 0 {
            info!("No more signatures available");
            break;
        }

        let tx_fetch_started = Instant::now();
        let transaction_info = helius_api.get_transaction(signatures).await?;
        info!(
            count = transaction_info.len(),
            elapsed_ms = tx_fetch_started.elapsed().as_millis(),
            "Transactions fetched"
        );

        let save_started = Instant::now();
        let save_stats = database
            .save_transaction_data(&transaction_info, address)
            .await?;

        info!(
            transactions_saved = save_stats.transactions,
            token_transfers_saved = save_stats.token_transfers,
            elapsed_ms = save_started.elapsed().as_millis(),
            "Transaction data saved"
        );
    }

    info!(
        elapsed_ms = started.elapsed().as_millis(),
        "Indexer finished"
    );

    Ok(())
}
