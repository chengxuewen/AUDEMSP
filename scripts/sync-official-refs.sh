#!/usr/bin/env bash
# sync-official-refs.sh — 拉取/更新官方 mediasoup 客户端参考源码（跨平台/跨主机开发用）
#
# 用途: 为 AI 分析参考提供稳定的官方源码基准（config 目录 .refinfo/，git-ignored）。
#       不依赖 /tmp 临时目录（跨主机/重启丢失），浅克隆 (--depth 1) 只取源码不要历史。
#
# 仓库: 仅 3 个官方客户端基准（协商流程的唯一权威对照）。
#       - mediasoup-client (JS)     — Chrome74.ts send(): 标准 produce 协商流程官方基准
#       - libmediasoupclient (C++)   — Handler.cpp Send(): 官方 C++ 客户端基准
#       - mediasoup-demo            — RoomClient.js: 官方 demo 端到端流程
#       刻意排除: mediasoup(server, 只调不改) 与 mediasoup-client-android(第三方 fork, 无关)。
#
# 网络: 国内 GitHub 干扰 (PIT-14) → 优先 git 原生, 失败回退 --http1.1 / 代理。
#
# 幂等: 目录已存在则跳过 (不覆盖), 可用 --force 强制重拉。
# 用法: bash scripts/sync-official-refs.sh [--force]

set -euo pipefail

REFS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/.refinfo"
FORCE=0
[[ "${1:-}" == "--force" ]] && FORCE=1

# url|dir  (git clone <url> <refs_dir>/<dir>)
REPOS=(
  "https://github.com/versatica/mediasoup-client   mediasoup-client"
  "https://github.com/versatica/libmediasoupclient  libmediasoupclient"
  "https://github.com/versatica/mediasoup-demo      mediasoup-demo"
)

mkdir -p "$REFS_DIR"

clone_with_fallback() {
  local url="$1" dir="$2"
  local target="$REFS_DIR/$dir"
  if [[ -d "$target/.git" && $FORCE -eq 0 ]]; then
    echo "✓ $dir — 已存在, 跳过 (--force 强制重拉)"
    return 0
  fi
  if [[ -d "$target/.git" ]]; then
    echo "→ $dir — 强制重拉"
    rm -rf "$target"
  fi
  echo "→ 拉取 $dir ..."
  # 尝试 git 原生, 失败回退 --http1.1 (PIT-14: HTTP/2 被干扰)
  if git clone --depth 1 --single-branch "$url" "$target" 2>/dev/null; then
    echo "  ✓ $dir 完成"
  elif git clone --depth 1 --single-branch --config http.version=HTTP/1.1 "$url" "$target" 2>/dev/null; then
    echo "  ✓ $dir 完成 (http1.1 回退)"
  else
    echo "  ✗ $dir 拉取失败 (网络问题, 参考 PIT-14: 设 HTTPS_PROXY 或重试)"
    return 1
  fi
}

fail=0
for entry in "${REPOS[@]}"; do
  url="${entry%% *}"
  dir="${entry##* }"
  clone_with_fallback "$url" "$dir" || fail=1
done

echo ""
echo "── 参考源码目录 ──────────────────────────────"
echo "  $REFS_DIR"
for entry in "${REPOS[@]}"; do
  dir="${entry##* }"
  if [[ -d "$REFS_DIR/$dir/.git" ]]; then
    commit="$(git -C "$REFS_DIR/$dir" rev-parse --short HEAD 2>/dev/null || echo '?')"
    echo "  • $dir @ $commit"
  fi
done
echo "  导航: docs/reference/webrtc/mediasoup-refs.md"
echo "  清理: rm -rf $REFS_DIR"
echo "──────────────────────────────────────────────"

[[ $fail -eq 0 ]] || echo "警告: 部分仓库拉取失败, 请检查网络后重试"
exit $fail