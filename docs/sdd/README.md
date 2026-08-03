# SDD 规格索引 — AUDEMSP Phase 1

> Spec-Driven Development 规格文档。每个 SDD 对应一个核心模块，覆盖接口定义、错误处理、测试计划。

## 索引

| # | 模块 | 文件 | 状态 | 关键决策引用 | 依赖 |
|---|------|------|------|-------------|------|
| 1 | CameraCapture | `01-camera-capture.md` | ✅ 已定义 | D64, D75, D-ERR-01 | audemsp-core |
| 2 | WebRTC Push | `02-webrtc-push.md` | ✅ 已定义 | D11, D32, D-ERR-01 | Transport trait |
| 3 | Decode + Render | `03-decode-render.md` | ✅ 已定义 | D46, D47, D48, D75 | Codec crate |
| 4 | DataChannel Control | `04-datachannel-control.md` | ✅ 已定义 | D65, D66, D117 | WebRTC plugin |
| 5 | Host Web Config | `05-host-web-config.md` | ✅ 已定义 | D81, D84, D85, D114 | axum |
| 6 | Server Monitoring | `06-server-monitoring.md` | ✅ 已定义 | D86-D91, D99, D-OPS-09, D111 | prometheus-client |
| 7 | Emergency Stop | `07-emergency-stop.md` | ✅ 已定义 | D117, D-SAFETY-02 | UDP |

## 跨模块引用

```
CameraCapture ──I420──▶ WebRTC Push ──RTP──▶ Server Relay
                                                  │
                                                  ▼
EmergencyStop ◀──UDP── DataChannel ◀──WebRTC── DecodeRender
                              │
                              ▼
                    Host Web Config (SSE)
```

## 约定

- 接口定义使用 Rust 伪代码（trait + struct 签名）
- 错误码遵循 D-ERR-02 编码体系（1xxx-9xxx）
- 测试计划按 4 层体系：单元 → 集成 → E2E → 实车 (D113)
- 延迟预算参考 D115 MVP 管线延迟分解
- 安全性参考 D116 STRIDE-Lite 威胁模型