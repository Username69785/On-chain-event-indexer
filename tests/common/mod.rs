#![allow(dead_code)]

use anyhow::Result;
use on_chain_event_indexer::requests::HeliusApi;
use serde_json::{Value, json};
use std::path::Path;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DEFAULT_RPS: u32 = 8;
const DEFAULT_MAX_CONCURRENT: usize = 2;
const DEFAULT_MAX_RATE_LIMIT_RETRIES: usize = 4;

pub fn create_helius_api(mock_server: &MockServer) -> Result<HeliusApi> {
    HeliusApi::new(
        DEFAULT_RPS,
        DEFAULT_MAX_CONCURRENT,
        DEFAULT_MAX_RATE_LIMIT_RETRIES,
        mock_server.uri(),
    )
}

pub fn rpc_error_envelope(code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": "1",
        "error": {
            "code": code,
            "message": message,
        }
    })
}

pub fn load_json_fixture(path: impl AsRef<Path>) -> Result<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    let data = std::fs::read_to_string(path)?;

    Ok(serde_json::from_str(&data)?)
}

pub fn load_transaction_fixture(name: &str) -> Result<Value> {
    load_json_fixture(format!("tests/fixtures/helius/transactions/{name}"))
}

pub async fn mount_post_json_response(mock_server: &MockServer, body: Value) {
    let response_template = ResponseTemplate::new(200).set_body_json(body);

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(response_template)
        .mount(mock_server)
        .await;
}

pub async fn mount_post_json_response_n_times(mock_server: &MockServer, body: Value, count: u64) {
    let response_template = ResponseTemplate::new(200).set_body_json(body);

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(response_template)
        .up_to_n_times(count)
        .expect(count)
        .mount(mock_server)
        .await;
}

pub async fn mount_post_raw_response(
    mock_server: &MockServer,
    status: u16,
    body: &str,
    content_type: &str,
) {
    let response_template =
        ResponseTemplate::new(status).set_body_raw(body.to_owned(), content_type);

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(response_template)
        .mount(mock_server)
        .await;
}

pub async fn mount_post_raw_response_n_times(
    mock_server: &MockServer,
    status: u16,
    body: &str,
    content_type: &str,
    count: u64,
) {
    let response_template =
        ResponseTemplate::new(status).set_body_raw(body.to_owned(), content_type);

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(response_template)
        .up_to_n_times(count)
        .expect(count)
        .mount(mock_server)
        .await;
}

pub async fn mount_http_429_json_response_n_times(
    mock_server: &MockServer,
    body: Value,
    count: u64,
) {
    let response_template = ResponseTemplate::new(429).set_body_json(body);

    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(response_template)
        .up_to_n_times(count)
        .expect(count)
        .mount(mock_server)
        .await;
}

pub fn fetch_additional_json() -> Result<[Value; 5]> {
    Ok([
        load_transaction_fixture("success_additional_1.json")?,
        load_transaction_fixture("success_additional_2.json")?,
        load_transaction_fixture("success_additional_3.json")?,
        load_transaction_fixture("success_additional_4.json")?,
        load_transaction_fixture("success_additional_5.json")?,
    ])
}

pub fn transaction_request_match(signature: &str) -> Value {
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

pub async fn mount_transaction_json_response(
    mock_server: &MockServer,
    signature: &str,
    body: Value,
) {
    let response = ResponseTemplate::new(200).set_body_json(body);

    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_partial_json(transaction_request_match(signature)))
        .respond_with(response)
        .expect(1)
        .mount(mock_server)
        .await;
}

pub async fn mount_transaction_json_response_n_times(
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

pub async fn mount_transaction_http_429_response_n_times(
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

pub async fn mount_transaction_raw_response(
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
