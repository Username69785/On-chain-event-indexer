#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod common;
use common::{
    create_helius_api, mount_http_429_json_response_n_times, mount_post_json_response,
    mount_post_json_response_n_times, mount_post_raw_response, mount_post_raw_response_n_times,
    rpc_error_envelope,
};

use anyhow::{Ok, Result};
use chrono::Utc;
use on_chain_event_indexer::requests::client::SignaturesPage;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use wiremock::MockServer;

const ADDRESS: &str = "address";

fn build_signatures_result(now_ts: i64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": "1",
        "result": [
            {
                "signature": "sig-fresh-1",
                "blockTime": now_ts - 60,
            },
            {
                "signature": "sig-fresh-2",
                "blockTime": now_ts - 300,
            },
            {
                "signature": "sig-1",
                "blockTime": now_ts - 3600,
            },
            {
                "signature": "sig-2",
                "blockTime": now_ts - 14_000,
            },
            {
                "signature": "sig-old",
                "blockTime": now_ts - 50_000,
            }
        ]
    })
}

fn build_signatures_result_with_null_blocktime(now_ts: i64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": "1",
        "result": [
            {
                "signature": "sig-fresh-1",
                "blockTime": now_ts - 60,
            },
            {
                "signature": "sig-null",
                "blockTime": Value::Null,
            },
            {
                "signature": "sig-fresh-2",
                "blockTime": now_ts - 300,
            },
            {
                "signature": "sig-old",
                "blockTime": now_ts - 7_200,
            },
            {
                "signature": "sig-after-old",
                "blockTime": now_ts - 120,
            }
        ]
    })
}

fn build_empty_signatures_result() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": "1",
        "result": []
    })
}

fn assert_signatures_page(
    result: &SignaturesPage,
    raw_count: usize,
    expected_signatures: &[&str],
    expected_last_signature: Option<&str>,
    reached_cutoff: bool,
) {
    assert_eq!(result.raw_count, raw_count);
    assert_eq!(result.response.result.len(), expected_signatures.len());

    let actual_signatures = result
        .response
        .result
        .iter()
        .map(|signature| signature.signature.as_str())
        .collect::<Vec<_>>();

    assert_eq!(actual_signatures, expected_signatures);
    assert_eq!(result.last_signature.as_deref(), expected_last_signature);
    assert_eq!(result.reached_cutoff, reached_cutoff);
}

async fn assert_all_signatures_rpc_requests(
    mock_server: &MockServer,
    expected_before: &[Value],
) -> Result<()> {
    let received_requests = mock_server.received_requests().await.unwrap();
    assert_eq!(received_requests.len(), expected_before.len());

    for (index, expected) in expected_before.iter().enumerate() {
        let request_json: Value = received_requests[index].body_json()?;

        assert_eq!(received_requests[index].method, "POST");
        assert_eq!(request_json["method"], "getSignaturesForAddress");
        assert_eq!(request_json["params"][0], ADDRESS);
        assert_eq!(request_json["params"][1]["before"], *expected);
        assert_eq!(request_json["params"][1]["limit"], 1000);
    }

    Ok(())
}

#[tokio::test]
async fn should_return_all_signatures_when_all_results_are_within_requested_window() -> Result<()> {
    let mock_server = MockServer::start().await;
    let now_ts = Utc::now().timestamp();

    mount_post_json_response(&mock_server, build_signatures_result(now_ts)).await;

    let helius_api = create_helius_api(&mock_server)?;
    let result = helius_api.get_signatures(ADDRESS, None, 24).await?;

    assert_signatures_page(
        &result,
        5,
        &["sig-fresh-1", "sig-fresh-2", "sig-1", "sig-2", "sig-old"],
        Some("sig-old"),
        false,
    );

    assert_all_signatures_rpc_requests(&mock_server, &[Value::Null]).await
}

#[tokio::test]
async fn should_filter_only_recent_signatures_and_set_reached_cutoff_true() -> Result<()> {
    let mock_server = MockServer::start().await;
    let now_ts = Utc::now().timestamp();

    mount_post_json_response(&mock_server, build_signatures_result(now_ts)).await;

    let helius_api = create_helius_api(&mock_server)?;
    let result = helius_api.get_signatures(ADDRESS, None, 2).await?;

    assert_signatures_page(
        &result,
        5,
        &["sig-fresh-1", "sig-fresh-2", "sig-1"],
        Some("sig-old"),
        true,
    );

    assert_all_signatures_rpc_requests(&mock_server, &[Value::Null]).await
}

#[tokio::test]
async fn should_send_cursor_in_before_param_when_provided() -> Result<()> {
    let mock_server = MockServer::start().await;
    let now_ts = Utc::now().timestamp();

    mount_post_json_response(&mock_server, build_signatures_result(now_ts)).await;

    let helius_api = create_helius_api(&mock_server)?;
    let result = helius_api
        .get_signatures(ADDRESS, Some("sig-fresh-2".to_string()), 24)
        .await?;

    assert_signatures_page(
        &result,
        5,
        &["sig-fresh-1", "sig-fresh-2", "sig-1", "sig-2", "sig-old"],
        Some("sig-old"),
        false,
    );

    assert_all_signatures_rpc_requests(&mock_server, &[json!("sig-fresh-2")]).await
}

#[tokio::test]
async fn should_return_empty_result_when_rpc_returns_empty_list() -> Result<()> {
    let mock_server = MockServer::start().await;

    mount_post_json_response(&mock_server, build_empty_signatures_result()).await;

    let helius_api = create_helius_api(&mock_server)?;
    let result = helius_api.get_signatures(ADDRESS, None, 4).await?;

    assert_signatures_page(&result, 0, &[], None, false);

    assert_all_signatures_rpc_requests(&mock_server, &[Value::Null]).await
}

#[tokio::test]
async fn should_skip_null_blocktime_and_continue_filtering() -> Result<()> {
    let mock_server = MockServer::start().await;
    let now_ts = Utc::now().timestamp();

    mount_post_json_response(
        &mock_server,
        build_signatures_result_with_null_blocktime(now_ts),
    )
    .await;

    let helius_api = create_helius_api(&mock_server)?;
    let result = helius_api.get_signatures(ADDRESS, None, 1).await?;

    assert_signatures_page(
        &result,
        5,
        &["sig-fresh-1", "sig-fresh-2"],
        Some("sig-after-old"),
        true,
    );

    assert_all_signatures_rpc_requests(&mock_server, &[Value::Null]).await
}

#[tokio::test]
async fn should_return_error_without_retry_on_regular_rpc_error() -> Result<()> {
    let mock_server = MockServer::start().await;

    mount_post_json_response(&mock_server, rpc_error_envelope(-32602, "Invalid params")).await;

    let helius_api = create_helius_api(&mock_server)?;
    let Err(error) = helius_api.get_signatures(ADDRESS, None, 4).await else {
        panic!("regular rpc error must fail without retry")
    };
    let error_text = error.to_string();
    assert!(error_text.contains("rpc error on getSignaturesForAddress"));
    assert!(error_text.contains("Invalid params"));

    assert_all_signatures_rpc_requests(&mock_server, &[Value::Null]).await
}

#[tokio::test]
async fn should_return_decode_error_on_invalid_json_at_200() -> Result<()> {
    let mock_server = MockServer::start().await;

    mount_post_raw_response(&mock_server, 200, "{invalid-json", "application/json").await;

    let helius_api = create_helius_api(&mock_server)?;
    let Err(error) = helius_api.get_signatures(ADDRESS, None, 4).await else {
        panic!("invalid json must fail decoding")
    };
    let error_text = error.to_string();
    assert!(error_text.contains("failed to decode getSignaturesForAddress response"));

    assert_all_signatures_rpc_requests(&mock_server, &[Value::Null]).await
}

#[tokio::test]
async fn should_retry_after_http_429_and_succeed() -> Result<()> {
    let mock_server = MockServer::start().await;
    let now_ts = Utc::now().timestamp();

    mount_http_429_json_response_n_times(
        &mock_server,
        rpc_error_envelope(-32005, "Too many requests"),
        3,
    )
    .await;
    mount_post_json_response(&mock_server, build_signatures_result(now_ts)).await;

    let helius_api = create_helius_api(&mock_server)?;
    let result = helius_api.get_signatures(ADDRESS, None, 24).await?;

    assert_signatures_page(
        &result,
        5,
        &["sig-fresh-1", "sig-fresh-2", "sig-1", "sig-2", "sig-old"],
        Some("sig-old"),
        false,
    );

    assert_all_signatures_rpc_requests(&mock_server, &[const { Value::Null }; 4]).await
}

#[tokio::test]
async fn should_retry_on_rpc_rate_limit_and_succeed() -> Result<()> {
    let mock_server = MockServer::start().await;
    let now_ts = Utc::now().timestamp();

    mount_post_json_response_n_times(
        &mock_server,
        rpc_error_envelope(-32429, "Too Many Requests"),
        3,
    )
    .await;
    mount_post_json_response(&mock_server, build_signatures_result(now_ts)).await;

    let helius_api = create_helius_api(&mock_server)?;
    let result = helius_api.get_signatures(ADDRESS, None, 24).await?;

    assert_signatures_page(
        &result,
        5,
        &["sig-fresh-1", "sig-fresh-2", "sig-1", "sig-2", "sig-old"],
        Some("sig-old"),
        false,
    );

    assert_all_signatures_rpc_requests(&mock_server, &[const { Value::Null }; 4]).await
}

#[tokio::test]
async fn should_exhaust_retries_on_429_with_invalid_json() -> Result<()> {
    let mock_server = MockServer::start().await;

    mount_post_raw_response_n_times(&mock_server, 429, "{invalid-json", "application/json", 5)
        .await;

    let helius_api = create_helius_api(&mock_server)?;
    let Err(error) = helius_api.get_signatures(ADDRESS, None, 4).await else {
        panic!("persistent 429 with invalid json must fail after retry budget is exhausted")
    };

    let error_text = error.to_string();
    assert!(error_text.contains("failed to decode getSignaturesForAddress response"));
    assert!(error_text.contains("status=429"));

    assert_all_signatures_rpc_requests(&mock_server, &[const { Value::Null }; 5]).await
}

#[tokio::test]
async fn should_fail_with_missing_result_error_on_invalid_envelope() -> Result<()> {
    let mock_server = MockServer::start().await;

    mount_post_json_response(
        &mock_server,
        json!({
            "jsonrpc": "2.0",
            "id": "1"
        }),
    )
    .await;

    let helius_api = create_helius_api(&mock_server)?;
    let Err(error) = helius_api.get_signatures(ADDRESS, None, 4).await else {
        panic!("envelope without result or error must fail")
    };

    assert_eq!(
        error.to_string(),
        "missing result field in getSignaturesForAddress response"
    );

    assert_all_signatures_rpc_requests(&mock_server, &[Value::Null]).await
}

#[tokio::test]
async fn should_exhaust_retries_and_fail_on_persistent_rpc_rate_limit() -> Result<()> {
    let mock_server = MockServer::start().await;

    mount_post_json_response_n_times(
        &mock_server,
        rpc_error_envelope(-32429, "Too Many Requests"),
        5,
    )
    .await;

    let helius_api = create_helius_api(&mock_server)?;
    let Err(error) = helius_api.get_signatures(ADDRESS, None, 4).await else {
        panic!("persistent rpc rate limit must fail after retry budget is exhausted")
    };

    let error_text = error.to_string();
    assert!(error_text.contains("rpc error on getSignaturesForAddress"));
    assert!(error_text.contains("code=-32429"));
    assert!(error_text.contains("rate_limited=true"));

    assert_all_signatures_rpc_requests(&mock_server, &[const { Value::Null }; 5]).await
}
