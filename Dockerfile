# ---- Base: Ubuntu 22.04 LTS + Rust + system deps ----
# Ubuntu 22.04 is mediasoup's recommended prebuild base (widest glibc compatibility)
FROM ubuntu:22.04 AS base
ENV DEBIAN_FRONTEND=noninteractive

# 国内镜像加速: apt 换清华源 (PIT-31/36 教训 — 国内网络)
RUN sed -i 's|archive.ubuntu.com|mirrors.tuna.tsinghua.edu.cn|g; s|security.ubuntu.com|mirrors.tuna.tsinghua.edu.cn|g' /etc/apt/sources.list \
    && apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates build-essential pkg-config cmake ninja-build git \
    libssl-dev libuv1-dev \
    python3 python3-pip \
    && rm -rf /var/lib/apt/lists/*

# 国内镜像加速: rustup 换清华镜像
# 构建期代理 — 容器内进程（mediasoup-sys meson wrapdb / tasks.py pip）需独立代理 (PIT-19/20/33)
# 经 docker build --build-arg 或 compose args 传入，不硬编码 (PIT-20)；CI (GitHub) 无需代理，留空即可
ARG HTTP_PROXY
ARG HTTPS_PROXY
ARG NO_PROXY
ENV RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup \
    RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup \
    HTTP_PROXY=${HTTP_PROXY:-} \
    HTTPS_PROXY=${HTTPS_PROXY:-} \
    NO_PROXY=${NO_PROXY:-} \
    PIP_INDEX_URL=https://pypi.tuna.tsinghua.edu.cn/simple \
    PIP_DISABLE_PIP_VERSION_CHECK=1

# Install Rust via rustup (matches rust-toolchain.toml: stable channel)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

# 国内镜像加速: cargo crates.io 换 rsproxy sparse 镜像 (D208: tuna 不镜像二进制, rsproxy index+二进制全通)
RUN mkdir -p /root/.cargo && printf '[source.crates-io]\nreplace-with = "rsproxy-sparse"\n[source.rsproxy-sparse]\nregistry = "sparse+https://rsproxy.cn/index/"\n' > /root/.cargo/config.toml

# Meson for mediasoup C++ Worker (范围约束防意外 major 升级, 与 pixi.toml 一致; pypi 清华镜像防超时)
RUN pip3 install -i https://pypi.tuna.tsinghua.edu.cn/simple 'meson>=1.1.0,<2'

# ---- Dev: full toolchain + source ----
# D208: manifests-first — fetch 层只在 Cargo.lock/Cargo.toml 变更时失效（源码变更不再触发重复 fetch）
FROM base AS dev
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY crates/omspbase-common/Cargo.toml crates/omspbase-common/
COPY crates/omspbase-media/Cargo.toml crates/omspbase-media/
COPY crates/omspbase-webrtc/Cargo.toml crates/omspbase-webrtc/
COPY crates/omspbase-codec/Cargo.toml crates/omspbase-codec/
COPY crates/omspbase-server/Cargo.toml crates/omspbase-server/
COPY crates/omspbase-host/Cargo.toml crates/omspbase-host/
COPY crates/omspbase-client/Cargo.toml crates/omspbase-client/
# dummy src 全建 — cargo fetch 要求依赖 crate 有 targets（缺 src 报 no targets specified）
# 且 media crate 声明了 [[example]]（square-gen-egui/viewer/square-gen）→ 需 touch 对应文件
RUN mkdir -p crates/omspbase-common/src && touch crates/omspbase-common/src/lib.rs && \
    mkdir -p crates/omspbase-server/src && echo 'fn main() {}' > crates/omspbase-server/src/main.rs && \
    mkdir -p crates/omspbase-media/src crates/omspbase-webrtc/src crates/omspbase-codec/src \
             crates/omspbase-host/src crates/omspbase-client/src && \
    touch crates/omspbase-media/src/lib.rs crates/omspbase-webrtc/src/lib.rs \
          crates/omspbase-codec/src/lib.rs crates/omspbase-host/src/lib.rs crates/omspbase-client/src/lib.rs && \
    mkdir -p crates/omspbase-media/examples && touch crates/omspbase-media/examples/square-gen-egui.rs \
          crates/omspbase-media/examples/viewer.rs crates/omspbase-media/examples/square-gen.rs
RUN cargo fetch
RUN rm -rf crates/*/src
COPY . .
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

# 2. Create dummy sources to build & cache dependencies (全部 member + media [[example]] 声明文件)
RUN mkdir -p crates/omspbase-common/src && touch crates/omspbase-common/src/lib.rs && \
    mkdir -p crates/omspbase-server/src && echo 'fn main() {}' > crates/omspbase-server/src/main.rs && \
    mkdir -p crates/omspbase-media/src crates/omspbase-webrtc/src crates/omspbase-codec/src \
             crates/omspbase-host/src crates/omspbase-client/src && \
    touch crates/omspbase-media/src/lib.rs crates/omspbase-webrtc/src/lib.rs \
          crates/omspbase-codec/src/lib.rs crates/omspbase-host/src/lib.rs crates/omspbase-client/src/lib.rs && \
    mkdir -p crates/omspbase-media/examples && touch crates/omspbase-media/examples/square-gen-egui.rs \
          crates/omspbase-media/examples/viewer.rs crates/omspbase-media/examples/square-gen.rs

# 3. Fetch and build dependencies (cached — only re-runs on Cargo.toml changes)
RUN cargo fetch && \
    cargo build --release --bin omspbase-server --features sfu-mediasoup

# 4. Remove dummy sources
RUN rm -rf crates/*/src

# 5. Copy real source code
COPY . .

# 5b. 强制重编 workspace crates — COPY 保留宿主 mtime（早于 dummy 构建）→ cargo fingerprint 误判
#     源码未变，链接空 common rlib 导致 cannot find protocol 连锁错误。touch 更新 mtime 解决。
RUN find crates -name '*.rs' -exec touch {} +

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
