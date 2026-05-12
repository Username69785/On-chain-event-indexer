mod jobs {
    #![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

    use anyhow::{Ok, Result};
    use futures::future::join_all;
    use on_chain_event_indexer::db;
    use pretty_assertions::assert_eq;
    use sqlx::postgres::PgPool;
    use std::collections::BTreeSet;

    #[derive(Debug)]
    struct InsertedJob {
        id: i64,
        address: String,
        status: String,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
        worker_id: Option<i16>,
        tx_limit: i16,
        requested_hours: i16,
    }

    async fn get_inserted_job(pool: &PgPool, job_id: i64) -> Result<InsertedJob> {
        let job = sqlx::query_as!(
            InsertedJob,
            "
            SELECT id, address, status, created_at, updated_at, worker_id, tx_limit, requested_hours
            FROM processing_data
            WHERE id = $1
            ",
            job_id
        )
        .fetch_one(pool)
        .await?;

        Ok(job)
    }

    async fn count_jobs_by_address(pool: &PgPool, address: &str) -> Result<i64> {
        let count = sqlx::query_scalar!(
            "
            SELECT COUNT(*) as count
            FROM processing_data
            WHERE address = $1
            ",
            address
        )
        .fetch_one(pool)
        .await?
        .unwrap_or_default();

        Ok(count)
    }

    async fn create_job(
        database: &db::Database,
        address: &str,
        tx_limit: i16,
        requested_hours: i16,
    ) -> Result<i64> {
        let job_id = database
            .create_processing_job(address, tx_limit, requested_hours)
            .await?
            .expect("processing job should be created");

        Ok(job_id)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_return_some_job_id_and_insert_pending_job_when_creating_new_processing_job(
        pool: PgPool,
    ) -> Result<()> {
        let database = db::Database::from_pool(pool.clone());
        let before_insert =
            sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>("SELECT clock_timestamp()")
                .fetch_one(&pool)
                .await?;

        let job_id = create_job(&database, "address-123", 5000, 8).await?;

        let after_insert =
            sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>("SELECT clock_timestamp()")
                .fetch_one(&pool)
                .await?;

        let inserted_job = get_inserted_job(&pool, job_id).await?;

        assert_eq!(inserted_job.id, job_id);
        assert_eq!(inserted_job.address, "address-123");
        assert_eq!(inserted_job.status, "pending");
        assert_eq!(inserted_job.worker_id, None);
        assert_eq!(inserted_job.tx_limit, 5000);
        assert_eq!(inserted_job.requested_hours, 8);
        assert!(inserted_job.created_at >= before_insert);
        assert!(inserted_job.created_at <= after_insert);
        assert!(inserted_job.updated_at >= before_insert);
        assert!(inserted_job.updated_at <= after_insert);
        assert!(inserted_job.updated_at >= inserted_job.created_at);

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_allow_duplicate_jobs_for_same_address_when_creating(
        pool: PgPool,
    ) -> Result<()> {
        let database = db::Database::from_pool(pool.clone());

        let before_insert =
            sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>("SELECT clock_timestamp()")
                .fetch_one(&pool)
                .await?;

        let job1 = create_job(&database, "address-1", 2500, 24).await?;
        let job2 = create_job(&database, "address-1", 5000, 12).await?;

        let after_insert =
            sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>("SELECT clock_timestamp()")
                .fetch_one(&pool)
                .await?;

        let inserted_job1 = get_inserted_job(&pool, job1).await?;
        let inserted_job2 = get_inserted_job(&pool, job2).await?;

        assert_ne!(job1, job2);
        assert_eq!(inserted_job1.id, job1);
        assert_eq!(inserted_job1.requested_hours, 24);
        assert_eq!(inserted_job1.address, "address-1");
        assert_eq!(inserted_job1.status, "pending");
        assert_eq!(inserted_job1.worker_id, None);
        assert_eq!(inserted_job1.tx_limit, 2500);
        assert!(inserted_job1.created_at >= before_insert);
        assert!(inserted_job1.created_at <= after_insert);
        assert!(inserted_job1.updated_at >= before_insert);
        assert!(inserted_job1.updated_at <= after_insert);

        assert_eq!(inserted_job2.id, job2);
        assert_eq!(inserted_job2.requested_hours, 12);
        assert_eq!(inserted_job2.address, "address-1");
        assert_eq!(inserted_job2.status, "pending");
        assert_eq!(inserted_job2.worker_id, None);
        assert_eq!(inserted_job2.tx_limit, 5000);
        assert!(inserted_job2.created_at >= before_insert);
        assert!(inserted_job2.created_at <= after_insert);
        assert!(inserted_job2.updated_at >= before_insert);
        assert!(inserted_job2.updated_at <= after_insert);
        assert_eq!(count_jobs_by_address(&pool, "address-1").await?, 2);

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_claim_unique_jobs_without_duplicates_when_multiple_tasks_claim_concurrently(
        pool: PgPool,
    ) -> Result<()> {
        let database = db::Database::from_pool(pool.clone());

        for index in 0..20 {
            create_job(&database, &format!("concurrent-address-{index}"), 1000, 24).await?;
        }

        let claim_tasks = (1_u32..=20).map(|worker_id| {
            let database = db::Database::from_pool(pool.clone());
            tokio::spawn(async move { database.claim_pending_job(worker_id).await })
        });

        let task_results = join_all(claim_tasks).await;
        let mut claimed_ids = BTreeSet::new();

        for task_result in task_results {
            let claimed_job = task_result??.expect("each worker should claim one pending job");
            assert!(claimed_ids.insert(claimed_job.job_id));
        }

        assert_eq!(claimed_ids.len(), 20);

        let rows = sqlx::query_as::<_, (i64, String, Option<i16>)>(
            "
            SELECT id, status, worker_id
            FROM processing_data
            ORDER BY id
            ",
        )
        .fetch_all(&pool)
        .await?;

        assert_eq!(rows.len(), 20);
        for (job_id, status, worker_id) in rows {
            assert!(claimed_ids.contains(&job_id));
            assert_eq!(status, "indexing");
            assert!(worker_id.is_some());
        }

        let extra_claim = database.claim_pending_job(21).await?;
        assert!(extra_claim.is_none());

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_only_update_status_for_indexing_jobs_when_updating_processing_status(
        pool: PgPool,
    ) -> Result<()> {
        let database = db::Database::from_pool(pool.clone());
        let pending_id = create_job(&database, "pending-address", 1000, 24).await?;
        let indexing_id = create_job(&database, "indexing-address", 1000, 24).await?;
        let ready_id = create_job(&database, "ready-address", 1000, 24).await?;
        let error_id = create_job(&database, "error-address", 1000, 24).await?;

        sqlx::query(
            "
            UPDATE processing_data
            SET status = CASE id
                WHEN $1 THEN 'indexing'
                WHEN $2 THEN 'ready'
                WHEN $3 THEN 'error'
                ELSE status
            END
            WHERE id IN ($1, $2, $3)
            ",
        )
        .bind(indexing_id)
        .bind(ready_id)
        .bind(error_id)
        .execute(&pool)
        .await?;

        assert_eq!(
            database
                .update_processing_status_by_job_id(pending_id, "ready")
                .await?,
            0
        );
        assert_eq!(
            database
                .update_processing_status_by_job_id(indexing_id, "ready")
                .await?,
            1
        );
        assert_eq!(
            database
                .update_processing_status_by_job_id(ready_id, "error")
                .await?,
            0
        );
        assert_eq!(
            database
                .update_processing_status_by_job_id(error_id, "ready")
                .await?,
            0
        );

        let rows = sqlx::query_as::<_, (i64, String)>(
            "
            SELECT id, status
            FROM processing_data
            WHERE id = ANY($1)
            ORDER BY id
            ",
        )
        .bind([pending_id, indexing_id, ready_id, error_id])
        .fetch_all(&pool)
        .await?;

        let statuses = rows.into_iter().collect::<Vec<_>>();
        assert_eq!(
            statuses,
            vec![
                (pending_id, String::from("pending")),
                (indexing_id, String::from("ready")),
                (ready_id, String::from("ready")),
                (error_id, String::from("error")),
            ]
        );

        Ok(())
    }
}

mod signatures {
    #![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

    use anyhow::{Ok, Result};
    use chrono::Utc;
    use futures::future::join_all;
    use on_chain_event_indexer::{db, requests::RpcResponse};
    use pretty_assertions::assert_eq;
    use sqlx::postgres::PgPool;
    use std::collections::BTreeSet;

    fn signatures_response(names: &[&str]) -> Result<RpcResponse> {
        let block_time = Utc::now().timestamp() - 60;
        let result = names
            .iter()
            .map(|signature| {
                serde_json::json!({
                    "signature": signature,
                    "blockTime": block_time,
                })
            })
            .collect::<Vec<_>>();

        Ok(serde_json::from_value(
            serde_json::json!({ "result": result }),
        )?)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_allow_same_signature_for_different_owners_when_using_composite_pk(
        pool: PgPool,
    ) -> Result<()> {
        let database = db::Database::from_pool(pool.clone());
        let response = signatures_response(&["shared-signature"])?;

        assert_eq!(database.write_signatures(&response, "owner-1").await?, 1);
        assert_eq!(database.write_signatures(&response, "owner-2").await?, 1);
        assert_eq!(database.write_signatures(&response, "owner-1").await?, 0);

        let rows = sqlx::query_as::<_, (String, String)>(
            "
            SELECT owner_address, signature
            FROM signatures
            ORDER BY owner_address, signature
            ",
        )
        .fetch_all(&pool)
        .await?;

        assert_eq!(
            rows,
            vec![
                (String::from("owner-1"), String::from("shared-signature")),
                (String::from("owner-2"), String::from("shared-signature")),
            ]
        );

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_return_disjoint_batches_and_mark_as_processing_when_claiming_signatures_concurrently(
        pool: PgPool,
    ) -> Result<()> {
        let database = db::Database::from_pool(pool.clone());
        let signature_names = (0..200)
            .map(|index| format!("signature-{index:03}"))
            .collect::<Vec<_>>();
        let signature_refs = signature_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let response = signatures_response(&signature_refs)?;

        assert_eq!(database.write_signatures(&response, "owner-1").await?, 200);

        let claim_tasks = (0..2).map(|_| {
            let database = db::Database::from_pool(pool.clone());
            tokio::spawn(async move { database.get_unprocessed_signatures("owner-1", 100).await })
        });

        let task_results = join_all(claim_tasks).await;
        let mut claimed = BTreeSet::new();

        for task_result in task_results {
            let batch = task_result??;
            assert_eq!(batch.len(), 100);
            for signature in batch {
                assert!(claimed.insert(signature));
            }
        }

        assert_eq!(claimed.len(), 200);

        let rows = sqlx::query_as::<_, (String, bool, Option<chrono::DateTime<chrono::Utc>>)>(
            "
            SELECT signature, is_processing, processing_started_at
            FROM signatures
            WHERE owner_address = $1
            ORDER BY signature
            ",
        )
        .bind("owner-1")
        .fetch_all(&pool)
        .await?;

        assert_eq!(rows.len(), 200);
        for (signature, is_processing, processing_started_at) in rows {
            assert!(claimed.contains(&signature));
            assert!(is_processing);
            assert!(processing_started_at.is_some());
        }

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_not_block_forever_on_failed_transaction_when_processing_signatures(
        pool: PgPool,
    ) -> Result<()> {
        let database = db::Database::from_pool(pool.clone());
        let response = signatures_response(&["stuck-signature"])?;

        assert_eq!(database.write_signatures(&response, "owner-1").await?, 1);
        assert_eq!(
            database.get_unprocessed_signatures("owner-1", 100).await?,
            vec![String::from("stuck-signature")]
        );
        assert!(
            database
                .get_unprocessed_signatures("owner-1", 100)
                .await?
                .is_empty()
        );

        sqlx::query(
            "
            UPDATE signatures
            SET processing_started_at = NOW() - INTERVAL '6 minutes'
            WHERE owner_address = $1
              AND signature = $2
            ",
        )
        .bind("owner-1")
        .bind("stuck-signature")
        .execute(&pool)
        .await?;

        assert_eq!(
            database.get_unprocessed_signatures("owner-1", 100).await?,
            vec![String::from("stuck-signature")]
        );

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_not_mark_job_as_ready_when_partial_transaction_failure_occurs(
        pool: PgPool,
    ) -> Result<()> {
        let database = db::Database::from_pool(pool.clone());
        let job_id = database
            .create_processing_job("owner-1", 1000, 24)
            .await?
            .expect("processing job should be created");
        let response = signatures_response(&["success-signature", "failed-signature"])?;

        assert_eq!(database.write_signatures(&response, "owner-1").await?, 2);
        assert!(database.claim_pending_job(1).await?.is_some());

        let claimed = database.get_unprocessed_signatures("owner-1", 100).await?;
        assert_eq!(claimed.len(), 2);
        assert_eq!(
            database
                .mark_signatures_processed("owner-1", &[String::from("success-signature")])
                .await?,
            1
        );

        let ready_update = database
            .update_processing_status_by_job_id(job_id, "ready")
            .await?;
        assert_eq!(ready_update, 0);

        let status = sqlx::query_scalar::<_, String>(
            "
            SELECT status
            FROM processing_data
            WHERE id = $1
            ",
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(status, "indexing");

        Ok(())
    }
}

mod transactions {
    #![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

    use anyhow::{Ok, Result};
    use on_chain_event_indexer::{db, requests::TransactionResult};
    use pretty_assertions::assert_eq;
    use serde_json::Value;
    use sqlx::postgres::PgPool;

    fn load_transaction_fixture(name: &str) -> Result<Value> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/helius/transactions")
            .join(name);
        let data = std::fs::read_to_string(path)?;

        Ok(serde_json::from_str(&data)?)
    }

    fn transaction_result_from_fixture(name: &str) -> Result<TransactionResult> {
        let mut transaction =
            serde_json::from_value::<TransactionResult>(load_transaction_fixture(name)?)?;
        transaction.calculate_token_transfer();

        Ok(transaction)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_not_duplicate_token_transfers_when_save_transaction_data_called_twice(
        pool: PgPool,
    ) -> Result<()> {
        let database = db::Database::from_pool(pool.clone());
        let transaction = transaction_result_from_fixture("success.json")?;
        let first_signature = transaction
            .result
            .transaction
            .signatures
            .first()
            .expect("fixture should contain a signature")
            .clone();
        let expected_transfers = i64::try_from(transaction.token_transfer_changes.len())?;

        let first_save = database
            .save_transaction_data(&[transaction], "tracked-owner")
            .await?;
        assert_eq!(first_save.transactions, 1);
        assert_eq!(
            first_save.token_transfers,
            u64::try_from(expected_transfers)?
        );

        let duplicate_transaction = transaction_result_from_fixture("success.json")?;
        let second_save = database
            .save_transaction_data(&[duplicate_transaction], "tracked-owner")
            .await?;
        assert_eq!(second_save.transactions, 0);
        assert_eq!(second_save.token_transfers, 0);

        let transaction_count = sqlx::query_scalar::<_, i64>(
            "
            SELECT COUNT(*)
            FROM transactions
            WHERE owner_address = $1
              AND signature = $2
            ",
        )
        .bind("tracked-owner")
        .bind(&first_signature)
        .fetch_one(&pool)
        .await?;
        assert_eq!(transaction_count, 1);

        let transfer_count = sqlx::query_scalar::<_, i64>(
            "
            SELECT COUNT(*)
            FROM token_transfers
            WHERE tracked_owner = $1
              AND signature = $2
            ",
        )
        .bind("tracked-owner")
        .bind(&first_signature)
        .fetch_one(&pool)
        .await?;
        assert_eq!(transfer_count, expected_transfers);

        Ok(())
    }
}

mod workflow {
    #![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

    use anyhow::{Ok, Result};
    use chrono::Utc;
    use on_chain_event_indexer::{AppState, db, indexer, requests::HeliusApi};
    use pretty_assertions::assert_eq;
    use serde_json::{Value, json};
    use sqlx::postgres::PgPool;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const OWNER: &str = "workflow-owner";
    const SUCCESS_SIGNATURE: &str = "5SgJtP7Z9YvNQ9o4mN4YQk7G9xK8qP6eL3cR2wA1bV0m";
    const FAILED_SIGNATURE: &str = "failed-signature";
    const WORKER_ID: u32 = 7;

    struct WorkflowHarness {
        pool: PgPool,
        database: db::Database,
        mock_server: MockServer,
    }

    impl WorkflowHarness {
        async fn new(pool: PgPool) -> Result<Self> {
            let mock_server = MockServer::start().await;
            let database = db::Database::from_pool(pool.clone());

            Ok(Self {
                pool,
                database,
                mock_server,
            })
        }

        fn app_state(&self) -> Result<AppState> {
            Ok(AppState {
                database: db::Database::from_pool(self.pool.clone()),
                helius_api: HeliusApi::new(100, 10, self.mock_server.uri())?,
            })
        }

        async fn create_job(&self, tx_limit: i16, requested_hours: i16) -> Result<i64> {
            Ok(self
                .database
                .create_processing_job(OWNER, tx_limit, requested_hours)
                .await?
                .expect("processing job should be created"))
        }

        async fn process_once(&self) -> Result<Option<on_chain_event_indexer::types::JobInfo>> {
            let app_state = self.app_state()?;
            indexer::process_pending_job_once(&app_state, WORKER_ID).await
        }

        async fn job_status(&self, job_id: i64) -> Result<String> {
            let status = sqlx::query_scalar::<_, String>(
                "
                SELECT status
                FROM processing_data
                WHERE id = $1
                ",
            )
            .bind(job_id)
            .fetch_one(&self.pool)
            .await?;

            Ok(status)
        }

        async fn signature_rows(&self) -> Result<Vec<(String, bool, bool)>> {
            Ok(sqlx::query_as::<_, (String, bool, bool)>(
                "
                SELECT signature, is_processed, is_processing
                FROM signatures
                WHERE owner_address = $1
                ORDER BY signature
                ",
            )
            .bind(OWNER)
            .fetch_all(&self.pool)
            .await?)
        }

        async fn count_transactions(&self) -> Result<i64> {
            Ok(sqlx::query_scalar::<_, i64>(
                "
                SELECT COUNT(*)
                FROM transactions
                WHERE owner_address = $1
                ",
            )
            .bind(OWNER)
            .fetch_one(&self.pool)
            .await?)
        }

        async fn count_token_transfers(&self) -> Result<i64> {
            Ok(sqlx::query_scalar::<_, i64>(
                "
                SELECT COUNT(*)
                FROM token_transfers
                WHERE tracked_owner = $1
                ",
            )
            .bind(OWNER)
            .fetch_one(&self.pool)
            .await?)
        }

        async fn assert_no_persisted_data(&self) -> Result<()> {
            assert!(self.signature_rows().await?.is_empty());
            assert_eq!(self.count_transactions().await?, 0);
            assert_eq!(self.count_token_transfers().await?, 0);
            Ok(())
        }

        async fn assert_processed_signature(&self, signature: &str) -> Result<()> {
            assert_eq!(
                self.signature_rows().await?,
                vec![(signature.to_string(), true, false)]
            );
            Ok(())
        }
    }

    fn signature_response(signatures: &[(&str, i64)]) -> Value {
        let result = signatures
            .iter()
            .map(|(signature, block_time)| {
                json!({
                    "signature": signature,
                    "blockTime": block_time,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "jsonrpc": "2.0",
            "id": "1",
            "result": result,
        })
    }

    fn signature_page(prefix: &str, count: usize, block_time: i64) -> Vec<(String, i64)> {
        (0..count)
            .map(|index| (format!("{prefix}-{index:04}"), block_time))
            .collect()
    }

    fn signature_refs(signatures: &[(String, i64)]) -> Vec<(&str, i64)> {
        signatures
            .iter()
            .map(|(signature, block_time)| (signature.as_str(), *block_time))
            .collect()
    }

    fn transaction_fixture(signature: &str) -> Result<Value> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/helius/transactions/success.json");
        let mut value: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        value["result"]["transaction"]["signatures"] = json!([signature]);
        Ok(value)
    }

    fn transaction_request(signature: &str) -> Value {
        json!({
            "method": "getTransaction",
            "params": [
                signature,
                {
                    "encoding": "jsonParsed",
                    "maxSupportedTransactionVersion": 0,
                }
            ]
        })
    }

    async fn mount_signature_response(mock_server: &MockServer, body: Value, count: u64) {
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(
                json!({ "method": "getSignaturesForAddress" }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .up_to_n_times(count)
            .expect(count)
            .mount(mock_server)
            .await;
    }

    async fn mount_transaction_response(
        mock_server: &MockServer,
        signature: &str,
        body: Value,
        count: u64,
    ) {
        Mock::given(method("POST"))
            .and(path("/"))
            .and(body_partial_json(transaction_request(signature)))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .up_to_n_times(count)
            .expect(count)
            .mount(mock_server)
            .await;
    }

    async fn mount_transaction_error(mock_server: &MockServer, signature: &str) {
        mount_transaction_response(
            mock_server,
            signature,
            json!({
                "jsonrpc": "2.0",
                "id": "1",
                "error": {
                    "code": -32602,
                    "message": "Invalid params",
                }
            }),
            1,
        )
        .await;
    }

    async fn assert_signature_requests(
        mock_server: &MockServer,
        expected_before: &[Value],
    ) -> Result<()> {
        let requests = mock_server.received_requests().await.unwrap();
        let signature_requests = requests
            .iter()
            .filter_map(|request| -> Option<Result<Value>> {
                let body: Value = request.body_json().ok()?;
                (body["method"] == "getSignaturesForAddress").then_some(Ok(body))
            })
            .collect::<Result<Vec<_>>>()?;

        assert_eq!(signature_requests.len(), expected_before.len());
        for (request, expected_before) in signature_requests.iter().zip(expected_before) {
            assert_eq!(request["params"][0], OWNER);
            assert_eq!(request["params"][1]["before"], *expected_before);
            assert_eq!(request["params"][1]["limit"], 1000);
        }

        Ok(())
    }

    async fn assert_transaction_requests(
        mock_server: &MockServer,
        expected_signatures: &[&str],
    ) -> Result<()> {
        let requests = mock_server.received_requests().await.unwrap();
        let mut actual = requests
            .iter()
            .filter_map(|request| -> Option<Result<String>> {
                let body: Value = request.body_json().ok()?;
                if body["method"] != "getTransaction" {
                    return None;
                }
                Some(Ok(body["params"][0]
                    .as_str()
                    .expect("signature should be a string")
                    .to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        actual.sort();

        let mut expected = expected_signatures
            .iter()
            .map(|signature| (*signature).to_string())
            .collect::<Vec<_>>();
        expected.sort();

        assert_eq!(actual, expected);
        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_mark_job_as_ready_and_save_transfers_when_happy_path(
        pool: PgPool,
    ) -> Result<()> {
        let harness = WorkflowHarness::new(pool).await?;
        let now = Utc::now().timestamp() - 60;
        let job_id = harness.create_job(1000, 24).await?;

        mount_signature_response(
            &harness.mock_server,
            signature_response(&[(SUCCESS_SIGNATURE, now)]),
            1,
        )
        .await;
        mount_transaction_response(
            &harness.mock_server,
            SUCCESS_SIGNATURE,
            transaction_fixture(SUCCESS_SIGNATURE)?,
            1,
        )
        .await;

        let job_info = harness
            .process_once()
            .await?
            .expect("pending job should be processed");

        assert_eq!(job_info.status, "ready");
        assert_eq!(job_info.total_transactions, 1);
        assert_eq!(job_info.processed_transactions, 1);
        assert_eq!(job_info.remaining_transactions, 0);
        assert_eq!(harness.job_status(job_id).await?, "ready");
        harness
            .assert_processed_signature(SUCCESS_SIGNATURE)
            .await?;
        assert_eq!(harness.count_transactions().await?, 1);
        assert!(harness.count_token_transfers().await? > 0);
        assert_signature_requests(&harness.mock_server, &[Value::Null]).await?;
        assert_transaction_requests(&harness.mock_server, &[SUCCESS_SIGNATURE]).await?;

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_mark_job_as_ready_with_empty_results_when_address_history_is_empty(
        pool: PgPool,
    ) -> Result<()> {
        let harness = WorkflowHarness::new(pool).await?;
        let job_id = harness.create_job(1000, 24).await?;

        mount_signature_response(
            &harness.mock_server,
            json!({ "jsonrpc": "2.0", "id": "1", "result": [] }),
            1,
        )
        .await;

        let job_info = harness
            .process_once()
            .await?
            .expect("pending job should be processed");

        assert_eq!(job_info.status, "ready");
        assert_eq!(job_info.total_transactions, 0);
        assert_eq!(job_info.processed_transactions, 0);
        assert_eq!(job_info.remaining_transactions, 0);
        assert_eq!(harness.job_status(job_id).await?, "ready");
        harness.assert_no_persisted_data().await?;
        assert_signature_requests(&harness.mock_server, &[Value::Null]).await?;
        assert_transaction_requests(&harness.mock_server, &[]).await?;

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_mark_job_as_error_when_get_signatures_returns_fatal_rpc_error(
        pool: PgPool,
    ) -> Result<()> {
        let harness = WorkflowHarness::new(pool).await?;
        let job_id = harness.create_job(1000, 24).await?;

        mount_signature_response(
            &harness.mock_server,
            json!({
                "jsonrpc": "2.0",
                "id": "1",
                "error": {
                    "code": -32602,
                    "message": "Invalid params",
                }
            }),
            1,
        )
        .await;

        let job_info = harness
            .process_once()
            .await?
            .expect("pending job should be processed");

        assert_eq!(job_info.status, "error");
        assert_eq!(harness.job_status(job_id).await?, "error");
        harness.assert_no_persisted_data().await?;
        assert_signature_requests(&harness.mock_server, &[Value::Null]).await?;
        assert_transaction_requests(&harness.mock_server, &[]).await?;

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_save_successful_signatures_and_not_mark_fully_ready_when_partial_transaction_failure(
        pool: PgPool,
    ) -> Result<()> {
        let harness = WorkflowHarness::new(pool).await?;
        let now = Utc::now().timestamp() - 60;
        let job_id = harness.create_job(1000, 24).await?;
        let successful_2 = "successful-signature-2";

        mount_signature_response(
            &harness.mock_server,
            signature_response(&[
                (SUCCESS_SIGNATURE, now),
                (FAILED_SIGNATURE, now),
                (successful_2, now),
            ]),
            1,
        )
        .await;
        mount_transaction_response(
            &harness.mock_server,
            SUCCESS_SIGNATURE,
            transaction_fixture(SUCCESS_SIGNATURE)?,
            1,
        )
        .await;
        mount_transaction_error(&harness.mock_server, FAILED_SIGNATURE).await;
        mount_transaction_response(
            &harness.mock_server,
            successful_2,
            transaction_fixture(successful_2)?,
            1,
        )
        .await;

        let job_info = harness
            .process_once()
            .await?
            .expect("pending job should be processed");

        assert_eq!(job_info.status, "error");
        assert_eq!(job_info.total_transactions, 3);
        assert_eq!(job_info.processed_transactions, 2);
        assert_eq!(job_info.remaining_transactions, 1);
        assert_eq!(harness.job_status(job_id).await?, "error");
        assert_eq!(
            harness.signature_rows().await?,
            vec![
                (SUCCESS_SIGNATURE.to_string(), true, false),
                (FAILED_SIGNATURE.to_string(), false, true),
                (successful_2.to_string(), true, false),
            ]
        );
        assert_eq!(harness.count_transactions().await?, 2);
        assert!(harness.count_token_transfers().await? > 0);
        assert_signature_requests(&harness.mock_server, &[Value::Null]).await?;
        assert_transaction_requests(
            &harness.mock_server,
            &[SUCCESS_SIGNATURE, FAILED_SIGNATURE, successful_2],
        )
        .await?;

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_fetch_all_pages_with_correct_before_when_pagination_required(
        pool: PgPool,
    ) -> Result<()> {
        let harness = WorkflowHarness::new(pool).await?;
        let now = Utc::now().timestamp() - 60;
        let job_id = harness.create_job(2000, 24).await?;
        let first_page = signature_page("first-page", 1000, now);
        let second_page = vec![(SUCCESS_SIGNATURE.to_string(), now)];
        let mut all_signatures = first_page.clone();
        all_signatures.extend(second_page.clone());

        mount_signature_response(
            &harness.mock_server,
            signature_response(&signature_refs(&first_page)),
            1,
        )
        .await;
        mount_signature_response(
            &harness.mock_server,
            signature_response(&signature_refs(&second_page)),
            1,
        )
        .await;

        for (signature, _) in &all_signatures {
            mount_transaction_response(
                &harness.mock_server,
                signature,
                transaction_fixture(signature)?,
                1,
            )
            .await;
        }

        let job_info = harness
            .process_once()
            .await?
            .expect("pending job should be processed");

        assert_eq!(job_info.status, "ready");
        assert_eq!(job_info.total_transactions, 1001);
        assert_eq!(job_info.processed_transactions, 1001);
        assert_eq!(job_info.remaining_transactions, 0);
        assert_eq!(harness.job_status(job_id).await?, "ready");
        assert_eq!(harness.count_transactions().await?, 1001);
        assert_signature_requests(
            &harness.mock_server,
            &[Value::Null, json!("first-page-0999")],
        )
        .await?;

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_stop_pagination_when_cutoff_signature_encountered_in_page(
        pool: PgPool,
    ) -> Result<()> {
        let harness = WorkflowHarness::new(pool).await?;
        let now = Utc::now().timestamp() - 60;
        let old = Utc::now().timestamp() - 7_200;
        let job_id = harness.create_job(1000, 1).await?;

        mount_signature_response(
            &harness.mock_server,
            signature_response(&[(SUCCESS_SIGNATURE, now), ("old-signature", old)]),
            1,
        )
        .await;
        mount_transaction_response(
            &harness.mock_server,
            SUCCESS_SIGNATURE,
            transaction_fixture(SUCCESS_SIGNATURE)?,
            1,
        )
        .await;

        let job_info = harness
            .process_once()
            .await?
            .expect("pending job should be processed");

        assert_eq!(job_info.status, "ready");
        assert_eq!(job_info.total_transactions, 1);
        assert_eq!(job_info.processed_transactions, 1);
        assert_eq!(harness.job_status(job_id).await?, "ready");
        harness
            .assert_processed_signature(SUCCESS_SIGNATURE)
            .await?;
        assert_signature_requests(&harness.mock_server, &[Value::Null]).await?;
        assert_transaction_requests(&harness.mock_server, &[SUCCESS_SIGNATURE]).await?;

        Ok(())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn should_respect_tx_limit_even_if_page_larger_when_processing_signatures(
        pool: PgPool,
    ) -> Result<()> {
        let harness = WorkflowHarness::new(pool).await?;
        let now = Utc::now().timestamp() - 60;
        let job_id = harness.create_job(100, 24).await?;
        let page = signature_page("limit-page", 150, now);

        mount_signature_response(
            &harness.mock_server,
            signature_response(&signature_refs(&page)),
            1,
        )
        .await;

        for (signature, _) in &page {
            mount_transaction_response(
                &harness.mock_server,
                signature,
                transaction_fixture(signature)?,
                1,
            )
            .await;
        }

        let job_info = harness
            .process_once()
            .await?
            .expect("pending job should be processed");

        assert_eq!(job_info.status, "ready");
        assert_eq!(job_info.total_transactions, 150);
        assert_eq!(job_info.processed_transactions, 150);
        assert_eq!(job_info.remaining_transactions, 0);
        assert_eq!(harness.job_status(job_id).await?, "ready");
        assert_eq!(harness.count_transactions().await?, 150);
        assert_signature_requests(&harness.mock_server, &[Value::Null]).await?;

        Ok(())
    }
}
