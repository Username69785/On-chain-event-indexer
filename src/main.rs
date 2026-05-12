use on_chain_event_indexer::{AppState, backoff, db, frontend, indexer, requests, telemetry};

use anyhow::Result;
use backoff::WorkerBackoff;
use frontend::create_server;
use indexer::process_claimed_job;
use requests::HeliusApi;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};
use tracing::warn;

#[tokio::main]
async fn main() -> Result<()> {
    telemetry::init()?;

    let api = dotenvy::var("API_KEY")?;
    let url = dotenvy::var("RPC_URL")?;

    let app_state = Arc::new(AppState {
        database: db::Database::new().await?,
        helius_api: HeliusApi::new(8, 2, format!("{url}{api}"))?,
    });

    app_state.database.migrate().await?;

    let server_handle = tokio::spawn(create_server(Arc::clone(&app_state)));

    let mut worker_handles: Vec<JoinHandle<Result<()>>> = Vec::new();
    for worker_id in 1..5 {
        let state = Arc::clone(&app_state);
        worker_handles.push(tokio::spawn(worker_loop(state, worker_id)));
        sleep(Duration::from_millis(700)).await;
    }

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

#[tracing::instrument(skip(app_state))]
async fn worker_loop(app_state: Arc<AppState>, worker_id: u32) -> Result<()> {
    let mut worker_backoff = WorkerBackoff::new(200.0, 2000.0, 2.0);
    loop {
        let claimed_job = loop {
            let claimed_job = app_state.database.claim_pending_job(worker_id).await;

            match claimed_job {
                Ok(Some(job)) => {
                    worker_backoff.reset();
                    break job;
                }
                Ok(None) => {
                    let delay = worker_backoff.step_and_get_sleep_duration();
                    sleep(delay).await;
                }
                Err(err) => {
                    tracing::warn!(%err, worker_id, "Failed to claim pending job");
                    let delay = worker_backoff.step_and_get_sleep_duration();
                    sleep(delay).await;
                }
            }
        };

        process_claimed_job(&app_state, worker_id, claimed_job).await;
    }
}
