# WebRTC GOP 控制修复 — request_key_frame 周期触发

> 计划: 2026-08-10 | 触发: PIT-76 首帧延迟 68s（稳态 GOP 99s）| 分支: `feat/webrtc-request-key-frame`
> 调研文档: `docs/reference/webrtc/gop-control-internal-encoder.md`（根因链 + 方案对比 + vendored 决策，已 commit `349e964`）
> 状态: **T0 已完成，T1-T4 待实施**

## 1. 背景与目标

### 1.1 问题
Host SFU 推流首帧渲染 68s。根因（三重证据确认）:
- Host 稳态关键帧间隔 **99s**（`VideoCodecVP8.keyFrameInterval` 未设置 → libvpx `kf_max_dist=0`）
- PLI 响应链路断裂（livekit fork 的 RtpVideoSender→VideoStreamEncoder RTCP 接线缺失，`take_keyframe_request()` 从未置位）
- SDP 注入 `x-google-max-keyframe-interval` 无效（Chrome 集成层特有参数，标准 libwebrtc 不解析）

### 1.2 目标
- 关键帧间隔 ≤ 2.5s（周期强制，不依赖 PLI/SDP）
- 浏览器首帧 < 2s
- **保留内部编码器路径**（simulcast/SVC/自动降级全保留）
- 走 libwebrtc 标准 API（`RtpEncodingParameters.request_key_frame` → `GenerateKeyFrame`）

### 1.3 方案（已确认）
| 项 | 决策 |
|----|------|
| 触发机制 | `RtpEncodingParameters.request_key_frame = true` 周期调用 `set_parameters`（标准路径，本地即时 IDR） |
| webrtc-sys 依赖 | **vendored `[patch]`**（非 fork+submodule，C20 合规，社区主流做法） |
| vendored 基线 | **0.3.41**（Cargo.lock 已锁定版本，registry 源码 6.8M 完整复制） |
| 触发间隔 | 2s（关键帧带宽开销 ~5-8% @2Mbps，可接受） |

## 2. 任务分解

### ✅ T0: vendored webrtc-sys patch（已完成）
- [x] `vendor/webrtc-sys/` = registry 0.3.41 完整复制（删 `Cargo.toml.orig`/`Cargo.lock`，6.8M）
- [x] `src/rtp_parameters.rs`: cxx 结构体加 `pub request_key_frame: bool`（含 AUDEMSP 注释）
- [x] `src/rtp_parameters.cpp`: `to_native_rtp_encoding_paramters` 加 `native.request_key_frame = parameters.request_key_frame;`
- [x] 根 `Cargo.toml` `[patch.crates-io] webrtc-sys = { path = "vendor/webrtc-sys" }`（含升级注意事项注释）
- [x] 修 `crates/audemsp-webrtc/src/backend/webrtc_sys.rs:1188` 构造点（加 `request_key_frame: false`）
- [x] `cargo check -p audemsp-webrtc` 通过（patch 生效: `Adding webrtc-sys v0.3.41 (path)`）
- [ ] Cargo.lock 更新提交

### 🔲 T1: audemsp-webrtc 上层 API（3 文件）
- [ ] `src/rtp.rs`: `RTCRtpEncodingParameters` 加 `pub request_key_frame: bool`（默认 false）
- [ ] `src/backend/webrtc_sys.rs:93`: 转换处传 `request_key_frame: e.request_key_frame`
- [ ] `RtpSender` 暴露 `request_key_frame()` 方法：webrtc-sys 实现（调 `set_parameters` 设 true 后复位 false——`GenerateKeyFrame` 是单次触发语义）；webrtc-rs 后端同步（`set_parameters` 同路径，检查 webrtc-rs 0.12 的 RtpEncodingParameters 是否已支持）；stub 后端 no-op
- [ ] trait 层（`traits.rs`）加 `request_key_frame()`（三后端签名一致）

### 🔲 T2: Host 帧循环周期触发
- [ ] `crates/audemsp-host/src/main.rs`: 帧循环加定时器，每 2s 调 `request_key_frame()`
- [ ] 首次触发时机: 协商完成后立即触发一次（快速首帧，不等 2s）
- [ ] 配置项: `host.conf` 加 `keyframe_interval_secs`（默认 2），或常量 + TODO 标注（PIT-76 阶段）

### 🔲 T3: 实测验证（宿主原生 + Docker server，C22）
- [ ] mediasoup worker 日志 `key frame received` 间隔 ≤ 2.5s（对比修复前 99s）
- [ ] 浏览器首帧 < 2s（Play 点击 → videoWidth，web 时间戳日志）
- [ ] 视频质量抽查（无花屏/卡顿，GOP 强制与 delta 帧无冲突）

### 🔲 T4: 回归 + 沉淀
- [ ] `e2e_sfu` 4/4 通过（Docker server + 宿主 host）
- [ ] workspace 相关 crate 测试通过
- [ ] `cargo clippy -- -D warnings`（新增代码无 lint）
- [ ] 记忆沉淀: PIT-77（vendored patch 全流程）+ status.md 更新（分支、webrtc-sys 0.3.41 vendored）
- [ ] 提交（按 slice: T1 → T2 → T3 验证 → T4）

## 3. 验收标准

| 标准 | 验证方式 | 通过条件 |
|------|---------|---------|
| 关键帧周期 | mediasoup worker 日志 | `key frame received` 间隔 ≤ 2.5s |
| 首帧延迟 | 浏览器时间戳日志 | Play → videoWidth < 2s |
| 回归 | e2e_sfu 测试 | 4/4 通过 |
| 编译 | cargo check/clippy | 无新 warning/error |
| 依赖干净 | `cargo tree -p audemsp-host -i mediasoup-sys` | 空（C21 保持）|

## 4. 风险与缓解

| 风险 | 缓解 |
|------|------|
| `GenerateKeyFrame` 对当前编码帧时序敏感（频繁调用可能退化画质）| 2s 间隔保守；若实测有花屏改为 3-4s |
| webrtc-rs 0.12 无 request_key_frame 等价物 | T1 检查；若无则 webrtc-rs 后端标 TODO（该后端非默认）|
| vendored 与未来 webrtc-sys 升级冲突 | Cargo.toml 注释 + patch 版本不匹配时解析失败（显式暴露）|
| set_parameters 全量替换参数副作用 | 只改 request_key_frame 字段，其余字段保持当前值回填 |

## 5. 相关文件

- `vendor/webrtc-sys/` — vendored 0.3.41 + 2 行 patch
- `crates/audemsp-webrtc/src/backend/webrtc_sys.rs` — 构造点（1188）+ 转换（93）+ 待加 request_key_frame()
- `crates/audemsp-webrtc/src/rtp.rs` — RTCRtpEncodingParameters（91-143）
- `crates/audemsp-webrtc/src/traits.rs` — RtpSender trait（三后端）
- `crates/audemsp-host/src/main.rs` — 帧循环（~370）+ 协商（~287）
- `crates/audemsp-host/config/host.conf` — 推流配置
- `docs/reference/webrtc/gop-control-internal-encoder.md` — 调研结论
- `.agents/memorys/pitfalls.md` — PIT-76 根因链
