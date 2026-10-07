# Reproducible build: pin the toolchain image tag; build in a clean container.
FROM rust:1.98-slim AS build

# System deps: pkg-config/libudev (serialport), libdbus (dbus-rs); sqlite and
# everything else compiles from source (bundled/pure Rust).
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       build-essential pkg-config libudev-dev libdbus-1-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .
RUN cargo build --release --workspace --features feedback-network,crash-reporting

# --- Runtime image ---------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       ca-certificates libdbus-1-3 libudev1 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /src/target/release/remote-app /usr/local/bin/
COPY --from=build /src/target/release/remote-app-headless /usr/local/bin/
COPY assets/remote-app.desktop /usr/share/applications/remote-app.desktop

ENTRYPOINT ["remote-app-headless"]
