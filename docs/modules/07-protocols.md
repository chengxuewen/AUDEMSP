# 协议与通信参考

> MediaServo 采用多协议分层设计，覆盖内部通信、外部接入和媒体传输。

---

## 一、协议分层

| 层次 | 协议 | 说明 |
|------|------|------|
| **内部 IPC** | FlatBuffers | 插件间零拷贝通信 |
| **AUDESYS** | C FFI | Rust → C 静态链接 |
| **AUDEBase** | napi-rs | Rust → Node.js 原生模块 |
| **后台服务** | gRPC (protobuf) | 控制面 API |
| **信令** | WebSocket | SDP 协商、ICE 交换 |
| **媒体传输** | RTP/RTCP, SRT, WebRTC | 数据面 |
| **客户端 ⇄ 后台** | gRPC + REST | 认证、权限拉取、配置同步 |

---

## 二、协议选择建议

| 协议 | 推流接入 | 播放分发 | 低延迟 | 穿透 NAT | 移动端 | 推荐度 |
|------|:---:|:---:|:---:|:---:|:---:|:---:|
| **RTMP** | ✅ | ⚠️ | 1-5s | ❌ | ❌ | 必须支持（生态兼容） |
| **SRT** | ✅ | - | 0.5-2s | ⚠️ | ❌ | 高（可靠推流） |
| **WebRTC/WHIP** | ✅ | ✅ | <500ms | 需 TURN | ✅ | 高（低延迟互动） |
| **HLS/LL-HLS** | - | ✅ | 2-10s | ✅ | ✅ | 必须（大规模分发） |
| **MPEG-DASH** | - | ✅ | 2-10s | ✅ | ✅ | 建议支持 |
| **HTTP-FLV** | - | ✅ | 1-3s | ✅ | ⚠️ | 可选（中文生态） |
| **RTSP** | ✅ | ✅ | 0.5-2s | ❌ | ❌ | 建议（IPC 接入） |
| **Media-over-QUIC** | ✅ | ✅ | <100ms | ✅ | - | 未来方向 |
| **MoQ (QUIC)** | ✅ | ✅ | <100ms | ✅ | - | 未来方向 |

---

## 三、WebRTC RTCDataChannel 协议（遥操作）

### 3.1 通道分离策略

| 通道类型 | ordered | 可靠性模式 | 典型负载 | 频率 |
|----------|---------|-----------|----------|------|
| **控制指令通道** | false | maxRetransmits: 0 | 方向盘/油门/刹车（uint16 × N） | 50-200Hz |
| **遥测通道** | false | maxRetransmits: 0 | 车速/GPS/IMU/状态位 | 20-50Hz |
| **可靠指令通道** | true | reliable | 系统重启/模式切换/配置下发 | 按需 |
| **文件/日志通道** | true | reliable | 日志上传/固件更新 | 按需 |
| **心跳通道** | false | maxRetransmits: 0 | 时间戳 + 序列号 | 1-10Hz |

### 3.2 二进制协议格式

**扩展控制包**（遥操作推荐）：
```
[timestamp: uint32 LE] [seq: uint16 LE] [cmd: uint8] [flags: uint8] [payload: N bytes]
总开销：8 字节头

flags:
  bit 0: emergency (急停)
  bit 1: heartbeat (心跳)
  bit 2: ack_requested (需要确认)
  bit 3-7: reserved
```

**遥测包**：
```
[timestamp: uint32 LE] [seq: uint16 LE]
[steering_angle: int16] [throttle: int16] [brake: uint16]
[speed: uint16] [yaw_rate: int16] [lat_accel: int16]
[gps_lat: int32] [gps_lon: int32] [heading: uint16]
[status_flags: uint16] [battery: uint8] [signal_rssi: int8]
总开销：37 字节
```

### 3.3 核心原则
- 有序可靠通道共享一个 SCTP 流——一条通道阻塞不影响其他通道
- 实时控制必须 `ordered: false`——旧指令到达反而危险
- 心跳独立通道——不与其他数据混合，测量真实网络 RTT
- 单条消息 ≤ 16KB（浏览器兼容上限）

---

### MQTT 5.0 (Phase 2+)

MQTT 5.0 is planned for Phase 2+ vehicle-to-cloud signaling scenarios. Phase 1 uses WebSocket exclusively. Key features: session persistence, shared subscriptions, request-response pattern. 参见决策 D74。

---

## 四、WebRTC 后端

MediaServo 支持三个 WebRTC 后端，通过 Cargo feature gate 切换：

| 后端 | Feature Gate | 用途 | 阶段 |
|------|-------------|------|------|
| **webrtc-sys** (libwebrtc FFI) | `webrtc-libwebrtc` | 默认后端，完整 WebRTC 支持 | Phase 1+ |
| **str0m** | `webrtc-str0m` | sans-I/O，轻量纯 Rust | Phase 1 (LAN) |
| **mediasoup-sys** | `webrtc-mediasoup` | SFU 多方会议 | Phase 2 |

决策依据：D137 (libwebrtc FFI 选型), D139 (webrtc-sys crate 定义), D118 (mediasoup SFU 规划)。

### 4.1 webrtc-sys（默认后端）

webrtc-sys 通过 FFI 绑定 Google libwebrtc，提供完整 RTCPeerConnection + RTP Track API。
作为默认 Cargo feature (`default = ["webrtc-libwebrtc"]`) 编译。
参见 `crates/webrtc-sys/`。
