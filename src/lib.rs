#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![allow(clippy::missing_errors_doc, clippy::must_use_candidate)]

pub mod backoff;
pub mod db;
pub mod frontend;
pub mod logging;
pub mod requests;
pub mod telemetry;
pub mod types;

use crate::db::Database;
use crate::requests::HeliusApi;

pub struct AppState {
    pub database: Database,
    pub helius_api: HeliusApi,
}
