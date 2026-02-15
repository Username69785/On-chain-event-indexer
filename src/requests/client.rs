use std::time::Instant;

use anyhow::{Result, anyhow};
use futures::{StreamExt, stream};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::time::{Duration, sleep};
use tracing::{debug, info, instrument, warn};

use crate::logging::mask_addr;

use super::types::*;

enum TransactionFetchOutcome {
    Success {
        signature: String,
        transaction: TransactionResult,
    },
    Failed(TransactionFetchError),
}

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
        let body_text = response.text().await?;

        let rpc_response: RpcEnvelope<Vec<Signature>> =
            serde_json::from_str(&body_text).map_err(|error| {
                anyhow!(
                    "failed to decode getSignaturesForAddress response: status={}, error={}",
                    status,
                    error
                )
            })?;

        if let Some(rpc_error) = rpc_response.error {
            return Err(anyhow!(
                "rpc error on getSignaturesForAddress: status={}, code={}, message={}",
                status,
                rpc_error.code,
                rpc_error.message
            ));
        }

        let result = rpc_response
            .result
            .ok_or_else(|| anyhow!("missing result field in getSignaturesForAddress response"))?;

        let dsrlz_response = RpcResponse { result };
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
    pub async fn get_transaction(&self, signatures: Vec<String>) -> Result<TransactionBatch> {
        let mut responses_res: Vec<TransactionResult> = Vec::new();
        let mut processed_signatures: Vec<String> = Vec::new();
        let mut failed_signatures: Vec<String> = Vec::new();
        let mut errors: Vec<TransactionFetchError> = Vec::new();

        for (chunk_index, signatures) in signatures.chunks(10).enumerate() {
            let chunk_span =
                tracing::info_span!("tx_chunk", chunk_index, chunk_len = signatures.len());
            let _chunk_guard = chunk_span.enter();
            let chunk_started = Instant::now();
            debug!("Fetching transaction chunk");

            let chunk_responses = stream::iter(signatures.iter().cloned())
                .map(|signature| async move { self.fetch_transaction_by_signature(signature).await })
                .buffered(10)
                .collect::<Vec<_>>()
                .await;

            let mut chunk_success = 0usize;
            let mut chunk_failed = 0usize;
            let mut first_chunk_error: Option<TransactionFetchError> = None;

            for response in chunk_responses {
                match response {
                    TransactionFetchOutcome::Success {
                        signature,
                        transaction,
                    } => {
                        chunk_success += 1;
                        processed_signatures.push(signature);
                        responses_res.push(transaction);
                    }
                    TransactionFetchOutcome::Failed(error) => {
                        chunk_failed += 1;
                        if first_chunk_error.is_none() {
                            first_chunk_error = Some(error.clone());
                        }
                        failed_signatures.push(error.signature.clone());
                        errors.push(error);
                    }
                }
            }

            info!(
                chunk_success,
                chunk_failed,
                total_success = responses_res.len(),
                total_failed = failed_signatures.len(),
                elapsed_ms = chunk_started.elapsed().as_millis(),
                "Transactions chunk received"
            );

            if let Some(sample_error) = first_chunk_error {
                warn!(
                    signature = %mask_addr(&sample_error.signature),
                    status_code = ?sample_error.status_code,
                    rpc_code = ?sample_error.rpc_code,
                    message = %sample_error.message,
                    "Transaction chunk completed with failures"
                );
            }

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

        if let Some(first_error) = errors.first() {
            warn!(
                failed_total = errors.len(),
                success_total = responses_res.len(),
                signature = %mask_addr(&first_error.signature),
                status_code = ?first_error.status_code,
                rpc_code = ?first_error.rpc_code,
                message = %first_error.message,
                "Some transaction requests failed; signatures left unprocessed for retry"
            );
        }

        Ok(TransactionBatch {
            transactions: responses_res,
            processed_signatures,
            failed_signatures,
            errors,
        })
    }

    async fn fetch_transaction_by_signature(&self, signature: String) -> TransactionFetchOutcome {
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
        let response = match self
            .client
            .post(format!("{}{}", self.url, self.api))
            .json(&body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return TransactionFetchOutcome::Failed(TransactionFetchError {
                    signature,
                    status_code: None,
                    rpc_code: None,
                    message: format!("request failed: {}", error),
                })
            }
        };

        let status = response.status();
        let body_text = match response.text().await {
            Ok(body_text) => body_text,
            Err(error) => {
                return TransactionFetchOutcome::Failed(TransactionFetchError {
                    signature,
                    status_code: Some(status.as_u16()),
                    rpc_code: None,
                    message: format!("failed to read response body: {}", error),
                });
            }
        };

        let rpc_response: RpcEnvelope<Value> = match serde_json::from_str(&body_text) {
            Ok(rpc_response) => rpc_response,
            Err(error) => {
                return TransactionFetchOutcome::Failed(TransactionFetchError {
                    signature,
                    status_code: Some(status.as_u16()),
                    rpc_code: None,
                    message: format!(
                        "failed to decode rpc envelope: {}; body={}",
                        error,
                        Self::body_snippet(&body_text)
                    ),
                });
            }
        };

        if let Some(rpc_error) = rpc_response.error {
            return TransactionFetchOutcome::Failed(TransactionFetchError {
                signature,
                status_code: Some(status.as_u16()),
                rpc_code: Some(rpc_error.code),
                message: rpc_error.message,
            });
        }

        let result_value = match rpc_response.result {
            Some(value) => value,
            None => {
                return TransactionFetchOutcome::Failed(TransactionFetchError {
                    signature,
                    status_code: Some(status.as_u16()),
                    rpc_code: None,
                    message: String::from("missing result field in rpc response"),
                });
            }
        };

        if result_value.is_null() {
            return TransactionFetchOutcome::Failed(TransactionFetchError {
                signature,
                status_code: Some(status.as_u16()),
                rpc_code: None,
                message: String::from("rpc result is null"),
            });
        }

        let tx_info: TransactionInfo = match serde_json::from_value(result_value) {
            Ok(tx_info) => tx_info,
            Err(error) => {
                return TransactionFetchOutcome::Failed(TransactionFetchError {
                    signature,
                    status_code: Some(status.as_u16()),
                    rpc_code: None,
                    message: format!("failed to decode transaction result: {}", error),
                });
            }
        };

        debug!(
            status = ?status,
            elapsed_ms = request_started.elapsed().as_millis(),
            "Transaction response received"
        );

        TransactionFetchOutcome::Success {
            signature,
            transaction: TransactionResult {
                result: tx_info,
                vec_transfers: Vec::new(),
                token_transfer_changes: Vec::new(),
            },
        }
    }

    fn body_snippet(body: &str) -> String {
        const MAX_CHARS: usize = 200;
        let mut snippet = String::new();

        for ch in body.chars().take(MAX_CHARS) {
            if ch == '\n' || ch == '\r' {
                snippet.push(' ');
            } else {
                snippet.push(ch);
            }
        }

        if body.chars().nth(MAX_CHARS).is_some() {
            snippet.push_str("...");
        }

        snippet
    }
}
