#!/usr/bin/env bash
# C++ header-only 绑定测试: 编译并运行 field/link/deck 三 SDK 测试程序。
# 前置: 三个 cdylib 已构建（pixi run build-c，含 .so.0 dev symlink）。
set -euo pipefail
cd "$(dirname "$0")/.."

export PATH="$HOME/.pixi/bin:$PATH"
export LD_LIBRARY_PATH="$PWD/target/debug${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

for sdk in field link deck; do
    echo "=== $sdk-cxx ==="
    g++ -std=c++17 -Wall -Wextra \
        -I "bindings/cxx/mediaservo-$sdk-cxx/include" \
        -I "bindings/c/mediaservo-$sdk-c/include" -I bindings/c/include \
        "bindings/cxx/mediaservo-$sdk-cxx/tests/test_$sdk.cpp" \
        -L target/debug -lmediaservo_$sdk -o "/tmp/opencode/test_${sdk}_cxx"
    "/tmp/opencode/test_${sdk}_cxx"
    echo "$sdk-cxx tests PASS"
done
