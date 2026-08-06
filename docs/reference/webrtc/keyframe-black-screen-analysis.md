# PIT-65 关键帧黑屏 — 根因分析与架构对比

**状态**: 分析中 (2026-08-05) | **关联**: PIT-65 | **主题**: Host raw I420 路径关键帧不可控 → 多页 consume 黑屏

## 0. 实验更新（x-google-max-keyframe-interval 验证结果）

**2026-08-05 实证**: 在 `build_remote_sdp` fmtp 行加 `x-google-max-keyframe-interval=2000` 后重建 Host，关键帧时间序列：

```
09:13:48 → 09:13:50 (2s)   # 启动斜坡突发
09:13:50 → 09:13:55 (5s)   # 启动斜坡突发
09:13:55 → 09:14:10 (15s)  # 启动斜坡突发
09:14:10 → 09:14:15 (5s)   # 启动斜坡突发
09:14:15 → 09:15:54 (99s)  # 稳态回落 → ~99s GOP
```

**结论: 快速验证失败**。稳态关键帧间隔仍 ~99s，参数未生效。与 b=AS:500 同为"启动突发 + 稳态回落 99s"模式。
**可能原因**: fmtp 加在 **remote SDP**（mediasoup 侧描述），而 Host 发送编码器的 keyframe interval 从 **local answer 的 negotiated codec parameters** 读取——我们手工构造 SDP 绕过了 libwebrtc 正常 codec-parameter 合并，参数未到达 OpenH264。

**下一步**: 转向方案 A（诊断原生 PLI 断点 H1/H2/H3，架构最优）。

## 1. 问题现象

3 个浏览器页面顺序连接到 mediasoup SFU 拉流（SquaresPattern 视频帧生成器版本），每页等待 60s：

- b=AS:2000 基线: `page1: OK | page2: BLACK | page3: BLACK`（1 成功 / 2 黑屏）
- b=AS:500 实验: `page1: BLACK | page2: BLACK | page3: BLACK`（全黑）

黑屏页面 consumer 已创建、transport 已连接（mediasoup 持续发 sync packet），但**始终等不到关键帧** → syncRequired 不解除 → 不转发视频帧。

## 2. 根因（已确认）

**Host 编码器每 ~99s 才产生一个关键帧（GOP ≈ 3000 帧 @30fps），且 consume 侧 PLI 请求无法强制其提前产生关键帧。** 晚加入的 consumer 在其 60s 等待窗口内等不到关键帧 → 黑屏。

### 证据链

| 证据 | 位置 |
|------|------|
| 关键帧间隔稳定 ~99s | server 日志 `key frame received [ssrc:...]` 间隔 08:32:14→08:33:53→08:35:32... 精确 99s |
| b=AS:500 不改变 GOP | 08:46:36-43 的 2-5s 是编码器启动斜坡突发，后稳定回落到 ~99s（08:48:22 后） |
| mediasoup 侧正常请求关键帧 | `ConsumerOptions.paused=false` 时 mediasoup 立即向 remote Producer 请求关键帧（mediasoup-rs consumer.rs:112-115） |
| Host 不响应 PLI | consumer 08:49:34 连接，直到 08:50:02（~99s 计划关键帧）才出现 producer 关键帧，28s 内无 PLI 触发 |
| 关键帧间隔排除项 | b=AS:2000 与 500 都是 ~99s → 非码率驱动 |

## 3. 排除项（已逐一否决）

- **b=AS**: 2000 与 500 稳态 GOP 均 ~99s → 非根因
- **编码器跳帧 / seq 不连续**: b=AS:2000 已修复（seq 连续），非根因
- **PLI 风暴**: 前端已移除 request_key_frame，非根因
- **分辨率**: 320x240 更差，非根因
- **peer_id 覆盖**: 已隔离（每页独立 sfuPeerId），仍黑 → 非根因
- **Consumer IsActive**: CONS-DUMP score=10 满活跃，非根因
- **ICE/DTLS**: 全就绪，非根因

## 4. 架构对比分析（对标官方客户端）

### 官方客户端如何关键帧？

| 客户端 | 方法 | 关键帧机制 |
|--------|------|-----------|
| **libmediasoupclient (C++)** | 原生 `MediaStreamTrackInterface*` → `pc->AddTransceiver(track, kSendOnly)` | **零关键帧/GOP 配置**，完全依赖原生 libwebrtc 编码器，原生 PLI 响应正常 |
| **mediasoup-client (JS)** | 浏览器原生 RTCRtpSender/Transceiver | 浏览器 libwebrtc 原生处理 PLI + GOP，无此问题 |
| **我们的 Host** | livekit webrtc-sys 自定义 raw I420 VideoTrackSource | `on_captured_frame` → `AdaptFrame` → `OnFrame`，**完全不感知关键帧** |

### 关键发现：raw 路径与原生 track 走同一条编码器管线

```
原生摄像头 track ──┐
                ├─→ VideoTrack → RtpVideoSender → VideoStreamEncoder → 真实编码器(OpenH264)
raw I420 source ──┘              └─ PLI → RequestKeyFrame → IDR（原生机制）
```

**两种 source 走同一个 VideoStreamEncoder**。PLI→IDR 是编码器层原生机制，理论上与 source 类型无关。这引出核心问题：

> **既然原生 PLI 在 libmediasoupclient 正常，为什么在我们 Host 不生效？**

我的初始判断（推 encoded 路径）是**过度反应**——把"raw 路径不检查 `keyframe_request_flag`"误当成"raw 路径不响应 PLI"。这是两个不同机制：
- `keyframe_request_flag` — 仅 encoded 路径的 PassthroughVideoEncoder 使用（应用层轮询）
- **原生 PLI → VideoStreamEncoder** — raw 路径一样走，应当正常工作

### 待诊断的 PLI 断点（PIT-50 归因顺序）

| 假设 | 验证方法 |
|------|---------|
| H1: mediasoup Consumer 的 PLI 没发出去（producer 侧无 RTCP PLI） | 查 mediasoup worker `RTC::Producer` RTCP 日志 |
| H2: PLI 到了 Host 但 webrtc-sys PC 未处理入站 RTCP（sendonly 方向未启用 RTCP 接收） | tcpdump Host RTP 端口 + libwebrtc verbose LogSink（`webrtc_sys::new_log_sink`） |
| H3: PLI 到了编码器但 OpenH264 长 GOP 忽略 PLI | 换 backend-webrtc-rs 对照，或 libwebrtc 编码器日志 |

## 5. 修复方向分叉

### 方案 A: 让原生 PLI 生效（架构最优，对标官方）

诊断 H1/H2/H3 定位 PLI 断点，修复让原生 PLI 触发 IDR。**这是对标本尊、根治问题**。

- 优势: 关键帧恢复正常，无需改架构
- 劣势: 需先诊断定位断点（tcpdump + LogSink），可能触及 webrtc-sys PC 的 RTCP 接收配置

### 方案 B: SDP fmtp 加 `x-google-max-keyframe-interval=2000`

libwebrtc 官方标准机制（mediasoup-demo 也用），在 SDP fmtp 行配置软件编码器关键帧间隔。

- 优势: 一行改动，webrtc-sys 内，快速验证
- 劣势: 依赖 OpenH264 是否解析该 Google 扩展参数（非 RFC 标准，有风险）；仍是战术性修复，受 libwebrtc 黑盒约束

### 方案 C: encoded 路径 + audemsp-codec（架构改造，长期）

switching to `capture_encoded_frame` + `take_keyframe_request()`，关键帧完全应用层可控。

- 优势: 关键帧 100% 显式可控（GOP 配置 + PLI 闭环），可扩展（H.265/VP9/AV1/simulcast）
- 劣势: 大改动（Host 依赖 audemsp-codec + 新路径 + 帧循环改造）

### 决策建议

先做 **方案 A（诊断原生 PLI 断点）**——这是对标本尊、根治问题的架构最优路径。C 是过度反应（我修正）。B 可作快速验证但不该作为架构决策。

## 6. 待办

- [ ] 诊断 H1/H2/H3 定位 PLI 断点（tcpdump Host RTP 端口 + libwebrtc verbose LogSink）
- [ ] 确认 mediasoup producer 是否实际发送 RTCP PLI（H1）
- [ ] 确认 webrtc-sys PC sendonly 方向是否启用 RTCP 接收（H2）
- [ ] 修复后重建 Host + server，实测关键帧间隔 + 多页 E2E
- [ ] 提交 peer_id 隔离修复（未提交，架构正确性，非黑屏根因）

## 7. 相关文件

- `crates/audemsp-host/src/sfu_media.rs`: `build_remote_sdp`（b=AS:2000, fmtp 行）
- `crates/audemsp-host/src/main.rs`: SFU 帧循环（SquaresPattern + 绝对时间轴, 640x480）; `keyframe_interval` 死配置 (line 592)
- `crates/audemsp-webrtc/src/backend/webrtc_sys.rs`: `WebrtcSysTrack` raw I420 路径 (line 587 on_captured_frame)
- `crates/audemsp-server/src/sfu.rs`: `create_consumer` (line 353)
- `~/.cargo/registry/src/*/webrtc-sys-0.3.41/src/video_track.cpp`: raw 路径 (167-247) 不查 keyframe flag; encoded 路径 (309) 用 flag
- `~/.cargo/registry/src/*/mediasoup-0.24.1/src/router/consumer.rs`: ConsumerOptions paused 注释 (112-115); `request_key_frame()` (1234)
- versionsatica/libmediasoupclient `src/Handler.cpp`: `SendHandler::Send()` 原生 track + 零关键帧配置
- livekit/rust-sdks webrtc-sys `src/passthrough_video_encoder.cpp`: 240-253 关键帧请求转发机制