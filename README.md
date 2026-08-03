# AUDEMSP

AUDEMSP — AUDE 生态多媒体系统。为 AUDESYS 和 AUDEBase 提供统一的多媒体基础设施，涵盖远程桌面、视频会议、直播推拉流、监控相机接入、WebRTC 遥操作等能力。

## 功能范围

- **远程桌面**：屏幕捕获、GPU 编码（H.264/H.265）、输入注入、<100ms 延迟
- **视频会议**：多方音视频通话、SFU/MCU、屏幕共享
- **推拉流**：RTMP/HLS/SRT 接入与分发、直播转码
- **监控接入**：ONVIF/GB28181 相机发现与流管理
- **WebRTC 遥操作**：低延迟视频 + DataChannel 控制（车辆/机器人）
- **车端推流**：车辆摄像头推流到云端 / 舱内拉流

## 架构

```
┌──────────────────────────────────────────────┐
│           AUDEMSP 后台服务                 │
│   用户管理 · 权限控制 · License · 信令       │
└──────────────────┬───────────────────────────┘
                   │ gRPC / REST
    ┌──────────────┼──────────────┐
    ▼              ▼              ▼
┌──────────┐ ┌──────────┐ ┌──────────────┐
│  Client  │ │   Host   │ │ 嵌入/模块     │
│ (GUI)    │ │(headless)│ │ AUDESYS/Base │
│ 操作端   │ │ 远端     │ │              │
└──────────┘ └──────────┘ └──────────────┘
```

- **Client**：桌面 GUI 全功能应用（Tauri v2），可控制他人也可被控制
- **Host**：无 GUI 守护进程，适合边缘设备/服务器/车端，纯产出媒体流
- **微内核 + 插件**：audemsp-common 微内核，领域功能以插件形式加载
- **Auth 双模式**：独立部署自带账户系统；作为 AUDEBase 模块时委托平台 RBAC/LDAP

详见 [`docs/architecture.md`](docs/architecture.md)。

## 技术栈

- **Native 层**：Rust (edition 2024)，libwebrtc (主) / str0m / webrtc-rs 三后端
- **绑定层**：napi-rs（Node.js）、C FFI（AUDESYS 静态链接）
- **信令**：WebSocket (Phase 1) + MQTT 5.0 (Phase 2+)
- **传输**：RTP/RTCP、SRT、WebRTC DataChannel
- **内部协议**：FlatBuffers（零拷贝，多语言）

## 开发状态

当前处于 Phase 3 完成阶段。7 crate workspace，webrtc triple-backend，codec 三后端 (stub+FFmpeg+GStreamer)，400+ tests 全部通过，Docker/CI/DevContainer 就位。

## 许可

Apache 2.0
