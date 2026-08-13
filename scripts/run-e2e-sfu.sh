#!/usr/bin/env bash
# 浏览器 SFU 拉流 E2E — 前置: server 容器 + Host 进程运行中
# 用法: bash scripts/run-e2e-sfu.sh [--headful]
set -euo pipefail
cd "$(dirname "$0")/.."

HEADFUL_FLAG=""
[ "${1:-}" = "--headful" ] && HEADFUL_FLAG="1"

# 从 server 日志取 admin token (bootstrap token 下一行)
TOKEN=$(docker compose -f docker-compose.dev.yml logs server 2>&1 \
  | grep -A1 "bootstrap token" | tail -1 | tr -d ' ' | sed 's/^server-1|//')
if [ -z "$TOKEN" ]; then
  echo "ERROR: 未找到 admin token — server 是否在运行?" >&2
  exit 1
fi

echo "== SFU 浏览器 E2E (headful=${HEADFUL_FLAG:-0}) =="
echo "  前置检查: server / host / vite"
for port in 9800 5173; do
  curl -s --noproxy "*" -o /dev/null "http://127.0.0.1:${port}/" \
    || { echo "ERROR: 127.0.0.1:${port} 未监听 — 先启动环境" >&2; exit 1; }
done
pgrep -x mediaservo-host > /dev/null || { echo "ERROR: Host 未运行" >&2; exit 1; }

export HEADFUL="$HEADFUL_FLAG"
node scripts/e2e-sfu-consume.cjs "$TOKEN"
