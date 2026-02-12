# План: MVP Solana Address Daily Report за 14 дней

## MVP-спека (заморожено на 2 недели)
**Ввод:** address + day (один календарный день).

**Вывод (HTML-отчёт):**
- Total transactions
- Success / Failed (+ success rate)
- Bar chart: tx per hour (0–23)
- Total fees (SOL)
- Total SOL moved (сумма всех SOL transfer’ов)
- Total SPL moved (сумма всех SPL transfer’ов)

**Источник:** Helius. **Хранилище:** Postgres. **Ingest:** Rust (tokio, sqlx). **Web:** Python FastAPI (HTML).

Ограничение: никаких «все возможные метрики» в v1. Только список выше.

---

## Рекомендованная архитектура MVP (надёжная)
**DB-очередь:** Python создаёт запись в `report_jobs`, Rust воркер берёт `pending` (SKIP LOCKED), индексирует/догружает данные за нужный день, помечает `ready`, Python рендерит HTML из БД.

Почему так:
- не блокирует HTTP запросы долгой индексацией
- у тебя уже есть паттерн SKIP LOCKED
- легко масштабировать воркеры

---

## Day-by-day (можно сдвигать, но порядок сохранять)

### День 1 — Web skeleton + report_jobs
**Цель:** job создаётся из веба.
- `report_jobs` table + уникальность (address, day)
- FastAPI `/` форма + `POST /generate`
- `/report/{address}/{day}` отдаёт «pending» заглушку

### День 2 — Rust job worker
**Цель:** Rust берёт job и меняет статус.
- SELECT pending job FOR UPDATE SKIP LOCKED
- статус `indexing`
- адрес/day из job (без хардкода)
- статус `ready`/`error`

### День 3 — Вертикальный срез end-to-end
**Цель:** нажал Generate → дождался ready.
- Python статусная страница
- авто-обновление статуса
- smoke test на 1 адрес/день

### День 4 — Ограничение по дню (важно для MVP)
**Цель:** индексация покрывает конкретный day.
- Правило остановки пагинации signatures по block_time
- Догрузка до границ дня, чтобы отчёт был корректным

### День 5 — SQL-агрегации: core counts
**Цель:** цифры считаются корректно.
- total tx
- success/fail
- success rate

### День 6 — Tx/hour
**Цель:** готов dataset для графика 0–23.
- SQL группировка по часу
- заполнение отсутствующих часов нулями (в Python)

### День 7 — Fees + SOL moved
**Цель:** 2 ключевые финансовые метрики.
- sum fees
- sum SOL moved (вход+выход, сумма всех transfer)

### День 8 — SPL moved
**Цель:** суммарный объём SPL transfer’ов.
- sum SPL moved
- (опционально) топ-5 mint’ов

### День 9 — HTML отчёт (карточки + таблица)
**Цель:** страница уже «как продукт».
- Jinja2 шаблон
- summary cards

### День 10 — Plotly график в HTML
**Цель:** tx/hour bar chart встроен.

### День 11 — Кэш и повторные запросы
**Цель:** не переиндексировать зря.
- если ready существует — сразу показывать
- если indexing/pending — показывать статус

### День 12 — Обработка ошибок и таймауты
**Цель:** сервис не ломается при сбоях Helius.
- ретраи/лимиты
- понятное сообщение в job.error

### День 13 — Docker-compose
**Цель:** локальный запуск одной командой.
- postgres + rust-worker + python-web
- `.env.example`

### День 14 — VPS деплой + README + демо
**Цель:** публично/полупублично показать.
- VPS запуск
- README quickstart
- скриншоты/демо

---

## Буфер/правило выживания
Если отстаёшь на 2–3 дня:
1) Сначала **вертикальный срез** (Day 3) любой ценой.
2) Потом метрики в порядке важности: counts → tx/hour → fees → SOL moved → SPL moved.
3) Деплой — в конце, но не позже Day 14.