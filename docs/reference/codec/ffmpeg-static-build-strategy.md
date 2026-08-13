# FFmpeg 静态构建与链接策略

> **决策引用**: D71 — GStreamer (Field/Host) + FFmpeg (Remote), 统一 VideoDecoder/VideoEncoder trait
> **目标**: Remote 端零运行时依赖的 FFmpeg 静态链接解码方案
> **适用 crate**: `mediaservo-codec` (Phase 2+), `mediaservo-client` decode path
> **参考**: BtbN/FFmpeg-Builds (预构建), ffmpeg-sys-next (build.rs 模式), rivet-transcoder (GPU 直连 FFI)


---

## 一、策略总览

**三个选项按复杂度递增**:

| 选项 | 复杂度 | 包体积 | 灵活性 | 适合阶段 |
|------|--------|--------|--------|---------|
| A: 引用 BtbN/FFmpeg-Builds 预构建包 | 低 | ~4 MB | 低（固定 codec 集） | Phase 1 快速集成 |
| B: mediaservo 自建预构建（本策略核心） | 中 | ~2 MB | 中（定制 codec 集） | Phase 2 正式方案 |
| C: 跟随 playa-ffmpeg 的 vcpkg 路径 | 低 | ~5 MB | 低 | 备选方案 |


```
┌─────────────────────────────────────────────────┐
│  预构建层 (CI/Docker)                            │
│  ffmpeg-build/                                   │
│  ├── Dockerfile.macos-arm64  → ffmpeg-arm64.tar  │
│  ├── Dockerfile.macos-x86_64 → ffmpeg-x86_64.tar │
│  ├── Dockerfile.linux-x86_64 → ffmpeg-linux64.tar│
│  ├── Dockerfile.linux-aarch64 → ffmpeg-arm64.tar │
│  └── Dockerfile.windows-msvc → ffmpeg-win64.tar  │
└──────────────────────┬──────────────────────────┘
                       │ 上传 CI artifacts / S3
┌──────────────────────▼──────────────────────────┐
│  cargo build 时                                  │
│  build.rs                                        │
│  ├── 检查 FFMPEG_DIR 环境变量                     │
│  ├── 未设置 → 从 CI cache 自动下载预构建包         │
│  ├── 验证 ABI: pkg-config 或 头文件探测            │
│  └── 输出 cargo: 指令 (rustc-link-search, etc.)   │
└──────────────────────┬──────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────┐
│  Rust 绑定层                                     │
│  mediaservo-codec::ffmpeg                          │
│  ├── 直接用 ffmpeg-sys/ffmpeg-next 的 FFI        │
│  └── 实现 VideoDecoder trait                     │
└─────────────────────────────────────────────────┘
```

**核心原则**:
- **不编译 FFmpeg 源码** — cargo build 零等待
- **CI 预构建为唯一真实源** — 一次构建, 所有开发者复用
- **fallback: FFMPEG_DIR 指向本地构建** — 开发者可自行编译
- **按需求裁剪** — Remote 端仅需 decode + colorspace convert

---

## 二、预构建脚本 (`ffmpeg-build/`)

### 2.1 目录结构

```
ffmpeg-build/
├── build.sh               # 入口脚本: 检测平台, 下载依赖, 调用 configure
├── configure-opts.sh      # 各平台的 configure flags
├── Dockerfile.linux       # Linux x86_64 构建容器
├── Dockerfile.linux-arm   # Linux aarch64 交叉编译容器
├── Dockerfile.macos       # macOS universal binary 构建
├── Dockerfile.windows     # Windows MSVC 交叉编译 (via mingw-w64)
├── patch/                 # 可选补丁
│   └── 0001-static-math.patch
└── README.md
```

### 2.2 最小 configure flags (按 D71 — decode only)

```bash
#!/bin/bash
# configure-opts.sh — Remote 端最小解码器集

COMMON_FLAGS=(
    # === 关闭一切 ===
    --disable-everything

    # === 仅启用解码器 ===
    --enable-decoder=h264
    --enable-decoder=hevc
    --enable-decoder=vp8
    --enable-decoder=vp9
    --enable-decoder=opus
    --enable-decoder=aac
    --enable-decoder=mp3
    --enable-decoder=pcm_s16le

    # === 仅启用解析器 ===
    --enable-parser=h264
    --enable-parser=hevc
    --enable-parser=vp8
    --enable-parser=vp9
    --enable-parser=opus
    --enable-parser=aac
    --enable-parser=mpegaudio

    # === 仅启用解封装 ===
    --enable-demuxer=h264
    --enable-demuxer=hevc
    --enable-demuxer=matroska
    --enable-demuxer=webm_dash_manifest
    --enable-demuxer=ogg
    --enable-demuxer=aac
    --enable-demuxer=mp3
    --enable-demuxer=wav

    # === 仅启用封装 (用于测试) ===
    --enable-muxer=null

    # === 协议 ===
    --enable-protocol=file
    --enable-protocol=pipe

    # === 滤镜 (色彩空间转换必须) ===
    --enable-filter=scale
    --enable-filter=format
    --enable-filter=null

    # === 不启用任何 encoder/hwaccel/postproc ===
    # (Remote 端只做解码, 无需这些)

    # === 二进制大小优化 ===
    --enable-small
    --disable-doc
    --disable-htmlpages
    --disable-manpages
    --disable-podpages
    --disable-txtpages
    --disable-ffmpeg
    --disable-ffplay
    --disable-ffprobe
    --disable-avdevice
    --disable-postproc
    --disable-swresample        # 不需要音频重采样 (WebRTC 自带)
    --disable-avfilter          # 不需要滤镜图 (颜色空间转换在 swscale)
    --disable-network           # Remote 端不解码网络流
    --disable-bsfs              # 不需要 bitstream filter

    # === 静态库 ===
    --enable-static
    --disable-shared
    --enable-pic                # 位置无关代码 (Rust 静态链接需要)
)

# === 平台特定 flags ===
case "$TARGET" in
    *linux*)
        EXTRA_FLAGS=(
            --disable-libxcb
            --disable-libxcb-shm
            --disable-libxcb-xfixes
            --disable-libxcb-shape
            --disable-xlib
            --disable-sdl2
            --disable-alsa
            --disable-pulse
        )
        ;;
    *macos*)
        EXTRA_FLAGS=(
            --disable-securetransport
            --disable-videotoolbox  # ponytail: Apple VT 用动态框架, 静态链接场景跳过
            --disable-avfoundation
            --disable-coreimage
        )
        ;;
    *windows*)
        EXTRA_FLAGS=(
            --disable-mediafoundation
            --disable-schannel
        )
        ;;
esac

ALL_FLAGS=("${COMMON_FLAGS[@]}" "${EXTRA_FLAGS[@]}")
```

### 2.3 构建脚本

```bash
#!/bin/bash
# build.sh — FFmpeg 静态构建入口
set -euo pipefail

FFMPEG_VERSION="${FFMPEG_VERSION:-7.0.2}"
FFMPEG_SRC="https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"
OUTPUT_DIR="${OUTPUT_DIR:-$PWD/out}"
JOBS="${JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)}"

echo "=== Building FFmpeg ${FFMPEG_VERSION} for ${TARGET:-native} ==="

# 1. 下载源码
if [ ! -d "ffmpeg-${FFMPEG_VERSION}" ]; then
    curl -sSL "$FFMPEG_SRC" | tar xJ
fi

cd "ffmpeg-${FFMPEG_VERSION}"

# 2. Configure
source ../configure-opts.sh
./configure \
    --prefix="$OUTPUT_DIR" \
    "${ALL_FLAGS[@]}"

# 3. Build
make -j"$JOBS"
make install

# 4. 打包
cd "$OUTPUT_DIR"
tar czf "ffmpeg-${FFMPEG_VERSION}-${TARGET}.tar.gz" lib/ include/

echo "=== Done: $OUTPUT_DIR/ffmpeg-${FFMPEG_VERSION}-${TARGET}.tar.gz ==="
```

---

## 三、GPL 合规

### 3.1 许可风险 — 关键事实

**libx264 是 GPL 许可，不是 LGPL！** 一旦 `./configure` 启用 `--enable-libx264`，FFmpeg 整套库（libavcodec, libavformat, libavutil, libswscale）全部从 LGPL 转为 **GPL 2+**，整个链接二进制被 GPL 传染。

同理：libx265 也是 GPL。ffmpeg-sys-next 通过 `build-license-gpl` feature flag 显式控制，默认不启用。

| FFmpeg 组件 | 默认许可 | --enable-libx264 后 | 影响 |
|-------------|---------|---------------------|------|
| libavcodec | LGPL 2.1+ | **GPL 2+** | 全部二进制 GPL |
| libavformat | LGPL 2.1+ | **GPL 2+** | 全部二进制 GPL |
| libavutil | LGPL 2.1+ | **GPL 2+** | 全部二进制 GPL |
| libswscale | LGPL 2.1+ | **GPL 2+** | 全部二进制 GPL |

**替代方案**: 使用 FFmpeg 内置的 H.264 解码器（`--enable-decoder=h264`，不依赖 libx264），保持 LGPL。内置解码器受专利限制（需另外考虑），但在许可层面不触发 GPL。

**结论**: 上述 `configure-opts.sh` 不启用任何 GPL 编码器，纯解码路径保持 LGPL。

### 3.2 LGPL 静态链接合规要求

根据 FSF LGPL 2.1 §6, 静态链接 LGPL 库要求:

1. **提供目标文件**: 提供 MediaServo `.o` 文件, 使最终用户可重新链接修改后的 FFmpeg
   - 可行方案: CI 中保留 `.rlib` 中间产物
2. **源码分发**: FFmpeg 源码直接引用 (版本号 + URL), 不修改的源码可以不重新分发
3. **通知**: LICENSE 文件中声明使用的 LGPL 组件及获取方式

### 3.3 推荐合规方案

```
MediaServo/
├── LICENSE           # Apache 2.0 (应用代码)
├── LICENSE.3rdparty  # 第三方许可声明
│   ├── FFmpeg 7.0.2 — LGPL 2.1+
│   ├── libwebrtc — BSD
│   └── ...
└── NOTICE            # 如何获取 FFmpeg 源码 + 如何重新链接
```

**NOTICE 模板**:
```
This product includes FFmpeg (version 7.0.2) licensed under LGPL 2.1+.
Source: https://ffmpeg.org/releases/ffmpeg-7.0.2.tar.xz
To relink against a modified FFmpeg, obtain MediaServo object files from:
  <release page URL>
and run: cargo build --target <your-target> -- FFMPEG_DIR=<path>
```

### 3.4 如果未来需要 GPL 编码器

若 Phase 3+ 需要在 Remote 端添加 libx264/libx265 编码:

- **选项 A**: 动态链接 GPL 库 (`.so`/`.dylib`) — GPL 要求动态链接不传染应用
- **选项 B**: 独立进程 (ffmpeg CLI subprocess) — 进程隔离, GPL 不传染
- **选项 C**: 全部 GPL 授权 — 不推荐

当前策略: 不解码依赖 GPL, 保持 LGPL 路径。

---

## 四、cargo build 集成

### 4.1 build.rs 设计

`mediaservo-codec/build.rs` (或 `mediaservo-client/build.rs`):

```rust
// build.rs — FFmpeg static linking detection
use std::env;
use std::path::PathBuf;

fn main() {
    if !cfg!(feature = "backend-ffmpeg") {
        return;
    }

    let ffmpeg_dir = match env::var("FFMPEG_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => {
            // 尝试从 CI cache 自动下载
            let target = env::var("TARGET").unwrap();
            let cache_dir = dirs_next().or_else(|| {
                eprintln!("cargo:warning=FFMPEG_DIR not set and no cache dir found");
                eprintln!("cargo:warning=Set FFMPEG_DIR to point to pre-built FFmpeg");
                eprintln!("cargo:warning=  e.g. FFMPEG_DIR=/opt/ffmpeg cargo build");
                std::process::exit(1);
            });
            // ponytail: CI cache lookup omitted; see ffmpeg-build/ci-download.sh
            eprintln!("cargo:warning=FFMPEG_DIR not set. Pre-built binaries not yet cached.");
            eprintln!("cargo:warning=Download from CI artifacts or build via ffmpeg-build/build.sh");
            std::process::exit(1);
        }
    };

    let lib_dir = ffmpeg_dir.join("lib");
    let include_dir = ffmpeg_dir.join("include");

    // 验证关键文件存在
    let required = [
        ("libavcodec", lib_dir.join("libavcodec.a")),
        ("libavformat", lib_dir.join("libavformat.a")),
        ("libavutil", lib_dir.join("libavutil.a")),
        ("libswscale", lib_dir.join("libswscale.a")),
    ];
    for (name, path) in &required {
        if !path.exists() {
            panic!("Missing {name} at {}", path.display());
        }
    }

    // cargo 指令
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=avcodec");
    println!("cargo:rustc-link-lib=static=avformat");
    println!("cargo:rustc-link-lib=static=avutil");
    println!("cargo:rustc-link-lib=static=swscale");

    // macOS 额外链接 (VideoToolbox 在头文件中引用但静态禁用时不需要)
    // Linux 额外链接
    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-lib=static=z");
        println!("cargo:rustc-link-lib=static=m");
        println!("cargo:rustc-link-lib=static=pthread");
    }

    // 头文件路径 (用于 bindgen 或 ffmpeg-sys)
    println!("cargo:include={}", include_dir.display());

    // 编译期验证符号存在 (见 §七)
    verify_symbols(&lib_dir);
}

fn verify_symbols(lib_dir: &PathBuf) {
    // 关键函数指针验证 — 确保 libavcodec 有 decode 能力
    let test_symbols = [
        "avcodec_find_decoder",
        "avcodec_alloc_context3",
        "avcodec_open2",
        "avcodec_send_packet",
        "avcodec_receive_frame",
        "sws_getContext",
        "sws_scale",
    ];
    // ponytail: 编译期符号验证通过 cc 编译一个小探针;
    // 详见 §七 build.rs 探针方案
    let _ = (lib_dir, test_symbols); // 占位
}
```

### 4.2 Cargo.toml feature flags

```toml
[features]
default = []
# FFmpeg 解码后端 (Remote 端静态链接)
backend-ffmpeg = []
# GStreamer 解码后端 (通用, 动态链接)
backend-gstreamer = []
```

### 4.3 代码层 feature gate

```rust
// mediaservo-codec/src/lib.rs

/// 统一解码器工厂 — 编译时选择后端
pub fn create_decoder(config: &DecodeConfig) -> Box<dyn VideoDecoder> {
    #[cfg(feature = "backend-ffmpeg")]
    {
        return Box::new(ffmpeg::FfmpegDecoder::new(config));
    }
    #[cfg(feature = "backend-gstreamer")]
    {
        return Box::new(gstreamer::GstDecoder::new(config));
    }
    #[cfg(not(any(feature = "backend-ffmpeg", feature = "backend-gstreamer")))]
    {
        compile_error!("No decoder backend selected. Enable backend-ffmpeg or backend-gstreamer.");
    }
}
```

---

## 五、CI/CD 策略

### 5.1 GitHub Actions 预构建流水线

```yaml
# .github/workflows/build-ffmpeg.yml
name: Build FFmpeg Static Libraries

on:
  push:
    paths:
      - 'ffmpeg-build/**'
      - '.github/workflows/build-ffmpeg.yml'
  workflow_dispatch:

jobs:
  build:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            dockerfile: Dockerfile.linux
            artifact: ffmpeg-linux-x86_64.tar.gz
          - target: aarch64-unknown-linux-gnu
            dockerfile: Dockerfile.linux-arm
            artifact: ffmpeg-linux-aarch64.tar.gz
          - target: x86_64-apple-darwin
            dockerfile: Dockerfile.macos
            artifact: ffmpeg-macos-x86_64.tar.gz
            runs-on: macos-13
          - target: aarch64-apple-darwin
            dockerfile: Dockerfile.macos
            artifact: ffmpeg-macos-arm64.tar.gz
            runs-on: macos-14
          - target: x86_64-pc-windows-msvc
            dockerfile: Dockerfile.windows
            artifact: ffmpeg-windows-x86_64.tar.gz

    runs-on: ${{ matrix.runs-on || 'ubuntu-latest' }}
    steps:
      - uses: actions/checkout@v4
      - name: Build FFmpeg
        run: |
          cd ffmpeg-build
          TARGET=${{ matrix.target }} ./build.sh
      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact }}
          path: ffmpeg-build/out/*.tar.gz
          retention-days: 90  # ponytail: 预构建 3 个月有效, 长期存 S3
```

### 5.2 主构建流水线 (cache 复用)

```yaml
# .github/workflows/ci.yml (片段)
jobs:
  build:
    steps:
      - uses: actions/checkout@v4
      - name: Download FFmpeg pre-built
        run: |
          # 从上次 build-ffmpeg workflow 下载最新 artifact
          # 或从 S3 bucket 下载 (长期缓存)
          mkdir -p /tmp/ffmpeg
          curl -sSL "$FFMPEG_CACHE_URL/$TARGET.tar.gz" | tar xz -C /tmp/ffmpeg
          echo "FFMPEG_DIR=/tmp/ffmpeg" >> $GITHUB_ENV
      - name: cargo build
        run: cargo build --features backend-ffmpeg
```

### 5.3 开发者本地

```bash
# 方式 1: 从 CI 下载
./ffmpeg-build/ci-download.sh linux-x86_64

# 方式 2: 本地构建 (慢, 但可自定义)
FFMPEG_VERSION=7.0.2 ./ffmpeg-build/build.sh

# 方式 3: 系统包管理器 (dev only)
brew install ffmpeg  # macOS
apt install libavcodec-dev libavformat-dev libswscale-dev  # Linux
export FFMPEG_DIR=/usr/local  # 指向系统安装

# 然后
FFMPEG_DIR=/opt/ffmpeg cargo build --features backend-ffmpeg
```

---

## 六、交叉编译

### 6.1 策略

**FFmpeg 交叉编译: 在 x86_64 CI 上, 用 Docker 交叉编译到 arm64。**

```dockerfile
# Dockerfile.linux-arm
FROM --platform=linux/arm64 ubuntu:24.04
RUN apt-get update && apt-get install -y \
    build-essential curl xz-utils \
    gcc-aarch64-linux-gnu
# 然后用 cross-compiler 构建 FFmpeg
```

```bash
# configure-opts.sh — linux-arm64 追加
--cross-prefix=aarch64-linux-gnu-
--target-os=linux
--arch=aarch64
```

### 6.2 Rust 交叉编译对齐

```
TARGET_TRIPLE              FFmpeg --target-os  --arch
=========================================================
x86_64-unknown-linux-gnu   linux               x86_64
aarch64-unknown-linux-gnu  linux               aarch64
x86_64-apple-darwin        darwin              x86_64
aarch64-apple-darwin       darwin              aarch64
x86_64-pc-windows-msvc     win32 (mingw-w64)   x86_64
```

**关键**: FFmpeg 交叉编译 `--target-os` + `--arch` 必须与 Rust `TARGET` 匹配, 否则 ABI 不兼容。

---

## 七、编译期验证

### 7.1 build.rs 符号探针

```rust
// build.rs (续) — 编译一个小 C 程序验证 FFmpeg 符号存在

fn verify_symbols(include_dir: &Path, lib_dir: &Path) {
    use std::process::Command;

    let probe_code = r#"
#include <libavcodec/avcodec.h>
#include <libavutil/frame.h>
int main() {
    // 关键: 确认 avcodec_find_decoder 可链接
    const AVCodec *c = avcodec_find_decoder(AV_CODEC_ID_H264);
    if (c) {
        // AVFrame fields exist (receive_frame 使用)
        AVFrame *f = av_frame_alloc();
        av_frame_free(&f);
        return 0;
    }
    return 1;
}
"#;

    let out_dir = env::var("OUT_DIR").unwrap();
    let probe_c = PathBuf::from(&out_dir).join("ffmpeg_probe.c");
    let probe_exe = PathBuf::from(&out_dir).join("ffmpeg_probe");

    std::fs::write(&probe_c, probe_code).unwrap();

    let status = Command::new(env::var("CC").unwrap_or_else(|_| "cc".into()))
        .arg("-o").arg(&probe_exe)
        .arg(&probe_c)
        .arg(format!("-I{}", include_dir.display()))
        .arg(format!("-L{}", lib_dir.display()))
        .arg("-lavcodec").arg("-lavutil")
        .arg("-lpthread").arg("-lz").arg("-lm")
        .status()
        .expect("failed to run cc for FFmpeg probe");

    if !status.success() {
        panic!(
            "FFmpeg symbol probe failed. Verify FFMPEG_DIR={} has valid static libs.",
            include_dir.parent().unwrap().display()
        );
    }

    // 运行探针 (可选, 确认运行时链接)
    let run_status = Command::new(&probe_exe).status()
        .expect("failed to run FFmpeg probe");
    assert!(run_status.success(), "FFmpeg probe returned non-zero");
}
```

### 7.1b ffmpeg-sys-next 的 `check_features()` 模式（高级）

```rust
/// 编译 C 探针，解析 FFmpeg #define 值，驱动 Rust #[cfg]。
/// ffmpeg-sys-next 的 build.rs 核心模式（参考实现）。
fn check_features(include_dir: &Path) {
    use std::process::Command;

    let probe_code = r"
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>

int main() {
    printf("FFMPEG_VERSION=%s\n", AV_STRINGIFY(LIBAVCODEC_VERSION));
    printf("H264_DECODER=%d\n",
        !!avcodec_find_decoder(AV_CODEC_ID_H264));
    printf("HEVC_DECODER=%d\n",
        !!avcodec_find_decoder(AV_CODEC_ID_HEVC));
    printf("VP9_DECODER=%d\n",
        !!avcodec_find_decoder(AV_CODEC_ID_VP9));
    return 0;
}
"#;

    let out = compile_and_run(probe_code, include_dir);
    // 解析 stdout 输出，设置 cargo:rustc-cfg
    for line in out.lines() {
        if line.starts_with("H264_DECODER=1") {
            println!("cargo:rustc-cfg=feature=\"h264_decode\"");
        }
        if line.starts_with("HEVC_DECODER=1") {
            println!("cargo:rustc-cfg=feature=\"hevc_decode\"");
        }
    }
}
```

这样 Rust 端可以用 `#[cfg(feature = "h264_decode")]` 编译期开关解码路径。

### 7.2 CI guard

```yaml
# ci.yml — 编译期验证作为 CI 步骤
- name: Verify FFmpeg symbols
  run: |
    FFMPEG_DIR=/tmp/ffmpeg cargo check --features backend-ffmpeg
    # check 会运行 build.rs, 触发符号探针
```

---

## 八、二进制大小优化

### 8.1 对比估算

| 场景 | 库大小 | 说明 |
|------|--------|------|
| 完整 FFmpeg (shared) | ~30 MB | 全部编解码器 |
| 完整 FFmpeg (static) | ~40 MB | 静态链接增大 |
| 最小解码器集 (static, --enable-small) | ~4 MB | --disable-everything + 最小解码器 |
| + LTO + strip | **~2 MB** | Rust target release profile |

### 8.2 recommended release profile

```toml
# Cargo.toml (mediaservo-codec 或 workspace 级)
[profile.release]
opt-level = "z"      # 优先体积 (替代 "s")
lto = true            # Rust + FFmpeg 跨库 LTO
codegen-units = 1     # 最大化 LTO 效果
strip = true          # 剥离符号
panic = "abort"       # 进一步减小体积
```

**注意**: FFmpeg 本身需要 `--enable-small --disable-debug` 编译选项才能受益。

---

## 九、参考项目分析

### 9.1 ffmpeg-next (zmwangx/rust-ffmpeg)

| 项目 | 结论 |
|------|------|
| 链接方式 | pkg-config 动态链接 (`.so`/`.dylib`), 非静态 |
| 适用性 | 不适用 MediaServo Remote 的零依赖需求 |
| 可借鉴 | feature flag 映射 codec 选择模式 (`ffmpeg_<ver>`) |

### 9.2 GStreamer (当前 Host 方案)

| 项目 | 结论 |
|------|------|
| 当前状态 | Host 端已集成 GStreamer, feature-gated |
| 局限性 | 全动态链接, 插件体系无法静态化, 不适合 Remote |
| 互补性 | Host GStreamer 编码 + Remote FFmpeg 解码 — D71 已决策 |

### 9.3 rivet-transcoder (参考)

| 项目 | 结论 |
|------|------|
| 策略 | CI 中预构建 FFmpeg, 上传到 GitHub Releases, cargo 通过环境变量引用 |
| 可借鉴 | **artifact-caching 模式**: GitHub Actions → Release → build.rs 下载 |
| 构建优化 | `--disable-everything` + 最小 codec 集, 最终 .a 约 3-5 MB |

### 9.4 playa-ffmpeg (vcpkg-first)

| 项目 | 结论 |
|------|------|
| 策略 | vcpkg 自动安装 FFmpeg（Linux/macOS/Windows），triplet 选静态链接 |
| 可借鉴 | **零配置路径**: 开发者无需设置 FFMPEG_DIR，vcpkg 自动处理一切 |
| 三平台 CI | GitHub Actions 已验证 Linux + macOS + Windows，带 vcpkg cache |
| 构建时间 | 首次 ~15min (vcpkg 下载+编译 FFmpeg)，后续 ~30s (cache hit) |
| 体积 | vcpkg 默认 full build → ~5 MB，可通过 vcpkg features 裁剪 |
| 适用性 | 快速原型阶段 — 不想管理 Docker 和 FFmpeg 构建时的备选方案 |

### 9.5 BtbN/FFmpeg-Builds（预构建标准参考）

| 项目 | 结论 |
|------|------|
| 策略 | GitHub Actions + Docker + crosstool-ng 构建**全静态** FFmpeg 二进制 |
| 覆盖 | Linux x86_64, Linux arm64, Windows x86_64 (预构建 `.tar.xz` 发布) |
| 可借鉴 | **预构建 artifact 模式**: Docker 内构建 → 上传到 GitHub Releases → build.rs 从 URL 下载 |
| 优势 | 社区维护，无需自建 Dockerfile；可直接引用作为选项 A |
| 劣势 | 固定 codec 集（全功能），体积约 30 MB；不提供 macOS target |

### 9.6 对比矩阵

| 维度 | ffmpeg-next (source) | playa-ffmpeg (vcpkg) | BtbN Builds | 本策略 (自建) |
|------|---------------------|---------------------|-------------|--------------|
| 构建方式 | git clone + make | vcpkg install | 预构建下载 | Docker + make |
| build.rs 复杂度 | 67KB（含完整构建器） | 5.6KB（vcpkg 调度） | N/A（需要自建） | ~2KB（探测+验证） |
| 首次构建耗时 | ~10min（源码编译） | ~15min（vcpkg编译） | ~30s（下载） | ~30s（下载预构建） |
| 预构建体积 | N/A（自编译） | ~5 MB | ~30 MB | ~2-4 MB |
| 交叉编译 | 一等公民（iOS/Android） | vcpkg triplet | 仅 Linux/Windows | Docker cross-compile |
| macOS 支持 | ✅ | ✅ | ❌（需自建） | ✅（GitHub Actions macos runner） |
| Windows MSVC | ✅ | ✅（最佳） | ✅（mingw-w64） | ✅ |
| Codec 定制 | ✅（完全自定义） | ✅（vcpkg features） | ❌（全功能） | ✅（最小集） |
| GPL 控制 | ✅（feature flag） | ❌（vcpkg 端口默认可能包含） | ❌ | ✅（configure 控制） |

---

## 十、实施路线图

| Phase | 任务 | 产出 | 工作量 |
|-------|------|------|--------|
| **0** | FFmpeg 预构建脚本 + Dockerfiles | `ffmpeg-build/` 目录 | 2-3 天 |
| **0** | CI 流水线: build-ffmpeg.yml | `.github/workflows/` | 1 天 |
| **1** | `mediaservo-codec` crate 骨架 | `crates/mediaservo-codec/` | 2 天 |
| **1** | build.rs (FFmpeg 检测+符号验证) | `build.rs` | 1 天 |
| **1** | VideoDecoder trait 的 FFmpeg 实现 | `codec/ffmpeg_decoder.rs` | 3 天 |
| **2** | Remote Client 集成解码 | `mediaservo-client` decode path | 2 天 |
| **2** | 二进制大小优化 + 基准测试 | release profile tuning | 1 天 |
| **3** | GPL 编码器支持 (如需) | 动态链接 或 子进程 | TBD |

---

## 十一、风险与缓解

| 风险 | 概率 | 缓解 |
|------|------|------|
| FFmpeg ABI 不稳定 (major 版本) | 中 | 锁定版本号 (`FFMPEG_VERSION=7.0.2`), CI 控制 |
| 预构建包过大 (>10 MB) 影响 clone | 低 | `--enable-small` + 最小 codec 集 → ~4 MB |
| LGPL 静态链接合规问题 | 中 | NOTICE 文件 + 保留 .rlib 中间产物 |
| macOS 签名问题 (静态库) | 低 | `.a` 文件不需要签名, 最终二进制由 Xcode 签名 |
| 交叉编译 ABI 不匹配 | 中 | Docker 隔离构建, TARGET 严格对齐 (见 §6.2) |
| 系统依赖 (libm, libz, libpthread) | 低 | 这些是系统库, 所有目标平台均有 |

---

**ponytail 摘要**: 
- 预构建 → build.rs 探测 → 静态链接 → 零运行时依赖
- 跳过: 实时编译 FFmpeg, vcpkg (体积大), 全功能构建
- Remote 端仅 decode, 不引入 encoder
- GPL 边界: 保持 LGPL (**libx264 = GPL，绝对不启用**)
- 快速开始: 选项 A (BtbN/FFmpeg-Builds 预构建) → 选项 B (自建) → 选项 C (vcpkg)

---

## 附录: 参考资源

- ffmpeg-sys-next build.rs: https://github.com/russelltg/rust-ffmpeg-sys
- playa-ffmpeg vcpkg 集成: https://github.com/ssoj13/playa-ffmpeg
- BtbN/FFmpeg-Builds 预构建: https://github.com/BtbN/FFmpeg-Builds
- rivet-transcoder GPU 架构: https://github.com/rivet-transcoder/rivet
- FFmpeg 官方 legal: https://www.ffmpeg.org/legal.html
- LGPL 合规清单: FSF LGPL 2.1 §6a-b (静态链接条款)
