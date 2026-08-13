#!/usr/bin/env bash
# check.sh — Quick SFU compile check for MediaServo
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

echo "=== Checking mediaservo-server (sfu-mediasoup) ==="
bash scripts/docker-cargo.sh check -p mediaservo-server --features sfu-mediasoup 2>&1
echo "=== Checking mediaservo-server (no features) ==="
bash scripts/docker-cargo.sh check -p mediaservo-server --no-default-features 2>&1
