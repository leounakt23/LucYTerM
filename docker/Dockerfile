# syntax=docker/dockerfile:1.7
FROM rust:1.85.1-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential pkg-config libudev-dev libdbus-1-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release --package remote-app --features feedback-network,crash-reporting \
    && cp target/release/remote-app target/release/remote-app-headless /tmp/

FROM debian:bookworm-slim

LABEL org.opencontainers.image.source="https://github.com/LucYTerM/mbxt" \
      org.opencontainers.image.description="Remote App headless client"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libdbus-1-3 libudev1 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 remote-app

COPY --from=builder /tmp/remote-app /usr/local/bin/remote-app
COPY --from=builder /tmp/remote-app-headless /usr/local/bin/remote-app-headless

USER remote-app
ENTRYPOINT ["remote-app-headless"]
