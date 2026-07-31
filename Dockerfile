# ---- Base: Ubuntu 22.04 LTS + Rust + system deps ----
# Ubuntu 22.04 is mediasoup's recommended prebuild base (widest glibc compatibility)
FROM ubuntu:22.04 AS base
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates build-essential pkg-config cmake ninja-build git \
    libssl-dev libuv1-dev \
    python3 python3-pip \
    && rm -rf /var/lib/apt/lists/*
# Install Rust via rustup (matches rust-toolchain.toml: stable channel)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"
# Meson for mediasoup C++ Worker
RUN pip3 install meson

# ---- Dev: full toolchain + source ----
FROM base AS dev
WORKDIR /workspace
COPY . .
RUN cargo fetch
CMD ["bash"]

# ---- Builder: release build with layer caching ----
FROM base AS builder
WORKDIR /workspace

# 1. Copy dependency manifests first (layer caching)
COPY Cargo.toml Cargo.lock ./
COPY crates/omspbase-common/Cargo.toml crates/omspbase-common/
COPY crates/omspbase-media/Cargo.toml crates/omspbase-media/
COPY crates/omspbase-webrtc/Cargo.toml crates/omspbase-webrtc/
COPY crates/omspbase-codec/Cargo.toml crates/omspbase-codec/
COPY crates/omspbase-server/Cargo.toml crates/omspbase-server/
COPY crates/omspbase-host/Cargo.toml crates/omspbase-host/
COPY crates/omspbase-client/Cargo.toml crates/omspbase-client/

# 2. Create dummy sources to build & cache dependencies
RUN mkdir -p crates/omspbase-common/src && \
    touch crates/omspbase-common/src/lib.rs && \
    mkdir -p crates/omspbase-server/src && \
    echo 'fn main() {}' > crates/omspbase-server/src/main.rs

# 3. Fetch and build dependencies (cached — only re-runs on Cargo.toml changes)
RUN cargo fetch && \
    cargo build --release --bin omspbase-server --features sfu-mediasoup

# 4. Remove dummy sources
RUN rm -rf crates/omspbase-common/src crates/omspbase-server/src

# 5. Copy real source code
COPY . .

# 6. Final build — only recompiles changed source
RUN cargo build --release --bin omspbase-server --features sfu-mediasoup

# ---- Runtime: minimal Ubuntu 22.04 ----
FROM ubuntu:22.04 AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 libuv1 ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -s /bin/bash omspbase
COPY --from=builder /workspace/target/release/omspbase-server /usr/local/bin/
USER omspbase
EXPOSE 9800 40000-40100/udp
HEALTHCHECK --interval=30s --timeout=3s CMD curl -f http://localhost:9800/health || exit 1
ENTRYPOINT ["omspbase-server"]
