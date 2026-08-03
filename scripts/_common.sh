#!/usr/bin/env bash
# _common.sh — shared utility for AUDEMSP scripts
# Source this from other scripts: source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"
set -euo pipefail

# Guard against double-sourcing
if [ -n "${_AUDEMSP_COMMON_SH_LOADED:-}" ]; then
    return 0
fi
_AUDEMSP_COMMON_SH_LOADED=true

# SCRIPT_DIR detects the CALLER's directory (BASH_SOURCE[1]), not this file
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[1]:-${BASH_SOURCE[0]}}")" && pwd -P)"

# PROJECT_ROOT = directory containing pixi.toml
if [ -f "${SCRIPT_DIR}/pixi.toml" ]; then
    PROJECT_ROOT="$SCRIPT_DIR"
else
    PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd -P)"
fi

# OS detection
IS_MACOS=false
IS_LINUX=false
IS_WIN=false
case "$(uname -s)" in
    Darwin)  IS_MACOS=true ;;
    Linux)   IS_LINUX=true ;;
    MINGW*|MSYS*|CYGWIN*) IS_WIN=true ;;
esac

# Architecture detection
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)  ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
esac

# pixi binary path
PIXI_BIN="${PIXI_BIN:-$HOME/.pixi/bin/pixi}"

# pixi cache directory (keeps downloaded packages between installs)
PIXI_CACHE_DIR="${PIXI_CACHE_DIR:-${PROJECT_ROOT}/.pixi-cache}"
export PIXI_CACHE_DIR

# pixi install directory
PIXI_HOME="${PIXI_HOME:-$HOME/.pixi}"
export PIXI_HOME
