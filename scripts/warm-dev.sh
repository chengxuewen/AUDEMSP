#!/usr/bin/env bash
# warm-dev.sh — 后台预热 Docker dev 环境（早晨跑一次，全天零等待）
# D208 构建优化: 预热 dev 镜像层 + cargo-cache 卷
# Usage: scripts/warm-dev.sh
# 说明: 预烘焙镜像落地后此脚本退役（届时 docker compose up 直接 pull）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"

log() { echo "[warm-dev] $*"; }

# 1. 构建 dev 镜像（层缓存命中时 ~1-2 min；Cargo.lock 变更日全量 15-30 min）
log "Building dev image..."
docker compose -f "$PROJECT_ROOT/docker-compose.dev.yml" build server

# 2. 预热 target 卷：编译一次 server 依赖（首次 15-28 min，之后秒级增量）
log "Warming cargo-cache volume (first run compiles deps)..."
docker compose -f "$PROJECT_ROOT/docker-compose.dev.yml" run --rm server \
    cargo check --bin audemsp-server --features sfu-mediasoup,admin-dashboard

log "Done. Dev environment warm."
