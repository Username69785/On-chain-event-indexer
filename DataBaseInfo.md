### 1. Таблица `token_transfers`
Хранит детальную информацию о переводах токенов.

*   **Tablespace:** `pg_default`

| # | Имя колонки | Тип данных | Not Null | Значение по умолчанию |
| :--- | :--- | :--- | :---: | :--- |
| 1 | `id` | `uuid` | [v] | `gen_random_uuid()` |
| 2 | `tracked_owner` | `text` | [v] | |
| 3 | `signature` | `text` | [v] | |
| 5 | `token_mint` | `text` | [ ] | |
| 7 | `decimals` | `int4` | [ ] | |
| 8 | `slot` | `int8` | [v] | |
| 9 | `block_time` | `int8` | [ ] | |
| 10 | `created_at` | `timestamptz` | [ ] | `now()` |
| 11 | `token_program` | `text` | [ ] | |
| 12 | `source_owner` | `text` | [ ] | |
| 13 | `destination_owner` | `text` | [ ] | |
| 14 | `source_token_account` | `text` | [ ] | |
| 15 | `destination_token_account` | `text` | [ ] | |
| 16 | `amount_raw` | `numeric(40)` | [v] | |
| 17 | `amount_ui` | `numeric` | [ ] | |
| 18 | `transfer_type` | `text` | [v] | |
| 19 | `asset_type` | `text` | [v] | |
| 20 | `direction` | `text` | [v] | `'unknown'::text` |
| 21 | `authority` | `text` | [ ] | |
| 22 | `instruction_idx` | `int4` | [ ] | |
| 23 | `inner_idx` | `int4` | [ ] | |

---

### 2. Таблица `transactions`
Хранит общую информацию о транзакциях.

*   **Tablespace:** `pg_default`

| # | Имя колонки | Тип данных | Not Null | Значение по умолчанию |
| :--- | :--- | :--- | :---: | :--- |
| 1 | `owner_address` | `text` | [v] | |
| 2 | `signature` | `text` | [v] | |
| 3 | `slot` | `int8` | [v] | |
| 4 | `block_time` | `int8` | [v] | |
| 6 | `fee` | `int8` | [v] | |
| 7 | `compute_units` | `int4` | [ ] | |
| 8 | `err` | `jsonb` | [ ] | |
| 9 | `num_signers` | `int4` | [ ] | |
| 10 | `num_instructions` | `int4` | [ ] | |

---

### 3. Таблица `signatures`
Вероятно, используется для отслеживания статуса обработки подписей транзакций.

*   **Tablespace:** `pg_default`

| # | Имя колонки | Тип данных | Not Null | Значение по умолчанию |
| :--- | :--- | :--- | :---: | :--- |
| 1 | `block_time` | `int8` | [v] | |
| 2 | `owner_address` | `text` | [v] | |
| 7 | `signature` | `text` | [v] | |
| 8 | `is_processed` | `bool` | [ ] | `false` |

### 2. Таблица `processing_data`

*   **Tablespace:** `pg_default`

| # | Имя колонки | Тип данных | Not Null | Значение по умолчанию |
| :--- | :--- | :--- | :---: | :--- |
| 1 | `id` | `bigserial` | [v] | `nextval('processing_data_id_seq'::regclass)` |
| 2 | `address` | `text` | [v] | |
| 3 | `day` | `date` | [v] | |
| 4 | `status` | `text` | [v] | |
| 5 | `created_at` | `timestamptz` | [v] | `now()` |
| 6 | `updated_at` | `timestamptz` | [v] | `now()` |
