#!/usr/bin/env bash
# cargo-sfu.sh — Wrapper for cargo commands that need mediasoup-sys
# Fixes: mediasoup-sys 0.13.0 tasks.py passes --buildtype AND meson.build
#        sets default_options buildtype=release, causing meson >=0.64 to error.
# Also: tasks.py overrides NINJA env var; we remove that override.
# Usage: scripts/cargo-sfu.sh check|build|test [extra cargo args...]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
source "${SCRIPT_DIR}/_common.sh"

# --- Use pixi's meson + ninja (skip mediasoup-sys pip install) ---
if [ -x "${PIXI_BIN}" ]; then
    MESON_PATH="$("${PIXI_BIN}" run -- which meson 2>/dev/null || echo "")"
    NINJA_PATH="$("${PIXI_BIN}" run -- which ninja 2>/dev/null || echo "")"
    if [ -n "${MESON_PATH}" ] && [ -x "${MESON_PATH}" ]; then
        export MESON="${MESON_PATH}"
    fi
    if [ -n "${NINJA_PATH}" ] && [ -x "${NINJA_PATH}" ]; then
        export NINJA="${NINJA_PATH}"
    fi
fi

# --- Patch mediasoup-sys tasks.py (idempotent) ---
TASKS_PY="$(find "${HOME}/.cargo/registry/src" -path "*/mediasoup-sys-*/tasks.py" 2>/dev/null | head -1)"
if [ -n "${TASKS_PY}" ]; then
    # Remove --buildtype from meson setup (default_options handles it)
    if grep -q -- "--buildtype" "${TASKS_PY}"; then
        sed -i 's/--buildtype release //g; s/--buildtype debug //g; s/--buildtype {MEDIASOUP_BUILDTYPE} //g; s/--buildtype {MEDIASOUP_BUILDTYPE\.lower()} //g' "${TASKS_PY}"
    fi
    # Override NINJA: point to pixi's ninja instead of pip-installed one
    if [ -n "${NINJA_PATH:-}" ]; then
        sed -i "s|os.environ\[\"NINJA\"] = f\"{PIP_MESON_NINJA_DIR}/bin/ninja\"|os.environ[\"NINJA\"] = \"${NINJA_PATH}\"|" "${TASKS_PY}"
        sed -i 's|os\.environ\["NINJA"\] = f"{PIP_MESON_NINJA_DIR}/bin/ninja.exe"|pass|' "${TASKS_PY}"
    fi
fi

# --- Run cargo ---
exec cargo "$@"
