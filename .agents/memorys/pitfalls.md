# OMSPBase Pitfalls & Gotchas

## PIT-01: macOS -ObjC linker flag (2026-07-20)

**症状**: `cargo run --example webrtc_loopback_egui --features backend-webrtc-sys` 编译成功但运行崩溃:
```
NSInvalidArgumentException: -[__NSCFConstantString webrtc:: capitalizationStyle]: unrecognized selector sent to instance
```

**根因**: libwebrtc 内部使用 Objective-C categories (NSString+StdString)，macOS 链接器默认会 dead-strip 未被显式引用的 category 方法。`cxx` crate 的 ObjC++ bridge 同样依赖 category 方法。

**解法**: `.cargo/config.toml`:
```toml
[target.x86_64-apple-darwin]
rustflags = ["-C", "link-args=-ObjC -Wl,-no_compact_unwind"]

[target.aarch64-apple-darwin]
rustflags = ["-C", "link-args=-ObjC -Wl,-no_compact_unwind"]
```

`-ObjC` 强制链接器保留所有 ObjC categories，`-no_compact_unwind` 修复 libwebrtc 中 zero-size C++ exception frames 的兼容性问题。

**来源**: webrtc-kit 的 `.cargo/config.toml`。

## PIT-02: webrtc-sys build hangs on macOS without explicit target (2026-07-20)

**症状**: `cargo check --features backend-webrtc-sys` 在 webrtc-sys crate resolution 阶段挂起/超时。

**根因**: webrtc-sys build.rs 触发 libwebrtc 预编译二进制下载 (~200MB)，首次下载耗时较长。在某些网络环境下超时。

**解法**: 
1. 确保网络畅通，首次构建容忍 5-10 分钟
2. 考虑为 CI 添加 `--target` 显式指定
3. 使用 stub backend (`cargo check` 无 features) 快速迭代

## PIT-03: cxx::SharedPtr borrow checker constraints (2026-07-20)

**症状**: webrtc-sys 类型为 `cxx::SharedPtr<T>`，不能跨线程自由传递，需要 `impl_thread_safety!` 宏标记 Send+Sync。

**解法**: webrtc-sys 已通过 `impl_thread_safety!` 标记 PeerConnection/PeerConnectionFactory/DataChannel/SessionDescription 为 Send+Sync。callback-based API 的 ctx 使用 `Box<PeerContext(Box<dyn Any+Send>)>` 传递状态跨 FFI 边界。

## PIT-04: webrtc-rs + webrtc-sys mutual exclusion must be compile_error! (2026-07-20)

**症状**: 同时启用 `backend-webrtc-rs` 和 `backend-webrtc-sys` features 导致 type alias 冲突（两个 backend 都声明 `ActivePc`）。

**解法**: `backend/mod.rs` 中:
```rust
#[cfg(all(feature = "backend-webrtc-rs", feature = "backend-webrtc-sys"))]
compile_error!("Only one backend can be enabled at a time.");
```

## PIT-05: egui example compilation requires full dependency tree (2026-07-20)

**症状**: `backend-webrtc-sys` feature 下 egui 示例需要 eframe/egui 完整编译（40+ crates, ~10 分钟）。

**解法**: 接受首次编译时间。后续增量编译仅需 1-2 分钟。

## PIT-06: SFU 消息字段名必须 snake_case (2026-07-28)

**症状**: 浏览器 SFU client 发送 `CreateWebRtcTransport` 后 server 无响应或返回错误。

**根因**: Rust serde 默认使用 snake_case 序列化。浏览器发送 camelCase (`createWebRtcTransport`) 不匹配。

**解法**: 浏览器端所有 SFU 消息 type 使用 snake_case：`create_web_rtc_transport`, `connect_web_rtc_transport`, `produce`, `consume`。

## PIT-07: SFU ConnectWebRtcTransport 必须实际调用 mediasoup API (2026-07-28)

**症状**: 浏览器 SFU transport 创建成功，但 WebRTC 连接失败（"Signal Lost"），video readyState=0。

**根因**: `handle_sfu_message` 中 ConnectWebRtcTransport 只记录日志返回 "transport_connected"，未调用 mediasoup 的 `transport.connect(dtls_parameters)` API。DTLS/ICE 实际连接未完成。

**解法**: ConnectWebRtcTransport 处理中必须调用 `sfu.connect_transport(room_id, peer_id, transport_id, dtls_params)` 执行真正的 mediasoup transport 连接。

## PIT-08: SFU 消息必须包含 peer_id 字段 (2026-07-28)

**症状**: Server 收到 SFU 消息但无法路由到正确的 transport。

**根因**: 浏览器 sfu-client 发送消息时缺少 `peer_id` 字段，server 端用 peer_id 做 transport 查找。

**解法**: 所有 SFU 消息必须包含 `peer_id` 字段，格式为 `{room_id}-{role}`（如 `test-room-consumer`）。

## PIT-09: 不允许未经用户同意的架构回退 (2026-07-28)

**症状**: Agent 在 SFU 实现遇到困难时自行回退到 P2P 方案。

**根因**: 用户明确要求 SFU 架构，Agent 不应私自做架构决策。

**解法**: 已写入 `.agents/rules/common/edit-safety.md`：任何架构变更（包括回退）必须经用户明确同意。

## PIT-10: 全局配置中的硬编码 API Key 存在泄露风险 (2026-07-28)

**症状**: `~/.config/opencode/opencode.jsonc` 第 10 行 apiKey 为明文 `sk-...`。屏幕共享、配置文件分享、git 操作不当均可导致泄露。

**根因**: OpenCode 全局 config 中 provider 的 apiKey 字段直接填入了明文密钥。

**解法**: 使用 OpenCode 环境变量插值语法 `"apiKey": "{env:NEW_API_KEY}"`，密钥存入 `~/.bashrc` 或密钥管理器。
**注意**: 当前用户选择保持现状（内网环境），但如需外部部署/代码分享时必须迁移。

## PIT-11: mediasoup-sys 0.13.0 与 meson >=0.64 buildtype 参数冲突 (2026-07-28)

**症状**: `cargo check -p omspbase-server --features sfu-mediasoup` 失败：
`ERROR: Got argument buildtype as both -Dbuildtype and --buildtype. Pick one.`

**根因**: mediasoup-sys 0.13.0 的 `tasks.py` 在 meson setup 命令中同时传入 `--buildtype debug`（命令行参数），而 `meson.build` 的 `default_options` 也设置了 `buildtype=release`。meson >=0.64 拒绝重复的 buildtype 参数。

**解法**: 项目级 `scripts/cargo-sfu.sh` wrapper 脚本，在每次 cargo 调用前自动：
1. 设置 `MESON` 环境变量指向 pixi 环境的 meson（避免 tasks.py pip-install 自己的 meson）
2. `sed -i` 移除 tasks.py 中的 `--buildtype` 参数（幂等操作）
3. 清除 mediasoup-sys 的构建缓存

**验证**: `pixi run check` 或 `bash check.sh` 不应再报 buildtype 错误。

## PIT-12: MESON 环境变量必须指定绝对路径 (2026-07-28)

**症状**: 设置 `MESON=meson` 后，mediasoup-sys 仍使用自己 pip 装的 meson。

**根因**: `tasks.py` 第 118 行 `if os.path.isfile(MESON): return` 检查 meson 文件是否存在。相对路径 `meson` 在 os.path.isfile() 中可能解析失败（工作目录不在 PATH 可解析的范围）。

**解法**: 必须用绝对路径：`MESON="$(pixi run -- which meson)"` 或 `MESON=$CONDA_PREFIX/bin/meson`。

**验证**: gradle tasks.py 输出中 meson 路径应为 `.../.pixi/envs/default/bin/meson`，而非 `.../pip_meson_ninja/bin/meson`。

## PIT-13: cargo clean -p 不清除构建脚本的 OUT_DIR (2026-07-28)

**症状**: `cargo clean -p mediasoup-sys` 后重新编译，构建脚本 hash 不变，仍使用缓存输出。

**根因**: `cargo clean -p` 只清除包的 target 产物，不删除 `target/debug/build/<pkg>-*/`（构建脚本的 OUT_DIR）。构建脚本的 stdout/stderr 被 cargo 缓存，跳过重新执行。

**解法**: 修改 build.rs 或改变环境变量后，需手动清除构建脚本缓存：
`rm -rf target/debug/build/mediasoup-sys-*`

**验证**: 清除后重新编译，构建脚本 hash 应改变。

## PIT-14: GitHub 在国内网络下 HTTP/2 被干扰 + 直连超时 (2026-07-28)

**症状**: `curl` 下载 GitHub release 报 `HTTP/2 stream 0 was not closed cleanly: PROTOCOL_ERROR (err 1)` 和 `SSL connection timeout (err 28)`。

**根因**: GitHub 的 HTTP/2 协议在某些网络环境下被中间设备干扰；直连 GitHub 延迟高、不稳定。

**解法**:
1. `curl --http1.1` 强制 HTTP/1.1，绕过 HTTP/2 干扰
2. GitHub 镜像回落：`mirror.ghproxy.com` 或 `ghproxy.net`
3. 代理：`export HTTPS_PROXY=http://127.0.0.1:7890`

## PIT-15: pixi 版本不应硬编码，Gitee 私人镜像不可靠 (2026-07-28)

**症状**: `bootstrap.sh` 卡在 "Installing pixi 0.67.2..."，Gitee 镜像 `gitee.com/chengxuewen-github/pixi` 下载失败。

**根因**: 旧版 PIXI_VERSION=0.67.2 可能已从 GitHub releases 清理；私人 Gitee 镜像仓库可能失效或不存在。

**解法**:
1. 默认 `PIXI_VERSION=latest`，使用官方 `pixi.sh/install.sh`
2. 指定版本时用 `PIXI_VERSION=x.y.z` 环境变量覆盖
3. 不用私人镜像，用 `mirror.ghproxy.com`（公共服务）
4. 下载的 tarball 缓存到 `.pixi-cache/downloads/` 复用
**注意**: 当前用户选择保持现状（内网环境），但如需外部部署/代码分享时必须迁移。

## PIT-16: pixi tasks 不认 `bash scripts/...` 或 `./scripts/...` 语法 (2026-07-29)

**症状**: pixi.toml tasks 中 `check = "bash scripts/cargo-sfu.sh ..."` 报 `expected a version specifier`，`./scripts/cargo-sfu.sh` 报 `it seems you're trying to add a path dependency`。

**根因**: pixi 解析 task value 时，`bash` 被识别为包名（dependency），`./` 被识别为路径依赖（path key）。两者都不被识别为命令。

**解法**: 脚本不可用 `bash` 前缀或 `./` 前缀。使用 `sh -c 'scripts/...'` 语法或让脚本可执行后直接调用。

**验证**: `pixi run check` 无 `expected a version specifier` 错误。

## PIT-17: conda-forge `clang` 包不提供 `libclang.so` (2026-07-29)

**症状**: `bindgen` 报 `Unable to find libclang: couldn't find any valid shared libraries matching: ['libclang.so']`。

**根因**: conda-forge 的 `clang` 包只提供编译器和 `libclang-cpp.so`（C++ API），`libclang.so`（C API）需要单独安装 `libclang` 包。

**解法**: pixi.toml 添加 `libclang = ">=15,<20"` 依赖。如仍有问题，`ln -sf libclang.so.N .pixi/envs/default/lib/libclang.so`。

**验证**: `find .pixi/envs/default -name libclang.so -type f` 存在。

## PIT-18: mediasoup tasks.py 覆盖 NINJA 环境变量 (2026-07-29)

**症状**: 设了 `NINJA` 环境变量指向 pixi 的 ninja，但 meson 仍报 `Could not detect Ninja v1.8.2 or newer`。

**根因**: tasks.py 第 82 行 `os.environ["NINJA"] = f"{PIP_MESON_NINJA_DIR}/bin/ninja"` 硬覆盖 NINJA，指向 pip 安装的路径（不存在）。

**解法**: cargo-sfu.sh 中用 `sed` 替换 NINJA 赋值为 pixi 路径。

**验证**: meson 日志中 ninja 路径应为 `.pixi/envs/default/bin/ninja`。

## PIT-19: sandbox 网络限制 — GitHub/OpenSSL 不可达 (2026-07-29)

**症状**: `curl https://github.com` 超时，`curl https://openssl.org` 超时。mediasoup meson 构建需下载 openssl 源码。

**根因**: OpenCode 沙箱只允许 npm registry 端口，GitHub 和其他站点被阻断。

**解法**: 读取 `~/.bashrc` 中的代理设置 (`http_proxy/https_proxy`)，在 pixi-shell.sh 中自动加载。

**验证**: `curl -I https://github.com` 在 pixi 环境中返回 200。

## PIT-20: 代理配置不应硬编码 (2026-07-29)

**症状**: pixi.toml activation 中硬编码 `http_proxy = "http://192.168.100.47:7897"`。

**根因**: 代理地址在不同环境不同（公司/家庭/CI），硬编码会导致跨环境失败。

**解法**: pixi-shell.sh 运行时从 `~/.bashrc` 读取 `export http_proxy=` 行，`eval` 注入。

**验证**: 重启 shell 后 `echo $http_proxy` 应有值。

## PIT-21: 不应修改依赖库源码 (2026-07-29)

**症状**: 尝试用 `sed` 修改 `~/.cargo/registry/src/.../mediasoup-sys-*/tasks.py` 和 `meson.build`。

**根因**: 用户明确要求不要修改依赖库源码/配置。tasks.py 的 patch 属于可接受的构建 wrapper，但 meson.build 不可。

**解法**: 只通过构建 wrapper 脚本（cargo-sfu.sh）修补 tasks.py，不触碰 meson.build。

**验证**: 无 meson.build 修改痕迹。

## PIT-22: pixi 不在 PATH 中，必须用绝对路径 (2026-07-29)

**症状**: `pixi run check` 报 `pixi: 未找到命令`，但 `~/.pixi/bin/pixi` 存在。

**根因**: pixi 安装在 `~/.pixi/bin/` 但未加入 shell PATH。VS Code 终端、脚本、子进程默认不继承用户 shell 的 PATH 配置。

**解法**: 始终使用绝对路径 `~/.pixi/bin/pixi run ...`，或在脚本中 `export PATH="$HOME/.pixi/bin:$PATH"`。

**验证**: `~/.pixi/bin/pixi --version` 返回版本号。

## PIT-23: Admin Dashboard 必须先构建再编译 server (2026-07-30)

**症状**: `curl http://localhost:9800/admin` 返回 `<html><h1>SPA not built</h1></html>`，HTTP 200 但内容是 fallback。

**根因**: `static_files.rs` 使用 `env!("ADMIN_DIST_DIR")` 编译时确定路径。如果 server 先编译、dashboard 后构建，二进制中的路径指向不存在的目录。

**解法**: 必须先 `pnpm build:admin`，再 `cargo build -p omspbase-server --features sfu-mediasoup`。顺序不可颠倒。

**验证**: `curl -s http://localhost:9800/admin | grep 'OMSPBase Admin'` 应返回完整 HTML。

## PIT-24: TypeScript 编辑后必须立即 typecheck (2026-07-30)

**症状**: `npx tsc --noEmit` 报大量语法错误（孤立行、重复代码、缺少括号）。

**根因**: 多次 `edit` 工具修改同一文件后，遗留了重复/孤立的代码行。每次 edit 只验证单次变更，未验证累积效果。

**解法**: 每次对 `.ts/.tsx` 文件执行 `edit` 后，立即运行 `npx tsc --noEmit` 验证。发现错误立即修复，不累积。

**验证**: `cd www/apps/admin && npx tsc --noEmit` 无输出（无错误）。

## PIT-25: mediasoup RouterOptions::default() 创建空 codec 列表 (2026-07-30)

**症状**: `produce()` 返回 "Unsupported codec [mime_type:Video(Vp8), payloadType:100]"。

**根因**: `RouterOptions::default()` 创建 `media_codecs: vec![]`。mediasoup 不提供默认 codec 列表——必须显式配置。

**解法**: 创建 `default_router_options()` 函数，包含 Opus + VP8 + H264 三个 codec。所有 Router 创建必须使用此函数而非 `RouterOptions::default()`。

**验证**: `cargo test -p omspbase-server --features sfu-mediasoup -- e2e_sfu_consume_pipeline` 通过。

## PIT-26: signaling.rs SFU 消息中 peer_id 不一致导致 "Peer not found" (2026-07-30)

**症状**: Produce 返回 "Peer not found in room"，但 CreateWebRtcTransport 刚成功创建了 peer。

**根因**: `CreateWebRtcTransport` 使用消息中的 `peer_id`（如 "host"），但 `Produce`/`ConnectWebRtcTransport` 使用 session 的 `relay_peer_id`（UUID）。两者不一致导致 SFU 找不到 peer。

**解法**: `handle_sfu_message` 中所有 SFU 操作统一使用 session 的 `peer_id`（函数参数），忽略消息中的 `peer_id` 字段。

**验证**: `cargo test -p omspbase-server --features sfu-mediasoup -- e2e_sfu_consume_pipeline` 通过。
---

---

## PIT-27: sfu-mediasoup feature 改变 test helper 函数签名 (2026-07-30)

**症状**: `cargo test --features sfu-mediasoup` 编译失败："this function takes 3 arguments but 2 arguments were supplied"。

**根因**: `SignalingServer::new` 在 `sfu-mediasoup` feature 下需要额外的 `Arc<SfuManager>` 参数。`AdminState` 需要 `sfu_manager` 字段。测试代码没有 cfg 条件编译。

**解法**: 使用 `#[cfg(feature = "sfu-mediasoup")]` 和 `#[cfg(not(feature = "sfu-mediasoup"))]` 两个版本的 test helper。async 版本调用 `SfuManager::new().await.unwrap()`。

**验证**: `cargo test -p omspbase-server --features sfu-mediasoup` 编译通过。

## PIT-28: mediasoup RtpCodecParameters 是 untagged enum (2026-07-30)

**症状**: `produce()` 返回 "Invalid RTP parameters: data did not match any variant of untagged enum RtpCodecParameters"。

**根因**: `RtpCodecParameters` 在 mediasoup-rs 中是 `#[serde(untagged)]` enum，有 `Audio` 和 `Video` 两个变体。每个变体需要特定字段：`mimeType`、`payloadType`、`clockRate`（Video 不需要 `channels`）。缺少任何字段或多余字段都会导致反序列化失败。

**解法**: 参考 mediasoup 官方测试（`rust/tests/integration/producer.rs`）构造正确的 JSON：
```json
{"mid": "0", "codecs": [{"mimeType": "video/VP8", "payloadType": 100, "clockRate": 90000}], "headerExtensions": [], "encodings": [{"ssrc": 12345}], "rtcp": {"reducedSize": true}}
```
注意：`payloadType` 必须匹配 Router 的 codec 列表中的值。

**验证**: `cargo test -p omspbase-server --features sfu-mediasoup -- e2e_sfu_consume_pipeline` 通过。

## PIT-29: SDP BUNDLE MID 必须与 a=mid 匹配 (2026-07-30)

**症状**: `setRemoteDescription` 失败："A BUNDLE group contains a MID='video' matching no m= section"。

**根因**: `a=group:BUNDLE video audio` 声明了 `video` 和 `audio` 作为 MID，但各媒体段使用 `a=mid:0` 和 `a=mid:1`，命名不匹配。

**解法**: `a=mid:` 值必须与 `a=group:BUNDLE` 中声明的 MID 一致。改为 `a=mid:video` 和 `a=mid:audio`。

**验证**: Playwright 测试中 `setRemoteDescription` 不再报错。

## PIT-30: Consumer 可能错过 NewProducer 广播（late-joiner）(2026-07-30)

**症状**: Consumer 连接后从未收到 `new_producer` 消息，不发 `consume`，无视频流。

**根因**: `NewProducer` 通过 broadcast channel 一次发送。Consumer 在 Host produce 之后才连接时，已经错过了广播。

**解法**: 1) Server 在 Consumer 进入 forward loop 时调用 `list_producers()` 查询已有 producer，主动发送 `NewProducer`。2) Browser 端需要排队 pending producer（`new_producer` 可能在 `web_rtc_transport_created` 之前到达，此时 `transportId` 未设置）。

**验证**: `cargo test -p omspbase-server --features sfu-mediasoup -- e2e_sfu` 通过。

## 参见

- [conventions.md](conventions.md) — 开发约定与约束
- [decisions.md](decisions.md) — 架构决策记录
- [status.md](status.md) — 项目状态与进度
