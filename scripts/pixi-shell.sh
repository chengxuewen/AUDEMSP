#!/usr/bin/env bash
# pixi-shell.sh — Activate pixi environment in current shell
# Source this file: source scripts/pixi-shell.sh
# Do NOT run as a subshell (./pixi-shell.sh) — env vars would be lost.

set -euo pipefail

SCRIPT_DIR_PSHELL="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
source "${SCRIPT_DIR_PSHELL}/_common.sh"

echo "Activating OMSPBase pixi environment..."
# ponytail: conda completion scripts reference ZSH_VERSION etc. — disable nounset during eval
set +u
eval "$("${PIXI_BIN}" shell-hook --manifest-path "${PROJECT_ROOT}/pixi.toml" --shell bash)"
set -u

echo ""
echo "OMSPBase environment active."
echo "  pixi run build     — cargo build --workspace"
echo "  pixi run test      — cargo test --workspace"
echo "  pixi run lint      — cargo clippy + fmt check"
echo "  pixi run check     — cargo check"
echo ""
echo "Deactivate with: exit  (or close this shell)"

# Set DYLD_LIBRARY_PATH for GStreamer runtime linking on macOS
export DYLD_LIBRARY_PATH="${CONDA_PREFIX}/lib:${DYLD_LIBRARY_PATH:-}"
