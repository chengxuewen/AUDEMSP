# SFU — mediasoup Integration

> 状态：Phase 2 设计 | 关联决策：D138 | 创建依据：doc-audit H7

## 概述

mediasoup-sys v0.22 作为 OMSPBase SFU 服务器，负责 Room 内 Host→Server→Remote 的媒体流转发。所有操作封装在 `SfuComponent` 中。

## 核心概念映射

| mediasoup | OMSPBase Component |
|-----------|-------------------|
| Worker | SfuComponent::WorkerPool |
| Router | RoomRouter (per-room) |
| Transport (PlainRtp) | HostRtpTransport (推流) |
| Transport (WebRtc) | RemoteWebRtcTransport (分发) |
| Producer | TrackProducer (上行) |
| Consumer | TrackConsumer (下行) |
| SDP ↔ RtpParameters | SdpAdapter (双向 codec) |

## Worker 生命周期

```
SfuComponent::init()
  ├─ WorkerPool (CPU 核数个 Worker)
  ├─ Worker::create({ rtcMinPort: 40000, rtcMaxPort: 49999 })
  └─ 监听 "died" → 自动重启
```
崩溃恢复：标记 Router dead → RoomManager 通知 Remote 重连 → WorkerPool::respawn → Remote 重建 Transport + Consumer。

## Router 配置

```rust
worker.create_router(RouterOptions {
    media_codecs: vec![
        RtpCodecCapability::Video { mime_type: "video/VP8", ... },
        RtpCodecCapability::Video { mime_type: "video/H264", ... },
        RtpCodecCapability::Audio { mime_type: "audio/opus", ... },
    ],
});
```

每 Room 一个 Router，`room_id` 索引。Room 关闭时销毁。

## Transport 创建

**PlainRtp (Host→Server)**:
```
Host → SDP offer → SdpAdapter::parse_offer()
  → router.createPlainRtpTransport()
  → transport.connect({ip, port})
```

**WebRtc (Server→Remote)**:
```
router.createWebRtcTransport({
    listen_ips: [{ip: "0.0.0.0", announced_ip: server_public_ip}],
    enable_udp: true, enable_tcp: true, prefer_udp: true,
}) → dtlsParameters + iceParameters
```

## Producer / Consumer

```
Host:  transport.produce({kind, rtpParameters}) → Producer {id, kind}
Remote: transport.consume({producer_id, rtp_capabilities}) → Consumer {id, paused: false}
```
Consumer pause/resume 控制带宽。

## SdpAdapter (S8)

双向 SDP ↔ mediasoup RtpParameters：

```rust
trait SdpAdapter {
    fn parse_offer(sdp: &str) -> Result<RtpCapabilities>;
    fn to_send_rtp_params(sdp: &str) -> Result<RtpParameters>;
    fn create_answer(caps: &RtpCapabilities) -> Result<String>;
}
```

内部使用 `webrtc-sdp` crate 解析，编解码器映射。

## Observer 集成

- **AudioLevelObserver**: 每 Router 挂一个，音量跟踪 → 自动静音
- **ActiveSpeakerObserver**: 活跃说话者检测 → 画中画切换 → RoomManager

## 崩溃恢复流程

```
1. Worker "died" → 标记 dead
2. Router → RoomEvent::WorkerLost { room_id }
3. RoomManager → broadcast RejoinRequired
4. WorkerPool::respawn
5. Remote join → 重建 Transport + Consumer
```

> 详见 `.sisyphus/plans/consolidated-mvp/plan.md` Phase 2 (S1-S18)

## Transport 故障场景与恢复

### 1. transport.connect() 超时

| 参数 | 默认值 | 说明 |
|------|--------|------|
| 超时时间 | 10s | mediasoup transport.connect() 默认超时 |
| 错误码 | 4000 | ConnectionFailed |
| 恢复策略 | 重新创建 transport | 不可恢复的连接需重建 transport + re-offer |

```
connect_transport() → timeout → send error 4000 → browser creates new transport → re-offer
```

### 2. DTLS 握手失败

| 故障 | 原因 | 恢复 |
|------|------|------|
| DTLS 指纹不匹配 | 浏览器 DTLS 签名与 transport 不一致 | 重新创建 transport（新 DTLS 参数） |
| SRTP 加密失败 | 浏览器与 mediasoup 协商失败 | 检查 crypto suite 配置（默认 AES_CM_128_HMAC_SHA1_80） |

### 3. ICE 重启

ICE 连接断开后，浏览器端应触发 ICE restart：
```
pc.restartIce()  →  new ICE candidates  →  server re-gathers  →  new candidate pair
```
- ICE restart 保留 transport 和 producer/consumer 不变
- 超过 30s 无新 candidate pair → 认为 transport 失败

### 4. produce() / consume() 失败

| 故障 | 原因 | 恢复 |
|------|------|------|
| Producer 创建失败 | RTP 参数不匹配 | 返回错误 4001，浏览器检查 RTP codec |
| Consumer 创建失败 | RTP capabilities 不匹配 | 返回错误 4002，浏览器检查 receiver capabilities |
| Producer 断开 | Host 端采集停止 | 广播 ProducerClosed，浏览器显示黑屏 |
| Consumer 断开 | SFU 主动暂停 | 触发 pause/resume，浏览器显示缓冲状态 |

### 5. Worker 崩溃（已有）

见上方「崩溃恢复流程」。Worker 崩溃是 transport 级别的终极失败——所有 transport + producer + consumer 全部销毁。

## Transport 安全

### DTLS

| 参数 | 默认值 | 说明 |
|------|--------|------|
| 指纹算法 | SHA-256 | mediasoup 默认使用 SHA-256 证书指纹 |
| 协商模式 | ICE-Lite | SFU 侧简化 ICE（server-side only） |
| DTLS 角色 | auto | 根据 ICE controlling/controlled 角色自动协商 |

### SRTP

| 参数 | 说明 |
|------|------|
| 加密套件 | AES_CM_128_HMAC_SHA1_80（默认） |
| 密钥协商 | 通过 DTLS 握手派生 SRTP 密钥 |
| 重放保护 | 启用（SSRC + ROC 计数器） |

### 安全加固建议

1. **ICE credentials**: 每个 transport 使用独立的 ice-ufrag/ice-pwd（mediasoup 默认行为）
2. **RTCP feedback**: 启用 NACK/FIR/PLI 确保丢包恢复，不依赖上层重传
3. **transport 认证**: 每个 SFU 消息携带 room_id + peer_id 做路由级认证（PIT-08）
4. **端口范围限制**: mediasoup `rtcPortsRange` 收窄到最小 UDP 范围（默认 40000-40100）
5. **消费者隔离**: 不同 consumer 的 RTP 流互不可见，仅通过 mediasoup Router 内部转发


## 编码降级策略

### 降级触发条件

| 指标 | 阈值 | 动作 |
|------|------|------|
| 丢包率 > 5% | 持续 3s | 切换到低码率层 |
| RTT > 300ms | 持续 5s | 降低分辨率 |
| 丢包率 > 15% | 持续 10s | 切换到最低层或暂停 |
| 带宽 < 500kbps | 即时 | 停止视频，保留音频 |

### 码率降级步骤（720p → 360p → 停止）

```
正常: 720p@30  H.264   2-4 Mbps
 ↓ 丢包 >5%
降级1: 480p@30  H.264   0.8-1.5 Mbps
 ↓ 丢包 >15%
降级2: 360p@15  H.264   0.3-0.5 Mbps
 ↓ 带宽 <500kbps
降级3: 停止视频，保留音频 (<50kbps)
```

### 质量层切换（Simulcast）

| 层 | 分辨率 | 帧率 | 码率 | 用途 |
|---|--------|------|------|------|
| High | 720p | 30 | 2-4 Mbps | 全屏播放 |
| Medium | 480p | 30 | 0.8-1.5 Mbps | 缩略图/中屏 |
| Low | 360p | 15 | 0.3-0.5 Mbps | 画中画/预览 |

切换逻辑：
1. SFU 检测 consumer 带宽 → 主动切换 spatial layer
2. 浏览器 receiver 检测 packet loss → 触发 PLI → SFU 切换
3. 手动切换：用户点击"低带宽模式" → 发送 consume 指定 spatial layer
