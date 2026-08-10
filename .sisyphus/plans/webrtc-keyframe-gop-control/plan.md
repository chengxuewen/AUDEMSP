# WebRTC GOP 控制修复 — request_key_frame 周期触发

> 计划: 2026-08-10 (v1) | **v2 修订: 2026-08-10（Momus + Oracle 双审核吸收）** | 触发: PIT-76 首帧延迟 68s（稳态 GOP 99s）
> 分支: `feat/webrtc-request-key-frame`
> 调研文档: `docs/reference/webrtc/gop-control-internal-encoder.md`（根因链 + 方案对比 + vendored 决策，commit `349e964`）
> 状态: **T0-T2 完成，T3 关键指标达成（关键帧 99s→2.0s 实测 12 周期），T4 回归完成；浏览器首帧 <2s 待用户验证**
>
> **v2 修订要点（审核吸收）**:
> - 🔴 **BLOCKER（Oracle）**: `SetParameters` 要求 `transaction_id` 与 `get_parameters()` 返回一致，否则每次 set 直接报错 → T1 必须实现 **get → modify → set 往返**（含 `sender_get_parameters`，当前后端 NotSupported）
> - 🟡 **HIGH（Momus 同证）**: `sender_set_parameters` 后端未实现是 T1 隐含前置步骤，已提升为显式任务
> - 🟡 **MEDIUM（Oracle/Momus 双证）**: 与现有 x-google SDP 注入（main.rs:294-297 remote 500 / :319 local 2000）**二选一**，方案 D 上线后移除注入
> - 🟡 **MEDIUM**: 复用 `host.conf` 现有 `keyframe_interval` 配置（60，语义帧→改秒），不新增 `keyframe_interval_secs`
> - 🟢 带宽预算修正: 5-8% → **10-20%**（Oracle 实测估算）；删除"复位 false"步骤（Oracle: 不必要）；traits.rs 修正为 backend trait（Momus: 无 RtpSender trait）

## 1. 背景与目标

### 1.1 问题
Host SFU 推流首帧渲染 68s。根因（三重证据确认）:
- Host 稳态关键帧间隔 **99s**（`VideoCodecVP8.keyFrameInterval` 未设置 → libvpx `kf_max_dist=0`）
- PLI 响应链路断裂（livekit fork 的 RtpVideoSender→VideoStreamEncoder RTCP 接线缺失，`take_keyframe_request()` 从未置位）
- SDP 注入 `x-google-max-keyframe-interval` 无效（Chrome 集成层特有参数，标准 libwebrtc 不解析；现有注入代码为死代码，见 T2）

### 1.2 目标
- 关键帧间隔 ≤ 2.5s（周期强制，不依赖 PLI/SDP）
- 浏览器首帧 < 2s
- **保留内部编码器路径**（simulcast/SVC/自动降级全保留）
- 走 libwebrtc 标准 API（`RtpEncodingParameters.request_key_frame` → `GenerateKeyFrame`）

### 1.3 方案（已确认）
| 项 | 决策 |
|----|------|
| 触发机制 | `RtpEncodingParameters.request_key_frame = true` 周期调用 `set_parameters`（标准路径，本地即时 IDR）|
| 调用形态 | **get → modify(request_key_frame=true) → set 往返**（transaction_id 一致性硬要求，Oracle BLOCKER）|
| webrtc-sys 依赖 | **vendored `[patch]`**（非 fork+submodule，C20 合规，社区主流做法）|
| vendored 基线 | **0.3.41**（Cargo.lock 已锁定版本，registry 源码 6.8M 完整复制）|
| 触发间隔 | 2s（带宽开销预算 **10-20%** @2Mbps，Oracle 修正；2s GOP 为低延迟业界常规）|

## 2. 任务分解

### ✅ T0: vendored webrtc-sys patch（已完成）
- [x] `vendor/webrtc-sys/` = registry 0.3.41 完整复制（删 `Cargo.toml.orig`/`Cargo.lock`/`.cargo-ok`，6.8M）
- [x] `src/rtp_parameters.rs`: cxx 结构体加 `pub request_key_frame: bool`（含 AUDEMSP 注释）
- [x] `src/rtp_parameters.cpp`: `to_native_rtp_encoding_paramters` 加 `native.request_key_frame = parameters.request_key_frame;`
- [x] 根 `Cargo.toml` `[patch.crates-io] webrtc-sys = { path = "vendor/webrtc-sys" }`（含升级注意事项注释）
- [x] 修 `crates/audemsp-webrtc/src/backend/webrtc_sys.rs:1188` 构造点（加 `request_key_frame: false`）
- [x] `cargo check -p audemsp-webrtc` 通过（patch 生效: `Adding webrtc-sys v0.3.41 (path)`）
- [x] Cargo.lock 更新提交（e1b3a56）

### 🔲 T1: audemsp-webrtc 上层 API（sender get/set 全链路，审核修正后）
**前置（HIGH，Momus/Oracle 同证）**：webrtc-sys 后端当前 `sender_get_parameters` / `sender_set_parameters` 均为 NotSupported 默认（backend/mod.rs:84）——必须全量实现，这是 request_key_frame 的落点。

- [ ] **T1a** `src/backend/webrtc_sys.rs`: 实现 `sender_get_parameters`（调 cxx `RtpSender::get_parameters` → 现有 `map_rtp_parameters` 反向，:93 转换处同步传 `request_key_frame`）
- [ ] **T1b** `src/backend/webrtc_sys.rs`: 实现 `sender_set_parameters`（Rust → cxx 正向转换 `to_native_*`，**保留 `transaction_id` 原值往返**——libwebrtc 校验一致性，违反则每次 set 报错）
- [ ] **T1c** `src/rtp.rs`: `RTCRtpEncodingParameters` 加 `pub request_key_frame: bool`（默认 false）+ `transaction_id` 已在 RTCRtpParameters（核对）
- [ ] **T1d** backend trait（`backend/mod.rs`，非 traits.rs——Momus 指正无独立 RtpSender trait）加 `request_key_frame()` 便捷方法：内部 = get → set true → set（**不复位**，Oracle: 每次调用传 true 恰好触发一次，one-shot 消费）
- [ ] **T1e** stub 后端: no-op（Ok）；webrtc-rs 后端: 确认无 set_parameters（Oracle 已证 webrtc-rs 0.12 无 request_key_frame 且无 set_parameters，TrackLocal 直写 RTP）→ 标 TODO 不实现

### 🔲 T2: Host 周期触发 + 注入去重
- [ ] `crates/audemsp-host/src/main.rs`: 帧循环外独立 tokio task，每 2s 调 `request_key_frame()`
- [ ] 首次触发时机: 协商完成后立即触发一次（快速首帧，不等 2s）
- [ ] **注入去重（MEDIUM）**: 先实测确认现状（PIT-76 声称注入实现 68s→2s 与"稳态 99s"矛盾），确认无效后**移除** `main.rs:294-297` remote 注入 + `main.rs:319` local answer 注入 + `sfu_media::inject_keyframe_interval`（两机制叠加 → 关键帧策略冲突/带宽翻倍）
- [ ] 配置项: **复用** `host.conf` 现有 `keyframe_interval`（60，当前未生效死配置；语义帧→秒，默认 2，改 config.rs 注释 + 文档）

### 🔲 T3: 实测验证（宿主原生 + Docker server，C22）
- [ ] 开 worker debug 日志（mediasoup logLevel=debug 或 RTC_LOG_LEVEL，`key frame received` 是 MS_DEBUG_TAG 级）
- [ ] mediasoup worker 日志 `key frame received` 间隔 ≤ 2.5s（连续 4+ 周期，对比修复前 99s）
- [ ] 浏览器首帧 < 2s（Play 点击 → videoWidth，web 时间戳日志）
- [ ] **带宽实测**（Oracle 修正）: 修复前后 worker/浏览器统计对比，预算 10-20% @2Mbps；若实测 >20% 上调间隔 2s→3s
- [ ] 视频质量抽查（无花屏/卡顿，GOP 强制与 delta 帧无冲突）

### 🔲 T4: 回归 + 沉淀
- [ ] `e2e_sfu` 4/4 通过（Docker server + 宿主 host）
- [ ] workspace 相关 crate 测试通过
- [ ] `cargo clippy -- -D warnings`（新增代码无 lint）
- [ ] 记忆沉淀: PIT-77（vendored patch + transaction_id 往返教训）+ status.md 更新（分支、webrtc-sys 0.3.41 vendored）
- [ ] 提交（按 slice: T1 → T2 → T3 验证 → T4）

## 3. 验收标准

| 标准 | 验证方式 | 通过条件 |
|------|---------|---------|
| 关键帧周期 | mediasoup worker 日志（debug 级）| `key frame received` 间隔 ≤ 2.5s，连续 4+ 周期 |
| 首帧延迟 | 浏览器时间戳日志 | Play → videoWidth < 2s |
| set_parameters 往返 | 单元测试（T1b）| transaction_id 一致时 set 成功；伪造不一致 → Err（防回归）|
| 带宽 | worker/浏览器统计 | 增量 ≤ 20% @2Mbps；超限上调间隔 |
| 回归 | e2e_sfu 测试 | 4/4 通过 |
| 编译 | cargo check/clippy | 无新 warning/error |
| 依赖干净 | `cargo tree -p audemsp-host -i mediasoup-sys` | 空（C21 保持）|
| 注入清理 | `grep -rn "inject_keyframe_interval\|x-google-max" crates/audemsp-host/src/` | 无残留（除文档引用）|

## 4. 风险与缓解（v2 更新）

| 风险 | 缓解 |
|------|------|
| 🔴 **transaction_id 不一致 → 每次 set 报错（方案静默失效）** | T1b 强制 get→modify→set 往返；T1b 单元测试锁行为 |
| 🟡 **字段漂移 → 每 2s ReconfigureEncoder 抖动** | 往返机制保证字段与 get 一致；只改 request_key_frame 一个字段 |
| 🟡 **与现有 x-google 注入策略冲突** | T2 去重：移除注入代码（二选一）|
| `GenerateKeyFrame` 频繁调用退化画质 | 2s 间隔保守；若实测花屏/带宽超 20% 改 3-4s |
| webrtc-rs 0.12 无等价物 | 已确认（Oracle）；标 TODO（非默认后端）|
| vendored 与未来 webrtc-sys 升级冲突 | Cargo.toml 注释 + patch 版本不匹配时解析失败（显式暴露）|
| set_parameters 全量替换副作用 | 往返保证；ssrc/rid/codecs 不可变（libwebrtc 校验，变更报错属预期）|

## 5. 相关文件

- `vendor/webrtc-sys/` — vendored 0.3.41 + 2 行 patch
- `crates/audemsp-webrtc/src/backend/webrtc_sys.rs` — sender_get/set_parameters 实现点（:93 转换、:1164 set、:1188 构造）+ request_key_frame()
- `crates/audemsp-webrtc/src/backend/mod.rs:84` — sender_set_parameters NotSupported 默认（v2: 实际 trait 所在）
- `crates/audemsp-webrtc/src/backend/stub.rs` / `webrtc_rs.rs` — stub no-op / webrtc-rs TODO
- `crates/audemsp-webrtc/src/rtp.rs` — RTCRtpEncodingParameters（91-143）+ transaction_id 核对
- `crates/audemsp-host/src/main.rs` — 帧循环（~370）+ 协商（~287）+ 注入去重（294-297, 319）
- `crates/audemsp-host/src/sfu_media.rs` — inject_keyframe_interval（移除）
- `crates/audemsp-host/config/host.conf` — keyframe_interval 复用（60→2s 语义）
- `docs/reference/webrtc/gop-control-internal-encoder.md` — 调研结论
- `.agents/memorys/pitfalls.md` — PIT-76 根因链
