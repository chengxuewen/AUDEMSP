# 客户端与 Host

> OMSPBase 提供两种客户端形态：Client（桌面 GUI 全功能）和 Host（Headless 远端）。

---

## 对比总览

| 维度 | OMSPBase Client | OMSPBase Host |
|------|-----------------|----------------|
| **运行环境** | 桌面操作系统 | 服务器 / 边缘 / 车端 / 无桌面 |
| **GUI** | Tauri v2 | Embedded Web (localhost 配置页) |
| **SDK** | 全量（生产 + 消费） | 仅生产（capture, encode, push） |
| **角色** | 可控制他人，也可被控制 | 仅产出媒体流 |
| **安装** | 桌面安装包 | 单一二进制 `omspbase-host` |
| **体积** | 大（含 GUI 框架） | 小（无 GUI 依赖） |

**双应用决策原因**：Host 需要运行在没有桌面环境的平台上（无 GUI 的 Linux 服务器、车端嵌入式设备）。

---

## 一、Client — 桌面 GUI 全功能应用

### 场景
操作员桌面，可远程控制他人，也可被远程控制。

### 架构
```
┌─────────────────────────┐
│  Tauri v2 GUI     │
│  ┌───────────────────┐  │
│  │ React 前端         │  │
│  │ 权限驱动 UI Module │  │
│  └───────────────────┘  │
│  ┌───────────────────┐  │
│  │ omspbase-common    │  │
│  │ 全量 SDK          │  │
│  └───────────────────┘  │
└─────────────────────────┘
```

### 权限驱动 UI

客户端启动时从后台拉取权限配置，动态加载 UI Module：

```typescript
const permissions = await backend.getPermissions(userId);

const modules: Module[] = [];
if (permissions.streaming)    modules.push(StreamingModule);
if (permissions.remote)       modules.push(RemoteDesktopModule);
if (permissions.conference)   modules.push(ConferenceModule);
if (permissions.surveillance) modules.push(SurveillanceModule);

// 无权限的模块完全不加载
```

### 场景覆盖
- 远程桌面（控制他人 / 被控制）
- 视频会议（加入/主持）
- 推拉流（观看/管理）
- 监控（查看/回放）
- 遥操作（操控车辆/机器人）

---

## 二、Host — Headless 远端

### 场景
车端、机房、边缘设备、摄像头——仅产出媒体流，无需 GUI。

### 架构

Host 单体架构 — GStreamer pipeline + WebRTC transport 同进程运行 (D155):

```
┌──────────────────────────────────────────┐
│  omspbase-host  (single binary)   │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │  GStreamer (C, glib dynamic .so)  │  │
│  │  capture → encode → appsink       │  │
│  │             │                      │  │
│  │    H.264 byte buffer (&[u8])      │  │
│  │             ↓                      │  │
│  ├────────────────────────────────────┤  │
│  │  omspbase-webrtc (Rust wrapper)   │  │
│  │  TrackLocal::write_frame(&[u8])   │  │
│  │             ↓                      │  │
│  ├────────────────────────────────────┤  │
│  │  webrtc-sys (C++, libwebrtc .a)   │  │
│  │  RTP packetizer → ICE → network   │  │
│  └────────────────────────────────────┘  │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │  axum HTTP/WS (信令 + 指标 + 配置) │  │
│  │  Signaling + Metrics + Session     │  │
│  └────────────────────────────────────┘  │
└──────────────────────────────────────────┘
```

关键设计: GStreamer 和 libwebrtc 通过 `&[u8]` 纯字节接口耦合，各自使用独立内存分配器 (glib malloc vs C++ new)，无内存共享、无符号冲突。GStreamer 为 optional feature，嵌入式平台直接排除。

### 特点
- 单一二进制 `omspbase-host` (~25 MB, GStreamer + libwebrtc 静态链接)
- GStreamer pipeline + WebRTC transport 同进程运行 (D155, 与 OBS/Sunshine 同模式)
- 接口耦合: appsink → `&[u8]` byte buffer → TrackLocal::write_frame (内存隔离)
- 无 GUI 依赖，适合嵌入式设备
- Embedded Web 配置页（localhost）
- 仅包含生产 SDK（采集、编码、推流）
