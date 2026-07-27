#!/usr/bin/env bash
# scan-hardcode.sh — grep-based scanner for hardcoded ports, URLs, and secrets
# Usage: ./scripts/scan-hardcode.sh [directory]
set -euo pipefail

TARGET="${1:-.}"
SEARCH_DIRS=("${TARGET}/crates" "${TARGET}/docs")
EXCLUDE_DIRS=("target" "node_modules" ".git" ".pixi-cache")

# Build exclude args for grep
EXCLUDE_ARGS=()
for d in "${EXCLUDE_DIRS[@]}"; do
    EXCLUDE_ARGS+=(--exclude-dir="$d")
done

# Colors
RED='\033[0;31m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

found_any=false

scan() {
    local pattern="$1"
    local label="$2"
    local severity="$3"
    local color="$4"

    local matches
    # ponytail: scan only source dirs, skip generated/target
    matches=$(grep -rnI "${EXCLUDE_ARGS[@]}" --include='*.rs' --include='*.toml' --include='*.sh' --include='*.ts' --include='*.json' --include='*.md' "$pattern" "${SEARCH_DIRS[@]}" 2>/dev/null || true)

    if [ -n "$matches" ]; then
        found_any=true
        echo -e "\n${color}=== $severity: $label ===${NC}"
        echo "$matches" | while IFS= read -r line; do
            local file=$(echo "$line" | cut -d: -f1)
            local lnum=$(echo "$line" | cut -d: -f2)
            echo -e "  ${color}${file}:${lnum}${NC}"
        done
    fi
}

echo "=== Hardcoded Values Scanner ==="
echo "Scanning: $TARGET"
echo ""

# CRITICAL: hardcoded secrets
scan 'token\s*=\s*"[^"]+' "Hardcoded tokens/secrets" "CRITICAL" "$RED"
scan 'password\s*=\s*"[^"]+' "Hardcoded passwords" "CRITICAL" "$RED"
scan 'secret\s*=\s*"[^"]+' "Hardcoded secrets" "CRITICAL" "$RED"
scan 'api[_-]?key\s*=\s*"[^"]+' "Hardcoded API keys" "CRITICAL" "$RED"

# HIGH: hardcoded ports — common patterns
scan ':9800\b' "Hardcoded port 9800" "HIGH" "$YELLOW"
scan 'localhost:[0-9]+' "localhost with hardcoded port" "HIGH" "$YELLOW"
scan '127\.0\.0\.1:[0-9]+' "127.0.0.1 with hardcoded port" "HIGH" "$YELLOW"
scan '0\.0\.0\.0:[0-9]+' "0.0.0.0 with hardcoded port" "HIGH" "$YELLOW"

# MEDIUM: hardcoded URLs that may change
scan 'http://[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' "Hardcoded IP URLs" "MEDIUM" "$CYAN"

if [ "$found_any" = false ]; then
    echo "No hardcoded values found."
fi
