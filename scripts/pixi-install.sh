#!/usr/bin/env bash
# pixi-install.sh — Install pixi package manager
# Usage: scripts/pixi-install.sh
# Env vars: PIXI_VERSION (default: latest), PIXI_HOME (default: ~/.pixi)
#           HTTP_PROXY / HTTPS_PROXY (optional, for corporate networks)
set -euo pipefail

SCRIPT_DIR_PINSTALL="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
source "${SCRIPT_DIR_PINSTALL}/_common.sh"

# --- Config ---
PIXI_VERSION="${PIXI_VERSION:-latest}"
FALLBACK_VERSION="0.74.0"
CACHE_DIR="${PIXI_CACHE_DIR}/downloads"

# curl flags: force HTTP/1.1 (fixes HTTP/2 PROTOCOL_ERROR behind some proxies/CDNs)
CURL="curl --http1.1 -# -fSL --connect-timeout 30 --max-time 300"

# --- Already installed? ---
if [ -x "${PIXI_BIN}" ]; then
    installed_ver="$("${PIXI_BIN}" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "")"
    if [ "$PIXI_VERSION" = "latest" ]; then
        echo "pixi ${installed_ver} already installed at ${PIXI_BIN}"
        exit 0
    elif [ "$installed_ver" = "$PIXI_VERSION" ]; then
        echo "pixi ${installed_ver} already installed at ${PIXI_BIN}"
        exit 0
    fi
    echo "pixi version mismatch: ${installed_ver} (want ${PIXI_VERSION}), reinstalling..."
fi

mkdir -p "$(dirname "${PIXI_BIN}")" "${CACHE_DIR}"

# --- Resolve download version + platform ---
download_ver="${PIXI_VERSION}"
if [ "$download_ver" = "latest" ]; then
    download_ver="$FALLBACK_VERSION"
fi

os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"
case "$os" in
    darwin) os_target="apple-darwin" ;;
    linux)  os_target="unknown-linux-musl" ;;
    *)      echo "ERROR: unsupported OS: $os"; exit 1 ;;
esac
case "$arch" in
    x86_64)          arch_target="x86_64" ;;
    aarch64|arm64)   arch_target="aarch64" ;;
    *)               echo "ERROR: unsupported arch: $arch"; exit 1 ;;
esac

fname="pixi-${arch_target}-${os_target}.tar.gz"
cached_tarball="${CACHE_DIR}/${fname}"

# --- Step 1: Check cache FIRST (zero network) ---
if [ -f "${cached_tarball}" ] && [ -s "${cached_tarball}" ]; then
    echo "Using cached tarball: ${cached_tarball}"
    tar xzf "${cached_tarball}" -C "$(dirname "${PIXI_BIN}")"
    chmod +x "${PIXI_BIN}"
    "${PIXI_BIN}" --version
    echo "pixi installed from cache at ${PIXI_BIN}"
    exit 0
fi

# --- Step 2: Try official installer ---
echo "Installing pixi ${download_ver} (via pixi.sh)..."
installer_script="${CACHE_DIR}/pixi-install.sh"
if [ ! -f "${installer_script}" ]; then
    $CURL "https://pixi.sh/install.sh" -o "${installer_script}" 2>/dev/null || true
fi

if [ -f "${installer_script}" ] && [ -s "${installer_script}" ]; then
    if PIXI_VERSION="$download_ver" PIXI_HOME="${PIXI_HOME:-$HOME/.pixi}" \
        bash "${installer_script}" 2>/dev/null; then
        "${PIXI_BIN}" --version
        echo "pixi installed at ${PIXI_BIN}"
        exit 0
    fi
    echo "Official installer failed, trying direct download..."
fi

# --- Step 3: Direct GitHub release download (with mirror) ---
github_url="https://github.com/prefix-dev/pixi/releases/download/v${download_ver}/${fname}"
mirror_url="https://mirror.ghproxy.com/${github_url}"

echo "Downloading pixi ${download_ver}..."
if $CURL "$github_url" -o "${cached_tarball}" 2>/dev/null; then
    echo "Downloaded from GitHub"
elif $CURL "$mirror_url" -o "${cached_tarball}" 2>/dev/null; then
    echo "Downloaded from mirror (ghproxy.com)"
else
    rm -f "${cached_tarball}"
    echo "ERROR: failed to download pixi ${download_ver}"
    echo "Try: export HTTPS_PROXY=http://your-proxy:port"
    exit 1
fi

tar xzf "${cached_tarball}" -C "$(dirname "${PIXI_BIN}")"
chmod +x "${PIXI_BIN}"

"${PIXI_BIN}" --version
echo "pixi ${download_ver} installed at ${PIXI_BIN}"
echo "Tarball cached at: ${cached_tarball}"
