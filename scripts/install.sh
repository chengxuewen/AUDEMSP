#!/bin/bash
# install.sh — MediaServo Host 裸机部署脚本 (D104 Phase 1 部署机制)
# 用途: 边缘设备/车端（无 Docker 环境）安装 host-legacy 守护进程
# 依赖: 先构建二进制 (cargo build -p mediaservo-host --release) 且与 host.conf/服务文件同目录
# 注意: Phase 2 起 Docker Compose 为主要部署方式 (D110)；本脚本保留边缘部署能力
set -e

PREFIX="${PREFIX:-/opt/mediaservo}"
echo "Installing MediaServo Host to $PREFIX..."

BIN_NAME="host-legacy"

# 二进制存在性检查（防静默失败 — PIT-39 教训）
if [ ! -f "${BIN_NAME}" ]; then
    echo "ERROR: ${BIN_NAME} not found in current directory." >&2
    echo "Build first: cargo build -p mediaservo-host --release" >&2
    exit 1
fi

# Create directory structure
mkdir -p "$PREFIX/bin" "$PREFIX/etc" "$PREFIX/web" "$PREFIX/logs"

# Copy binary
cp "${BIN_NAME}" "$PREFIX/bin/"
chmod +x "$PREFIX/bin/${BIN_NAME}"

# Copy default config (don't overwrite existing)
if [ -f host.conf ]; then
    if [ ! -f "$PREFIX/etc/host.conf" ]; then
        cp host.conf "$PREFIX/etc/host.conf"
        echo "Default config created: $PREFIX/etc/host.conf"
    else
        echo "Existing config preserved: $PREFIX/etc/host.conf"
    fi
else
    echo "Note: host.conf not found next to installer; skipping config copy"
fi

# Register systemd service
if [ -f "${BIN_NAME}.service" ] && command -v systemctl &> /dev/null; then
    cp "${BIN_NAME}.service" /etc/systemd/system/
    systemctl daemon-reload
    systemctl enable "${BIN_NAME}"
    echo "systemd service registered. Edit $PREFIX/etc/host.conf then:"
    echo "  systemctl start ${BIN_NAME}"
else
    echo "No systemd unit found (or systemd unavailable). Start manually:"
    echo "  $PREFIX/bin/${BIN_NAME} --config $PREFIX/etc/host.conf"
fi

echo "Installation complete."
