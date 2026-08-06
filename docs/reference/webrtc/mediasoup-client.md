# mediasoup 客户端架构参考（画像 · 用法 · 架构 · 案例）

> 创建: 2026-08-05 | 来源: `.refinfo/` 官方源码 + 团队审核实证
> 参考入口: `docs/reference/webrtc/mediasoup-refs.md`（导航）
> 适用: AUDEMSP Host SFU produce / consume 对齐官方用法（C18）

## 1. 画像（Overview）

mediasoup 是一套 **SFU 多媒体服务器 + 客户端 SDK**：

| 组件 | 语言 | 角色 |
|------|------|------|
| **mediasoup** (server) | Node.js + C++ Worker | SFU 核心：Router/Transport/Producer/Consumer/DataProducer/DataConsumer |
| **mediasoup-client** | JavaScript (浏览器) | 官方 JS 客户端 SDK |
| **libmediasoupclient** | C++ | 官方 C++ 客户端 SDK（基于 libwebrtc） |
| **mediasoup-demo** | JS | 官方端到端 demo（信令 + 媒体协商完整流程） |

**核心设计**: 媒体面**纯 ORTC**（RTP 参数匹配），**不交换 SDP**。信令面只传 JSON 对象（rtpParameters、dtlsParameters、iceParameters），SDP 由客户端内部构造（本地 offer + 客户端自构 answer）。

**🔑 最重要的事实（团队审核实证）**: 服务端**从不返回 SDP answer**。客户端是 offerer —— `addTransceiver(sendonly) → createOffer → setLocalDescription(offer) → 发 rtpParameters → 服务端 produce → 客户端自构 answer → setRemoteDescription(answer)`。

## 2. 官方 produce 协商流程（标准）

### 2.1 JS — `Chrome74.ts send()`（`.refinfo/mediasoup-client`）

```
① pc.addTransceiver(track, { direction: 'sendonly', streams, sendEncodings })
② let offer = await pc.createOffer()
③ ortc.getSendingRtpParameters(kind, extendedCapabilities)
   → sendingRtpParameters (ssrc/PT/fmtp 从 offer 推导)
④ await pc.setLocalDescription(offer)
⑤ 发 sendingRtpParameters → 服务端 transport.produce() → 回 rtpParameters (无 SDP)
⑥ 客户端自构 answer: this._remoteSdp.send({offerMediaObject, offerRtpParameters,
   answerRtpParameters}) → answer = { sdp: this._remoteSdp.getSdp() }
   await pc.setRemoteDescription(answer)
```

**关键代码**（verbatim，file:line）:
```ts
// Chrome74.ts:360-364
const transceiver = this._pc.addTransceiver(track, {
    direction: 'sendonly',
    streams: [this._sendStream],
    sendEncodings: encodings,
});
// Chrome74.ts:394-397
const sendingRtpParameters = ortc.getSendingRtpParameters(
    track.kind, sendExtendedRtpCapabilities);
// Chrome74.ts:542-560  ← 客户端自构 answer, 非服务端返回!
this._remoteSdp.send({ offerMediaObject, offerRtpParameters: sendingRtpParameters,
    answerRtpParameters: sendingRemoteRtpParameters, codecOptions });
const answer = { type: 'answer', sdp: this._remoteSdp.getSdp() };
await this._pc.setRemoteDescription(answer);
```

### 2.2 C++ — `SendHandler::Send()`（`.refinfo/libmediasoupclient`）

```
① pc->AddTransceiver(track, { direction: kSendOnly, send_encodings })
② offer = pc->CreateOffer(options)
③ ortc::getSendingRtpParameters(kind, ...) → sendingRtpParameters
④ pc->SetLocalDescription(kOffer, offer)
⑤ 发 sendingRtpParameters → transport.produce()
⑥ answer = remoteSdp->GetSdp()  ← 客户端自构
   pc->SetRemoteDescription(kAnswer, answer)
```

**关键**（verbatim，file:line）:
```cpp
// Handler.cpp:191-200
webrtc::RtpTransceiverInit transceiverInit;
transceiverInit.direction = webrtc::RtpTransceiverDirection::kSendOnly;
if (encodings && !encodings->empty())
    transceiverInit.send_encodings = *encodings;
auto transceiver = this->pc->AddTransceiver(scopedTrack, transceiverInit);
// Handler.cpp:365-369  ← ssrc 来自 offer SDP, 客户端自构 answer
auto answer = this->remoteSdp->GetSdp();
this->pc->SetRemoteDescription(webrtc::SdpType::kAnswer, answer);
```

## 3. 关键机制（架构细节）

### 3.1 ssrc / PT / fmtp 来源

| 参数 | 来源 | 说明 |
|------|------|------|
| **ssrc** | libwebrtc 生成的 offer SDP（`getRtpEncodings`，Handler.cpp:306-308） | 非手工硬编码 |
| **payloadType** | `getSendingRtpParameters`（ortc.cpp:1473） | 从协商能力 `localPayloadType` 推导 |
| **codec parameters (fmtp)** | `getSendingRtpParameters` `localParameters` + `codecOptions` | 官方只注入 `x-google-{start,max,min}-bitrate`（MediaSection.cpp:255-277） |

### 3.2 客户端自构 answer 机制

`RemoteSdp::Send()`（RemoteSdp.cpp:157-185）用 `offerMediaObject`（本地）+ `offerRtpParameters`（发送参数）+ `answerRtpParameters`（remote 参数）+ `codecOptions` 构造 `AnswerMediaSection`，生成 answer 的 m= 段。**codec 参数经 answer 的 fmtp 行写回，随后 `setRemoteDescription(answer)` 让 libwebrtc 读取**。

### 3.3 关键帧 / 码率控制

- **官方不用 `x-google-max-keyframe-interval`**（webrtc-sys `RtpEncodingParameters` 无此字段；官方 MediaSection.cpp 只注入 bitrate 三件套）
- 关键帧靠**标准 PLI → RequestKeyFrame → IDR 反馈链路**（roffer + sendonly 方向协商完整时生效）
- 码率控制: `send_encodings.max_bitrate` / `codecOptions.videoGoogleMaxBitrate`

### 3.4 方向

- Producer 侧: mediasoup 是**接收方**，remote SDP 方向 **recvonly**（官方客户端 remote 也 recvonly）。真正的差异是**"谁生成本地 m= 行"**（offer vs answer），不是方向本身（审计修正，纠正初版"方向反转"误判）。

## 4. 案例对照：AUDEMSP Host 当前实现 vs 官方

| # | 官方 | AUDEMSP Host 现状 | 问题 |
|---|------|------------------|------|
| 1 | `addTransceiver(sendonly)` 生成 offer | `setRemoteDescription(手工 recvonly SDP)` | 绕过协商，codec 参数放错位置 |
| 2 | `addTrack(track, stream)` / `addTransceiver` | `add_track("video")` 非 W3C 签名 | 无 sendEncodings |
| 3 | `createOffer` + `setLocalDescription(offer)` | `create_answer()` 在 remote offer 上做 | 角色逆转，协商语义错位 |
| 4 | `getSendingRtpParameters` 从 offer 推导 | 手工 `build_produce_rtp_parameters` (PT=101) | 双硬编码（PIT-54） |
| 5 | ssrc 从 offer SDP 解析 | 手工 `negotiated_ssrc_from_sdp`(answer) | 绕过标准推导 |

**重构方案**: 见 `.sisyphus/plans/host-sfu-w3c-alignment/plan.md`（P1 webrtc 补 API + P2 Host 标准协商）。

## 5. 相关文档

- `mediasoup-refs.md` — 源码导航 + 脚本用法
- `webrtc-w3c-alignment.md` — 对齐分析（含 §0 团队审核 3 处修正）
- `.agents/memorys/conventions.md` C18 — 官方用法优先约束
- `.sisyphus/plans/host-sfu-w3c-alignment/plan.md` — 重构计划