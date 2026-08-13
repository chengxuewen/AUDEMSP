# 14. Remote 应用架构

> Phase 0 — 架构定义 | 2026-07-17
> 关联决策: D66-D69, D70-D72, D76, D80, D118 + 主文档引用: [架构文档](../architecture.md) §2, §7

## 14.1 概述

mediaservo-client 是座舱侧应用程序，从 Server 拉取视频流，解码渲染到屏幕，同时通过 RTCDataChannel 发送控制指令。Remote 只拉流和控制，不推流。

```
Server (mediaservo-server)
  │ WebRTC forward (RTP/SRTP)
  │ Signaling (WS /ws, JSON SDP/ICE)
  ▼
mediaservo-client
  ┌────────────────────────────────────────────┐
  │  Signaling Client                          │
  │  axum WS, join room, SDP/ICE 交换          │
  ├────────────────────────────────────────────┤
  │  Pull Engine (libwebrtc RTCPeerConnection)    │
  │  订阅 Server 转发的 Host 视频流            │
  │  H.264/H.265 RTP stream                    │
  ├────────────────────────────────────────────┤
  │  Decoder Pipeline                          │
  │  libwebrtc 内置(codec) / FFmpeg (str0m)    │
  │  硬件解码: NVDEC/VAAPI/VideoToolbox        │
  │  软件 fallback: VP8 libvpx / H.264 openh264│
  ├────────────────────────────────────────────┤
  │  Render                                    │
  │  Phase 1: CPU 回读 → Canvas                │
  │  Phase 2: GPU direct (wgpu interop)        │
  ├────────────────────────────────────────────┤
  │  RTCDataChannel Control                       │
  │  控制指令发送 (转向/刹车/油门)             │
  │  HMAC-SHA256 签名, unordered=0 重传        │
  ├────────────────────────────────────────────┤
  │  Monitoring                                │
  │  /health /ready /metrics                   │
  │  解码性能、控制帧丢率                      │
  └────────────────────────────────────────────┘
```

## 14.2 核心组件

| 组件 | 职责 | 决策 |
|------|------|------|
| Signaling Client | 连接 Server WS，SDP/ICE 交换，房间管理 | D52, D54 |
| Pull Engine | libwebrtc RTCPeerConnection 拉流，订阅 Host 视频 | D66, D118 |
| Decoder | 解码 H.264/H.265 帧。libwebrtc 内置 codec 或 FFmpeg | D70-D72 |
| Render | 渲染解码后帧到屏幕。Phase 1 CPU 回读，Phase 2 GPU direct | D47 |
| RTCDataChannel Control | 发送控制指令 (转向/刹车/油门)，HMAC-SHA256 签名 | D66, D117 |
| Config | remote.conf (YAML)，只读本地配置 | D80 |

## 14.3 技术栈

| 组件 | 选型 | 理由 |
|------|------|------|
| WebRTC 拉流 | libwebrtc (via webrtc-sys) | 内置 VP8/VP9/H.264 编解码，弱网抗性 |
| 解码 | libwebrtc 内置 codec (默认) / FFmpeg (str0m 后端) | D70-D72 |
| 渲染 | CPU 回读 appsink → Canvas (Phase 1) | 极简实现 |
| 控制 | RTCDataChannel (unordered, maxRetransmits=0) | 低延迟控制指令 |
| 签名 | HMAC-SHA256, DTLS-SRTP 派生密钥 | D117 |
| HTTP | axum 0.7 (/health /ready /metrics) | 与 Server/Host 统一 |
| 配置 | serde_yaml + env 覆盖 | 全局统一模式 |
| 日志 | tracing-subscriber (JSON stdout) | 与 Server/Host 统一 |
| 平台 | macOS / Linux / Windows | D118 跨平台桌面基线 |

## 14.4 数据流

```
Server → WebRTC RTP (H.264/H.265)
  │
  ▼
libwebrtc RTCPeerConnection (pull)
  │ 解码后帧 (I420/NV12)
  ▼
Decoder (libwebrtc 内置 / FFmpeg)
  │ 解码后 RGBA buffer
  ▼
Render (CPU fallback → GPU direct Phase 2)
  │ 帧显示到窗口
  │
  ─── RTCDataChannel (控制指令, 反向流向 Host)
       steering / brake / throttle
       HMAC-SHA256 signed
```

## 14.5 Remote vs Client

| 维度 | mediaservo-client | mediaservo-client |
|------|----------------|-----------------|
| GUI | 无 GUI (纯 SDK) / 简单渲染窗口 | Tauri v2 全功能桌面应用 |
| SDK 形态 | mediaservo-client-c (.a + .so + .h) | 直接依赖 Rust crate |
| 场景 | C/C++ 嵌入 (ROS/自驾/移动端) | 操作员桌面应用 |
| 解码 | libwebrtc 内置 / FFmpeg | 同 remote |
| 控制 | RTCDataChannel 控制指令 | 全功能 GUI 操作 |
| 配置 | remote.conf (只读本地) | GUI 设置面板 |

## 14.6 平台支持

| 平台 | 采集 | 解码 | 硬件加速 |
|------|------|------|---------|
| macOS | — | VideoToolbox | VT H.264/H.265 |
| Linux | — | VAAPI / NVDEC | VAAPI H.264, NVDEC |
| Windows | — | D3D11VA / NVDEC | NVDEC H.264/H.265 |

Remote 不需要采集。解码优先使用硬件加速，GPU 不可用时 fallback 到软件解码 (libvpx VP8 / openh264 H.264)。

## 14.7 状态机

```
INIT → CONNECTING (连接 Server) → RECEIVING (接收+渲染)
  │                                    │
  └── shutdown ──────────────────────┘
                                      │
                                (断连) → CONNECTING (指数退避重连)
```

## 14.8 交叉引用

- Remote 作为 SDK 层: [SDK 分层](04-sdk-layers.md)
- Client 与 Host 对比: [客户端与 Host](03-client-host.md)
- 信令协议: [信令架构](10-signaling-architecture.md)
- 安全模型: [架构文档](../architecture.md) §3 (Auth)
- Server 中继: [Server 架构](13-server-architecture.md)