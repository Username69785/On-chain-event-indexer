FROM rust:1.95-slim-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    clang \
    mold \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Start with just the manifests — dependencies are cached in a separate layer
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo

RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --release --locked --bin on_chain_event_indexer

# Now copy the actual code — only this is rebuilt
COPY . .

ENV SQLX_OFFLINE=true

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    touch src/main.rs \
    && cargo build --release --locked --bin on_chain_event_indexer \
    && cp target/release/on_chain_event_indexer /tmp/indexer

FROM debian:bookworm-slim
WORKDIR /app
ENV LOG_DIR=/app/logs

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /tmp/indexer ./indexer

RUN useradd --uid 10001 --create-home --shell /usr/sbin/nologin appuser \
    && mkdir -p /app/logs \
    && chown -R appuser:appuser /app

USER appuser

CMD ["./indexer"]
