use std::vec;

use anyhow::{Ok, Result};
use futures::future::try_join_all;
use futures::{self, StreamExt, stream};
use reqwest::{Client, Response};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
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

#[derive(Deserialize, Debug)]
pub struct Signature {
    #[serde(rename = "blockTime")]
    pub block_time: i64,

    #[serde(rename = "confirmationStatus")]
    pub confirmation_status: Option<ConfirmationStatus>,

    pub err: Value,
    // memo: Option<String>,
    pub signature: String,
    pub slot: i64,
}

#[derive(serde::Deserialize, Debug)]
pub struct RpcResponse {
    pub result: Vec<Signature>,
}

#[derive(Deserialize, Debug)]
pub struct TransactionResult {
    result: TransactionInfo,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInfo {
    block_time: i32,
    meta: Meta,               // err, compute_units_consumed, fee
    transaction: Transaction, //signatures
    slot: i32,

    #[serde(skip)]
    raw_json: Value, //полный json
}

#[derive(Deserialize, Debug)]
pub struct Meta {
    compute_units_consumed: i32,
    fee: i32,

    #[serde(deserialize_with = "parse_error")]
    err: bool,
}

#[derive(Deserialize, Debug)]
pub struct Transaction {
    signatures: String,
}

fn parse_error<'de, D>(data: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let opt: Option<Value> = Option::deserialize(data)?;

    std::result::Result::Ok(opt.is_none())
}

impl HeliusApi {
    pub fn new() -> Self {
        let api = dotenvy::var("api").expect("api не найден в .env");
        let client = Client::new();
        let url = String::from("https://mainnet.helius-rpc.com/?api-key=");

        HeliusApi { api, url, client }
    }

    pub async fn get_signatures(
        &self,
        adress: &str,
        last_signature: Option<String>,
    ) -> Result<(RpcResponse, String)> {
        let params = Params {
            before: last_signature.as_deref(),
            encoding: None,
        };

        let body = Request {
            jsonrpc: "2.0",
            id: "1",
            method: "getSignaturesForAddress",
            params: (
                adress, params, // before: Option<String>
            ),
        };

        let response = self
            .client
            .post(format!("{}{}", self.url, self.api))
            .json(&body)
            .send()
            .await?;

        let dsrlz_response: RpcResponse = response.json().await?;
        let last_signatures = dsrlz_response.result.last().unwrap().signature.clone();

        Ok((dsrlz_response, last_signatures))
    }

    pub async fn get_transaction(&self, signatures: Vec<String>) -> Result<Vec<TransactionResult>> {
        let responses = stream::iter(signatures)
            .map(async move |signature| {
                let params = Params {
                    before: None,
                    encoding: Some("json"),
                };
                let body = Request {
                    jsonrpc: "2.0",
                    id: "1",
                    method: "getTransaction",
                    params: (&signature, params),
                };

                let response: Response = self
                    .client
                    .post(format!("{}{}", self.url, self.api))
                    .json(&body)
                    .send()
                    .await?;

                let text_response: String = response.text().await?;
                let mut transaction: TransactionResult = serde_json::from_str(&text_response)?;
                transaction.result.raw_json = serde_json::from_str(&text_response)?;

                Ok(transaction)
            })
            .buffer_unordered(10)
            .collect::<Vec<_>>()
            .await;

        // добавить получение инфы о токенах + задуматься о структуре

        responses.into_iter().collect()
    }
}
