use anyhow::{Ok, Result};
use serde::{Deserialize, Serialize};
use reqwest::{Client, Response};
use futures::future::try_join_all;
use tokio::time::{Duration, sleep};

pub struct HeliusApi {
    api: String,
    url: String,
    client: Client,
}

#[derive(Serialize, Debug)]
struct Request<'a> {
    jsonrpc: &'a str,
    id: &'a str,
    method: &'a str,
    params: (&'a str, Params<'a>),
}

#[derive(Serialize, Debug)]
struct Params<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    before: Option<&'a str>,

    #[serde(skip_serializing_if = "Option::is_none")]
    encoding: Option<&'a str>,
}

#[derive(Deserialize, Serialize, Debug, sqlx::Type)]
#[serde(rename_all = "lowercase")] 
pub enum ConfirmationStatus {
    Processed,
    Confirmed,
    Finalized,
}

impl ConfirmationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfirmationStatus::Processed => "processed",
            ConfirmationStatus::Confirmed => "confirmed",
            ConfirmationStatus::Finalized => "finalized",
        }
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct Signature {
    #[serde(rename = "blockTime")]
    pub block_time: i64,

    #[serde(rename = "confirmationStatus")]
    pub confirmation_status: Option<ConfirmationStatus>,

    pub err: serde_json::Value,
    // memo: Option<String>,
    pub signature: String,
    pub slot: i64,
}

#[derive(serde::Deserialize, Debug)]    
pub struct RpcResponse {
    pub result: Vec<Signature>,
}

pub struct Transaction {

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
        let params = Params { before: last_signature.as_deref() , encoding: None};

        let body = Request {
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

    pub async fn get_transaction(&self, signatures: Vec<String>) -> Result<Vec<Response>> {
        let responses = try_join_all(signatures
            .chunks(100)
            .map(async |signatures| {
            let mut batch: Vec<Request> = Vec::with_capacity(100);

            for signature in signatures {
                let params = Params { before: None , encoding: Some("json")};
                let body = Request {
                    jsonrpc: "2.0",
                    id: "1",
                    method: "getTransaction",
                    params: (signature, params)
                };

                batch.push(body);
            }

            let response: Response = self.client.post(format!("{}{}", self.url, self.api))
            .json(&batch)
            .send()
            .await?;

            sleep(Duration::from_millis(125)).await;

            Ok(response)
        })).await?;

        Ok(responses)
    }
}
