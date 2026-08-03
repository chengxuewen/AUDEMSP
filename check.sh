#!/usr/bin/env bash
# check.sh — Quick SFU compile check for AUDEMSP
# Usage: bash check.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
cd "${SCRIPT_DIR}"

# Auto-detect pixi
PIXI="${HOME}/.pixi/bin/pixi"
if [ ! -x "${PIXI}" ]; then
    PIXI="$(command -v pixi 2>/dev/null || echo "")"
fi
if [ -z "${PIXI}" ]; then
    echo "ERROR: pixi not found. Run: bash bootstrap.sh"
    exit 1
fi

echo "=== Checking audemsp-server (sfu-mediasoup) ==="
bash scripts/cargo-sfu.sh check -p audemsp-server --features sfu-mediasoup 2>&1
echo ""
echo "=== Checking audemsp-server (no features) ==="
bash scripts/cargo-sfu.sh check -p audemsp-server --no-default-features 2>&1
