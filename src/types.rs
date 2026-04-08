use serde::Serialize;
use sqlx::FromRow;

#[derive(Serialize, FromRow)]
pub struct JobInfo {
    pub status: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub total_transactions: i64,
    pub processed_transactions: i64,
    pub remaining_transactions: i64,
}

#[derive(Debug, FromRow)]
pub struct ClaimedJob {
    pub job_id: i64,
    pub address: String,
    pub requested_hours: i16,
    pub tx_limit: i16,
}

pub struct SaveStats {
    pub transactions: u64,
    pub token_transfers: u64,
}
