use sqlx::postgres::{PgPoolOptions, PgPool};
use sqlx::{QueryBuilder, query};
use anyhow::{Ok, Result};

use super::RpcResponse;
//use requests::RpcResponse;

pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new_pool() -> Result<Self> {
        let url = dotenvy::var("database_url").expect("database_url не найден в .env");
        let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await?;

        Ok(Database { pool })
    }

    pub async fn write_signatures(&self, signatures: &RpcResponse, adress: &str) -> Result<()> {
        let mut query_builder: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "INSERT INTO signatures 
            (owner_address, signature, slot, block_time, confirmation_status, err)"
        );

        let signatures_iter = signatures.result.iter();

        query_builder.push_values(signatures_iter, |mut b, signature| {
            b.push_bind(&adress).
            push_bind(&signature.signature).
            push_bind(signature.slot).
            push_bind(signature.block_time).
            push_bind(signature.confirmation_status.as_ref().map(|x| x.as_str())).
            push_bind(&signature.err);
        });
        query_builder.push("ON CONFLICT (signature) DO NOTHING");

        let query = query_builder.build();

        query.execute(&self.pool).await?;

        Ok(())
    }
}
