#!/usr/bin/env bash
# docker-cargo.sh — run cargo inside mediasoup Docker dev container
# Usage: scripts/docker-cargo.sh check|build|test [extra cargo args...]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"

# Build dev image if not cached
docker compose -f "$PROJECT_ROOT/docker-compose.dev.yml" build dev

# Run cargo inside container
exec docker compose -f "$PROJECT_ROOT/docker-compose.dev.yml" run --rm dev cargo "$@"
