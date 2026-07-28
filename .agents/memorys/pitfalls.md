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
