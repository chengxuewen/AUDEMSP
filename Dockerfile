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

# ---- Builder: release build ----
FROM base AS builder
WORKDIR /workspace
COPY . .
RUN cargo build --release --bin omspbase-server

# ---- Runtime: minimal Ubuntu 22.04 ----
FROM ubuntu:22.04 AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 libuv1 ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/omspbase-server /usr/local/bin/
EXPOSE 9800 40000-40100/udp
ENTRYPOINT ["omspbase-server"]
