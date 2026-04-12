use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use futures::{StreamExt, stream};
use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
};
use reqwest::{
    Client, StatusCode,
    header::{HeaderMap, RETRY_AFTER},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::time::sleep;
use tracing::{Instrument, debug, info, instrument, warn};

use crate::backoff::WorkerBackoff;
use crate::logging::mask_addr;

use super::types::{
    RpcEnvelope, RpcResponse, Signature, TransactionBatch, TransactionFetchError, TransactionInfo,
    TransactionResult,
};

const MAX_RATE_LIMIT_RETRIES: usize = 4;

struct RpcHttpResponse {
    status: StatusCode,
    retry_after: Option<Duration>,
    body_text: String,
}

enum TransactionFetchOutcome {
    Success {
        signature: String,
        transaction: Box<TransactionResult>,
    },
    Failed(TransactionFetchError),
}

enum FetchAttempt {
    Success(TransactionInfo, StatusCode),
    RateLimited(TransactionFetchError, Option<Duration>),
    Fatal(TransactionFetchError),
}

type GlobalRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

pub struct SignaturesPage {
    pub response: RpcResponse,
    pub last_signature: Option<String>,
    pub raw_count: usize,
    pub reached_cutoff: bool,
}

struct RecentSignaturesResult {
    signatures: Vec<Signature>,
    reached_cutoff: bool,
    skipped_null_block_time: usize,
}

pub struct HeliusApi {
    api: String,
    url: String,
    client: Client,
    rate_limiter: Arc<GlobalRateLimiter>,
    semaphore: Arc<Semaphore>,
    rate_limit_backoff: Arc<Mutex<WorkerBackoff>>,
    rate_limit_until: Arc<Mutex<Option<Instant>>>,
    last_rate_limit_at: Arc<Mutex<Option<Instant>>>,
}

impl HeliusApi {
    pub fn new(rps: u32, max_concurrent: usize) -> Result<Self> {
        let quota = Quota::per_second(
            std::num::NonZeroU32::new(rps)
                .ok_or_else(|| anyhow!("RPS не может быть равен нулю"))?,
        )
        .allow_burst(std::num::NonZeroU32::MIN);
        let rate_limiter = Arc::new(RateLimiter::direct(quota));
        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        let rate_limit_backoff = Arc::new(Mutex::new(WorkerBackoff::new(500.0, 10_000.0, 2.0)));
        let rate_limit_until = Arc::new(Mutex::new(None));
        let last_rate_limit_at = Arc::new(Mutex::new(None));

        let api = dotenvy::var("api")?;
        let client = Client::new();
        let url = String::from("https://mainnet.helius-rpc.com/?api-key=");

        Ok(Self {
            api,
            url,
            client,
            rate_limiter,
            semaphore,
            rate_limit_backoff,
            rate_limit_until,
            last_rate_limit_at,
        })
    }

    #[instrument(target = "client", skip(self), fields(address = %mask_addr(address), before = ?last_signature))]
    pub async fn get_signatures(
        &self,
        address: &str,
        last_signature: Option<String>,
        requested_hours: i16,
    ) -> Result<SignaturesPage> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "getSignaturesForAddress",
            "params": [
                address,
                {
                    "before": last_signature.as_deref(),
                    "max_supported_transaction_version": 0,
                }
            ]
        });

        for attempt in 1..=MAX_RATE_LIMIT_RETRIES + 1 {
            let request_started = Instant::now();
            let response = self.send_rpc_request(&body).await?;
            let status = response.status;

            let rpc_response: RpcEnvelope<Vec<Signature>> = match serde_json::from_str(
                &response.body_text,
            ) {
                Ok(rpc_response) => rpc_response,
                Err(error) => {
                    if status == StatusCode::TOO_MANY_REQUESTS && attempt <= MAX_RATE_LIMIT_RETRIES
                    {
                        let delay = self.register_rate_limit(response.retry_after).await;
                        warn!(
                            target: "client",
                            address = %mask_addr(address),
                            status = ?status,
                            attempt,
                            max_attempts = MAX_RATE_LIMIT_RETRIES + 1,
                            sleep_ms = delay.as_millis(),
                            "Rate limit detected on getSignaturesForAddress, retrying"
                        );
                        continue;
                    }

                    return Err(anyhow!(
                        "failed to decode getSignaturesForAddress response: status={status}, error={error}"
                    ));
                }
            };

            if let Some(rpc_error) = rpc_response.error {
                if rpc_error.is_rate_limited() && attempt <= MAX_RATE_LIMIT_RETRIES {
                    let delay = self.register_rate_limit(response.retry_after).await;
                    warn!(
                        target: "client",
                        address = %mask_addr(address),
                        status = ?status,
                        rpc_code = rpc_error.code,
                        attempt,
                        max_attempts = MAX_RATE_LIMIT_RETRIES + 1,
                        sleep_ms = delay.as_millis(),
                        "Rate limit detected on getSignaturesForAddress, retrying"
                    );
                    continue;
                }

                return Err(anyhow!(
                    "rpc error on getSignaturesForAddress: status={}, code={}, message={}, rate_limited={}",
                    status,
                    rpc_error.code,
                    rpc_error.message,
                    rpc_error.is_rate_limited()
                ));
            }

            let result = rpc_response.result.ok_or_else(|| {
                anyhow!("missing result field in getSignaturesForAddress response")
            })?;
            let raw_count = result.len();
            let last_signature = result.last().map(|last| last.signature.clone());
            let recent_signatures = take_recent_signatures(&result, requested_hours);
            let filtered_count = recent_signatures.signatures.len();

            debug!(
                target: "client",
                status = ?status,
                raw_count,
                filtered_count,
                reached_cutoff = recent_signatures.reached_cutoff,
                skipped_null_block_time = recent_signatures.skipped_null_block_time,
                elapsed_ms = request_started.elapsed().as_millis(),
                "Signatures response received"
            );

            self.reset_rate_limit_backoff_if_idle().await;

            if last_signature.is_none() {
                warn!(target: "client", "Empty signatures response");
            }

            return Ok(SignaturesPage {
                response: RpcResponse {
                    result: recent_signatures.signatures,
                },
                last_signature,
                raw_count,
                reached_cutoff: recent_signatures.reached_cutoff,
            });
        }

        Err(anyhow!(
            "getSignaturesForAddress exhausted retry budget after {} attempts",
            MAX_RATE_LIMIT_RETRIES + 1
        ))
    }

    pub async fn fetch_transaction_chunk(&self, signatures: &[String]) -> Result<TransactionBatch> {
        let mut responses_res: Vec<TransactionResult> = Vec::new();
        let mut processed_signatures: Vec<String> = Vec::new();
        let mut failed_signatures: Vec<String> = Vec::new();
        let mut errors: Vec<TransactionFetchError> = Vec::new();

        let chunk_started = Instant::now();
        debug!(target: "client", "Fetching transaction chunk");

        let chunk_responses = stream::iter(signatures.iter().cloned())
            .map(|signature| async move { self.fetch_transaction_by_signature(signature).await })
            .buffered(5)
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
                    responses_res.push(*transaction);
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
            target: "client",
            chunk_success,
            chunk_failed,
            total_success = responses_res.len(),
            total_failed = failed_signatures.len(),
            elapsed_ms = chunk_started.elapsed().as_millis(),
            "Transactions chunk received"
        );

        if let Some(sample_error) = first_chunk_error {
            warn!(
                target: "client",
                signature = %mask_addr(&sample_error.signature),
                status_code = ?sample_error.status_code,
                rpc_code = ?sample_error.rpc_code,
                message = %sample_error.message,
                rate_limited = sample_error.is_rate_limited(),
                "Transaction chunk completed with failures"
            );
        }

        Ok(TransactionBatch {
            transactions: responses_res,
            processed_signatures,
            failed_signatures,
            errors,
        })
    }

    #[instrument(target = "client", skip(self, signatures), fields(total = signatures.len()))]
    pub async fn get_transaction(&self, signatures: &[String]) -> Result<TransactionBatch> {
        let mut total_batch = TransactionBatch {
            transactions: Vec::new(),
            processed_signatures: Vec::new(),
            failed_signatures: Vec::new(),
            errors: Vec::new(),
        };

        for (chunk_index, signatures) in signatures.chunks(10).enumerate() {
            let chunk_span = tracing::info_span!(
                target: "client",
                "tx_chunk",
                chunk_index,
                chunk_len = signatures.len()
            );

            let mut chunk = self
                .fetch_transaction_chunk(signatures)
                .instrument(chunk_span)
                .await?;

            for signature in &mut chunk.transactions {
                signature.calculate_token_transfer();
            }

            total_batch.transactions.append(&mut chunk.transactions);
            total_batch
                .processed_signatures
                .append(&mut chunk.processed_signatures);
            total_batch
                .failed_signatures
                .append(&mut chunk.failed_signatures);
            total_batch.errors.append(&mut chunk.errors);

            sleep(Duration::from_millis(100)).await;
        }

        let mut total_token_changes = 0usize;
        for res in &mut total_batch.transactions {
            total_token_changes += res.token_transfer_changes.len();
        }
        debug!(
            target: "client",
            total_token_changes,
            "Calculated token balance changes"
        );

        if let Some(first_error) = total_batch.errors.first() {
            warn!(
                target: "client",
                failed_total = total_batch.errors.len(),
                success_total = total_batch.transactions.len(),
                signature = %mask_addr(&first_error.signature),
                status_code = ?first_error.status_code,
                rpc_code = ?first_error.rpc_code,
                message = %first_error.message,
                rate_limited = first_error.is_rate_limited(),
                "Some transaction requests failed; signatures left unprocessed for retry"
            );
        }

        Ok(total_batch)
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

        let mut last_rate_limit_error: Option<TransactionFetchError> = None;

        for attempt in 1..=MAX_RATE_LIMIT_RETRIES + 1 {
            let request_started = Instant::now();

            match self.try_fetch_transaction_once(&signature, &body).await {
                FetchAttempt::Success(tx_info, status) => {
                    debug!(
                        target: "client",
                        status = ?status,
                        elapsed_ms = request_started.elapsed().as_millis(),
                        "Transaction response received"
                    );
                    self.reset_rate_limit_backoff_if_idle().await;

                    return TransactionFetchOutcome::Success {
                        signature,
                        transaction: Box::new(TransactionResult {
                            result: tx_info,
                            token_transfer_changes: Vec::new(),
                        }),
                    };
                }
                FetchAttempt::RateLimited(fetch_error, retry_after) => {
                    if attempt <= MAX_RATE_LIMIT_RETRIES {
                        last_rate_limit_error = Some(fetch_error.clone());
                        let delay = self.register_rate_limit(retry_after).await;
                        warn!(
                            target: "client",
                            signature = %mask_addr(&signature),
                            status_code = ?fetch_error.status_code,
                            rpc_code = ?fetch_error.rpc_code,
                            attempt,
                            max_attempts = MAX_RATE_LIMIT_RETRIES + 1,
                            sleep_ms = delay.as_millis(),
                            "Rate limit detected on getTransaction, retrying"
                        );
                        continue;
                    }
                    return TransactionFetchOutcome::Failed(fetch_error);
                }
                FetchAttempt::Fatal(fetch_error) => {
                    return TransactionFetchOutcome::Failed(fetch_error);
                }
            }
        }

        TransactionFetchOutcome::Failed(last_rate_limit_error.unwrap_or_else(|| {
            TransactionFetchError {
                signature,
                status_code: None,
                rpc_code: None,
                message: format!(
                    "getTransaction exhausted retry budget after {} attempts",
                    MAX_RATE_LIMIT_RETRIES + 1
                ),
            }
        }))
    }

    async fn try_fetch_transaction_once(&self, signature: &str, body: &Value) -> FetchAttempt {
        let response = match self.send_rpc_request(body).await {
            Ok(response) => response,
            Err(error) => {
                return FetchAttempt::Fatal(TransactionFetchError {
                    signature: signature.to_string(),
                    status_code: None,
                    rpc_code: None,
                    message: format!("request failed: {error}"),
                });
            }
        };

        let status = response.status;
        let rpc_response: RpcEnvelope<Value> = match serde_json::from_str(&response.body_text) {
            Ok(rpc_response) => rpc_response,
            Err(error) => {
                let fetch_error = TransactionFetchError {
                    signature: signature.to_string(),
                    status_code: Some(status.as_u16()),
                    rpc_code: None,
                    message: format!(
                        "failed to decode rpc envelope: {}; body={}",
                        error,
                        Self::body_snippet(&response.body_text)
                    ),
                };

                if fetch_error.is_rate_limited() {
                    return FetchAttempt::RateLimited(fetch_error, response.retry_after);
                }

                return FetchAttempt::Fatal(fetch_error);
            }
        };

        if let Some(rpc_error) = rpc_response.error {
            let fetch_error = TransactionFetchError {
                signature: signature.to_string(),
                status_code: Some(status.as_u16()),
                rpc_code: Some(rpc_error.code),
                message: rpc_error.message,
            };

            if fetch_error.is_rate_limited() {
                return FetchAttempt::RateLimited(fetch_error, response.retry_after);
            }

            return FetchAttempt::Fatal(fetch_error);
        }

        let Some(result_value) = rpc_response.result else {
            return FetchAttempt::Fatal(TransactionFetchError {
                signature: signature.to_string(),
                status_code: Some(status.as_u16()),
                rpc_code: None,
                message: String::from("missing result field in rpc response"),
            });
        };

        if result_value.is_null() {
            return FetchAttempt::Fatal(TransactionFetchError {
                signature: signature.to_string(),
                status_code: Some(status.as_u16()),
                rpc_code: None,
                message: String::from("rpc result is null"),
            });
        }

        let tx_info: TransactionInfo = match serde_json::from_value(result_value) {
            Ok(tx_info) => tx_info,
            Err(error) => {
                return FetchAttempt::Fatal(TransactionFetchError {
                    signature: signature.to_string(),
                    status_code: Some(status.as_u16()),
                    rpc_code: None,
                    message: format!("failed to decode transaction result: {error}"),
                });
            }
        };

        FetchAttempt::Success(tx_info, status)
    }

    async fn send_rpc_request(&self, body: &Value) -> Result<RpcHttpResponse> {
        let _permit = self.acquire_request_slot().await?;
        let response = self
            .client
            .post(format!("{}{}", self.url, self.api))
            .json(body)
            .send()
            .await?;
        let status = response.status();
        let retry_after = Self::parse_retry_after(response.headers());
        let body_text = response.text().await?;

        Ok(RpcHttpResponse {
            status,
            retry_after,
            body_text,
        })
    }

    async fn acquire_request_slot(&self) -> Result<OwnedSemaphorePermit> {
        self.wait_for_rate_limit_cooldown().await;
        let permit = self.semaphore.clone().acquire_owned().await?;
        self.wait_for_rate_limit_cooldown().await;
        self.rate_limiter.until_ready().await;
        Ok(permit)
    }

    async fn wait_for_rate_limit_cooldown(&self) {
        loop {
            let delay = {
                let mut rate_limit_until = self.rate_limit_until.lock().await;
                match *rate_limit_until {
                    Some(until) if until > Instant::now() => {
                        Some(until.duration_since(Instant::now()))
                    }
                    Some(_) => {
                        *rate_limit_until = None;
                        None
                    }
                    None => None,
                }
            };

            match delay {
                Some(delay) => sleep(delay).await,
                None => break,
            }
        }
    }

    async fn register_rate_limit(&self, retry_after: Option<Duration>) -> Duration {
        let delay = if let Some(delay) = retry_after.filter(|delay| !delay.is_zero()) {
            delay
        } else {
            let mut backoff = self.rate_limit_backoff.lock().await;
            backoff.step_and_get_sleep_duration()
        };

        let until = Instant::now()
            .checked_add(delay)
            .unwrap_or_else(Instant::now);

        let mut rate_limit_until = self.rate_limit_until.lock().await;
        match *rate_limit_until {
            Some(current_until) if current_until >= until => {}
            _ => *rate_limit_until = Some(until),
        }

        drop(rate_limit_until);

        let mut last_rl = self.last_rate_limit_at.lock().await;
        *last_rl = Some(Instant::now());

        delay
    }

    async fn reset_rate_limit_backoff_if_idle(&self) {
        let cooldown_active = {
            let mut rate_limit_until = self.rate_limit_until.lock().await;
            match *rate_limit_until {
                Some(until) if until > Instant::now() => true,
                Some(_) => {
                    *rate_limit_until = None;
                    false
                }
                None => false,
            }
        };

        if !cooldown_active {
            let should_reset = {
                let last_rl = self.last_rate_limit_at.lock().await;
                last_rl.is_none_or(|at| at.elapsed() > Duration::from_secs(5))
            };

            if should_reset {
                let mut backoff = self.rate_limit_backoff.lock().await;
                backoff.reset();
            }
        }
    }

    fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
        Self::parse_retry_after_at(headers, Utc::now().timestamp())
    }

    fn parse_retry_after_at(headers: &HeaderMap, now_ts: i64) -> Option<Duration> {
        let value = headers.get(RETRY_AFTER)?;
        let value = value.to_str().ok()?.trim();

        if let Ok(date) = DateTime::parse_from_rfc2822(value) {
            let secs = u64::try_from(date.timestamp() - now_ts).unwrap_or(0u64);
            return Some(Duration::from_secs(secs));
        }

        let seconds = value.parse::<u64>().ok()?;
        Some(Duration::from_secs(seconds))
    }

    fn body_snippet(body: &str) -> String {
        const MAX_CHARS: usize = 200;

        let mut chars = body.chars();
        let mut snippet: String = chars
            .by_ref()
            .take(MAX_CHARS)
            .map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch })
            .collect();

        if chars.next().is_some() {
            snippet.push_str("...");
        }

        snippet
    }
}

fn take_recent_signatures(
    signatures: &[Signature],
    requested_hours: i16,
) -> RecentSignaturesResult {
    let cutoff_ts = Utc::now().timestamp() - i64::from(requested_hours.max(0)) * 3600;
    let input_count = signatures.len();
    let mut filtered = Vec::with_capacity(signatures.len());
    let mut skipped_null_block_time = 0usize;
    let mut reached_cutoff = false;

    for sig in signatures {
        match sig.block_time {
            Some(ts) if ts >= cutoff_ts => filtered.push(Signature {
                block_time: sig.block_time,
                signature: sig.signature.clone(),
            }),
            Some(_) => {
                reached_cutoff = true;
                break;
            }
            None => {
                skipped_null_block_time += 1;
            }
        }
    }

    debug!(
        target: "client",
        requested_hours,
        cutoff_ts,
        input_count,
        filtered_count = filtered.len(),
        skipped_null_block_time,
        reached_cutoff,
        "Filtered signatures by requested time window"
    );

    RecentSignaturesResult {
        signatures: filtered,
        reached_cutoff,
        skipped_null_block_time,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn create_signature(signature: &str, ts: Option<i64>) -> Signature {
        Signature {
            block_time: ts,
            signature: signature.to_string(),
        }
    }

    #[test]
    fn should_return_all_signatures_when_all_are_fresh() {
        let now = Utc::now().timestamp();
        let signatures = [
            create_signature("sig-1", Some(now)),
            create_signature("sig-2", Some(now - 60)),
        ];

        let result = take_recent_signatures(&signatures, 1);

        assert_eq!(result.signatures.len(), 2);
        assert_eq!(result.signatures[0].signature, "sig-1");
        assert_eq!(result.signatures[1].signature, "sig-2");
        assert!(!result.reached_cutoff);
        assert_eq!(result.skipped_null_block_time, 0);
    }

    #[test]
    fn should_break_when_old_signature_reaches_cutoff() {
        let now = Utc::now().timestamp();
        let signatures = [
            create_signature("sig-1", Some(now)),
            create_signature("sig-2", Some(now - 30)),
            create_signature("sig-old", Some(now - 7200)),
            create_signature("sig-after-old", Some(now)),
        ];

        let result = take_recent_signatures(&signatures, 1);

        assert_eq!(result.signatures.len(), 2);
        assert_eq!(result.signatures[0].signature, "sig-1");
        assert_eq!(result.signatures[1].signature, "sig-2");
        assert!(result.reached_cutoff);
        assert_eq!(result.skipped_null_block_time, 0);
    }

    #[test]
    fn should_skip_none_block_time_without_breaking_iteration() {
        let now = Utc::now().timestamp();
        let signatures = [
            create_signature("sig-none-1", None),
            create_signature("sig-fresh", Some(now - 60)),
            create_signature("sig-none-2", None),
            create_signature("sig-fresh-2", Some(now - 120)),
        ];

        let result = take_recent_signatures(&signatures, 1);

        assert_eq!(result.signatures.len(), 2);
        assert_eq!(result.signatures[0].signature, "sig-fresh");
        assert_eq!(result.signatures[1].signature, "sig-fresh-2");
        assert!(!result.reached_cutoff);
        assert_eq!(result.skipped_null_block_time, 2);
    }

    #[test]
    fn should_return_empty_result_for_empty_input() {
        let signatures = [];

        let result = take_recent_signatures(&signatures, 1);

        assert!(result.signatures.is_empty());
        assert!(!result.reached_cutoff);
        assert_eq!(result.skipped_null_block_time, 0);
    }

    #[test]
    fn should_treat_negative_hours_the_same_as_zero_hours() {
        let now = Utc::now().timestamp();
        let current_second = create_signature("sig-now", Some(now));
        let previous_second = create_signature("sig-prev", Some(now - 1));
        let signatures = [current_second, previous_second];

        let zero_hours = take_recent_signatures(&signatures, 0);
        let negative_hours = take_recent_signatures(&signatures, -1);

        assert_eq!(zero_hours.signatures.len(), 1);
        assert_eq!(zero_hours.signatures[0].signature, "sig-now");
        assert!(zero_hours.reached_cutoff);

        assert_eq!(negative_hours.signatures.len(), 1);
        assert_eq!(negative_hours.signatures[0].signature, "sig-now");
        assert!(negative_hours.reached_cutoff);
    }

    #[test]
    fn should_truncate_text_to_max_length_and_add_ellipsis() {
        let text = "X".repeat(250);
        let result = HeliusApi::body_snippet(&text);

        assert_eq!(result.len(), 203, "Incorrect length");
        assert!(result.ends_with("..."), "No ellipsis ");
    }

    #[test]
    fn should_remove_special_ch() {
        let text = "text\n\ntext\r\rtext\n123\n\r";
        let result = HeliusApi::body_snippet(text);

        assert_eq!(
            result, "text  text  text 123  ",
            "Should replace newlines with spaces"
        );
        assert!(!result.contains('\r'));
        assert!(!result.contains('\n'));
    }

    #[test]
    fn should_not_change_the_short_text() {
        let text = "Short text";
        let result = HeliusApi::body_snippet(text);

        assert_eq!(result, "Short text", "The text has been cut off too short");
    }

    #[test]
    fn should_handle_exactly_limit_length_without_ellipsis() {
        let text = "X".repeat(200);
        let result = HeliusApi::body_snippet(&text);

        assert!(!result.ends_with("..."));
    }

    #[test]
    fn should_return_none_if_there_no_title() {
        let map = HeaderMap::new();
        let result = HeliusApi::parse_retry_after(&map);

        assert!(result.is_none());
    }

    #[test]
    fn should_return_some_duration_when_valid_integer_seconds() -> Result<()> {
        let mut map = HeaderMap::new();
        map.insert(RETRY_AFTER, "20".parse()?);

        let result = HeliusApi::parse_retry_after(&map);
        let correct_result = Duration::new(20, 0);

        assert!(result.is_some());
        assert_eq!(result, Some(correct_result));

        Ok(())
    }

    #[test]
    fn should_return_duration_when_header_contains_http_date_format() -> Result<()> {
        let mut map = HeaderMap::new();
        let retry_after = "Sat, 10 Apr 2027 09:20:20 GMT";
        map.insert(RETRY_AFTER, retry_after.parse()?);

        let now_ts = DateTime::parse_from_rfc2822("Sat, 10 Apr 2027 09:20:10 GMT")?.timestamp();
        let result = HeliusApi::parse_retry_after_at(&map, now_ts);

        assert_eq!(result, Some(Duration::from_secs(10)));

        Ok(())
    }

    #[test]
    fn should_return_none_when_header_contains_garbage_text() -> Result<()> {
        let mut map = HeaderMap::new();
        map.insert(RETRY_AFTER, "Aorstwymt 66654".parse()?);

        let result = HeliusApi::parse_retry_after(&map);

        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn should_return_none_when_header_is_empty_string() -> Result<()> {
        let mut map = HeaderMap::new();
        map.insert(RETRY_AFTER, "".parse()?);

        let result = HeliusApi::parse_retry_after(&map);

        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn should_return_none_when_header_contains_negative_number() -> Result<()> {
        let mut map = HeaderMap::new();
        map.insert(RETRY_AFTER, "-55".parse()?);

        let result = HeliusApi::parse_retry_after(&map);

        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn should_return_none_when_seconds_exceed_u64_max() -> Result<()> {
        let mut map = HeaderMap::new();
        map.insert(RETRY_AFTER, "9999999999999999999999999999".parse()?);

        let result = HeliusApi::parse_retry_after(&map);

        assert!(result.is_none());

        Ok(())
    }

    #[test]
    fn should_return_none_when_header_contains_float_number() -> Result<()> {
        let mut map = HeaderMap::new();
        map.insert(RETRY_AFTER, "24.46".parse()?);

        let result = HeliusApi::parse_retry_after(&map);

        assert!(result.is_none());

        Ok(())
    }
}
