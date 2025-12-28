use reqwest::Client;
use serde_json::{self, json};
use anyhow::Result;
use std::fs;

#[derive(serde::Deserialize, Debug)]
struct Signature {
    blockTime: u32,
    confirmationStatus: String,
    err: Option<String>,
    memo: Option<String>,
    signature: String,
    slot: u32,
}

#[derive(serde::Deserialize, Debug)]
struct RpcResponse {
    result: Vec<Signature>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new();
    let body = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getSignaturesForAddress",
        "params": ["FCnD9Z8cwHjBcA8AW8c9VT974N99E2UNoFhZfbQDRiTP"]
    });

    let response = client.post("https://mainnet.helius-rpc.com/?api-key=7eea4741-97b9-45a3-9d67-e31cae965197")
    .json(&body)
    .send()
    .await?;

    let response_text: RpcResponse = response.json().await?;

    println!("len: {}", response_text.result.len());

    Ok(())
}
