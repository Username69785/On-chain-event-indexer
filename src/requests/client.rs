use std::time::Instant;

use anyhow::{Result, anyhow};
use futures::{self, StreamExt, stream};
use reqwest::{Client, Response};
use serde::Serialize;

use serde_json::json;
use tokio::time::{Duration, sleep};
use tracing::{debug, info, instrument, warn};

use crate::logging::mask_addr;

use super::types::*;

pub struct HeliusApi {
    api: String,
    url: String,
    client: Client,
}

impl HeliusApi {
    pub fn new() -> Self {
        let api = dotenvy::var("api").expect("api не найден в .env");
        let client = Client::new();
        let url = String::from("https://mainnet.helius-rpc.com/?api-key=");

        HeliusApi { api, url, client }
    }

    #[instrument(skip(self), fields(address = %mask_addr(adress), before = ?last_signature))]
    pub async fn get_signatures(
        &self,
        adress: &str,
        last_signature: Option<String>,
    ) -> Result<(RpcResponse, String)> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "getSignaturesForAddress",
            "params": [
                adress,
                {
                    "before": last_signature.as_deref(),
                    "max_supported_transaction_version": 0,
                }
            ]
        });

        let request_started = Instant::now();
        let response = self
            .client
            .post(format!("{}{}", self.url, self.api))
            .json(&body)
            .send()
            .await?;
        let status = response.status();

        let dsrlz_response: RpcResponse = response.json().await?;
        let response_len = dsrlz_response.result.len();
        debug!(
            status = ?status,
            response_len,
            elapsed_ms = request_started.elapsed().as_millis(),
            "Signatures response received"
        );

        let last_signatures = match dsrlz_response.result.last() {
            Some(last) => last.signature.clone(),
            None => {
                warn!("Empty signatures response");
                return Err(anyhow!("empty signatures response"));
            }
        };

        Ok((dsrlz_response, last_signatures))
    }

    #[instrument(skip(self, signatures), fields(total = signatures.len()))]
    pub async fn get_transaction(&self, signatures: Vec<String>) -> Result<Vec<TransactionResult>> {
        let mut responses_res: Vec<TransactionResult> = Vec::new();

        for (chunk_index, signatures) in signatures.chunks(10).enumerate() {
            let chunk_span =
                tracing::info_span!("tx_chunk", chunk_index, chunk_len = signatures.len());
            let _chunk_guard = chunk_span.enter();
            let chunk_started = Instant::now();
            debug!("Fetching transaction chunk");

            let response = stream::iter(signatures)
                .map(async |signature| {
                    let body = json!({
                        "jsonrpc": "2.0",
                        "id": "1",
                        "method": "getTransaction",
                        "params": [
                            signature,
                            {
                                "maxSupportedTransactionVersion": 0,
                                "encoding": "jsonParsed",
                            }
                        ]
                    });

                    let request_started = Instant::now();
                    let response: Response = self
                        .client
                        .post(format!("{}{}", self.url, self.api))
                        .json(&body)
                        .send()
                        .await?;
                    let status = response.status();

                    let transactions: TransactionResult = response.json().await?;
                    debug!(
                        status = ?status,
                        elapsed_ms = request_started.elapsed().as_millis(),
                        "Transaction response received"
                    );

                    Ok(transactions)
                })
                .buffered(10)
                .collect::<Vec<_>>()
                .await;
            let response_len = response.len();

            responses_res.append(
                &mut response
                    .into_iter()
                    .collect::<Result<Vec<TransactionResult>, anyhow::Error>>()?,
            );

            info!(
                chunk_len = response_len,
                total = responses_res.len(),
                elapsed_ms = chunk_started.elapsed().as_millis(),
                "Transactions chunk received"
            );

            sleep(Duration::from_millis(1150)).await;
        }

        let mut total_transfers = 0usize;
        let mut total_token_changes = 0usize;
        responses_res.iter_mut().for_each(|res| {
            res.calculate_transfers();
            res.calculate_token_transfer();
            total_transfers += res.vec_transfers.len();
            total_token_changes += res.token_transfer_changes.len();
        });
        debug!(
            total_transfers,
            total_token_changes, "Calculated balance changes"
        );

        Ok(responses_res)
    }
}
