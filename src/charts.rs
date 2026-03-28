use anyhow::Result;
use chrono::Utc;
use sqlx::PgPool;

pub struct SuccessFailTx {
    pub failed_tx: i64,
    pub success_tx: i64,
}

pub async fn tx_time_line(pool: &PgPool, requested_hours: u16, address: &str) -> Result<[u16; 30]> {
    let timestamps: Vec<i64> = sqlx::query_scalar(
        "
        SELECT block_time 
        FROM transactions 
        WHERE owner_address = $1 
        ORDER BY block_time ASC
        ",
    )
    .bind(address)
    .fetch_all(pool)
    .await?;
    let interval = (i64::from(requested_hours) * 60 * 60) / 30;
    let mut colums: [u16; 30] = [0; 30];
    let now: i64 = Utc::now().timestamp();

    for time in timestamps {
        let delta: i64 = now - time;
        let colum_number = usize::try_from(delta / interval).unwrap_or(1);
        if colum_number > 30 {
            continue;
        }

        colums[colum_number] += 1;
    }

    Ok(colums)
}

pub async fn count_success_fail_tx(pool: &PgPool, address: &str) -> Result<SuccessFailTx> {
    let counts = sqlx::query_as!(
        SuccessFailTx,
        "
        SELECT 
            COUNT(*) FILTER (WHERE err IS NULL) as \"failed_tx!\",
            COUNT(*) FILTER (WHERE err IS NOT NULL) as \"success_tx!\"
        FROM transactions
        WHERE owner_address = $1
        ",
        address
    )
    .fetch_one(pool)
    .await?;

    Ok(counts)
}

pub async fn native_volume_lamports(pool: &PgPool, address: &str) -> Result<i64> {
    let volume: Option<i64> = sqlx::query_scalar(
        "
        SELECT COALESCE(SUM(amount_raw), 0)::bigint
        FROM token_transfers
        WHERE tracked_owner = $1
          AND asset_type = 'native'
        ",
    )
    .bind(address)
    .fetch_one(pool)
    .await?;

    Ok(volume.unwrap_or(0))
}

pub async fn total_fee_lamports(pool: &PgPool, address: &str) -> Result<i64> {
    let fee: i64 = sqlx::query_scalar(
        "
        SELECT COALESCE(SUM(fee), 0)::bigint
        FROM transactions
        WHERE owner_address = $1
        ",
    )
    .bind(address)
    .fetch_one(pool)
    .await?;

    Ok(fee)
}
