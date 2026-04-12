use std::fs;

use anyhow::Result;
use reqwest::Client;
use serde_json::{Value, json};

const HELIUS_URL: &str = "https://mainnet.helius-rpc.com/?api-key=";
const SIGNATURES_ADDRESS: &str = "4UCSDDYGdcMUjmNJSTRRAUN6ev9H8BQ9z4hNbZQ57kvb";
const TRANSACTION_SIGNATURE: &str =
    "4TXLkNSE8Rpv9VuqJ5aYUnd7JvvikoEqDB8M9oumN43n4Ma2Deyg34dkqVTuYCbVFzLwzkUPdM6nd3jNc7Fua1Sr";

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    save_signatures_success_fixture().await?;
    save_signatures_generic_error_fixture().await?;
    save_transaction_success_fixture().await?;
    save_transaction_generic_error_fixture().await?;

    Ok(())
}

fn helius_url(api: &str) -> String {
    format!("{HELIUS_URL}{api}")
}

fn write_fixture(path: &str, body: String) -> Result<()> {
    fs::write(path, body)?;
    println!("saved {path}");
    Ok(())
}

async fn send_rpc_request(client: &Client, url: &str, body: &Value) -> Result<String> {
    Ok(client.post(url).json(body).send().await?.text().await?)
}

async fn save_signatures_success_fixture() -> Result<()> {
    let api = dotenvy::var("api")?;
    let client = Client::new();
    let url = helius_url(&api);

    let body = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getSignaturesForAddress",
        "params": [
            SIGNATURES_ADDRESS,
            {
                "max_supported_transaction_version": 0
            }
        ]
    });

    let response = send_rpc_request(&client, &url, &body).await?;
    write_fixture("tests/fixtures/helius/signatures/success.json", response)
}

async fn save_signatures_generic_error_fixture() -> Result<()> {
    let api = dotenvy::var("api")?;
    let client = Client::new();
    let url = helius_url(&api);

    let body = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getSignaturesForAddress",
        "params": [
            123,
            {
                "before": 456,
                "limit": "bad"
            }
        ]
    });

    let response = send_rpc_request(&client, &url, &body).await?;
    write_fixture(
        "tests/fixtures/helius/signatures/rpc_error_generic.json",
        response,
    )
}

async fn save_transaction_success_fixture() -> Result<()> {
    let api = dotenvy::var("api")?;
    let client = Client::new();
    let url = helius_url(&api);

    let body = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getTransaction",
        "params": [
            TRANSACTION_SIGNATURE,
            {
                "encoding": "jsonParsed",
                "maxSupportedTransactionVersion": 0
            }
        ]
    });

    let response = send_rpc_request(&client, &url, &body).await?;
    write_fixture("tests/fixtures/helius/transactions/success.json", response)
}

async fn save_transaction_generic_error_fixture() -> Result<()> {
    let api = dotenvy::var("api")?;
    let client = Client::new();
    let url = helius_url(&api);

    let body = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getTransaction",
        "params": [
            123,
            {
                "encoding": false,
                "maxSupportedTransactionVersion": "bad"
            }
        ]
    });

    let response = send_rpc_request(&client, &url, &body).await?;
    write_fixture(
        "tests/fixtures/helius/transactions/rpc_error_generic.json",
        response,
    )
}
