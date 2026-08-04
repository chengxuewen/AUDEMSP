# AUDEMSP Pitfalls & Gotchas

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

**症状**: `cargo check -p audemsp-server --features sfu-mediasoup` 失败：
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
2. ~~GitHub 镜像回落：`mirror.ghproxy.com` 或 `ghproxy.net`~~ **已修订 (2026-08-03)**：`mirror.ghproxy.com` 已停运（原项目 2024 年终止，curl 实测连接失败 000），脚本回退需换 `gh-proxy.com` 或从 conda 镜像安装（pixi 在 conda-forge 有包）
3. 代理：`export HTTPS_PROXY=http://127.0.0.1:7890`

## PIT-15: pixi 版本不应硬编码，Gitee 私人镜像不可靠 (2026-07-28)

**症状**: `bootstrap.sh` 卡在 "Installing pixi 0.67.2..."，Gitee 镜像 `gitee.com/chengxuewen-github/pixi` 下载失败。

**根因**: 旧版 PIXI_VERSION=0.67.2 可能已从 GitHub releases 清理；私人 Gitee 镜像仓库可能失效或不存在。

**解法**:
1. 默认 `PIXI_VERSION=latest`，使用官方 `pixi.sh/install.sh`
2. 指定版本时用 `PIXI_VERSION=x.y.z` 环境变量覆盖
3. ~~不用私人镜像，用 `mirror.ghproxy.com`（公共服务）~~ **已修订 (2026-08-03)**：ghproxy.com 已停运（见 PIT-14），pixi 安装回退改为 `gh-proxy.com` 或 conda-forge 安装
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

**解法**: 必须先 `pnpm build:admin`，再 `cargo build -p audemsp-server --features sfu-mediasoup`。顺序不可颠倒。

**验证**: `curl -s http://localhost:9800/admin | grep 'AUDEMSP Admin'` 应返回完整 HTML。

## PIT-24: TypeScript 编辑后必须立即 typecheck (2026-07-30)

**症状**: `npx tsc --noEmit` 报大量语法错误（孤立行、重复代码、缺少括号）。

**根因**: 多次 `edit` 工具修改同一文件后，遗留了重复/孤立的代码行。每次 edit 只验证单次变更，未验证累积效果。

**解法**: 每次对 `.ts/.tsx` 文件执行 `edit` 后，立即运行 `npx tsc --noEmit` 验证。发现错误立即修复，不累积。

**验证**: `cd www/apps/admin && npx tsc --noEmit` 无输出（无错误）。

## PIT-25: mediasoup RouterOptions::default() 创建空 codec 列表 (2026-07-30)

**症状**: `produce()` 返回 "Unsupported codec [mime_type:Video(Vp8), payloadType:100]"。

**根因**: `RouterOptions::default()` 创建 `media_codecs: vec![]`。mediasoup 不提供默认 codec 列表——必须显式配置。

**解法**: 创建 `default_router_options()` 函数，包含 Opus + VP8 + H264 三个 codec。所有 Router 创建必须使用此函数而非 `RouterOptions::default()`。

**验证**: `cargo test -p audemsp-server --features sfu-mediasoup -- e2e_sfu_consume_pipeline` 通过。

## PIT-26: signaling.rs SFU 消息中 peer_id 不一致导致 "Peer not found" (2026-07-30)

**症状**: Produce 返回 "Peer not found in room"，但 CreateWebRtcTransport 刚成功创建了 peer。

**根因**: `CreateWebRtcTransport` 使用消息中的 `peer_id`（如 "host"），但 `Produce`/`ConnectWebRtcTransport` 使用 session 的 `relay_peer_id`（UUID）。两者不一致导致 SFU 找不到 peer。

**解法**: `handle_sfu_message` 中所有 SFU 操作统一使用 session 的 `peer_id`（函数参数），忽略消息中的 `peer_id` 字段。

**验证**: `cargo test -p audemsp-server --features sfu-mediasoup -- e2e_sfu_consume_pipeline` 通过。
---

---

## PIT-27: sfu-mediasoup feature 改变 test helper 函数签名 (2026-07-30)

**症状**: `cargo test --features sfu-mediasoup` 编译失败："this function takes 3 arguments but 2 arguments were supplied"。

**根因**: `SignalingServer::new` 在 `sfu-mediasoup` feature 下需要额外的 `Arc<SfuManager>` 参数。`AdminState` 需要 `sfu_manager` 字段。测试代码没有 cfg 条件编译。

**解法**: 使用 `#[cfg(feature = "sfu-mediasoup")]` 和 `#[cfg(not(feature = "sfu-mediasoup"))]` 两个版本的 test helper。async 版本调用 `SfuManager::new().await.unwrap()`。

**验证**: `cargo test -p audemsp-server --features sfu-mediasoup` 编译通过。

## PIT-28: mediasoup RtpCodecParameters 是 untagged enum (2026-07-30)

**症状**: `produce()` 返回 "Invalid RTP parameters: data did not match any variant of untagged enum RtpCodecParameters"。

**根因**: `RtpCodecParameters` 在 mediasoup-rs 中是 `#[serde(untagged)]` enum，有 `Audio` 和 `Video` 两个变体。每个变体需要特定字段：`mimeType`、`payloadType`、`clockRate`（Video 不需要 `channels`）。缺少任何字段或多余字段都会导致反序列化失败。

**解法**: 参考 mediasoup 官方测试（`rust/tests/integration/producer.rs`）构造正确的 JSON：
```json
{"mid": "0", "codecs": [{"mimeType": "video/VP8", "payloadType": 100, "clockRate": 90000}], "headerExtensions": [], "encodings": [{"ssrc": 12345}], "rtcp": {"reducedSize": true}}
```
注意：`payloadType` 必须匹配 Router 的 codec 列表中的值。

**验证**: `cargo test -p audemsp-server --features sfu-mediasoup -- e2e_sfu_consume_pipeline` 通过。

## PIT-29: SDP BUNDLE MID 必须与 a=mid 匹配 (2026-07-30)

**症状**: `setRemoteDescription` 失败："A BUNDLE group contains a MID='video' matching no m= section"。

**根因**: `a=group:BUNDLE video audio` 声明了 `video` 和 `audio` 作为 MID，但各媒体段使用 `a=mid:0` 和 `a=mid:1`，命名不匹配。

**解法**: `a=mid:` 值必须与 `a=group:BUNDLE` 中声明的 MID 一致。改为 `a=mid:video` 和 `a=mid:audio`。

**验证**: Playwright 测试中 `setRemoteDescription` 不再报错。

## PIT-30: Consumer 可能错过 NewProducer 广播（late-joiner）(2026-07-30)

**症状**: Consumer 连接后从未收到 `new_producer` 消息，不发 `consume`，无视频流。

**根因**: `NewProducer` 通过 broadcast channel 一次发送。Consumer 在 Host produce 之后才连接时，已经错过了广播。

**解法**: 1) Server 在 Consumer 进入 forward loop 时调用 `list_producers()` 查询已有 producer，主动发送 `NewProducer`。2) Browser 端需要排队 pending producer（`new_producer` 可能在 `web_rtc_transport_created` 之前到达，此时 `transportId` 未设置）。

**验证**: `cargo test -p audemsp-server --features sfu-mediasoup -- e2e_sfu` 通过。

## 参见

- [conventions.md](conventions.md) — 开发约定与约束
- [decisions.md](decisions.md) — 架构决策记录
- [status.md](status.md) — 项目状态与进度

## PIT-31: Docker Hub 不可达 — daemon 需独立代理配置 (2026-07-31)

**症状**: `docker run --rm hello-world` 报 `failed to resolve reference "docker.io/library/hello-world"` / `dial tcp 157.240.2.50:443: i/o timeout`。curl 测试镜像源返回 200 但 docker pull 仍失败。

**根因**: 国内网络 Docker Hub 被墙。用户 shell 的 `http_proxy` 环境变量**不影响 docker daemon**（daemon 是 systemd 服务，独立进程）。curl 走用户代理成功，daemon 直连超时。

**解法**: 双重配置：
1. 镜像加速器 `/etc/docker/daemon.json` (registry-mirrors)
2. daemon 代理 `/etc/systemd/system/docker.service.d/proxy.conf` (HTTP_PROXY/HTTPS_PROXY/NO_PROXY) → `systemctl daemon-reload && systemctl restart docker`

**验证**: `docker run --rm hello-world` 返回 "Hello from Docker!"。

## PIT-32: docker compose `image:` + `build:` 同存反模式 (2026-07-31)

**症状**: 配置 `image: ghcr.io/...` + `build: target: dev` 后，`docker compose up` 仍本地构建而非拉取预编译镜像。

**根因**: Compose 同时存在 `build:` 和 `image:` 时，**始终执行 build**，`image:` 只作为构建产物的 tag。预编译镜像永远不会被拉取。

**解法**: 分离 compose 文件：`docker-compose.yml`（生产，仅 `image:`） + `docker-compose.dev.yml`（开发，仅 `build:`）。OpenVidu 同此模式。

**验证**: `docker compose pull && docker compose up -d` 应直接拉取镜像（<30s 启动）。

## PIT-33: mediasoup-sys flatbuffers subproject 构建失败 (2026-07-31)

**症状**: `cargo check -p audemsp-server --features sfu-mediasoup` 报 `ERROR: Subproject flatbuffers is buildable: NO` / `Subproject exists but has no meson.build file`。手动解压 flatbuffers tar.gz 后仍无 meson.build。

**根因**: flatbuffers 的 meson.build 来自 wrapdb.mesonbuild.com 的 patch zip（`flatbuffers_24.3.25-1_patch.zip`），wrapdb 不可达时 patch 下载失败。flatbuffers 源码 tarball 本身只有 CMake，无 meson.build。

**解法**: 无法本地补救（patch 必须从 wrapdb 下载）。走 Docker 统一构建（C13）——镜像内预装依赖或使用层缓存。排查时 `find target -name "meson.build"` 确认缺失，不要反复重试原生构建。

**验证**: Docker 构建成功（`docker compose -f docker-compose.dev.yml build`）。

## PIT-34: 子代理完成声明不可信 — 必须验证产物 (2026-07-31)

**症状**: P1b 子代理声称 "docker-compose.dev.yml created"，实际文件**不存在**；生产 docker-compose.yml 还丢了 environment 字段。若直接信完成声明继续，CI 会失败。

**根因**: 子代理响应截断或声称提前（"Good. Now let me create..." 后即返回）。完成声明 ≠ 产物落盘。

**解法**: 编排者必须验证实际产物：`cat` 文件存在性 + `docker compose config` 校验 + grep 关键字段。验证失败 → resume session 修复。

**验证**: `ls docker-compose.dev.yml && grep environment docker-compose.yml`。

## PIT-35: 参考文档子代理幻觉 — 事实核查必要 (2026-07-31)

**症状**: OpenVidu 参考文档 openvidu-deployment.md 写入不存在的容器（Kurento/Coturn/Kibana/PostgreSQL）、错误描述 LiveKit 为"单独服务"、错误记录 ghcr.io。

**根因**: 子代理基于推测补全未知细节，未严格对照上游仓库实际文件。参考文档生成后未做事实核查。

**解法**: 生成参考文档后必须对照上游源码核查事实（容器清单、镜像注册表、端口、数据库）。发现错误 → 修正文档（本轮修正 4 处）。核查时以仓库实际 docker-compose.yaml 为准，不信二手描述。

**验证**: `grep -i "kurento\|coturn" openvidu-deployment.md` 应为空。

## PIT-36: Docker builder dummy→COPY 层 mtime 坑 — cargo fingerprint 误判源码未变 (2026-08-03)

**症状**: `docker build --target builder` 最终构建报 `cannot find protocol in audemsp_common` + `str::clone` 系列连锁错误，但源码明显正确（grep 确认 pub mod protocol 存在）。

**根因**: Dockerfile 模式「dummy src 编译依赖 → `rm -rf crates/*/src` → `COPY . .` 真实源码 → 最终构建」。**COPY 保留宿主文件 mtime**，宿主 .rs 文件 mtime 早于 dummy 构建时间 → cargo fingerprint 按 mtime 判断源码未变更 → 链接 dummy 阶段编译的**空 common rlib** → 连锁类型错误。

**解法**: COPY . . 之后、最终构建之前 touch 源码更新 mtime：
```dockerfile
COPY . .
RUN find crates -name '*.rs' -exec touch {} +
RUN cargo build --release --bin audemsp-server --features sfu-mediasoup
```

**验证**: 最终构建输出显示 `Compiling audemsp-common`（真实重编）而非跳过。

## PIT-37: cargo fetch 要求全部 workspace member 有 targets + [[example]] 文件 (2026-08-03)

**症状**: manifests-first 模式的 `cargo fetch` 报 `no targets specified in the manifest`（缺 src 的 member）或 `can't find square-gen-egui example`（声明了 [[example]] 的 crate 缺 examples/ 文件）。

**根因**: cargo fetch 解析 workspace 时**检查所有 member 的 manifest targets 完整性**（非仅构建目标）。`[[example]]` 显式声明（如 audemsp-media 的 square-gen-egui/viewer/square-gen）会校验文件存在性；自动发现的 examples/*.rs 不校验。

**解法**: dummy 阶段全建：所有 member `touch src/lib.rs`（bin crate 建 main.rs）+ 显式声明的 example 文件 `touch`。builder 与 dev 两处都需。

**验证**: `docker build --target builder` 通过 fetch 阶段。

## PIT-38: 容器内进程代理不继承 daemon 代理 — mediasoup wrapdb 超时 (2026-08-03)

**症状**: 容器内 mediasoup-sys meson 构建报 `WrapDB connection failed to https://wrapdb.mesonbuild.com/v2/openssl_3.0.8-3/get_patch ... timed out`；tasks.py pip 装 meson 报 pypi.org ReadTimeout。

**根因**: Docker daemon 代理（systemd proxy.conf，PIT-31）**只影响 daemon 拉镜像**，不传递给容器内进程。容器内 mediasoup-sys build script 的 python urllib/pip 直连 wrapdb.mesonbuild.com / pypi.org → 国内超时（PIT-33 根因复现）。

**解法**: 构建期代理经 build-arg 显式传入（PIT-20 不硬编码）：
```dockerfile
ARG HTTP_PROXY
ARG HTTPS_PROXY
ENV HTTP_PROXY=${HTTP_PROXY:-} HTTPS_PROXY=${HTTPS_PROXY:-}
```
compose build args 从宿主环境读：`HTTP_PROXY: ${http_proxy:-}`。pip 另加 `PIP_INDEX_URL=https://pypi.tuna.tsinghua.edu.cn/simple`（tasks.py 的 pip 调用生效，不改依赖源码）。

**⚠️ 修复必须双路径（2026-08-03 补充）**: build 阶段（Dockerfile ARG/ENV）**和** run 阶段（compose `environment:`）都要传代理——`docker compose run server cargo check` 容器内编译 mediasoup-sys 同样需要。只修 build 路径，docker-cargo.sh 第二步仍会 wrapdb 超时。

**验证**: meson 日志显示 wrapdb patch 下载成功；`Failed to build libmediasoup-worker` 消失；`scripts/docker-cargo.sh check -p audemsp-server --features sfu-mediasoup` EXIT 0。

**验证**: meson 日志显示 wrapdb patch 下载成功；`Failed to build libmediasoup-worker` 消失。

## PIT-39: audemsp-server 从未被真实编译 — Docker dev 链路历史故障 (2026-08-03)

**症状**: 冒烟构建暴露 main.rs:75 `if/else 类型不一致`（String vs &str）——任何真实编译都会报的错。

**根因**: Docker dev 链路**历史上从未成功运行过**，故障链：docker-cargo.sh 服务名 `dev` 不存在（必失败）→ C13 的 check-server 从未生效 → devcontainer 指向生产 compose（无工具链）→ builder dummy→COPY mtime 坑（PIT-36）→ CI 构建从未产出真实二进制。多个独立 bug 相互掩盖，导致 server 真实源码从未被编译验证。

**解法**: 逐一修复（D208 本周项 4/6 + PIT-36/37/38），冒烟构建作为最终验证。**教训**: 声称"构建通过"的 CI 需抽查产物真实性（C14）；服务名/路径类 bug 长期静默是因为失败路径从未被触发。

**验证**: `docker build --target builder` EXIT 0 + runtime 容器 health 200。

## PIT-40: team-mode 成员模型配额耗尽后回退 session 挂起 — 需 kill + 独立任务重试 (2026-08-03)

**症状**: doc-review-team 的 tech-reviewer 在首次会话因 `token-plan 1-week quota exhausted` 失败后自动重试到回退模型，但新 session 持续 1h12m **无任何产出**（其他 3 个成员 2-4 min 完成），发消息唤醒（2 次）无响应。

**根因**: 团队成员的模型配额耗尽（1 周额度）→ 重试机制创建回退 session，但该 session 挂起（idle + unread 消息不被处理）。团队消息队列无法唤醒死 session。

**解法**: 不再等待 → `team_shutdown_request` + `team_approve_shutdown` 终止死成员 → 用独立 `task(category=..., run_in_background=true)` 以干净上下文重试（本案例 4m41s 完成同等工作）。**团队成员不响应时不要无限等待，5 min 无产出即 kill 换独立任务**。

**验证**: 独立任务完成（bg_20e704a5 4m41s EXIT 正常）；审核报告交付。

## PIT-41: 批量 edit 多个 replace 操作覆盖相邻结构 — 提交后必须立即跑配置验证 (2026-08-03)

**症状**: 修改 docker-compose.yml 时，edits 数组第二个操作（删 volumes 段）误把 proxy 服务整体替换掉，`docker compose config` 报 `services.proxy must be a mapping`。若未验证直接提交会破坏生产部署。

**根因**: 批量 edits 数组内多个 replace 操作引用相邻区域时，边界行号/内容易错位（一个操作覆盖了另一个操作的保留区域）。edit 工具按原始快照应用，操作间不互相校验。

**解法**: 每次 edit 调用后**立即跑对应格式验证**：YAML → `docker compose config --quiet`；shell → `bash -n`；Rust → `cargo check`。发现破坏 → 重读文件恢复。本案例通过 config 验证发现并修复（proxy 服务完整恢复）。

**验证**: `docker compose -f docker-compose.yml config --quiet` EXIT 0 + `--services` 输出 server/proxy。

## PIT-45: ~~webrtc-sys (livekit) Linux gathering 失效~~ — 已推翻 (2026-08-04)

> **❌ 已修订 (2026-08-04)**：此结论**不成立**——真根因是应用层 SDP 构造 bug（candidate 行位置，见 PIT-46）。libwebrtc gathering 实际正常（strace 证明）。测试套件不验证真实连接的观察仍有效（PIT-50 方法论）。

**症状**: Host（backend-webrtc-sys）连接 mediasoup SFU 时 ICE/DTLS 30s 超时；tcpdump 显示 0 STUN 包；answer SDP 无 a=candidate 行；on_ice_gathering_change/on_ice_candidate/on_ice_connection_state_change 回调**零触发**（连 Gathering/Complete 都没有）。容器（bridge/host 网络）与**本机原生桌面**（真实网卡 192.168.2.127）一致失败。

**根因**: webrtc-sys 0.3.39/0.3.41（livekit/rust-sdks 的 libwebrtc 预编译包）在 Linux 上 **gathering 永不启动**（libwebrtc 从未调用 OnIceGatheringChange——C++ 转发与 observer 注册均正常，排除应用层）。上游测试套件**从未验证过真实 ICE 连接**：loopback/media_frame_e2e 测试只交换 SDP 不等待 connected、不断言对端收帧（CountingSink 无断言）——"测试全过"≠"ICE 可用"。LiveKit 服务器端用 Go pion（go.mod 实证），webrtc-sys 仅用于客户端 SDK（Linux 目标=桌面），容器/无头场景无任何成功先例；官方 C++ 客户端（libmediasoupclient）Linux CI 亦被禁用。

**解法**: ① 应用层无法修复（库层运行时问题），向 livekit/rust-sdks 报 issue（附证据链）；② 容器/无头环境验证改用 `backend-webrtc-rs`（纯 Rust ICE，pion 同类路线）；③ 车端 Ubuntu 桌面可再试 webrtc-sys 更新版或真实设备验证。

**验证**: 升级 webrtc-sys 0.3.41 后重跑仍超时（2026-08-04 实测）——版本升级无效。

## PIT-44: mediasoup WebRtcServer listen 0.0.0.0 必须设 announced_address (2026-08-04)

**症状**: SFU transport 候选公告 0.0.0.0:20000，对端 ICE 0 包（tcpdump）——Linux 内核把发往 0.0.0.0 的 UDP 路由到 **loopback**（`ip route get 0.0.0.0` → `local ... dev lo` 实证），STUN 永远到不了 mediasoup。

**根因**: WebRtcServerOptions ListenInfo `ip: 0.0.0.0` + `announced_address: None` 违反 mediasoup 官方要求（"0.0.0.0 必须配 announcedAddress"）；`expose_internal_ip: true` 仅在 announced_address 非空时生效（worker 源码 WebRtcTransport.cpp else 分支）。mediasoup 维护者明确不校验 0.0.0.0（issue #717），仅文档约束。

**解法**: 容器场景启动时探测本机 IP（零依赖 UDP connect 技巧）作 announced_address；生产按 mediasoup F.A.Q. 配方 `0.0.0.0` + `announcedAddress: 公网/可达 IP`。
```rust
fn detect_local_ip() -> String {
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() { return addr.ip().to_string(); }
        }
    }
    "0.0.0.0".to_string()
}
```

**验证**: 修复后 transport 候选为 172.18.0.2:20000（remote SDP 打印确认）。

## PIT-43: webrtc-sys on_ice_candidate 回调空桩 (2026-08-04)

**症状**: webrtc_sys.rs:625 `fn on_ice_candidate(&self, _) {}`——本地候选被静默丢弃，P2P relay 路径无法 trickle，且掩盖本地收集状态（无法区分"无候选"与"回调丢失"）。

**根因**: 封装层空实现（livekit 客户端走完整回调转发，audemsp-webrtc 未实现）。

**解法**: 至少记录日志（已实现 tracing::debug + gathering 状态日志）；P2P 路径需完整转发到 ObserverCallbacks。

**验证**: RUST_LOG=debug 可观察 ICE gathering/candidate 事件。

## PIT-46: SDP a=candidate 行必须在 m= 行之后（media section 内）— webrtc-sys "Linux gathering 失效"真根因 (2026-08-04)

**症状**: Host（webrtc-sys）连 mediasoup SFU 30s ICE 超时、tcpdump 0 STUN、answer 无候选行、所有 observer 回调零触发——容器（bridge/host 网络）与原生桌面一致失败。曾被误判为"webrtc-sys 库层 Linux gathering 失效"（PIT-45，已推翻）。

**根因**: Host 手工构造 remote SDP 时把 `a=candidate:...` 放在 `m=video` **之前**（会话级）——SDP 规范中 candidate 属性必须属于 media section（m= 行之后）→ libwebrtc 忽略会话级 candidate → **remote candidate 从未被接受** → ICE 无对端可 ping（0 STUN）+ P2PTransportChannel 未进入连接阶段（无内部日志）。strace 证明 libwebrtc 实际正常枚举接口（netlink RTM_GETLINK）并 bind UDP socket（gathering 在工作）——问题纯在应用层 SDP 构造。

**解法**: candidate 行移到 m= 行之后（media section 内）：
```
m=video 7 UDP/TLS/RTP/SAVPF 101
c=IN IP4 172.18.0.2
a=mid:video
a=candidate:udpcandidate 1 UDP 1076302079 172.18.0.2 20000 typ host
a=end-of-candidates
```
修复后 ICE 秒连（Checking→Connected→Completed）+ Produce 发送成功。

**验证**: 修复后 Host 日志 `SFU ICE state: Connected` + `Produce (Video) sent` + server 侧 `OnIceServerCompleted() | ICE completed`。

## PIT-47: WebSocket 子协议认证 — RFC 6455 token 禁止空格，JWT 必须纯子协议 (2026-08-04)

**症状**: 浏览器 sfu-client 连 server /ws 反复失败：先 "Failed to construct WebSocket: subprotocol 'Bearer xxx' is invalid"（空格），修后 "closed before connection established"（server 未回显子协议），再修后认证失败（signaling 的 jwt_secret 未配置）。

**根因**: ① WebSocket 子协议是 RFC 6455 token（禁止空格）——`Bearer <jwt>` 前缀非法，浏览器构造即抛错；② server（axum）必须 `ws.protocols(...)` 回显客户端子协议，否则浏览器协商失败；③ signaling 的 JWT 用 `jwt_secret`（非 `admin_jwt_secret`），未配置则 JWT 路径不可用（PSK fallback 又因浏览器不发 PSK 而失败）。

**解法**: ① 浏览器传纯 JWT 子协议：`new WebSocket(url, [token])`；② server 解析兼容 `Bearer ` 前缀与纯 JWT，并 `protocols(client_protocols)` 回显；③ server.docker.yaml 配 `jwt_secret`（与 admin_jwt_secret 同值，admin token 可验证）；④ sfu-client 有 JWT 子协议时不再发明文 PSK。

**验证**: 页面内 `new WebSocket('ws://127.0.0.1:5173/ws', [TOKEN])` open + 收到 `{"code":0,"message":"authenticated"}`；server 日志 `JWT authenticated: peer=admin`。

## PIT-48: React StrictMode 双挂载 — close() 必须设标志防 onclose 重连 (2026-08-04)

**症状**: Dashboard 的 VideoPlayer（React 组件）SFU 连接失败（Signal Lost），而页面内直接调用 SfuConsumerClient 成功——同一 token/URL 行为不同。

**根因**: `main.tsx` 用 `<React.StrictMode>`（dev 双挂载：mount→unmount→remount）——第一个 client 的 `close()` 触发 WS onclose → `reconnect()`（无关闭标志）→ 泄漏连接与第二个 client 竞争。

**解法**: sfu-client 加 `private closed` 标志：`connect()` 重置、`onclose` 检查跳过重连、`close()` 先置标志。
```ts
this.ws.onclose = () => { if (this.closed) return; ... };
close() { this.closed = true; ... }
```

**验证**: 修复后 VideoPlayer（React 路径）SFU 连接正常（server 侧 JWT authenticated + transport 流程）。

## PIT-49: mediasoup Router codec preferred_payload_type 必须显式 — 自动分配与 produce 参数冲突 (2026-08-04)

**症状**: Host produce 消息到达 server（`Produce received`）但 producer 未创建（list_producers 0）；早期报 `Duplicated preferred payload type 101`。

**根因**: `default_router_options()` 的 codec `preferred_payload_type: None`——mediasoup 自动分配 payloadType（VP8 可能自动分配 101 与 H264 冲突；H264 自动分配 ≠ Host produce 发的 101 → produce 失败）。Host 的 rtp_parameters 固定 payloadType 101。

**解法**: Router codec 显式化：Opus=111、VP8=96、H264=101（与 Host produce 匹配）。注意字段类型是 `Option<u8>`（非 NonZeroU8）。

**验证**: 显式化后 Router 创建成功（无 duplicated 错误）；produce 进入 `transport.produce().await`（注：当前 await 挂起为独立问题，见会话记录）。

## PIT-50: WebRTC 调试归因顺序 — 先验证协议层事实，勿过早归因库层 (2026-08-04)

**症状/教训**: "webrtc-sys Linux gathering 失效"结论（PIT-45）在投入大量实验后被推翻——真根因是应用层 SDP 构造 bug（PIT-46）。libwebrtc 一直正常（strace 证明接口枚举 + UDP bind 正常）。

**根因**: 调试时先假设库层问题（webrtc-kit/LiveKit 预编译），跳过了协议层验证（SDP 结构是否符合规范、remote candidate 是否被接受）。

**正确调试链（WebRTC 类问题）**:
1. **tcpdump/strace** — 网络事实（0 包 vs 有包、socket 是否创建）
2. **libwebrtc 内部日志**（LogSink：`webrtc_sys::webrtc::ffi::new_log_sink` → LS_VERBOSE 全级别）— 库内部状态
3. **SDP 规范对照** — 结构校验（candidate 位置、媒体段顺序）
4. **最小复现对照**（页面内直接调用 vs React 组件 → 隔离环境问题）
5. 最后才归因库层（且要有上游证据：CI 是否真验证过、官方文档/issue）

**验证**: 按此链定位 PIT-46/47/48 均为应用层问题；PIT-45 已修订（"库层问题"不成立）。

## PIT-51: pixi.toml 重复 key 与缺失 feature 定义 — pixi install 从未成功过 (2026-08-04)

**症状**: `pixi install` 报 `duplicate key: coverage`（两个 coverage task）+ `feature 'test' is not defined`（[environments] test 引用不存在 feature）。

**根因**: pixi.toml L57-58 两个 coverage 任务重复；`[feature.test]` 从未定义（只有 dev/ci）。项目原生构建链路（pixi）从未真正运行过——与 PIT-39（Docker dev 链路从未成功）同类。

**解法**: 合并 coverage task（--out Html --out Lcov）；补 `[feature.test.dependencies]` 空表。

**验证**: `pixi install` EXIT 0 + `.pixi/envs/default/bin/cargo --version` 可用（原生编译链路首次打通）。

## PIT-53: P2P 双 full ICE 不连接 — webrtc-sys trickle 候选激活失败 (2026-08-04)

**症状**: 新增真实 ICE 连接测试（ice_connect_e2e：双 PC SDP 交换 + trickle 候选双向转发 + 等待 Connected）15s 超时失败；libwebrtc 无 p2p_transport/PortAllocator 日志（ICE transport 未激活）；候选 gather 正常（回调产生 172.18.0.1/192.168.2.127 host 候选）+ add_ice_candidate 全部 OK。对照：Host→mediasoup（ICE-Lite remote + SDP 内嵌候选）连接正常（PIT-46 修复后）。

**根因**: webrtc-sys（livekit libwebrtc）的 P2P 双 full ICE 场景——trickled 候选添加成功但 ICE transport 未激活（无内部日志）。与 ICE-Lite 场景（remote 候选内嵌 SDP）行为不同。**测试门禁暴露**：此前测试套件从不等待真实连接，此缺陷从未被发现（PIT-50 方法论）。

**解法**: ① 测试保留（#[ignore] 标记）作为门禁——P2P ICE 修复后启用；② 修复方向：webrtc-sys 封装的 ICE transport 激活条件、候选 generation/ufrag 匹配、或双 full ICE 角色协商；③ Host→SFU 生产路径（ICE-Lite）不受影响。

**验证**: `cargo test -p audemsp-webrtc --features backend-webrtc-sys --test ice_connect_e2e -- --ignored` 当前失败（预期）；移除 #[ignore] 且通过 = 修复完成。

## PIT-54: produce 报 UnsupportedCodec 却表现"挂起" — Err 分支无日志 + Host 不处理响应 (2026-08-04)

**症状**: Host produce 后 server 无 Producer created 日志、无失败日志，表现如 `transport.produce().await` 挂起；实际是快速失败——`RTP mapping error: Unsupported codec [Video(H264), payloadType:101]`。

**根因**: ① **真根因**：Host 手工构造 rtp_parameters（main.rs json!）缺 codec parameters——mediasoup match_codecs 对 H264 strict 匹配，producer 缺省 `packetization-mode` 按 0 处理，Router capability 是 1 → 不匹配 → UnsupportedCodec（PIT-51 显式 payloadType 只是必要条件）。② **静默假象**：signaling.rs Err 分支只构造 Error 响应**不打日志**，且 Host 发完 produce 后**不读响应**（main.rs:386 后直接跑帧循环）→ 错误在两端都被静默吞掉，看起来像挂起。

**解法**: ① Host produce JSON 补 H264 parameters：`{"level-asymmetry-allowed":1,"packetization-mode":1,"profile-level-id":"4d0032"}`（与 Router 一致，4d0032=Main profile，mediasoup-demo 标准）。② signaling.rs Err 分支加 `tracing::error!`。③ Host 应处理 produce 响应（当前忽略）。

**验证**: server 日志 `Producer <id> (Video) created` + `SFU: broadcast NewProducer`；Host `SFU produce transport ready — I420 frame loop started`。

**调试教训**: ① 日志矛盾（response sent 出现在 produce() 之后）指向 Err 分支无日志——gdb 断点（signaling.rs:691/704/716）一锤定音。② 容器 gdb 需要 `cap_add: [SYS_PTRACE]`（已加入 docker-compose.dev.yml）；apt 包每次容器重建丢失 → gdb 已入 dev Dockerfile。③ **设计缺陷**：Host 手工构造 rtp_parameters 是双硬编码（SDP + produce JSON 各自写死 PT/SSRC），且两处已不一致——SDP fmtp 是 `profile-level-id=42e01f`（Baseline, main.rs:293），produce JSON 是 `4d0032`（Main, main.rs:380），靠 mediasoup answer 用 Router codec (4d0032) 应答才偶然对齐；正确形态是 audemsp-webrtc 补 `get_rtp_parameters(track_id)` API 从协商结果提取——记入待办。
