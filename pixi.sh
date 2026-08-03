#!/usr/bin/env bash
# pixi.sh — Source this file to activate AUDEMSP pixi environment
# Usage: source pixi.sh

set -euo pipefail

PIXI_SH_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
source "${PIXI_SH_DIR}/scripts/pixi-shell.sh"
