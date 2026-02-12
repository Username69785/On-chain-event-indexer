Информация из файла:

**token_transfers** (рекомендуемая таблица)

Основные поля:

- `id` — uuid, PK, NOT NULL
- ~~`signature`~~ — text, NOT NULL, FK на `transactions.signature`
- ~~`slot`~~ — int8, NOT NULL, сортировка по блоку
- ~~`block_time`~~ — int8, nullable (изменено с NOT NULL), время для графиков или берётся join из transactions
- ~~`owner_address`~~ — удалён
- ~~`tracked_owner`~~ — text, nullable, добавлено, заменяет owner_address, это твой owner_address если трансфер относится к отслеживаемому адресу
- ~~`direction`~~ — text/enum, nullable, добавлено, значения: `in/out/self/unknown`, относительно tracked_owner

Идентификация актива:

- ~~`asset_type`~~ — enum/text, NOT NULL, добавлено, значения: `native` или `spl`
- ~~`token_mint`~~ — text, изменено с NOT NULL в nullable, mint токена только для SPL, для SOL = NULL
- ~~`token_program`~~ — text, nullable, добавлено, SPL Token vs Token-2022
- ~~`decimals`~~ — int4, изменено с NOT NULL в nullable, для SPL; для SOL можно фиксировать 9 или NULL

Откуда → куда:

- ~~`source_owner`~~ — text, nullable, добавлено, владелец (кошелёк) отправителя
- ~~`destination_owner`~~ — text, nullable, добавлено, владелец получателя
- ~~`source_token_account`~~ — text, nullable, добавлено, для SPL: token-account отправителя
- ~~`destination_token_account`~~ — text, nullable, добавлено, для SPL: token-account получателя
- ~~`source_address`~~ — удалён, заменён source_owner / source_token_account

Примечание: для SOL заполняют source_owner/destination_owner, token_account полями будут NULL. Для SPL желательно заполнить и token_account, и owners (если owners известны).

Сумма и тип операции:

- ~~`amount_raw`~~ — numeric(40) / int8, NOT NULL, добавлено, в base units, всегда положительное, заменяет amount_change
- ~~`amount_ui`~~ — numeric, nullable, добавлено, удобно для графиков
- ~~`amount_change`~~ — удалён, заменён amount_raw
- ~~`transfer_type`~~ — enum/text, NOT NULL, добавлено, значения: `transfer/mint/burn/unknown`

Привязка к месту в транзакции:

- ~~`instruction_idx`~~ — int4, nullable, добавлено, индекс инструкции
- ~~`inner_idx`~~ — int4, nullable, добавлено, индекс inner (если CPI)
- ~~`authority`~~ — text, nullable, добавлено, кто авторизовал (часто важно для SPL)

Техполя:

- `created_at` — timestamptz, nullable, default now(), аудит/отладка

---

**transactions**

- `owner_address` — text, NOT NULL
- `signature` — text, NOT NULL
- `slot` — int8, NOT NULL
- `block_time` — int8, NOT NULL
- `fee` — int8, NOT NULL
- `compute_units` — int4, nullable
- `err` — jsonb, nullable
- `num_signers` — int4, nullable, добавлено
- `num_instructions` — int4, nullable, добавлено

---

**signatures**

Минимальная полезная схема:

- `signature` — text, NOT NULL, PK/UNIQUE
- `owner_address` — text, NOT NULL
- `block_time` — int8, NOT NULL
- `is_processed` — bool, nullable, default false (queued/processed/failed)

Убраны как не существенные: `slot`, `confirmation_status`, `err`, `created_at`
