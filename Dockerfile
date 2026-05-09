FROM rust:1.95-slim-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y \
    clang \
    lld \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Start with just the manifests — dependencies are cached in a separate layer
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

# Now copy the actual code — only this is rebuilt
COPY . .
ENV SQLX_OFFLINE=true
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/on_chain_event_indexer ./indexer
COPY migrations ./migrations
CMD ["./indexer"]