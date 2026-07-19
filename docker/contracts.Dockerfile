# syntax=docker/dockerfile:1.7
FROM rust:1.80-slim AS planner
WORKDIR /workspace
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown
COPY Cargo.toml Cargo.lock ./
COPY contracts ./contracts
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo fetch --locked

FROM planner AS builder
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/workspace/target \
    cargo build --target wasm32-unknown-unknown --release --locked \
    && mkdir -p /out \
    && find /workspace/target/wasm32-unknown-unknown/release -maxdepth 1 -name '*.wasm' -exec cp {} /out/ \;

FROM scratch AS artifact
COPY --from=builder /out/ /wasm/
