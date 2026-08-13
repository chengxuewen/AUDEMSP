#!/usr/bin/env bash
# build-server.sh — Build + start MediaServo server via Docker dev compose
# Usage: bash scripts/build-server.sh [build|up|down|logs|check]
set -euo pipefail

COMPOSE="docker compose -f docker-compose.dev.yml"
ACTION="${1:-build}"

case "$ACTION" in
  build)
    echo "=== Building dev image (first build: 15-30min) ==="
    $COMPOSE build
    ;;
  up)
    echo "=== Starting server ==="
    $COMPOSE up -d
    echo "=== Health check ==="
    sleep 3
    curl -s http://localhost:9800/health || echo "server not ready yet"
    ;;
  down)
    $COMPOSE down
    ;;
  logs)
    $COMPOSE logs -f
    ;;
  check)
    echo "=== docker check ==="
    docker compose version
    docker compose -f docker-compose.dev.yml config --quiet && echo "compose config OK"
    ;;
  *)
    echo "Usage: $0 [build|up|down|logs|check]"
    exit 1
    ;;
esac
