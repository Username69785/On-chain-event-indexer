use anyhow::{Ok, Result};
use log::Record;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::{QueryBuilder, query};

use super::{RpcResponse, TransactionInfo};

pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new_pool() -> Result<Self> {
        let url = dotenvy::var("DATABASE_URL").expect("database_url не найден в .env");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;

        Ok(Database { pool })
    }

    pub async fn get_signatures_db(&self, adress: &str) -> Result<Vec<String>> {
        let signatures = sqlx::query!(
            "SELECT signature FROM signatures WHERE owner_address = $1",
            adress
        )
        .fetch_all(&self.pool)
        .await?;

        let signatures_vec: Vec<String> = signatures
        .into_iter()
        .map(|record| record.signature)
        .collect();

        Ok(signatures_vec)
    }

    pub async fn write_signatures(&self, signatures: &RpcResponse, adress: &str) -> Result<()> {
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "INSERT INTO signatures 
            (owner_address, signature, slot, block_time, confirmation_status, err)",
        );

        let signatures_iter = signatures.result.iter();

        query_builder.push_values(signatures_iter, |mut b, signature| {
            b.push_bind(&adress)
                .push_bind(&signature.signature)
                .push_bind(signature.slot)
                .push_bind(signature.block_time)
                .push_bind(signature.confirmation_status.as_ref().map(|x| x.as_str()))
                .push_bind(&signature.err);
        });
        query_builder.push("ON CONFLICT (signature) DO NOTHING");

        let query = query_builder.build();

        query.execute(&self.pool).await?;

        Ok(())
    }

    pub async fn write_transaction_info(&self, transaction_info: Vec<TransactionInfo>, adress: &str) -> Result<()> {
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
            "INSERT INTO signatures 
            (owner_address, signature, slot, block_time, confirmation_status, err)",
        );
/*     
    TODO:
    1. оформить таблицу
        понять какие поля буду
        определить типы
        создать
    2. дописать эту функцию
*/
        Ok(())
    }
}
