#!/usr/bin/env bash
# cargo-sfu.sh — Wrapper for cargo commands that need mediasoup-sys
# Fixes: mediasoup-sys 0.13.0 tasks.py passes --buildtype AND meson.build
#        sets default_options buildtype=release, causing meson >=0.64 to error.
# Usage: scripts/cargo-sfu.sh check|build|test [extra cargo args...]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
source "${SCRIPT_DIR}/_common.sh"

# --- Use pixi's meson (skip mediasoup-sys pip install) ---
if [ -x "${PIXI_BIN}" ]; then
    MESON_PATH="$("${PIXI_BIN}" run -- which meson 2>/dev/null || echo "")"
    if [ -n "${MESON_PATH}" ] && [ -x "${MESON_PATH}" ]; then
        export MESON="${MESON_PATH}"
        echo "Using pixi meson: ${MESON}"
    fi
fi

# --- Patch mediasoup-sys tasks.py (idempotent) ---
# Remove --buildtype from meson setup commands; meson.build default_options handles it.
TASKS_PY="$(find "${HOME}/.cargo/registry/src" -path "*/mediasoup-sys-*/tasks.py" 2>/dev/null | head -1)"
if [ -n "${TASKS_PY}" ] && grep -q -- "--buildtype" "${TASKS_PY}"; then
    echo "Patching mediasoup-sys tasks.py (remove duplicate --buildtype)..."
    sed -i 's/--buildtype release //g; s/--buildtype debug //g; s/--buildtype {MEDIASOUP_BUILDTYPE} //g; s/--buildtype {MEDIASOUP_BUILDTYPE\.lower()} //g' "${TASKS_PY}"
    echo "Patched: ${TASKS_PY}"
fi

# --- Clean stale mediasoup-sys build cache ---
if [ -d "${PROJECT_ROOT}/target/debug/build" ]; then
    rm -rf "${PROJECT_ROOT}"/target/debug/build/mediasoup-sys-*
fi

# --- Run cargo ---
exec cargo "$@"
