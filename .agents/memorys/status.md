# OMSPBase Status

**生成**: 2026-07-28 | 决策: 195+ (D1-D202) | Phase: 3 完成 | 91 commits

**当前**: 7 crate workspace。Phase 2 mediasoup SFU 集成完成。Phase 3 生产就绪全部完成。Docker/CI/DevContainer 就位。macOS 混合开发工作流。P2P 管线生产就绪。**OpenCode 配置优化完成** (D199-D202)。

**当前**: 7 crate workspace。Phase 2 mediasoup SFU 集成完成。Phase 3 生产就绪全部完成 (3A P0 安全+容错, 3B P1 日志+文档, 3C P2 高级特性)。Docker/CI/DevContainer 就位，全部 Ubuntu 22.04 LTS。macOS 混合开发工作流 (Host/Client 原生 + Server Docker)。P2P 管线生产就绪。

## 测试

| Crate | Lib Tests | Integration | 备注 |
|-------|:---------:|:------------:|------|
| omspbase-common | 68 | — | +backup +logging +auth tests |
| omspbase-media | 54 | — | |
| omspbase-webrtc (stub) | 11 | 67+ | |
| omspbase-webrtc (webrtc-sys) | 11 | 49 (4 ICE 预存) | |
| omspbase-webrtc (webrtc-rs) | 11 | 29 (9 SDP/ICE 预存) | |
| omspbase-codec (stub) | 0 | 32 | |
| omspbase-codec (FFmpeg) | 0 | 35 | |
| omspbase-codec (GStreamer) | 0 | 27 | pixi 环境 |
| omspbase-server | 12 | 30 (25 e2e + 5 integration) | +2 SFU E2E (Linux only) |
| omspbase-host | — | E2E 脚本 9/9 ✅ | macOS native |
| omspbase-client | — | E2E 脚本 9/9 ✅ | macOS native |

### macOS E2E 验证 (2026-07-24)
```
Host (macOS) → WS :9800 → Docker Server → WS :9800 → Client (macOS)
                         └── P2P WebRTC (574 bytes relayed) ──┘
9/9 tests pass: Server health → Build → Host connect → Client connect → SDP → DC → Relay
```

| Phase | 状态 |
|-------|:----:|
| 0-1 基础设施 | ✅ |
| 2a-2d mediasoup SFU | ✅ |
| 3A P0 安全+容错 | ✅ |
| 3B P1 日志+文档 | ✅ |
| 3C P2 高级特性 | ✅ |
| Docker/CI/DevContainer | ✅ |
| macOS E2E 验证 | ✅ |
| Admin Dashboard (P1-P5) | ✅ |
| Admin Dashboard (P6) | 🟡 |
| OpenCode 配置优化 | ✅ |

## 决策状态

| 决策 | 内容 | 状态 | Phase |
|------|------|:----:|:-----:|
| D124-D195 | (见 decisions.md) | ✅ | 0-3 |
| D196 | Admin Dashboard 架构 | ✅ | 4 |
| D197 | D87 范围限定 (Client GUI only) | ✅ | 4 |
| D198 | SFU Server-Offer 架构 | ✅ | 4 |
| D199 | Instructions 精简化 | ✅ | Config |
| D200 | OMO Agent 模型分配优化 | ✅ | Config |
| D201 | Pre-commit Hook | ✅ | Config |
| D202 | Global Provider Config 修复 | ✅ | Config |

## Admin Dashboard 测试

| Crate | Lib Tests |
|-------|:---------:|
| omspbase-common | 71 (+3) |
| omspbase-server | 32 (新增 admin) |
| omspbase-server e2e | 25 |
| omspbase-server integration | 5 |

## SFU Video Playback

| Phase | 状态 |
|-------|:----:|
| Docker SFU Foundation | ✅ |
| Browser SFU Client | ✅ (Server-Offer) |
| Admin WS SFU Routing | ✅ |
| Web UI (Video Grid + Metrics) | 🟡 |
| Host SFU Produce | ✅ (VideoFrameGenerator squares) |
| Integration E2E | 🟡 (transport connect deferred) |

### SFU 当前阻塞

mediasoup transport connect 是 deferred 的：`handle_sfu_message` 中 ConnectWebRtcTransport
只记录日志返回 "transport_connected"，未调用 mediasoup 实际 `transport.connect()` API。
浏览器 RTCPeerConnection 无法与 mediasoup transport 建立 DTLS/ICE 连接。

### 下一步

1. 修复 ConnectWebRtcTransport：调用 `sfu.connect_transport()` 实际连接
2. 实现 Consume 消息处理：创建 mediasoup consumer → 返回 RTP params
3. 浏览器 ontrack → video.srcObject → 视频帧渲染
4. Playwright 端到端验证
