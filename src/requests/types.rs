#[cfg(test)]
use anyhow::Result as AnyResult;
use serde::{Deserialize, de::Deserializer};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
pub struct Signature {
    #[serde(rename = "blockTime")]
    pub block_time: Option<i64>,

    pub signature: String,
}

#[derive(serde::Deserialize, Debug)]
pub struct RpcResponse {
    pub result: Vec<Signature>,
}

#[derive(Deserialize, Debug)]
pub struct RpcEnvelope<T> {
    #[serde(default)]
    pub result: ResponseField<T>,

    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, Default)]
pub enum ResponseField<T> {
    #[default]
    Missing,
    Null,
    Value(T),
}

impl<'de, T> Deserialize<'de> for ResponseField<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        (Option::<T>::deserialize(deserializer)?)
            .map_or_else(|| Ok(Self::Null), |value| Ok(Self::Value(value)))
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

pub fn is_rate_limited(status_code: Option<u16>, rpc_code: Option<i64>, message: &str) -> bool {
    const HTTP_TOO_MANY_REQUESTS: u16 = 429;
    const RPC_RATE_LIMITED_CODE: i64 = -32429;

    if status_code == Some(HTTP_TOO_MANY_REQUESTS) {
        return true;
    }

    if rpc_code == Some(RPC_RATE_LIMITED_CODE) {
        return true;
    }

    let normalized = message.to_ascii_lowercase();
    normalized.contains("rate limit")
        || normalized.contains("rate-limited")
        || normalized.contains("too many requests")
}

impl RpcError {
    pub fn is_rate_limited(&self) -> bool {
        is_rate_limited(None, Some(self.code), &self.message)
    }
}

#[derive(Deserialize, Debug)]
pub struct TransactionResult {
    pub result: TransactionInfo,

    #[serde(skip)]
    pub token_transfer_changes: Vec<TokenTransferChange>,
}

#[derive(Debug, Clone)]
pub struct TransactionFetchError {
    pub signature: String,
    pub status_code: Option<u16>,
    pub rpc_code: Option<i64>,
    pub message: String,
}

impl TransactionFetchError {
    pub fn is_rate_limited(&self) -> bool {
        is_rate_limited(self.status_code, self.rpc_code, &self.message)
    }
}

#[derive(Debug)]
pub struct TransactionBatch {
    pub transactions: Vec<TransactionResult>,
    pub processed_signatures: Vec<String>,
    pub failed_signatures: Vec<String>,
    pub errors: Vec<TransactionFetchError>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInfo {
    pub block_time: i32,
    pub meta: Meta,
    pub transaction: Transaction,
    pub slot: i32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub compute_units_consumed: i32,
    pub fee: i32,

    #[serde(default)]
    pub err: Value,

    #[serde(default)]
    pub loaded_addresses: Option<LoadedAddresses>,

    #[serde(default)]
    pub inner_instructions: Vec<InnerInstructions>,

    pub pre_token_balances: Vec<TokenBalance>,
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
    pub message: AccountKeys,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageHeader {
    pub required_signatures: u8,
}

#[derive(Deserialize, Debug)]
pub struct AccountKeys {
    #[serde(rename = "accountKeys")]
    keys: Vec<AccountKey>,

    #[serde(default)]
    pub instructions: Vec<Instruction>,

    #[serde(default)]
    pub header: Option<MessageHeader>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Instruction {
    #[serde(default, deserialize_with = "deserialize_parsed_instruction_opt")]
    pub parsed: Option<ParsedInstruction>,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub program_id: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ParsedInstruction {
    pub info: ParsedInfo,
    #[serde(rename = "type")]
    pub instruction_type: String,
}

fn deserialize_parsed_instruction_opt<'de, D>(
    deserializer: D,
) -> Result<Option<ParsedInstruction>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    let parsed = match value {
        Some(raw @ Value::Object(_)) => serde_json::from_value::<ParsedInstruction>(raw).ok(),
        _ => None,
    };
    Ok(parsed)
}

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
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TokenBalance {
    pub account_index: u8,
    pub mint: String,
    pub owner: String,
    pub ui_token_amount: UiTokenAmount,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenAmount {
    pub amount: String,
    pub decimals: u8,
    pub ui_amount: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct TokenTransferChange {
    pub token_mint: Option<String>,
    pub token_program: Option<String>,
    pub source_owner: Option<String>,
    pub destination_owner: Option<String>,
    pub source_token_account: Option<String>,
    pub destination_token_account: Option<String>,
    pub amount_raw: i128,
    pub amount_ui: Option<f64>,
    pub decimals: Option<u8>,
    pub transfer_type: String,
    pub asset_type: String,
    pub direction: String,
    pub authority: Option<String>,
    pub instruction_idx: Option<i32>,
    pub inner_idx: Option<i32>,
}

impl TransactionResult {
    pub fn num_signers(&self) -> i32 {
        if let Some(header) = &self.result.transaction.message.header {
            return i32::from(header.required_signatures);
        }
        let signatures_len = self.result.transaction.signatures.len();
        if signatures_len > 0 {
            return i32::try_from(signatures_len).unwrap_or(0);
        }
        i32::try_from(self.result.transaction.message.count_signers()).unwrap_or(0)
    }

    pub fn num_instructions(&self) -> i32 {
        i32::try_from(self.result.transaction.message.instructions.len()).unwrap_or(0)
    }

    pub fn all_account_keys(&self) -> Vec<String> {
        let mut keys = self.result.transaction.message.pubkeys();
        if let Some(loaded) = &self.result.meta.loaded_addresses {
            keys.extend(loaded.writable.clone());
            keys.extend(loaded.readonly.clone());
        }
        keys
    }

    pub fn calculate_token_transfer(&mut self) {
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
            Self::collect_token_transfer(
                instruction,
                i32::try_from(idx).unwrap_or(0),
                None,
                &token_account_meta,
                &mut transfers,
            );
        }

        for inner in &self.result.meta.inner_instructions {
            for (inner_idx, instruction) in inner.instructions.iter().enumerate() {
                Self::collect_token_transfer(
                    instruction,
                    i32::from(inner.index),
                    Some(i32::try_from(inner_idx).unwrap_or(0)),
                    &token_account_meta,
                    &mut transfers,
                );
            }
        }

        self.token_transfer_changes = transfers;
    }

    pub fn collect_token_transfer(
        instruction: &Instruction,
        instruction_idx: i32,
        inner_idx: Option<i32>,
        token_account_meta: &HashMap<String, TokenAccountMeta>,
        out: &mut Vec<TokenTransferChange>,
    ) {
        const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";
        const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

        let Some(parsed) = &instruction.parsed else {
            return;
        };

        let transfer_type_raw = parsed.instruction_type.as_str();
        let is_relevant_transfer = matches!(
            transfer_type_raw,
            "transfer" | "transferChecked" | "mintTo" | "mintToChecked" | "burn" | "burnChecked"
        );
        if !is_relevant_transfer {
            return;
        }

        let program_id = instruction.program_id.as_deref();
        let program = instruction.program.as_deref();
        let is_system = program_id == Some(SYSTEM_PROGRAM) || program == Some("system");
        let is_spl = program_id == Some(TOKEN_PROGRAM)
            || program == Some("spl-token")
            || program == Some("spl-token-2022");

        let normalized_transfer_type = match transfer_type_raw {
            "transfer" | "transferChecked" => "transfer",
            "mintTo" | "mintToChecked" => "mint",
            "burn" | "burnChecked" => "burn",
            _ => "unknown",
        }
        .to_string();

        let transfer_change = if is_spl {
            Self::parse_spl_transfer(
                &parsed.info,
                normalized_transfer_type,
                program_id,
                token_account_meta,
                instruction_idx,
                inner_idx,
            )
        } else if is_system {
            Self::parse_native_transfer(
                &parsed.info,
                normalized_transfer_type,
                instruction_idx,
                inner_idx,
            )
        } else {
            None
        };

        if let Some(change) = transfer_change {
            out.push(change);
        }
    }

    fn parse_native_transfer(
        info: &ParsedInfo,
        transfer_type: String,
        instruction_idx: i32,
        inner_idx: Option<i32>,
    ) -> Option<TokenTransferChange> {
        let lamports = info.lamports?;

        let high = u32::try_from(lamports >> 32).unwrap_or(0);
        let low = u32::try_from(lamports & 0xFFFF_FFFF).unwrap_or(0);
        let as_f64 = f64::from(high) * 4_294_967_296.0 + f64::from(low);

        Some(TokenTransferChange {
            token_mint: None,
            token_program: None,
            source_owner: info.source.clone(),
            destination_owner: info.destination.clone(),
            source_token_account: None,
            destination_token_account: None,
            amount_raw: i128::from(lamports),
            amount_ui: Some(as_f64 / 1_000_000_000.0),
            decimals: Some(9),
            transfer_type,
            asset_type: String::from("native"),
            direction: String::from("unknown"),
            authority: info.authority.clone(),
            instruction_idx: Some(instruction_idx),
            inner_idx,
        })
    }

    fn parse_spl_transfer(
        info: &ParsedInfo,
        transfer_type: String,
        program_id: Option<&str>,
        token_account_meta: &HashMap<String, TokenAccountMeta>,
        instruction_idx: i32,
        inner_idx: Option<i32>,
    ) -> Option<TokenTransferChange> {
        let (amount_raw, mut amount_ui, mut decimals, mut token_mint) =
            if let Some(token_amount) = &info.token_amount {
                let Ok(amount_raw) = token_amount.amount.parse::<i128>() else {
                    return None;
                };
                (
                    amount_raw,
                    token_amount.ui_amount,
                    Some(token_amount.decimals),
                    info.mint.clone(),
                )
            } else if let Some(amount_str) = &info.amount {
                let Ok(amount_raw) = amount_str.parse::<i128>() else {
                    return None;
                };
                (amount_raw, info.ui_amount, info.decimals, info.mint.clone())
            } else {
                return None;
            };

        let mut source = info.source.clone();
        let mut destination = info.destination.clone();

        if transfer_type == "mint" {
            destination = info.account.clone().or(destination);
        }
        if transfer_type == "burn" {
            source = info.account.clone().or(source);
        }

        let source_owner = source
            .as_ref()
            .and_then(|addr| token_account_meta.get(addr).map(|m| m.owner.clone()));
        let destination_owner = destination
            .as_ref()
            .and_then(|addr| token_account_meta.get(addr).map(|m| m.owner.clone()));

        if token_mint.is_none() {
            token_mint = source
                .as_ref()
                .and_then(|addr| token_account_meta.get(addr).map(|m| m.mint.clone()))
                .or_else(|| {
                    destination
                        .as_ref()
                        .and_then(|addr| token_account_meta.get(addr).map(|m| m.mint.clone()))
                });
        }

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

        if amount_ui.is_none()
            && let Some(decimals) = decimals
        {
            #[allow(clippy::cast_precision_loss)]
            let amount_f64 = amount_raw as f64;
            amount_ui = Some(amount_f64 / 10f64.powi(i32::from(decimals)));
        }

        Some(TokenTransferChange {
            token_mint,
            token_program: program_id.map(str::to_string),
            source_owner,
            destination_owner,
            source_token_account: source,
            destination_token_account: destination,
            amount_raw,
            amount_ui,
            decimals,
            transfer_type,
            asset_type: String::from("spl"),
            direction: String::from("unknown"),
            authority: info.authority.clone(),
            instruction_idx: Some(instruction_idx),
            inner_idx,
        })
    }

    pub fn token_account_meta_map(&self) -> HashMap<String, TokenAccountMeta> {
        let keys = self.all_account_keys();
        let mut map = HashMap::new();

        for balance in self
            .result
            .meta
            .pre_token_balances
            .iter()
            .chain(self.result.meta.post_token_balances.iter())
        {
            if let Some(token_account) = keys.get(balance.account_index as usize) {
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
pub struct TokenAccountMeta {
    pub owner: String,
    pub mint: String,
    pub decimals: u8,
}

impl AccountKeys {
    pub fn pubkeys(&self) -> Vec<String> {
        self.keys
            .iter()
            .map(|k| match k {
                AccountKey::Pubkey(s) => s.clone(),
                AccountKey::Info(info) => info.pubkey.clone(),
            })
            .collect()
    }

    pub fn count_signers(&self) -> usize {
        self.keys
            .iter()
            .filter(|k| match k {
                AccountKey::Info(info) => info.signer.unwrap_or(false),
                AccountKey::Pubkey(_) => false,
            })
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::error::Category;

    fn parse_transaction_envelope(data: &str) -> AnyResult<RpcEnvelope<Value>> {
        Ok(serde_json::from_str(data)?)
    }

    fn extract_transaction_info(response: RpcEnvelope<Value>) -> AnyResult<TransactionInfo> {
        let ResponseField::Value(result) = response.result else {
            anyhow::bail!("transaction fixture must contain a non-null result payload");
        };
        Ok(serde_json::from_value(result)?)
    }

    #[test]
    fn should_deserialize_success_response_when_valid_success_json() -> AnyResult<()> {
        let data = include_str!("../../tests/fixtures/helius/signatures/success.json");
        let response: RpcEnvelope<Vec<Signature>> = serde_json::from_str(data)?;

        assert!(response.error.is_none());
        let ResponseField::Value(result) = response.result else {
            panic!("success signature fixture must deserialize into ResponseField::Value");
        };

        assert_eq!(result.len(), 1000);

        let first = &result[0];
        assert_eq!(
            first.signature,
            "2WgqNhsgJnPE2UuR4sTRVugHVqVYLLu2nxb6y2KiUyDHL22mw5VPBeDB5eKyvx8GHSQjKVghSFL8szcSZLZgsv3V"
        );
        assert_eq!(first.block_time, Some(1_775_979_983));

        let second = &result[1];
        assert_eq!(
            second.signature,
            "hRrpBWCFK1mRhNXmt8VqRHZPRemU2G7bQRg8C9rVCKW27vDEbkPiXMG3dppo6dTjBDP9L1LAUKndyQB7pbPGGAd"
        );
        assert_eq!(second.block_time, Some(1_775_977_386));

        let tx_with_rpc_error_payload = &result[17];
        assert_eq!(
            tx_with_rpc_error_payload.signature,
            "4KAP8oXQsVi6QbfQcWLgjUeLuoU935Hpb4bkYEzPUtrCWnwRwXCvF4mkcQy2BfGBeg3rJfSPzjjTN5hE6xuKw37P"
        );
        assert_eq!(tx_with_rpc_error_payload.block_time, Some(1_775_976_858));

        let last = result.last().unwrap();
        assert_eq!(
            last.signature,
            "VPuoe9Hy4rPHio2DDrZyDjz1UPuggpRj1CdRFUFQMUKDr4shyf65xaPM1otFssTssWG6bsnrvALvjudF1erXxG4"
        );
        assert_eq!(last.block_time, Some(1_775_928_590));

        Ok(())
    }

    #[test]
    fn should_deserialize_empty_result_when_empty_result_json() -> AnyResult<()> {
        let data = include_str!("../../tests/fixtures/helius/signatures/empty_result.json");
        let response: RpcEnvelope<Vec<Signature>> = serde_json::from_str(data)?;

        assert!(response.error.is_none());
        let ResponseField::Value(result) = response.result else {
            panic!("empty signature fixture must deserialize into ResponseField::Value");
        };
        assert_eq!(result.len(), 0);

        Ok(())
    }

    #[test]
    fn should_deserialize_generic_rpc_error_when_rpc_error_generic_json() -> AnyResult<()> {
        let data = include_str!("../../tests/fixtures/helius/signatures/rpc_error_generic.json");
        let response: RpcEnvelope<Vec<Signature>> = serde_json::from_str(data)?;

        assert!(matches!(response.result, ResponseField::Missing));
        let rpc_error = response.error.unwrap();
        assert!(!rpc_error.is_rate_limited());

        assert_eq!(rpc_error.code, i64::from(-32602));
        assert_eq!(
            rpc_error.message,
            String::from("Invalid params: invalid type: integer `123`, expected a string")
        );

        Ok(())
    }

    #[test]
    fn should_deserialize_rate_limit_error_when_rpc_error_rate_limit_json() -> AnyResult<()> {
        let data = include_str!("../../tests/fixtures/helius/signatures/rpc_error_rate_limit.json");
        let response: RpcEnvelope<Vec<Signature>> = serde_json::from_str(data)?;

        assert!(matches!(response.result, ResponseField::Missing));

        let rpc_error = response.error.unwrap();

        assert_eq!(rpc_error.code, i64::from(429));
        assert_eq!(rpc_error.message, String::from("Too Many Requests"));

        assert!(rpc_error.is_rate_limited());

        Ok(())
    }

    #[test]
    fn should_return_error_when_deserializing_malformed_json() {
        let data = include_str!("../../tests/fixtures/helius/signatures/malformed_json.txt");
        let Err(error) = serde_json::from_str::<RpcEnvelope<Vec<Signature>>>(data) else {
            panic!("malformed signature fixture must fail to deserialize")
        };

        assert!(matches!(error.classify(), Category::Syntax | Category::Eof));
    }

    #[test]
    fn should_succeed_when_valid_transaction_json_is_provided() -> AnyResult<()> {
        let data = include_str!("../../tests/fixtures/helius/transactions/success.json");
        let response = parse_transaction_envelope(data)?;
        let transaction = extract_transaction_info(response)?;

        assert_eq!(transaction.slot, 412_675_806);
        assert_eq!(transaction.block_time, 1_775_977_452);
        assert_eq!(transaction.meta.compute_units_consumed, 161_456);
        assert_eq!(transaction.meta.fee, 124_000);
        assert!(transaction.meta.err.is_null());

        Ok(())
    }

    #[test]
    fn should_deserialize_transaction_message_shape_from_success_fixture() -> AnyResult<()> {
        let data = include_str!("../../tests/fixtures/helius/transactions/success.json");
        let response = parse_transaction_envelope(data)?;
        let transaction = extract_transaction_info(response)?;

        assert_eq!(transaction.transaction.signatures.len(), 3);
        assert_eq!(transaction.transaction.message.count_signers(), 3);
        assert!(!transaction.transaction.message.keys.is_empty());
        assert!(!transaction.transaction.message.instructions.is_empty());
        assert_eq!(
            transaction.transaction.message.pubkeys()[0],
            "AvdQRq82hfuTLAmFMkkPy2XsTdoNzGmU7mq54vGjGDEZ"
        );

        Ok(())
    }

    #[test]
    fn should_deserialize_top_level_instructions_from_success_fixture() -> AnyResult<()> {
        let data = include_str!("../../tests/fixtures/helius/transactions/success.json");
        let response = parse_transaction_envelope(data)?;
        let transaction = extract_transaction_info(response)?;

        let first_instruction = &transaction.transaction.message.instructions[0];
        assert!(first_instruction.parsed.is_none());
        assert_eq!(
            first_instruction.program_id.as_deref(),
            Some("ComputeBudget111111111111111111111111111111")
        );

        let ata_instruction = &transaction.transaction.message.instructions[2];
        assert_eq!(
            ata_instruction.program.as_deref(),
            Some("spl-associated-token-account")
        );
        assert_eq!(
            ata_instruction.program_id.as_deref(),
            Some("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
        );

        let parsed = ata_instruction
            .parsed
            .as_ref()
            .expect("ATA instruction must contain parsed payload");
        assert_eq!(parsed.instruction_type, "createIdempotent");
        assert_eq!(
            parsed.info.account.as_deref(),
            Some("2naDnfYtHQAiUfxcMFsygUXCDCbiqiY79eCwmB7ExTAM")
        );
        assert_eq!(
            parsed.info.mint.as_deref(),
            Some("So11111111111111111111111111111111111111112")
        );

        Ok(())
    }

    #[test]
    fn should_deserialize_inner_instructions_and_token_balances_from_success_fixture()
    -> AnyResult<()> {
        let data = include_str!("../../tests/fixtures/helius/transactions/success.json");
        let response = parse_transaction_envelope(data)?;
        let transaction = extract_transaction_info(response)?;

        assert!(!transaction.meta.inner_instructions.is_empty());
        assert_eq!(transaction.meta.inner_instructions[0].index, 2);
        assert_eq!(
            transaction.meta.inner_instructions[0].instructions[0]
                .program
                .as_deref(),
            Some("spl-token")
        );
        assert_eq!(
            transaction.meta.inner_instructions[0].instructions[0]
                .parsed
                .as_ref()
                .expect("inner instruction must contain parsed payload")
                .instruction_type,
            "getAccountDataSize"
        );

        assert_eq!(transaction.meta.pre_token_balances.len(), 5);
        assert_eq!(transaction.meta.post_token_balances.len(), 6);
        assert!(!transaction.meta.pre_token_balances[0].mint.is_empty());
        assert!(!transaction.meta.pre_token_balances[0].owner.is_empty());
        assert!(!transaction.meta.post_token_balances[0].mint.is_empty());
        assert!(!transaction.meta.post_token_balances[0].owner.is_empty());

        Ok(())
    }

    #[test]
    fn should_succeed_when_optional_fields_are_missing() -> AnyResult<()> {
        let data = include_str!(
            "../../tests/fixtures/helius/transactions/success_missing_optional_fields.json"
        );
        let response = parse_transaction_envelope(data)?;
        let transaction = extract_transaction_info(response)?;

        assert!(transaction.meta.inner_instructions.is_empty());
        assert!(transaction.transaction.message.header.is_none());
        assert_eq!(transaction.slot, 412_675_806);

        Ok(())
    }

    #[test]
    fn should_preserve_optional_instruction_defaults_when_fields_are_missing() -> AnyResult<()> {
        let data = include_str!(
            "../../tests/fixtures/helius/transactions/success_missing_optional_fields.json"
        );
        let response = parse_transaction_envelope(data)?;
        let transaction = extract_transaction_info(response)?;

        match &transaction.transaction.message.keys[3] {
            AccountKey::Info(info) => assert_eq!(info.signer, None),
            AccountKey::Pubkey(_) => panic!("expected AccountKey::Info for fixture key"),
        }

        let ata_instruction = &transaction.transaction.message.instructions[2];
        assert!(ata_instruction.program.is_none());
        assert_eq!(
            ata_instruction.program_id.as_deref(),
            Some("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
        );
        assert_eq!(
            ata_instruction
                .parsed
                .as_ref()
                .expect("ATA instruction must contain parsed payload")
                .instruction_type,
            "createIdempotent"
        );

        let system_instruction = &transaction.transaction.message.instructions[4];
        assert_eq!(system_instruction.program.as_deref(), Some("system"));
        assert_eq!(
            system_instruction.program_id.as_deref(),
            Some("11111111111111111111111111111111")
        );
        assert!(system_instruction.parsed.is_none());

        Ok(())
    }

    #[test]
    fn should_deserialize_generic_rpc_error_for_transaction_envelope() -> AnyResult<()> {
        let data = include_str!("../../tests/fixtures/helius/signatures/rpc_error_generic.json");
        let response: RpcEnvelope<Value> = serde_json::from_str(data)?;

        assert!(matches!(response.result, ResponseField::Missing));
        let rpc_error = response.error.unwrap();

        assert_eq!(rpc_error.code, -32602);
        assert!(!rpc_error.is_rate_limited());

        Ok(())
    }

    #[test]
    fn should_deserialize_null_transaction_result_as_response_field_null() -> AnyResult<()> {
        let data = include_str!("../../tests/fixtures/helius/transactions/result_null.json");
        let response = parse_transaction_envelope(data)?;

        assert!(matches!(response.result, ResponseField::Null));
        assert!(response.error.is_none());

        Ok(())
    }

    #[test]
    fn should_succeed_when_malformed_parsed_instruction_json_is_provided() -> AnyResult<()> {
        let data = include_str!(
            "../../tests/fixtures/helius/transactions/malformed_parsed_instruction.json"
        );
        let response = parse_transaction_envelope(data)?;

        assert!(response.error.is_none());
        let transaction = extract_transaction_info(response)?;

        let malformed_instruction = &transaction.transaction.message.instructions[2];
        assert_eq!(
            malformed_instruction.program.as_deref(),
            Some("spl-associated-token-account")
        );
        assert!(malformed_instruction.parsed.is_none());

        let next_instruction = &transaction.transaction.message.instructions[3];
        assert_eq!(
            next_instruction
                .parsed
                .as_ref()
                .expect("next instruction must still deserialize")
                .instruction_type,
            "createIdempotent"
        );

        Ok(())
    }

    #[test]
    fn should_return_error_when_malformed_transaction_json_is_provided() {
        let data = include_str!("../../tests/fixtures/helius/transactions/malformed_json.txt");
        let Err(error) = serde_json::from_str::<RpcEnvelope<Value>>(data) else {
            panic!("malformed transaction fixture must fail to deserialize")
        };

        assert!(matches!(error.classify(), Category::Syntax | Category::Eof));
    }
}
