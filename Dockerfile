# ---- Base: Ubuntu 22.04 LTS + Rust + system deps ----
# Ubuntu 22.04 is mediasoup's recommended prebuild base (widest glibc compatibility)
FROM ubuntu:22.04 AS base
ENV DEBIAN_FRONTEND=noninteractive

# 国内镜像加速: apt 换清华源 (PIT-31/36 教训 — 国内网络)
RUN sed -i 's|archive.ubuntu.com|mirrors.tuna.tsinghua.edu.cn|g; s|security.ubuntu.com|mirrors.tuna.tsinghua.edu.cn|g' /etc/apt/sources.list \
    && apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates build-essential pkg-config cmake ninja-build git \
    libssl-dev libuv1-dev \
    libglib2.0-dev libclang-dev \
    libglib2.0-dev \
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
RUN apt-get update && apt-get install -y --no-install-recommends gdb && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY crates/audemsp-common/Cargo.toml crates/audemsp-common/
COPY crates/audemsp-media/Cargo.toml crates/audemsp-media/
COPY crates/audemsp-webrtc/Cargo.toml crates/audemsp-webrtc/
COPY crates/audemsp-codec/Cargo.toml crates/audemsp-codec/
COPY crates/audemsp-server/Cargo.toml crates/audemsp-server/
COPY crates/audemsp-host/Cargo.toml crates/audemsp-host/
COPY crates/audemsp-client/Cargo.toml crates/audemsp-client/
# dummy src 全建 — cargo fetch 要求依赖 crate 有 targets（缺 src 报 no targets specified）
# 且 media crate 声明了 [[example]]（square-gen-egui/viewer/square-gen）→ 需 touch 对应文件
RUN mkdir -p crates/audemsp-common/src && touch crates/audemsp-common/src/lib.rs && \
    mkdir -p crates/audemsp-server/src && echo 'fn main() {}' > crates/audemsp-server/src/main.rs && \
    mkdir -p crates/audemsp-media/src crates/audemsp-webrtc/src crates/audemsp-codec/src \
             crates/audemsp-host/src crates/audemsp-client/src && \
    touch crates/audemsp-media/src/lib.rs crates/audemsp-webrtc/src/lib.rs \
          crates/audemsp-codec/src/lib.rs crates/audemsp-host/src/lib.rs crates/audemsp-client/src/lib.rs && \
    mkdir -p crates/audemsp-media/examples && touch crates/audemsp-media/examples/square-gen-egui.rs \
          crates/audemsp-media/examples/viewer.rs crates/audemsp-media/examples/square-gen.rs
RUN cargo fetch
RUN rm -rf crates/*/src
COPY . .
CMD ["bash"]

# ---- Builder: release build with layer caching ----
FROM base AS builder
WORKDIR /workspace

# 1. Copy dependency manifests first (layer caching)
COPY Cargo.toml Cargo.lock ./
COPY crates/audemsp-common/Cargo.toml crates/audemsp-common/
COPY crates/audemsp-media/Cargo.toml crates/audemsp-media/
COPY crates/audemsp-webrtc/Cargo.toml crates/audemsp-webrtc/
COPY crates/audemsp-codec/Cargo.toml crates/audemsp-codec/
COPY crates/audemsp-server/Cargo.toml crates/audemsp-server/
COPY crates/audemsp-host/Cargo.toml crates/audemsp-host/
COPY crates/audemsp-client/Cargo.toml crates/audemsp-client/

# 2. Create dummy sources to build & cache dependencies (全部 member + media [[example]] 声明文件)
RUN mkdir -p crates/audemsp-common/src && touch crates/audemsp-common/src/lib.rs && \
    mkdir -p crates/audemsp-server/src && echo 'fn main() {}' > crates/audemsp-server/src/main.rs && \
    mkdir -p crates/audemsp-media/src crates/audemsp-webrtc/src crates/audemsp-codec/src \
             crates/audemsp-host/src crates/audemsp-client/src && \
    touch crates/audemsp-media/src/lib.rs crates/audemsp-webrtc/src/lib.rs \
          crates/audemsp-codec/src/lib.rs crates/audemsp-host/src/lib.rs crates/audemsp-client/src/lib.rs && \
    mkdir -p crates/audemsp-media/examples && touch crates/audemsp-media/examples/square-gen-egui.rs \
          crates/audemsp-media/examples/viewer.rs crates/audemsp-media/examples/square-gen.rs

# 3. Fetch and build dependencies (cached — only re-runs on Cargo.toml changes)
RUN cargo fetch && \
    cargo build --release --bin audemsp-server --features sfu-mediasoup
# 4. Remove dummy sources
RUN rm -rf crates/*/src

# 5. Copy real source code
COPY . .

# 5b. 强制重编 workspace crates — COPY 保留宿主 mtime（早于 dummy 构建）→ cargo fingerprint 误判
#     源码未变，链接空 common rlib 导致 cannot find protocol 连锁错误。touch 更新 mtime 解决。
RUN find crates -name '*.rs' -exec touch {} +

# 5c. PIT-23: admin dist 必须在 cargo build 前构建（build.rs 依赖 www/apps/admin/dist 存在）
#     dist 是 gitignore 产物（不在仓库）→ 容器内必须现场构建；否则 build.rs 回退
#     ADMIN_DIST_DIR=/nonexistent → /admin 运行时 404。Node 20 + pnpm 10。
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - && \
    apt-get install -y --no-install-recommends nodejs && \
    npm install -g pnpm@10.32.1 && \
    cd www && pnpm install --frozen-lockfile && pnpm build:admin && \
    cd / && rm -rf /workspace/www/node_modules

# 6. Final build — only recompiles changed source（含正确 ADMIN_DIST_DIR）
RUN cargo build --release --bin audemsp-server --features sfu-mediasoup

# ---- Runtime: minimal Ubuntu 22.04 ----
FROM ubuntu:22.04 AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 libuv1 ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -s /bin/bash audemsp
COPY --from=builder /workspace/target/release/audemsp-server /usr/local/bin/
USER audemsp
EXPOSE 9800 40000-40100/udp
HEALTHCHECK --interval=30s --timeout=3s CMD curl -f http://localhost:9800/health || exit 1
ENTRYPOINT ["audemsp-server"]
