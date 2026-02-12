use std::collections::HashMap;
use std::time::Instant;

use anyhow::{Result, anyhow};
use futures::{self, StreamExt, stream};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::{Duration, sleep};
use tracing::{debug, info, instrument, warn};

use crate::logging::mask_addr;
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

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct Params<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    before: Option<&'a str>,

    #[serde(skip_serializing_if = "Option::is_none")]
    encoding: Option<&'a str>,

    #[serde(skip_serializing_if = "Option::is_none")]
    max_supported_transaction_version: Option<u8>,
}

#[derive(Deserialize, Debug)]
pub struct Signature {
    #[serde(rename = "blockTime")]
    pub block_time: i64,

    pub signature: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct RpcResponse {
    pub result: Vec<Signature>,
}

#[derive(Deserialize, Debug)]
pub struct TransactionResult {
    pub result: TransactionInfo,

    #[serde(skip)]
    pub vec_transfers: Vec<Transfers>,

    /// Изменения балансов SPL токенов (рассчитывается после десериализации)
    #[serde(skip)]
    pub token_transfer_changes: Vec<TokenTransferChange>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInfo {
    pub block_time: i32,
    pub meta: Meta,               // err, compute_units_consumed, fee
    pub transaction: Transaction, //signatures
    pub slot: i32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub compute_units_consumed: i32,
    pub fee: i32,

    #[serde(default)]
    pub err: Value,

    pub post_balances: Vec<u64>,
    pub pre_balances: Vec<u64>,

    /// Доп. адреса из Address Lookup Tables (для versioned tx)
    #[serde(default)]
    pub loaded_addresses: Option<LoadedAddresses>,

    /// Внутренние инструкции (CPI), приходят только в jsonParsed
    #[serde(default)]
    pub inner_instructions: Vec<InnerInstructions>,

    /// Балансы SPL токенов ДО транзакции
    pub pre_token_balances: Vec<TokenBalance>,
    /// Балансы SPL токенов ПОСЛЕ транзакции
    pub post_token_balances: Vec<TokenBalance>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LoadedAddresses {
    pub writable: Vec<String>,
    pub readonly: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct Transaction {
    pub signatures: Vec<String>,
    pub message: AccountKeys, // account_keys
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageHeader {
    pub num_required_signatures: u8,
    pub num_readonly_signed_accounts: u8,
    pub num_readonly_unsigned_accounts: u8,
}

#[derive(Deserialize, Debug)]
pub struct AccountKeys {
    #[serde(rename = "accountKeys")]
    account_keys: Vec<AccountKey>,

    /// Инструкции верхнего уровня (jsonParsed)
    #[serde(default)]
    pub instructions: Vec<Instruction>,

    #[serde(default)]
    pub header: Option<MessageHeader>,
}

/// Инструкция из jsonParsed (верхняя/inner)
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Instruction {
    /// Распарсенная инструкция (если программа известна)
    #[serde(default)]
    pub parsed: Option<ParsedInstruction>,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub program_id: Option<String>,
    #[serde(default)]
    pub program_id_index: Option<u16>,
    #[serde(default)]
    pub accounts: Vec<AccountRef>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub stack_height: Option<u8>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum AccountRef {
    Index(u16),
    Address(String),
}

#[derive(Deserialize, Debug, Clone)]
pub struct ParsedInstruction {
    pub info: ParsedInfo,
    #[serde(rename = "type")]
    pub instruction_type: String,
}

/// Поля parsed.info, которые нужны для transfer/mint/burn
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ParsedInfo {
    #[serde(default)]
    pub token_amount: Option<UiTokenAmount>,
    #[serde(default)]
    pub amount: Option<String>,
    #[serde(default)]
    pub decimals: Option<u8>,
    #[serde(default)]
    pub ui_amount: Option<f64>,
    #[serde(default)]
    pub ui_amount_string: Option<String>,
    #[serde(default)]
    pub mint: Option<String>,
    #[serde(default)]
    pub authority: Option<String>,
    #[serde(default)]
    pub lamports: Option<u64>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InnerInstructions {
    pub index: u16,
    pub instructions: Vec<Instruction>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum AccountKey {
    Pubkey(String),
    Info(AccountKeyInfo),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AccountKeyInfo {
    pub pubkey: String,
    pub signer: Option<bool>,
    pub source: Option<String>,
    pub writable: Option<bool>,
}

#[derive(Deserialize, Debug)]
pub struct Transfers {
    transfers: i64,
    address: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TokenBalance {
    pub account_index: u8,
    pub mint: String,
    pub owner: String,
    pub program_id: String,
    pub ui_token_amount: UiTokenAmount,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenAmount {
    pub amount: String,
    pub decimals: u8,
    pub ui_amount: Option<f64>,
    pub ui_amount_string: String,
}

/// Изменение баланса токена для конкретного аккаунта
#[derive(Debug, Clone)]
pub struct TokenTransferChange {
    /// Адрес токена (mint)
    pub token_mint: Option<String>,

    /// Программа токена (SPL/Token-2022)
    pub token_program: Option<String>,

    /// Владелец (кошелёк) отправителя
    pub source_owner: Option<String>,

    /// Владелец получателя
    pub destination_owner: Option<String>,

    /// Для SPL: токен-аккаунт отправителя
    pub source_token_account: Option<String>,

    /// Для SPL: токен-аккаунт получателя
    pub destination_token_account: Option<String>,

    /// Количество токенов в base units, всегда положительное
    pub amount_raw: i128,

    /// Удобное отображение (base units -> UI)
    pub amount_ui: Option<f64>,

    /// Количество десятичных знаков
    pub decimals: Option<u8>,

    /// Тип операции (transfer/mint/burn/unknown)
    pub transfer_type: String,

    /// Тип актива (native/spl)
    pub asset_type: String,

    /// Направление относительно tracked_owner
    pub direction: String,

    /// Кто авторизовал (часто важно для SPL)
    pub authority: Option<String>,

    /// Индекс инструкции
    pub instruction_idx: Option<i32>,

    /// Индекс inner-инструкции (если CPI)
    pub inner_idx: Option<i32>,
}

impl TransactionResult {
    pub fn num_signers(&self) -> i32 {
        if let Some(header) = &self.result.transaction.message.header {
            return i32::from(header.num_required_signatures);
        }
        let signatures_len = self.result.transaction.signatures.len();
        if signatures_len > 0 {
            return signatures_len as i32;
        }
        self.result.transaction.message.count_signers() as i32
    }

    pub fn num_instructions(&self) -> i32 {
        self.result.transaction.message.instructions.len() as i32
    }

    fn all_account_keys(&self) -> Vec<String> {
        let mut keys = self.result.transaction.message.pubkeys();
        if let Some(loaded) = &self.result.meta.loaded_addresses {
            keys.extend(loaded.writable.clone());
            keys.extend(loaded.readonly.clone());
        }
        keys
    }

    fn calculate_transfers(&mut self) {
        let pre_balances: &Vec<u64> = &self.result.meta.pre_balances;
        let post_balances: &Vec<u64> = &self.result.meta.post_balances;
        let account_keys: Vec<String> = self.all_account_keys();

        if pre_balances.len() != account_keys.len() || post_balances.len() != account_keys.len() {
            warn!(
                pre_len = pre_balances.len(),
                post_len = post_balances.len(),
                keys_len = account_keys.len(),
                "Balances length doesn't match account keys; skipping unmatched entries"
            );
        }

        self.vec_transfers = pre_balances
            .iter()
            .enumerate() // (i, pre)
            .zip(post_balances) // ((i, pre), post)
            .filter_map(|((i, pre), post)| {
                let difference = *pre as i64 - *post as i64;

                if difference != 0 {
                    Some(Transfers {
                        transfers: difference,
                        address: account_keys.get(i)?.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();
    }

    /// Собирает перемещения токенов и SOL из jsonParsed инструкций.
    fn calculate_token_transfer(&mut self) {
        let mut transfers: Vec<TokenTransferChange> = Vec::new();
        let token_account_meta = self.token_account_meta_map();

        for (idx, instruction) in self
            .result
            .transaction
            .message
            .instructions
            .iter()
            .enumerate()
        {
            self.collect_token_transfer(
                instruction,
                idx as i32,
                None,
                &token_account_meta,
                &mut transfers,
            );
        }

        for inner in &self.result.meta.inner_instructions {
            for (inner_idx, instruction) in inner.instructions.iter().enumerate() {
                self.collect_token_transfer(
                    instruction,
                    inner.index as i32,
                    Some(inner_idx as i32),
                    &token_account_meta,
                    &mut transfers,
                );
            }
        }

        self.token_transfer_changes = transfers;
    }

    fn collect_token_transfer(
        &self,
        instruction: &Instruction,
        instruction_idx: i32,
        inner_idx: Option<i32>,
        token_account_meta: &HashMap<String, TokenAccountMeta>,
        out: &mut Vec<TokenTransferChange>,
    ) {
        const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";
        const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

        // 1. Получаем распарсенные данные (если есть)
        let parsed = match &instruction.parsed {
            Some(parsed) => parsed,
            None => return,
        };

        // 2. Проверяем тип инструкции: нас интересуют только transfer/mint/burn
        let transfer_type_raw = parsed.instruction_type.as_str();
        let is_relevant_transfer = matches!(
            transfer_type_raw,
            "transfer" | "transferChecked" | "mintTo" | "mintToChecked" | "burn" | "burnChecked"
        );
        if !is_relevant_transfer {
            return;
        }

        // 3. Определяем программу (SPL или System/Native)
        let program_id = instruction.program_id.as_deref();
        let program = instruction.program.as_deref();
        let is_system = program_id == Some(SYSTEM_PROGRAM) || program == Some("system");
        let is_spl = program_id == Some(TOKEN_PROGRAM)
            || program == Some("spl-token")
            || program == Some("spl-token-2022");

        let (asset_type, token_program) = if is_spl {
            (String::from("spl"), program_id.map(str::to_string))
        } else if is_system {
            (String::from("native"), None)
        } else {
            return; // Игнорируем неизвестные программы
        };

        let info = &parsed.info;

        // 4. Извлекаем количество, decimals и mint в зависимости от типа актива
        let (amount_raw, mut amount_ui, mut decimals, mut token_mint) = if asset_type == "native" {
            let lamports = match info.lamports {
                Some(lamports) => lamports,
                None => return,
            };
            (
                lamports as i128,
                Some(lamports as f64 / 1_000_000_000f64),
                Some(9),
                None,
            )
        } else if let Some(token_amount) = &info.token_amount {
            // Вариант 1: передана структура tokenAmount (часто в transferChecked)
            let amount_raw = match token_amount.amount.parse::<i128>() {
                Ok(v) => v,
                Err(_) => return,
            };
            (
                amount_raw,
                token_amount.ui_amount,
                Some(token_amount.decimals),
                info.mint.clone(),
            )
        } else if let Some(amount_str) = &info.amount {
            // Вариант 2: передана строка amount (обычный transfer)
            let amount_raw = match amount_str.parse::<i128>() {
                Ok(v) => v,
                Err(_) => return,
            };
            (amount_raw, info.ui_amount, info.decimals, info.mint.clone())
        } else {
            return;
        };

        // 5. Определяем отправителя и получателя
        // Откуда → куда: для SPL берём token-accounts, для native — кошельки
        let (source_token_account, destination_token_account, source_owner, destination_owner) =
            if asset_type == "native" {
                (None, None, info.source.clone(), info.destination.clone())
            } else {
                let mut source = info.source.clone();
                let mut destination = info.destination.clone();

                // Корректируем логику для mint/burn, где source/dest могут называться иначе (account)
                if transfer_type_raw.starts_with("mint") {
                    destination = info.account.clone().or(destination);
                }
                if transfer_type_raw.starts_with("burn") {
                    source = info.account.clone().or(source);
                }

                // Пытаемся найти владельцев (owner) через lookup таблицу метаданных (token_account_meta)
                let source_owner = source
                    .as_ref()
                    .and_then(|addr| token_account_meta.get(addr).map(|m| m.owner.clone()));
                let destination_owner = destination
                    .as_ref()
                    .and_then(|addr| token_account_meta.get(addr).map(|m| m.owner.clone()));

                // Дозаполняем mint, если не нашли ранее
                if token_mint.is_none() {
                    token_mint = source
                        .as_ref()
                        .and_then(|addr| token_account_meta.get(addr).map(|m| m.mint.clone()))
                        .or_else(|| {
                            destination.as_ref().and_then(|addr| {
                                token_account_meta.get(addr).map(|m| m.mint.clone())
                            })
                        });
                }

                // Дозаполняем decimals, если не нашли ранее
                if decimals.is_none() {
                    decimals = source
                        .as_ref()
                        .and_then(|addr| token_account_meta.get(addr).map(|m| m.decimals))
                        .or_else(|| {
                            destination
                                .as_ref()
                                .and_then(|addr| token_account_meta.get(addr).map(|m| m.decimals))
                        });
                }

                // Если есть decimals, но нет UI amount — считаем сами
                if amount_ui.is_none() {
                    if let Some(decimals) = decimals {
                        amount_ui = Some((amount_raw as f64) / 10f64.powi(decimals as i32));
                    }
                }

                (source, destination, source_owner, destination_owner)
            };

        // 6. Нормализуем тип трансфера
        let transfer_type = match transfer_type_raw {
            "transfer" | "transferChecked" => "transfer",
            "mintTo" | "mintToChecked" => "mint",
            "burn" | "burnChecked" => "burn",
            _ => "unknown",
        }
        .to_string();

        // 7. Сохраняем результат
        out.push(TokenTransferChange {
            token_mint,
            token_program,
            source_owner,
            destination_owner,
            source_token_account,
            destination_token_account,
            amount_raw,
            amount_ui,
            decimals,
            transfer_type,
            asset_type,
            direction: String::from("unknown"),
            authority: info.authority.clone(),
            instruction_idx: Some(instruction_idx),
            inner_idx,
        });
    }

    /// Быстрый маппинг token-account -> {owner, mint, decimals} из pre/post балансов.
    fn token_account_meta_map(&self) -> HashMap<String, TokenAccountMeta> {
        let account_keys = self.all_account_keys();
        let mut map = HashMap::new();

        for balance in self
            .result
            .meta
            .pre_token_balances
            .iter()
            .chain(self.result.meta.post_token_balances.iter())
        {
            if let Some(token_account) = account_keys.get(balance.account_index as usize) {
                map.insert(
                    token_account.clone(),
                    TokenAccountMeta {
                        owner: balance.owner.clone(),
                        mint: balance.mint.clone(),
                        decimals: balance.ui_token_amount.decimals,
                    },
                );
            }
        }

        map
    }
}

#[derive(Debug, Clone)]
struct TokenAccountMeta {
    owner: String,
    mint: String,
    decimals: u8,
}

impl AccountKeys {
    fn pubkeys(&self) -> Vec<String> {
        self.account_keys
            .iter()
            .map(|k| match k {
                AccountKey::Pubkey(s) => s.clone(),
                AccountKey::Info(info) => info.pubkey.clone(),
            })
            .collect()
    }

    fn count_signers(&self) -> usize {
        self.account_keys
            .iter()
            .filter(|k| match k {
                AccountKey::Info(info) => info.signer.unwrap_or(false),
                _ => false,
            })
            .count()
    }
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
        let params = Params {
            before: last_signature.as_deref(),
            encoding: None,
            max_supported_transaction_version: Some(0),
        };

        let body = Request {
            jsonrpc: "2.0",
            id: "1",
            method: "getSignaturesForAddress",
            params: (
                adress, params, // before: Option<String>
            ),
        };

        let request_started = Instant::now();
        let response = self
            .client
            .post(format!("{}{}", self.url, self.api))
            .json(&body)
            .send()
            .await?;
        let status = response.status();

        let dsrlz_response: RpcResponse = response.json().await?;
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
    pub async fn get_transaction(&self, signatures: Vec<String>) -> Result<Vec<TransactionResult>> {
        let params = Params {
            before: None,
            encoding: Some("jsonParsed"),
            max_supported_transaction_version: Some(0),
        };

        let mut responses_res: Vec<TransactionResult> = Vec::new();

        for (chunk_index, signatures) in signatures.chunks(10).enumerate() {
            let chunk_span =
                tracing::info_span!("tx_chunk", chunk_index, chunk_len = signatures.len());
            let _chunk_guard = chunk_span.enter();
            let chunk_started = Instant::now();
            debug!("Fetching transaction chunk");

            let response = stream::iter(signatures)
                .map(async |signature| {
                    let body = Request {
                        jsonrpc: "2.0",
                        id: "1",
                        method: "getTransaction",
                        params: (&signature, params.clone()),
                    };

                    let request_started = Instant::now();
                    let response: Response = self
                        .client
                        .post(format!("{}{}", self.url, self.api))
                        .json(&body)
                        .send()
                        .await?;
                    let status = response.status();

                    let transactions: TransactionResult = response.json().await?;
                    debug!(
                        status = ?status,
                        elapsed_ms = request_started.elapsed().as_millis(),
                        "Transaction response received"
                    );

                    Ok(transactions)
                })
                .buffered(10)
                .collect::<Vec<_>>()
                .await;
            let response_len = response.len();

            responses_res.append(
                &mut response
                    .into_iter()
                    .collect::<Result<Vec<TransactionResult>, anyhow::Error>>()?,
            );

            info!(
                chunk_len = response_len,
                total = responses_res.len(),
                elapsed_ms = chunk_started.elapsed().as_millis(),
                "Transactions chunk received"
            );

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

        Ok(responses_res)
    }
}
