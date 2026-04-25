#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use anyhow::{Ok, Result};
use on_chain_event_indexer::requests::HeliusApi;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MAX_RATE_LIMIT_ATTEMPTS: usize = 5;

fn fetch_additional_json() -> Result<[Value; 5]> {
    let json1: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/success_additional_1.json"
    ))?;
    let json2: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/success_additional_2.json"
    ))?;
    let json3: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/success_additional_3.json"
    ))?;
    let json4: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/success_additional_4.json"
    ))?;
    let json5: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/success_additional_5.json"
    ))?;

    Ok([json1, json2, json3, json4, json5])
}

fn transaction_request_match(signature: &str) -> Value {
    json!({
        "method": "getTransaction",
        "params": [
            signature,
            {
                "encoding": "jsonParsed",
                "maxSupportedTransactionVersion": 0
            }
        ]
    })
}

async fn mount_transaction_json_response(mock_server: &MockServer, signature: &str, body: Value) {
    let response = ResponseTemplate::new(200).set_body_json(body);

    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(transaction_request_match(signature)))
        .respond_with(response)
        .expect(1)
        .mount(mock_server)
        .await;
}

async fn mount_transaction_json_response_n_times(
    mock_server: &MockServer,
    signature: &str,
    body: Value,
    count: u64,
) {
    let response = ResponseTemplate::new(200).set_body_json(body);

    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(transaction_request_match(signature)))
        .respond_with(response)
        .up_to_n_times(count)
        .expect(count)
        .mount(mock_server)
        .await;
}

async fn mount_transaction_http_429_response_n_times(
    mock_server: &MockServer,
    signature: &str,
    body: Value,
    count: u64,
) {
    let response = ResponseTemplate::new(429).set_body_json(body);

    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(transaction_request_match(signature)))
        .respond_with(response)
        .up_to_n_times(count)
        .expect(count)
        .mount(mock_server)
        .await;
}

async fn mount_transaction_raw_response(
    mock_server: &MockServer,
    signature: &str,
    status: u16,
    body: &str,
    content_type: &str,
) {
    let response = ResponseTemplate::new(status).set_body_raw(body.to_owned(), content_type);

    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(transaction_request_match(signature)))
        .respond_with(response)
        .expect(1)
        .mount(mock_server)
        .await;
}

async fn assert_transaction_rpc_request(
    mock_server: &MockServer,
    expected_request_count: usize,
    signature: &str,
) -> Result<()> {
    let received_requests = mock_server.received_requests().await.unwrap();

    assert_eq!(received_requests.len(), expected_request_count);

    let request_json: Value = received_requests[0].body_json()?;

    assert_eq!(received_requests[0].method, "POST");
    assert_eq!(request_json["method"], "getTransaction");
    assert_eq!(request_json["params"][0], signature);
    assert_eq!(request_json["params"][1]["encoding"], "jsonParsed");
    assert_eq!(
        request_json["params"][1]["maxSupportedTransactionVersion"],
        0
    );

    Ok(())
}

async fn assert_transaction_rpc_request_counts(
    mock_server: &MockServer,
    expected_counts: &[(&str, usize)],
) -> Result<()> {
    let received_requests = mock_server.received_requests().await.unwrap();
    let mut actual_counts: BTreeMap<String, usize> = BTreeMap::new();

    for request in &received_requests {
        let request_json: Value = request.body_json()?;
        assert_eq!(request.method, "POST");
        assert_eq!(request_json["method"], "getTransaction");
        assert_eq!(request_json["params"][1]["encoding"], "jsonParsed");
        assert_eq!(
            request_json["params"][1]["maxSupportedTransactionVersion"],
            0
        );

        let signature = request_json["params"][0].as_str().unwrap().to_string();
        *actual_counts.entry(signature).or_insert(0) += 1;
    }

    let expected = expected_counts
        .iter()
        .map(|(signature, count)| ((*signature).to_string(), *count))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(actual_counts, expected);

    Ok(())
}

async fn assert_transaction_rpc_requests(
    mock_server: &MockServer,
    expected_signatures: &[&str],
) -> Result<()> {
    let received_requests = mock_server.received_requests().await.unwrap();
    assert_eq!(received_requests.len(), expected_signatures.len());

    let mut actual_signatures = received_requests
        .iter()
        .map(|request| -> Result<String> {
            let request_json: Value = request.body_json()?;
            assert_eq!(request.method, "POST");
            assert_eq!(request_json["method"], "getTransaction");
            assert_eq!(request_json["params"][1]["encoding"], "jsonParsed");
            assert_eq!(
                request_json["params"][1]["maxSupportedTransactionVersion"],
                0
            );

            Ok(request_json["params"][0].as_str().unwrap().to_string())
        })
        .collect::<Result<Vec<_>>>()?;

    let mut expected = expected_signatures
        .iter()
        .map(|signature| (*signature).to_string())
        .collect::<Vec<_>>();

    actual_signatures.sort();
    expected.sort();

    assert_eq!(actual_signatures, expected);

    Ok(())
}

#[tokio::test]
async fn should_return_single_transaction_when_valid_get_transaction_response() -> Result<()> {
    let mock_server = MockServer::start().await;
    let data = include_str!("../tests/fixtures/helius/transactions/success.json");
    let json: Value = serde_json::from_str(data)?;

    let response = ResponseTemplate::new(200).set_body_json(json);

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(response)
        .mount(&mock_server)
        .await;

    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api.get_transaction(&[String::from("sig-1")]).await?;

    assert!(result.errors.is_empty());
    assert!(result.failed_signatures.is_empty());
    assert_eq!(result.transactions.len(), 1);
    assert_eq!(result.processed_signatures.len(), 1);
    assert_eq!(result.processed_signatures[0], "sig-1");

    let transaction = &result.transactions[0].result;
    assert_eq!(transaction.slot, 412_675_806);
    assert_eq!(transaction.block_time, 1_775_977_452);
    assert_eq!(transaction.meta.compute_units_consumed, 161_456);
    assert_eq!(transaction.meta.fee, 124_000);
    assert_eq!(transaction.transaction.signatures.len(), 3);
    assert!(!result.transactions[0].token_transfer_changes.is_empty());

    assert_transaction_rpc_request(&mock_server, 1, "sig-1").await
}

#[tokio::test]
async fn should_calculate_token_transfer_changes_after_successful_fetch() -> Result<()> {
    let mock_server = MockServer::start().await;
    let data = include_str!("../tests/fixtures/helius/transactions/success.json");
    let json: Value = serde_json::from_str(data)?;

    let response = ResponseTemplate::new(200).set_body_json(json);

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(response)
        .mount(&mock_server)
        .await;

    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api.get_transaction(&[String::from("sig-1")]).await?;
    let transfer_changes = &result.transactions[0].token_transfer_changes;

    assert_eq!(transfer_changes.len(), 7);

    let native_transfer = &transfer_changes[0];
    assert_eq!(native_transfer.token_mint, None);
    assert_eq!(native_transfer.token_program, None);
    assert_eq!(
        native_transfer.source_owner.as_deref(),
        Some("8TPACXaKotSZ7WXktfmKDRhgoypyGXNzo1ctr2YBzxLc")
    );
    assert_eq!(
        native_transfer.destination_owner.as_deref(),
        Some("2naDnfYtHQAiUfxcMFsygUXCDCbiqiY79eCwmB7ExTAM")
    );
    assert_eq!(native_transfer.amount_raw, 138_528_528);
    assert_eq!(native_transfer.amount_ui, Some(0.138_528_528));
    assert_eq!(native_transfer.decimals, Some(9));
    assert_eq!(native_transfer.transfer_type, "transfer");
    assert_eq!(native_transfer.asset_type, "native");
    assert_eq!(native_transfer.direction, "unknown");
    assert_eq!(native_transfer.instruction_idx, Some(4));
    assert_eq!(native_transfer.inner_idx, None);

    let mint_transfer = &transfer_changes[4];
    assert_eq!(
        mint_transfer.token_mint.as_deref(),
        Some("FZN7QZ8ZUUAxMPfxYEYkH3cXUASzH8EqA6B4tyCL8f1j")
    );
    assert_eq!(
        mint_transfer.token_program.as_deref(),
        Some("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
    );
    assert_eq!(mint_transfer.source_owner, None);
    assert_eq!(
        mint_transfer.destination_owner.as_deref(),
        Some("AFCp9ZKwdvg1f8b4BjGiXfzQA7FsLaDSez4pCdFS1vAA")
    );
    assert_eq!(mint_transfer.source_token_account, None);
    assert_eq!(
        mint_transfer.destination_token_account.as_deref(),
        Some("GmiMy7APQHjmvoPNG5BBLKHbiUb6PocFQPdpV6xPuHdW")
    );
    assert_eq!(mint_transfer.amount_raw, 123_779_259);
    assert_eq!(mint_transfer.amount_ui, Some(0.123_779_259));
    assert_eq!(mint_transfer.decimals, Some(9));
    assert_eq!(mint_transfer.transfer_type, "mint");
    assert_eq!(mint_transfer.asset_type, "spl");
    assert_eq!(mint_transfer.direction, "unknown");
    assert_eq!(mint_transfer.instruction_idx, Some(6));
    assert_eq!(mint_transfer.inner_idx, Some(4));

    let burn_transfer = transfer_changes.last().expect("must contain burn transfer");
    assert_eq!(
        burn_transfer.token_mint.as_deref(),
        Some("BKFZ9STTizz49mMxuA5DzuPWvQHT58ZFLaMznAH2kk5L")
    );
    assert_eq!(
        burn_transfer.token_program.as_deref(),
        Some("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
    );
    assert_eq!(
        burn_transfer.source_owner.as_deref(),
        Some("AFCp9ZKwdvg1f8b4BjGiXfzQA7FsLaDSez4pCdFS1vAA")
    );
    assert_eq!(burn_transfer.destination_owner, None);
    assert_eq!(
        burn_transfer.source_token_account.as_deref(),
        Some("AFCp9ZKwdvg1f8b4BjGiXfzQA7FsLaDSez4pCdFS1vAA")
    );
    assert_eq!(burn_transfer.destination_token_account, None);
    assert_eq!(burn_transfer.amount_raw, 63_594_763_190);
    assert_eq!(burn_transfer.amount_ui, Some(63_594.763_19));
    assert_eq!(burn_transfer.decimals, Some(6));
    assert_eq!(burn_transfer.transfer_type, "burn");
    assert_eq!(burn_transfer.asset_type, "spl");
    assert_eq!(burn_transfer.direction, "unknown");
    assert_eq!(burn_transfer.instruction_idx, Some(6));
    assert_eq!(burn_transfer.inner_idx, Some(7));

    assert_transaction_rpc_request(&mock_server, 1, "sig-1").await
}

#[tokio::test]
async fn should_process_multiple_signatures_with_separate_requests_when_all_succeed() -> Result<()>
{
    let mock_server = MockServer::start().await;
    let list_jsons: [Value; 5] = fetch_additional_json()?;
    let signatures: Vec<String> = ["sig-1", "sig-2", "sig-3", "sig-4", "sig-5"]
        .iter()
        .map(ToString::to_string)
        .collect();

    for (signature, json) in signatures.iter().zip(list_jsons) {
        mount_transaction_json_response(&mock_server, signature, json).await;
    }

    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api.get_transaction(&signatures).await?;

    assert!(result.errors.is_empty());
    assert!(result.failed_signatures.is_empty());
    assert_eq!(result.transactions.len(), 5);
    assert_eq!(result.processed_signatures, signatures);
    assert_eq!(result.transactions[0].result.slot, 414_018_196);
    assert_eq!(result.transactions[1].result.slot, 414_017_743);
    assert_eq!(result.transactions[2].result.slot, 414_018_023);
    assert_eq!(result.transactions[3].result.slot, 414_018_322);
    assert_eq!(result.transactions[4].result.slot, 414_018_322);

    assert_transaction_rpc_requests(&mock_server, &["sig-1", "sig-2", "sig-3", "sig-4", "sig-5"])
        .await
}

#[tokio::test]
async fn should_return_partial_success_with_failed_signatures_when_some_rpc_calls_fail()
-> Result<()> {
    let mock_server = MockServer::start().await;
    let success: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/success.json"
    ))?;
    let rpc_error: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/rpc_error_generic.json"
    ))?;

    mount_transaction_json_response(&mock_server, "sig-1", success.clone()).await;
    mount_transaction_json_response(&mock_server, "sig-2", rpc_error).await;
    mount_transaction_json_response(&mock_server, "sig-3", success).await;

    let signatures = ["sig-1", "sig-2", "sig-3"]
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api.get_transaction(&signatures).await?;

    assert_eq!(result.transactions.len(), 2);
    assert_eq!(result.processed_signatures, vec!["sig-1", "sig-3"]);
    assert_eq!(result.failed_signatures, vec!["sig-2"]);
    assert_eq!(result.errors.len(), 1);

    let error = &result.errors[0];
    assert_eq!(error.signature, "sig-2");
    assert_eq!(error.status_code, Some(200));
    assert_eq!(error.rpc_code, Some(-32602));
    assert_eq!(
        error.message,
        "Invalid params: invalid type: integer `123`, expected a string"
    );

    assert_transaction_rpc_requests(&mock_server, &["sig-1", "sig-2", "sig-3"]).await
}

#[tokio::test]
async fn should_mark_signature_as_failed_when_rpc_result_is_null() -> Result<()> {
    let mock_server = MockServer::start().await;
    let result_null: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/result_null.json"
    ))?;

    mount_transaction_json_response(&mock_server, "sig-null", result_null).await;

    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api
        .get_transaction(&[String::from("sig-null")])
        .await?;

    assert!(result.transactions.is_empty());
    assert!(result.processed_signatures.is_empty());
    assert_eq!(result.failed_signatures, vec!["sig-null"]);
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].signature, "sig-null");
    assert_eq!(result.errors[0].status_code, Some(200));
    assert_eq!(result.errors[0].rpc_code, None);
    assert_eq!(result.errors[0].message, "rpc result is null");

    assert_transaction_rpc_request(&mock_server, 1, "sig-null").await
}

#[tokio::test]
async fn should_fail_signature_with_missing_result_error_when_result_field_is_absent() -> Result<()>
{
    let mock_server = MockServer::start().await;
    mount_transaction_raw_response(
        &mock_server,
        "sig-missing",
        200,
        r#"{"jsonrpc":"2.0","id":"1"}"#,
        "application/json",
    )
    .await;

    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api
        .get_transaction(&[String::from("sig-missing")])
        .await?;

    assert!(result.transactions.is_empty());
    assert!(result.processed_signatures.is_empty());
    assert_eq!(result.failed_signatures, vec!["sig-missing"]);
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].signature, "sig-missing");
    assert_eq!(result.errors[0].status_code, Some(200));
    assert_eq!(result.errors[0].rpc_code, None);
    assert_eq!(
        result.errors[0].message,
        "missing result field in rpc response"
    );

    assert_transaction_rpc_request(&mock_server, 1, "sig-missing").await
}

#[tokio::test]
async fn should_mark_signature_failed_on_decode_error_when_invalid_json_received() -> Result<()> {
    let mock_server = MockServer::start().await;
    let malformed_json = include_str!("../tests/fixtures/helius/transactions/malformed_json.txt");

    mount_transaction_raw_response(
        &mock_server,
        "malformed-sig",
        200,
        malformed_json,
        "application/json",
    )
    .await;

    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api
        .get_transaction(&[String::from("malformed-sig")])
        .await?;

    assert!(result.processed_signatures.is_empty());
    assert!(result.transactions.is_empty());
    assert_eq!(result.failed_signatures, vec!["malformed-sig"]);

    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].signature, "malformed-sig");
    assert_eq!(result.errors[0].status_code, Some(200));
    assert_eq!(result.errors[0].rpc_code, None);
    assert!(
        result.errors[0]
            .message
            .contains("failed to decode rpc envelope")
    );

    assert_transaction_rpc_request(&mock_server, 1, "malformed-sig").await
}

#[tokio::test]
async fn should_fail_signature_without_retry_on_non_rate_limit_rpc_error() -> Result<()> {
    let mock_server = MockServer::start().await;
    let rpc_error: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/rpc_error_generic.json"
    ))?;

    mount_transaction_json_response(&mock_server, "sig-1", rpc_error).await;

    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api.get_transaction(&["sig-1".to_string()]).await?;

    assert!(result.transactions.is_empty());
    assert!(result.processed_signatures.is_empty());
    assert_eq!(result.failed_signatures, vec!["sig-1"]);
    assert_eq!(result.errors.len(), 1);

    let error = &result.errors[0];
    assert_eq!(error.signature, "sig-1");
    assert_eq!(error.status_code, Some(200));
    assert_eq!(error.rpc_code, Some(-32602));
    assert_eq!(
        error.message,
        "Invalid params: invalid type: integer `123`, expected a string"
    );

    assert_transaction_rpc_request(&mock_server, 1, "sig-1").await
}

#[tokio::test]
async fn should_retry_after_http_429_and_mark_signature_processed_after_success() -> Result<()> {
    let mock_server = MockServer::start().await;
    let success: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/success.json"
    ))?;

    mount_transaction_http_429_response_n_times(
        &mock_server,
        "sig-retry",
        json!({"error": "Too Many Requests"}),
        1,
    )
    .await;
    mount_transaction_json_response(&mock_server, "sig-retry", success).await;

    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api
        .get_transaction(&[String::from("sig-retry")])
        .await?;

    assert!(result.errors.is_empty());
    assert!(result.failed_signatures.is_empty());
    assert_eq!(result.transactions.len(), 1);
    assert_eq!(result.processed_signatures, vec!["sig-retry"]);

    assert_transaction_rpc_request_counts(&mock_server, &[("sig-retry", 2)]).await
}

#[tokio::test]
async fn should_retry_on_rpc_rate_limit_error_and_mark_signature_processed_after_success()
-> Result<()> {
    let mock_server = MockServer::start().await;
    let success: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/success.json"
    ))?;

    mount_transaction_json_response_n_times(
        &mock_server,
        "sig-rate-limited",
        json!({
            "jsonrpc": "2.0",
            "id": "1",
            "error": {
                "code": -32429,
                "message": "Too Many Requests"
            }
        }),
        1,
    )
    .await;
    mount_transaction_json_response(&mock_server, "sig-rate-limited", success).await;

    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api
        .get_transaction(&[String::from("sig-rate-limited")])
        .await?;

    assert!(result.errors.is_empty());
    assert!(result.failed_signatures.is_empty());
    assert_eq!(result.transactions.len(), 1);
    assert_eq!(result.processed_signatures, vec!["sig-rate-limited"]);

    assert_transaction_rpc_request_counts(&mock_server, &[("sig-rate-limited", 2)]).await
}

#[tokio::test]
async fn should_mark_signature_failed_when_rate_limit_retry_budget_is_exhausted() -> Result<()> {
    let mock_server = MockServer::start().await;

    mount_transaction_json_response_n_times(
        &mock_server,
        "sig-exhausted",
        json!({
            "jsonrpc": "2.0",
            "id": "1",
            "error": {
                "code": -32429,
                "message": "Too Many Requests"
            }
        }),
        MAX_RATE_LIMIT_ATTEMPTS as u64,
    )
    .await;

    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api
        .get_transaction(&[String::from("sig-exhausted")])
        .await?;

    assert!(result.transactions.is_empty());
    assert!(result.processed_signatures.is_empty());
    assert_eq!(result.failed_signatures, vec!["sig-exhausted"]);
    assert_eq!(result.errors.len(), 1);

    let error = &result.errors[0];
    assert_eq!(error.signature, "sig-exhausted");
    assert_eq!(error.status_code, Some(200));
    assert_eq!(error.rpc_code, Some(-32429));
    assert_eq!(error.message, "Too Many Requests");
    assert!(error.is_rate_limited());

    assert_transaction_rpc_request_counts(
        &mock_server,
        &[("sig-exhausted", MAX_RATE_LIMIT_ATTEMPTS)],
    )
    .await
}

#[tokio::test]
async fn should_return_mixed_batch_when_one_signature_retries_successfully_and_another_fails()
-> Result<()> {
    let mock_server = MockServer::start().await;
    let success: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/success.json"
    ))?;
    let rpc_error: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/rpc_error_generic.json"
    ))?;

    mount_transaction_http_429_response_n_times(
        &mock_server,
        "sig-retry",
        json!({"error": "Too Many Requests"}),
        1,
    )
    .await;
    mount_transaction_json_response(&mock_server, "sig-retry", success).await;
    mount_transaction_json_response(&mock_server, "sig-fatal", rpc_error).await;

    let signatures = vec![String::from("sig-retry"), String::from("sig-fatal")];
    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;
    let result = helius_api.get_transaction(&signatures).await?;

    assert_eq!(result.transactions.len(), 1);
    assert_eq!(result.processed_signatures, vec!["sig-retry"]);
    assert_eq!(result.failed_signatures, vec!["sig-fatal"]);
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.errors[0].signature, "sig-fatal");
    assert_eq!(result.errors[0].rpc_code, Some(-32602));

    assert_transaction_rpc_request_counts(&mock_server, &[("sig-fatal", 1), ("sig-retry", 2)]).await
}

#[tokio::test]
async fn should_process_signatures_across_chunks_of_ten() -> Result<()> {
    let mock_server = MockServer::start().await;
    let success: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/helius/transactions/success.json"
    ))?;
    let signatures = (1..=11)
        .map(|index| format!("sig-{index}"))
        .collect::<Vec<_>>();

    for signature in &signatures {
        mount_transaction_json_response(&mock_server, signature, success.clone()).await;
    }

    let helius_api = HeliusApi::new(100, 20, mock_server.uri())?;
    let result = helius_api.get_transaction(&signatures).await?;

    assert!(result.errors.is_empty());
    assert!(result.failed_signatures.is_empty());
    assert_eq!(result.transactions.len(), 11);
    assert_eq!(result.processed_signatures, signatures);

    let expected_counts = (1..=11)
        .map(|index| (format!("sig-{index}"), 1usize))
        .collect::<Vec<_>>();
    let expected_count_refs = expected_counts
        .iter()
        .map(|(signature, count)| (signature.as_str(), *count))
        .collect::<Vec<_>>();

    assert_transaction_rpc_request_counts(&mock_server, &expected_count_refs).await
}

#[tokio::test]
async fn should_return_empty_transaction_batch_without_requests_when_input_is_empty() -> Result<()>
{
    let mock_server = MockServer::start().await;
    let helius_api = HeliusApi::new(8, 2, mock_server.uri())?;

    let result = helius_api.get_transaction(&[]).await?;

    assert!(result.transactions.is_empty());
    assert!(result.processed_signatures.is_empty());
    assert!(result.failed_signatures.is_empty());
    assert!(result.errors.is_empty());

    let received_requests = mock_server.received_requests().await.unwrap();
    assert!(received_requests.is_empty());

    Ok(())
}
