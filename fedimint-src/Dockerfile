# syntax=docker/dockerfile:1
# ── Stage 1: Build fedimintd ─────────────────────────────────────────────────
FROM rust:1.88-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake \
    make \
    protobuf-compiler \
    pkg-config \
    libssl-dev \
    clang \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt

WORKDIR /src
COPY . .

# Clear macOS-specific paths set in .cargo/config.toml [env] section.
# On Linux these paths don't exist; the system headers are found automatically.
ENV CPLUS_INCLUDE_PATH="" \
    CXXFLAGS="" \
    CFLAGS=""

# BuildKit cache mounts keep the cargo registry and build artifacts between
# docker build runs, making incremental rebuilds much faster.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/tmp/hellas-falcon-build \
    cargo build --release --bin fedimintd \
    && cp /tmp/hellas-falcon-build/release/fedimintd /fedimintd

# ── Stage 2: Minimal runtime image ───────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /fedimintd /usr/local/bin/fedimintd

EXPOSE 8173 8174 8175 8176

ENTRYPOINT ["fedimintd"]
