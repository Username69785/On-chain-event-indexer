use anyhow::{Ok, Result};
use serde::{Deserialize, Serialize};
use reqwest::Client;

pub struct HeliusApi {
    api: String,
    url: String,
    client: Client,
}

#[derive(Serialize, Debug)]
struct RequestGetSignatures<'a> {
    jsonrpc: &'a str,
    id: &'a str,
    method: &'a str,
    params: (&'a str, Params<'a>),
}

#[derive(Serialize, Debug)]
struct Params<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    before: Option<&'a str>,
}

#[derive(serde::Deserialize, Debug)]
pub struct Signature {
    #[serde(rename = "blockTime")]
    block_time: u32,

    #[serde(rename = "confirmationStatus")]
    confirmation_status: String,

    err: serde_json::Value,
    memo: Option<String>,
    pub signature: String,
    slot: u32,
}

#[derive(serde::Deserialize, Debug)]    
pub struct RpcResponse {
    pub result: Vec<Signature>,
}

impl HeliusApi {
    pub fn new() -> Self {
        let api = dotenvy::var("api").expect("api не найден в .env");
        let client = Client::new();
        let url = String::from("https://mainnet.helius-rpc.com/?api-key=");

        HeliusApi { api, url, client}
    }

    pub async fn get_signatures(
        &self,
        adress: &str,
        last_signature: Option<String>,
    ) -> Result<(RpcResponse, String)> {
        let params = Params { before: last_signature.as_deref() };

        let body = RequestGetSignatures{
            jsonrpc: "2.0",
            id: "1",    
            method: "getSignaturesForAddress",
            params: (
                adress,
                params, // before: Option<String>
            ),
        };

        let response = self.client.post(format!("{}{}", self.url, self.api))
        .json(&body)
        .send()
        .await?;

        let dsrlz_response: RpcResponse = response.json().await?;
        let last_signatures = dsrlz_response.result.last().unwrap().signature.clone();

        Ok((dsrlz_response, last_signatures))
    }
}
