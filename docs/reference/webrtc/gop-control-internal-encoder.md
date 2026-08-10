# libwebrtc 内部编码器 GOP 控制分析 — 首帧延迟根因与修复路径

> 文档: 2026-08-10 | 状态: **调研完成，方案待实施** | 关联: [keyframe-black-screen-analysis.md](keyframe-black-screen-analysis.md) (PIT-65)
> 背景: Host 推流首帧延迟 68s → 定位为 libwebrtc 内部编码器稳态 GOP 99s + PLI 响应断裂

## 1. 问题现象

| 指标 | 实测值 |
|------|--------|
| 协商 + ICE + consume 耗时 | 73ms（正常）|
| consumed → 首帧渲染 | **68.4s**（异常）|
| Host 稳态关键帧间隔 | **99s**（mediasoup worker `key frame received` 日志证实）|
| consume 后 request_key_frame (PLI) | 到达 Host UDP（tcpdump 72B 证实）但 **take_keyframe_request() flag 从未置位** |
| `x-google-max-keyframe-interval` SDP 注入（local answer / remote offer）| **均无效**（仅启动瞬态 2s/20s，稳态回落 99s）|

## 2. 根因链（完整）

```
libwebrtc 内部编码器路径 (raw I420 → VideoStreamEncoder → libvpx VP8)
  ├─ GOP 调度: VideoCodecVP8.keyFrameInterval = 0 (默认, 未设置)
  │    → libvpx kf_max_dist = 0 → 不主动出关键帧
  │    → 仅靠场景切换/码率波动偶然出关键帧 → 稳态 99s
  │
  ├─ PLI 响应: mediasoup request_key_frame → RTCP PLI 到达 Host UDP 端口
  │    → 但 livekit fork 的 RtpVideoSender→VideoStreamEncoder RTCP 接线断裂
  │    → take_keyframe_request() flag 从未置位 (诊断日志证实)
  │
  └─ SDP 注入: x-google-max-keyframe-interval 是 Chrome 集成层特有参数
       → 标准 libwebrtc 库内无解析代码 (webrtc_video_engine.cc 无此字符串)
       → livekit fork 保留 GOP 链路但无 SDP 解析入口
```

**本质**：livekit fork 的 libwebrtc 内部编码器路径，**GOP 配置入口与 PLI 响应链路均不可用**——这是 fork 架构性裁剪，非配置问题。

## 3. libwebrtc 原本的 GOP 控制机制（标准链路）

标准 libwebrtc 的周期关键帧 = **编码器层机制**（`kf_max_dist`），非调度层：

```
应用层设置 VideoEncoderConfig.encoder_specific_settings
  = Vp8EncoderSpecificSettings{ VideoCodecVP8{ keyFrameInterval: N } }
  │
  ▼
VideoSendStreamImpl::ReconfigureVideoEncoder(config)   // video_send_stream_impl.cc:611
  ▼
VideoStreamEncoder::ConfigureEncoder → InitEncode(&send_codec_)  // video_stream_encoder.cc:1386
  ▼
libvpx_vp8_encoder.cc:651-653:
  if (inst->VP8().keyFrameInterval > 0) {
    vpx_configs_[0].kf_max_dist = inst->VP8().keyFrameInterval;
  }
  ▼
libvpx 每 kf_max_dist 帧自动出关键帧（确定性，不依赖 PLI）
```

**关键事实**：
1. `VideoCodecVP8.keyFrameInterval`（帧数）→ `kf_max_dist` → libvpx 周期关键帧
2. 入口只有 `VideoEncoderConfig.encoder_specific_settings`（编程 API）
3. **标准 libwebrtc 不解析 SDP fmtp 的 `x-google-max-keyframe-interval`**（那是 Chromium 集成层扩展，非 libwebrtc 库）
4. livekit fork **保留了整条链路**（`Vp8EncoderSpecificSettings`/`FillVideoCodecVp8`/`LibvpxVp8`/`kf_max_dist` 符号均在 libwebrtc.a 中）——只是 webrtc-sys 未暴露设置入口

## 4. 触发单次关键帧的标准机制（方案选型关键）

libwebrtc 标准 API：`RtpEncodingParameters.request_key_frame = true`：

```
RtpSender::SetParameters({ encodings: [{ request_key_frame: true }] })
  ▼
RtpSenderBase::SetParameters (rtp_sender.cc:650)
  ▼
SetRtpParametersOnWorkerThread(media_channel_) → WebRtcVideoChannel::SetParameters
  ▼
webrtc_video_engine.cc:2227: if (encoding.request_key_frame) → GenerateKeyFrame()
  ▼
下一帧出 IDR（确定性，本地调用不经 RTCP）
```

**webrtc-sys 现状**：
- C++ `RtpEncodingParameters` 有 `request_key_frame` 字段（rtp_parameters.h:707）
- Rust 绑定 `RtpEncodingParameters` **缺该字段**（rtp_parameters.rs:126-146）
- `to_native_rtp_encoding_paramters`（rtp_parameters.cpp:98）**未设置它**

## 5. 方案对比

### 5.1 候选方案

| 方案 | GOP 控制 | Simulcast | SVC | 首帧 | 改动量 |
|------|:---:|:---:|:---:|:---:|------|
| **A. 周期性重建 VideoTrackSource** | ✅ 待实测 | ❌ | ❌ | <2s? | 中（main.rs）|
| **B. capture_encoded_frame + passthrough** | ✅ 完全可控 | ❌ (supports_simulcast=false) | ❌ (L1T1) | <0.5s | 大（帧循环改造）|
| **C. 接受现状** | ❌ 99s | — | — | ~50s | 零 |
| **D. request_key_frame 周期触发（推荐）** | ✅ 每 N 秒 1 个 IDR | ✅ 保留 | ✅ 保留 | <2s | **小（2 行 FFI + 帧循环）**|

### 5.2 推荐：方案 D（request_key_frame 周期触发）

**为什么最优**：
- 保留内部编码器路径（Auto 后端，simulcast/SVC/自动降级全保留）——符合"简单内部编码器"倾向
- 走 libwebrtc **标准 API**（`RtpEncodingParameters.request_key_frame`），非 hack
- 改动极小：webrtc-sys 2 行 + audemsp-webrtc 1 方法 + Host 帧循环定时
- 与 PLI 断裂无关（本地 SetParameters，不经 RTCP）
- livekit 官方 `peer_transport.rs` 也做 SDP munging（x-google-start-bitrate）证明 SDP 注入是官方认可路径，但 max-keyframe-interval 无解析 → 需程序化触发

**代价**：每 N 秒强制 IDR 增加带宽（关键帧比 delta 大 ~10x）。2s 间隔在 2Mbps 下约增加 5-8% 带宽——可接受。

## 6. webrtc-sys 依赖方式决策：vendored [patch]（非 fork）

### 6.1 为什么不用 fork + git submodule

| 维度 | fork + submodule | vendored [patch] |
|------|:---:|:---:|
| 改动量 | 2 行 FFI，fork 管理开销 > 收益 | 同改动，零管理开销 |
| 团队工作流 | 嵌套 git，submodule 更新/同步复杂 | 普通目录，git 正常跟踪 |
| libwebrtc 预编译 | 仍从 livekit releases 下载（download_url 不变）| 同 |
| 未来改动扩大 | 可升级为 fork | 可随时升级 |
| C20 合规 | ✅ | ✅（用户同意后 vendored）|

### 6.2 实施方式（已确认）

```bash
mkdir -p vendor
cp -r ~/.cargo/registry/src/index.crates.io-*/webrtc-sys-0.3.39 vendor/webrtc-sys
rm vendor/webrtc-sys/Cargo.toml.orig
```

根 Cargo.toml：
```toml
[patch.crates-io]
webrtc-sys = { path = "vendor/webrtc-sys" }
```

**关键前提**（已验证）：
- registry 的 Cargo.toml 是规范化版——**无 workspace 继承、无 path 依赖，可直接独立编译**
- `webrtc-sys-build`（0.3.18）仍从 crates.io 拉——libwebrtc 下载逻辑不变
- 总大小 6.7M，可入 git

### 6.3 需要改动的文件（最小集）

| 文件 | 改动 |
|------|------|
| `vendor/webrtc-sys/src/rtp_parameters.rs` | `RtpEncodingParameters` 加 `pub request_key_frame: bool` |
| `vendor/webrtc-sys/src/rtp_parameters.cpp` | `to_native_rtp_encoding_paramters` 加 `native.request_key_frame = parameters.request_key_frame;` |
| `crates/audemsp-webrtc/src/...` | `RtpSender` 暴露 `request_key_frame()` 方法 |
| `crates/audemsp-host/src/main.rs` | 帧循环每 2s 调 `request_key_frame()` |

## 7. 关联调研结论（livekit rust-sdks 架构）

参考源码：`.refinfo/rust-sdks`（本地完整 clone，v0.3.41，含 f1d8e5a passthrough 路径）

### 7.1 livekit 发布两条路径

| 路径 | API | GOP 控制 | Simulcast | SVC | 用途 |
|------|-----|:---:|:---:|:---:|------|
| raw（内部编码器）| `capture_frame()` + `VideoEncoderBackend::Auto` | ❌ 无 | ✅ | ✅ | 摄像头/屏幕简单推流（默认）|
| encoded（passthrough）| `capture_encoded_frame()` + `VideoEncoderBackend::PreEncoded` | ✅ `take_keyframe_request()` | ❌ | ❌ L1T1 | 预编码/专业推流 |

### 7.2 livekit 对 GCC/SVC/simulcast 的处理

- **GCC**：SFU 侧（livekit-server Go/Pion）`pkg/sfu/bwe`（TWCC 默认/REMB）；客户端 rust-sdk 经 `take_rate_control_request()` 响应
- **Simulcast**：客户端多 `RtpEncodingParameters`（RID 层）→ libwebrtc SimulcastEncoderAdapter；passthrough `supports_simulcast=false`
- **SVC**：客户端 `scalability_mode`（如 "L3T3_KEY"）→ 内部编码器真 SVC；passthrough 仅 L1T1

### 7.3 关键源码证据（.refinfo/rust-sdks）

- `libwebrtc/src/native/video_source.rs`：`take_keyframe_request` 注释明确"raised by the **pass-through encoder**...forward the request to the **upstream encoder**"——仅为 encoded 路径设计
- `livekit/src/rtc_engine/rtc_session.rs:1920`：`transceiver.sender().set_video_encoder_backend(options.video_encoder)`——发布时显式选后端
- `livekit/src/room/options.rs:170`：默认 `VideoEncoderBackend::Auto`
- `examples/local_video/src/publisher.rs`：官方示例默认 raw 路径 + 可选 backend

## 8. 下一步（待用户确认实施）

1. 创建 `vendor/webrtc-sys`（完整复制 registry 0.3.39）
2. 加 2 行 FFI 改动（`request_key_frame` 字段 + 转换）
3. audemsp-webrtc 暴露 `request_key_frame()`
4. Host 帧循环每 2s 触发
5. 实测：关键帧间隔 ~2s、首帧 <2s、e2e 回归 4/4

## 9. 相关文件

- `docs/reference/webrtc/keyframe-black-screen-analysis.md` — PIT-65 前序分析
- `.agents/memorys/pitfalls.md` — PIT-76（首帧 68s 完整根因链）
- `.agents/memorys/conventions.md` — C20（vendored 合规）、C22（Host 禁 Docker）
- `.refinfo/rust-sdks/` — livekit rust-sdks 本地源码参考
- 标准 libwebrtc 源码（调研时下载）：video_stream_encoder.cc、video_codec.h、video_encoder_config.h、webrtc_video_engine.cc、rtp_sender.cc、rtp_parameters.h、libvpx_vp8_encoder.cc
