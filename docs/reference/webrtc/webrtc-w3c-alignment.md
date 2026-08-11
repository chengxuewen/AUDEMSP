# WebRTC W3C API 对齐重构分析

> 状态: 分析文档（未实施） | 日期: 2026-08-05 | 触发: PIT-65 关键帧问题 + C18 用户显式要求
>
> **结论先行**: 当前 Host SFU produce 流程绕过标准协商，用 5 处"手工构造/最小接口"替代官方 mediasoup 客户端架构。这是 PIT-65 黑屏的架构性根因（`x-google-max-keyframe-interval` 加在错误位置正是绕过协商的恶果）。重构 = 对齐官方 mediasoup-client / libmediasoupclient 的标准 offer/answer 协商流程。
> **结论先行**: 当前 Host SFU produce 流程绕过标准协商。**关键修正（团队审核）: mediasoup 媒体面是纯 ORTC，服务端从不返回 SDP answer —— 官方客户端自构 answer**（Chrome74.ts + libmediasoupclient /tmp 源码实证）。重构 = 对齐官方 mediasoup-client / libmediasoupclient 的标准 offer/answer 协商流程。

## 0. 团队审核修正（2026-08-05, gap-auditor + 官方源码实证）

初版文档 3 处错误，均经官方仓库源码逐行核验：

| # | 初版断言 | 官方源码实证 | 修正 |
|---|---------|-------------|------|
| 1 | §1.1/§3.2 “服务端返回 SDP answer” | mediasoup 媒体面纯 ORTC，**从不返回 SDP**。Chrome74.ts:542-560 answer 客户端自构（`this._remoteSdp.getSdp()`）；libmediasoupclient Handler.cpp:365-369 同（`remoteSdp->GetSdp()`）；本项目 server Produce handler 只回 `Produced{producer_id}`（signaling.rs:715） | 重构 P2 改为“客户端自构 answer”，**server 不改** |
| 2 | §3.3 “x-google-max-keyframe-interval 放 send_encodings” | webrtc-sys `RtpEncodingParameters` 无该字段（rtp_transceiver.rs:21-24）；官方注入路径 = `codecOptions` → answer 的 fmtp（MediaSection.cpp:255-277 只注入 x-google-{start,max,min}-bitrate） | 官方**根本不用 keyframe-interval**；对齐靠标准 PLI→RequestKeyFrame 链路 |
| 3 | §3.1 “需新增 create_offer/set_local/set_remote” | 三者已存在（traits.rs:40,46,49）；真正缺的只有 `add_transceiver` + `get_sending_rtp_parameters`；webrtc-sys 0.3.41 已暴露 `add_transceiver`（peer_connection.rs:171-179） | 缩小 P1 范围 |

**官方流程实证（/tmp 克隆源码）**: 客户端是 offerer —— `addTransceiver(sendonly) → createOffer → getSendingRtpParameters(从 offer 推导) → setLocalDescription(offer) → 发 rtpParameters 等 produce 响应 → 客户端自构 answer(RemoteSdp::Send 用本地 offerMediaObject + 服务端 rtpParameters) → setRemoteDescription(answer)`。ssrc 来自 libwebrtc 生成的 offer SDP（`getRtpEncodings`，Handler.cpp:306-308），非手工硬编码。


## 1. 官方标准流程（对照基准）

### 1.1 mediasoup-client JS — `Chrome74.ts` send() 流程

官方 `mediasoup-client/src/handlers/Chrome74.ts`（`/tmp/mediasoup-client`）标准 produce 协商：

```ts
// Chrome74.ts send()
const pc = this._pc;

// ① 标准 W3C API：addTransceiver（指定方向 + 编码参数）
pc.addTransceiver(track, {
  direction: 'sendonly',
  sendEncodings: [{ ... }],          // 可选的 simulcast/layers
});

// ② createOffer() 让浏览器协商本地能力
const offer = await pc.createOffer();

// ③ 从 offer 的 m= 段提取发送 RTP 参数（ortc.getSendingRtpParameters）
//    —— RTP 参数由浏览器协商结果推导，不是手工构造
const rtpParameters = this._extractRtpParameters(offer);

// ④ setLocalDescription(offer) —— 标准时序
await pc.setLocalDescription(offer);

// ⑤ 发送 offer → 服务端产生 answer
// ⑥ await pc.setRemoteDescription(answer) —— 关键：必须交给标准协商
```

关键点：**所有 RTP 参数（ssrc/PT/fmtp）都从协商结果推导**。**注意（审核修正 §0）：mediasoup 媒体面纯 ORTC，服务端从不返回 SDP answer —— 客户端自构 answer**（`RemoteSdp::Send` 用本地 offerMediaObject + 服务端 rtpParameters 构造）。官方 `setRemoteDescription(answer)` 的 answer 是客户端自构的，非服务端返回。

### 1.2 libmediasoupclient C++ — `SendHandler::Send()`

`/tmp/libmediasoupclient/src/Handler.cpp` 同样的 W3C 模式：

```cpp
// SendHandler::Send()
// ① 标准 W3C：AddTransceiver with direction
pc->AddTransceiver(track, { direction = sendonly });

// ② CreateOffer
auto offer = pc->CreateOffer();

// ③ 从 offer 推导 rtpParameters
auto rtpParameters = ... // from offer

// ④ SetLocalDescription(offer)
// ⑤ 发 offer，等 answer
// ⑥ SetRemoteDescription(answer)
```

### 1.3 mediasoup-demo — `RoomClient.js` produce

`/tmp/mediasoup-demo/app/src/RoomClient.js` 中 produce 流程同样标准：`pc.addTransceiver` → `createOffer` → `setLocalDescription` → 发 offer → `setRemoteDescription(answer)` → 从协商后的发送参数构造 rtpParameters。

## 2. 当前 Host 实现 vs 官方 —— 5 处偏差

当前流程（`crates/audemsp-host/src/main.rs:277-361` + `sfu_media.rs`）：

```
① pc.setRemoteDescription(手工构造 recvonly SDP)   ← 方向反转
② pc.add_track("video")                            ← 非 W3C 最小接口
③ pc.create_answer() + set_local_description       ← 在 remote offer 上做 answer（角色反了）
④ 手工 build_produce_rtp_parameters(ssrc)          ← 硬编码 PT=101
⑤ 从 answer 手工提取 ssrc                          ← 绕过标准 rtpParameters 推导
```

| # | 当前实现 | 官方标准 | 后果 |
|---|---------|---------|------|
| 1 | 手工构造 remote SDP（`a=recvonly`，fprintf 拼接 candidate/fmtp） | `addTransceiver(sendonly)` 让浏览器生成 offer | 方向反转；编码器参数放错位置（fmtp 在 remote 不被本地读）；SDP 手工拼接易错 |
| 2 | `add_track("video", TrackKind::Video)` 非 W3C 签名 | `addTrack(track, stream)` / `addTransceiver(track, {...})` | 无 sendEncodings 能力；方向由 setRemoteDescription 隐式决定而非显式 |
| 3 | `setRemoteDescription(offer)` 后 `create_answer()` | `createOffer()` + `setLocalDescription(offer)` + `setRemoteDescription(answer)` | 角色逆转：Host 当被叫方，mediasoup 才是 offerer；协商语义错位 |
| 4 | 手工 `build_produce_rtp_parameters(ssrc)` 硬编码 H264 PT=101 + 固定参数 | `ortc.getSendingRtpParameters()` 从 offer 推导 | PT/参数与协商结果可能不一致（PIT-54 双硬编码教训）；addTransceiver 的 sendEncodings 未用 |
| 5 | `negotiated_ssrc_from_sdp(&answer.sdp)` 手工解析 ssrc | 从 addTransceiver tracked 的发送参数直接取 | 绕过标准协商；ssrc 硬编码 vs 协商值漂移 |

### 偏差的连锁恶果（PIT-65 实证）

- **`x-google-max-keyframe-interval=2000` 失效**：该参数加在 `build_remote_sdp` 的 fmtp 行（remote SDP），但 libwebrtc 关键帧间隔从**本地 answer 的协商 codec 参数**读取。由于 Host 走 `setRemoteDescription(手工 recvonly offer)` → 本地 answer 不含该参数 → 参数被丢弃 → GOP 仍 ~99s。**审核修正：官方根本不用 keyframe-interval**（webrtc-sys RtpEncodingParameters 无该字段，官方 MediaSection.cpp 只注入 x-google-{start,max,min}-bitrate）—— 对齐靠标准 PLI→RequestKeyFrame 链路，非该参数。
- **PLI 关键帧请求链路异常**：方向反转（recvonly offer）绕过了 libwebrtc 的发送方向协商，PLI → RequestKeyFrame → IDR 反馈链路在 Host 侧不完整（PIT-65 §根因确认：Host 28s 无关键帧响应）。

## 3. 重构方案（对齐官方）

### 3.0 原则

- **audemsp-webrtc 仅暴露完整 W3C WebRTC API**（C18）：`addTransceiver`/`addTrack`/`createOffer`/`setLocalDescription`/`setRemoteDescription`/`onicecandidate`/`ontrack`/`getSendingRtpParameters` 齐全，禁止裁剪成最小接口。
- **Host SFU 走标准 offer/answer 协商**（对齐 libmediasoupclient / mediasoup-client / mediasoup-demo）。
- **ssrc/PT/fmtp 全部从协商结果推导**，禁止手工 JSON rtp_parameters + 手工 SDP 拼接。

### 3.1 audemsp-webrtc：补齐 W3C API 面

`crates/audemsp-webrtc/src/backend/traits.rs` `PeerConnectionApi` 需新增/对齐：

```rust
// W3C 标准接口（webrtc-rs 原生支持，webrtc-sys backend 需在 FFI 层补齐）
async fn add_transceiver(&self, kind: TrackKind, init: RTCRtpTransceiverInit) -> Result<RTCRtpTransceiver>;
// RTCRtpTransceiverInit { direction: Sendonly/Recvonly/Sendrecv, send_encodings: Vec<RTCRtpEncodingParameters> }
async fn create_offer(&self, options: RTCOfferOptions) -> Result<RTCSessionDescription>;
async fn set_remote_description(&self, desc: &RTCSessionDescription) -> Result<()>;
async fn set_local_description(&self, desc: &RTCSessionDescription) -> Result<()>;
fn on_ice_candidate(&mut self, cb: ...);   // 补全 trickle 转发（PIT-43 空桩）
fn get_sending_rtp_parameters(&self, track_id: &str) -> Result<RtpParameters>;  // 从协商结果推导
```

### 3.2 Host 标准 produce 流程（对齐 Chrome74.ts）

替换 `main.rs:277-361` 的 `setRemoteDescription(手工) → add_track → create_answer → build_produce_rtp_parameters`：

```
① pc.add_transceiver(Video, { direction: Sendonly, send_encodings: [H264 encode] })
   // 审核修正: 官方不注入 keyframe-interval (RtpEncodingParameters 无该字段); 码率用 x-google-max-bitrate 或 send_encodings.max_bitrate
② let offer = pc.create_offer()
③ pc.set_local_description(&offer)
④ let rtp_parameters = pc.get_sending_rtp_parameters(track_id)   // 从 offer 推导, 非手工
⑤ 发 rtp_parameters → 服务端 produce → 回 Produced{producer_id} (无 SDP answer)
⑥ 客户端自构 answer (mirror 本地 offer codec 参数 + 服务端 rtpParameters) → set_remote_description(answer)
   // standard时序完成后, PLI → RequestKeyFrame → IDR 反馈链路完整
⑦ write_raw_i420 帧循环（SquaresPattern，不变）
```

### 3.3 关键帧参数的正确位置

- **官方不用 `x-google-max-keyframe-interval`**（审核修正 §0）：webrtc-sys `RtpEncodingParameters` 无该字段，官方 MediaSection.cpp:255-277 只注入 x-google-{start,max,min}-bitrate。关键帧靠**标准 PLI→RequestKeyFrame 链路**（对齐后 Host 是 offerer+sendonly，libwebrtc 发送方向协商完整 → PLI 能触发 IDR）。
- 码率控制：send_encodings.max_bitrate 或 codecOptions x-google-max-bitrate（官方路径），替代当前 b=AS 手工注入。
- 不再依赖手工 remote fmtp（当前失效路径，参数放 remote 被本地 answer 丢弃）。

### 3.4 分阶段实施

| 阶段 | 内容 | 验证 |
|------|------|------|
| P1 | audemsp-webrtc 补 `add_transceiver`/`create_offer`/`get_sending_rtp_parameters`（traits + webrtc-sys backend） | `cargo test -p audemsp-webrtc --features backend-webrtc-sys` |
| P2 | Host SFU produce 改标准协商流程 | E2E 截图渲染 + 关键帧间隔 < 5s |
| P3 | 验证多页面黑屏（PIT-65）修复 | `/tmp/multi-page.cjs` 3 页 + 首帧延迟 |
| P4 | sfu-client.ts（浏览器）同步对齐官方 mediasoup-client 消费流程 | 浏览器 consume 正常 |

## 4. 风险与取舍

- **webrtc-sys backend 接口缺口**：`add_transceiver`/`create_offer`/`get_sending_rtp_parameters` 在 webrtc-sys FFI 层可能未暴露，需在 `webrtc_sys.rs` 补齐（livekit sdk 应支持，参照官方用法落地）。
- **P2P 路径回归**：P2P relay 也走 `PeerConnectionApi`，改 trait 后需回归 P2P E2E。
- **这是架构改动**：按 edit-safety.md 架构决策门，**需用户确认后才实施**。本文档仅分析。

## 7. 相关文件

- `crates/audemsp-host/src/main.rs:277-361` — 当前 SFU produce 流程
- `crates/audemsp-host/src/sfu_media.rs` — `build_remote_sdp` / `build_produce_rtp_parameters` / `negotiated_ssrc_from_sdp`
- `crates/audemsp-webrtc/src/backend/traits.rs` — `PeerConnectionApi` 需补齐的 W3C 接口
- `crates/audemsp-webrtc/src/backend/webrtc_sys.rs` — webrtc-sys backend 实现
- `/tmp/mediasoup-client/src/handlers/Chrome74.ts` — 官方 send() 流程基准
- `/tmp/libmediasoupclient/src/Handler.cpp` — 官方 C++ 客户端基准
- `/tmp/mediasoup-demo/app/src/RoomClient.js` — 官方 demo 基准

---

## 5. 无法实现的 W3C API（标注未来实现）

> 计划: 2026-08-06 v2（团队审核 + W3C 对标审计后） | 依据: webrtc-sys 0.3.x FFI inventory 实测（§6）
> 以下 API 因缺 FFI 或浏览器专属，**不实施**，仅标注供未来触发：

| API | 原因 | 未来触发 |
|-----|------|---------|
| `RTCDTMFSender` | webrtc-sys 无 DTMF FFI（需新 C++） | 遥控/电话网关场景 |
| `setIdentityProvider` / `RTCIdentityAssertion` | webrtc-sys 无 identity FFI | DTLS 证书级身份认证 |
| `generateCertificate`（静态） | libwebrtc 内部自动管理证书，非必需 | 自定义证书指纹 |
| `getDefaultIceServers`（静态） | 浏览器专属 | 无 |
| `sctp` 属性 / `RTCSctpTransport` | 浏览器专属 | 无 |
| `RTCRtpScriptTransform` / `RTCEncodedFrame` | 浏览器专属（插入式 transform） | 服务端 E2E 加密（FrameCryptor 已覆盖） |
| `addStream` / `createDTMFSender` / `removeStream` | 已废弃（obsolete） | 永不 |
| `RTCRtpReceiver.getContributingSources/getSynchronizationSources` | webrtc-sys 无 CSRC/SSRC 列表 FFI | 统计/监控场景 |
| `RTCRtpSender/Receiver.transport` 属性 | webrtc-sys 无 DTLS transport 句柄暴露 | 传输诊断 |

**注意（v2 修正）**: Sender 的 `set_parameters`/`replace_track`/`set_streams`/`get_stats` 与 Receiver 的 `get_parameters` **均可实现**（webrtc-sys FFI 有），属于 host-sfu-w3c-alignment P1/P2 实施范围（已实施完成，D214），**不列入**未来实现。

**audemsp-webrtc 已覆盖/将覆盖**: 上述以外的 W3C RTCPeerConnection / RTCRtpTransceiver / RTCRtpSender / RTCRtpReceiver / RTCDataChannel / RTCRtpCapabilities 接口全部实现（host-sfu-w3c-alignment P0-P2，已实施完成，D214）。

---

## 6. webrtc-sys 0.3.x FFI 能力清单（完整 inventory）

> 实测 2026-08-06 | 源码: `/tmp/webrtc-investigation/rust-sdks/webrtc-sys/src/`

### 6.1 PeerConnection（peer_connection.rs, 18 方法）

```
set_configuration(config) -> Result
create_offer(options, ctx, on_success, on_error)   // async 回调
create_answer(options, ctx, on_success, on_error)  // async 回调
set_local_description(desc, ctx, on_complete)
set_remote_description(desc, ctx, on_complete)
add_track(track, stream_ids) -> Result<SharedPtr<RtpSender>>
remove_track(sender) -> Result
get_stats(ctx, on_stats)  // async 回调, JSON
add_transceiver(track, init) -> Result<SharedPtr<RtpTransceiver>>
add_transceiver_for_media(media_type, init) -> Result<SharedPtr<RtpTransceiver>>
get_senders() -> Vec<RtpSenderPtr>
get_receivers() -> Vec<RtpReceiverPtr>
get_transceivers() -> Vec<RtpTransceiverPtr>
create_data_channel(label, init) -> Result<SharedPtr<DataChannel>>
add_ice_candidate(candidate, ctx, on_complete)
restart_ice()
current_local_description() -> UniquePtr<SessionDescription>
current_remote_description() -> UniquePtr<SessionDescription>
connection_state / signaling_state / ice_gathering_state / ice_connection_state
close()
// 枚举: PeerConnectionState, SignalingState, IceConnectionState, IceGatheringState, ContinualGatheringPolicy, IceTransportsType
// 结构体: RtcOfferAnswerOptions, IceServer, RtcConfiguration
```

### 6.2 RtpTransceiver（rtp_transceiver.rs, 16 方法）

```
media_type() -> MediaType
mid() -> Result<String>
sender() -> SharedPtr<RtpSender>
receiver() -> SharedPtr<RtpReceiver>
stopped() / stopping() -> bool
direction() -> RtpTransceiverDirection
set_direction(direction) -> Result
current_direction() -> Result
fired_direction() -> Result
stop_standard() -> Result
set_codec_preferences(codecs) / codec_preferences()
header_extensions_to_negotiate() / negotiated_header_extensions()
set_header_extensions_to_negotiate(headers)
// 结构体: RtpTransceiverInit { direction, stream_ids, send_encodings }
```

### 6.3 RtpSender（rtp_sender.rs, 12 方法）

```
set_track(track) -> bool
track() -> SharedPtr<MediaStreamTrack>
get_stats(ctx, on_stats)
ssrc() -> u32
media_type() -> MediaType
id() -> String
stream_ids() -> Vec<String>
set_streams(stream_ids)
init_send_encodings() -> Vec<RtpEncodingParameters>
get_parameters() -> RtpParameters
set_parameters(parameters) -> Result
set_video_encoder_backend(backend)
```

### 6.4 RtpReceiver（rtp_receiver.rs, 8 方法）

```
track() -> SharedPtr<MediaStreamTrack>
get_stats(ctx, on_stats)
stream_ids() / streams()
media_type() -> MediaType
id() -> String
get_parameters() -> RtpParameters
set_jitter_buffer_minimum_delay(is_some, delay_seconds)
```

### 6.5 参数类型（rtp_parameters.rs）

```
RtpParameters { transaction_id, mid, codecs, header_extensions, encodings, rtcp, degradation_preference }
RtpEncodingParameters { ssrc, bitrate_priority, network_priority, max_bitrate_bps, min_bitrate_bps, max_framerate, num_temporal_layers, scale_resolution_down_by, scalability_mode, active, rid, adaptive_ptime }
RtpCodecParameters { mime_type, name, kind, payload_type, clock_rate, num_channels, max_ptime, ptime, rtcp_feedback, parameters }
RtpCapabilities { codecs, header_extensions, fec }
RtpCodecCapability { mime_type, name, kind, clock_rate, preferred_payload_type, num_channels, rtcp_feedback, parameters }
RtpHeaderExtensionCapability { uri, preferred_id, preferred_encrypt, direction }
RtcpParameters { ssrc, cname, reduced_size, mux }
RtpExtension { uri, id, encrypt }
// 枚举: FecMechanism, RtcpFeedbackType, RtcpFeedbackMessageType, DegradationPreference, RtpExtensionFilter
```

### 6.6 Factory（peer_connection_factory.rs）

```
create_peer_connection_factory()
create_peer_connection(config, observer)
create_video_track(label, source) / create_audio_track(label, source)
rtp_sender_capabilities(kind) -> RtpCapabilities
rtp_receiver_capabilities(kind) -> RtpCapabilities
// Observer 回调: on_signaling_change / on_renegotiation_needed / on_negotiation_needed_event / on_ice_connection_change / on_connection_change / on_ice_gathering_change / on_ice_candidate / on_ice_candidate_error / on_ice_candidates_removed / on_ice_connection_receiving_change / on_ice_selected_candidate_pair_changed / on_add_track / on_track / on_remove_track
```

### 6.7 DataChannel（data_channel.rs）

```
send(data) / close() / id() / label() / state() / buffered_amount()
// 枚举: Priority, DataState | 结构体: DataChannelInit, DataBuffer
// Observer: on_state_change / on_message / on_buffered_amount_change
```

### 6.8 不透明指针包装（helper.rs）

```
MediaStreamPtr / CandidatePtr / AudioTrackPtr / VideoTrackPtr / RtpSenderPtr / RtpReceiverPtr / RtpTransceiverPtr
// 目的: 绕 cxx 限制 (SharedPtr<T> 不能直接放 rust::Vec)
```
- `docs/reference/webrtc/keyframe-black-screen-analysis.md` — PIT-65 关键帧分析