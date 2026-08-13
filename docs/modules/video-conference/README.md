# MediaServo 视频会议模块 — 参考文档

> Phase 0 架构定义 | 2026-07-16

> **文档版本**: 写于 2026-07-16 (D126-D155 决策前)。后续 D126 (三层模型), D137-D139 (WebRTC 后端), D142 (整合计划) 可能影响本文档内容，请对照最新决策阅读。

本文档是 MediaServo 视频会议 (VC) 模块的参考手册，涵盖架构、API、信令协议、数据模型、部署和 SFU 配置。

## 文档索引

| 文档 | 内容 |
|------|------|
| [architecture.md](architecture.md) | 组件架构、SFU 引擎设计、Worker 模型、级联策略 |
| [api.md](api.md) | Conference SDK API 参考 (Rust/TS)，管线与事件模型 |
| [signaling.md](signaling.md) | WebSocket 信令协议、房间管理、SDP/ICE 协商 |
| [data-model.md](data-model.md) | Room/Participant/Track 模型、编码层、权限 |
| [deployment.md](deployment.md) | 四种部署形态与配置参考 |
| [sfu-config.md](sfu-config.md) | SFU 调优、Worker 分配、带宽管理、容量规划 |

## 设计原则

1. **媒体层与控制层分离** — SFU 只做 RTP 转发，不解码不转码。信令和业务逻辑独立。
2. **SFU 优先** — 音频视频均走 SFU 转发。MCU 混音仅在 PSTN 桥接和录制桥接时使用。
3. **Worker 隔离** — 每个 mediasoup Worker 是独立进程，崩溃不影响其他会议。
4. **级联而非全连接** — 跨区域通过 pipeToRouter 级联，避免 N×N 连接爆炸。
5. **客户端混音** — 客户端解码多路音频流并本地混音，服务端不混音。

## 技术选型

| 组件 | 选型 | 理由 |
|------|------|------|
| SFU 引擎 | mediasoup (C++) | 性能最优、Worker 隔离、信令不可知 |
| 信令 | 自研 WebSocket + gRPC | 灵活可控，借鉴 Colibri2 + PSRPC |
| 音频 | SFU 转发 + 客户端混音 | 避免 MCU 扩展性瓶颈 |
| 录制 | RTP Forwarding | SFU 流复制推送到录制服务 |
| 绑定层 | napi-rs | Rust -> Node.js 原生模块 |

详见 [research doc](../../research/video-conference.md) 了解调研过程和对比分析。