#!/usr/bin/env bash
# bootstrap.sh — First-time setup for AUDEMSP development
# Usage: source bootstrap.sh
#
# This is the user-facing entry point. Run once per machine:
#   source bootstrap.sh
# After initial setup, use:
#   source pixi.sh
set -euo pipefail

START_TIME=$(date +%s)
BOOTSTRAP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"

echo "================================================"
echo "  AUDEMSP Development Environment Bootstrap"
echo "================================================"
echo ""

# --- Source common config ---
source "${BOOTSTRAP_DIR}/scripts/_common.sh"

# --- Detect pre-installed pixi ---
pixi_needs_install=true
if command -v pixi &>/dev/null; then
    pixi_ver="$(pixi --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "unknown")"
    echo "Found pixi ${pixi_ver} in PATH: $(command -v pixi)"
    pixi_needs_install=false
elif [ -x "${PIXI_BIN}" ]; then
    pixi_ver="$("${PIXI_BIN}" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "unknown")"
    echo "Found pixi ${pixi_ver} at ${PIXI_BIN}"
    pixi_needs_install=false
fi

if $pixi_needs_install; then
    # Step 1: Install pixi + project dependencies
    echo "[1/2] Installing pixi and project dependencies..."
    bash "${BOOTSTRAP_DIR}/scripts/pixi-init.sh"
else
    # Step 1: Only install project dependencies
    echo "[1/2] Installing project dependencies..."
    cd "${PROJECT_ROOT}"
    "${PIXI_BIN}" install --manifest-path "${PROJECT_ROOT}/pixi.toml" || true
fi

echo ""

# Step 2: Activate pixi environment
echo "[2/2] Activating pixi environment..."
source "${BOOTSTRAP_DIR}/scripts/pixi-shell.sh"

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

echo ""
echo "================================================"
echo "  AUDEMSP environment ready! (${ELAPSED}s)"
echo "================================================"
echo ""
echo "Next time, just run:  source pixi.sh"
echo "CLI ready: ./audemsp.sh -h   (build/up/e2e/clean/config/status...)"
