# Error Model — 错误模型

> 状态：Phase 3 前设计 | 关联决策：D-ERR-01, D-ERR-02 | 创建依据：doc-audit M7

## 错误传播

```
Host → [FlatBuffers ErrorPayload] → Server → [HTTP status + JSON body] → Client/Remote
```

每个 crate 内部错误不得直接泄漏到边界。Component 层 `ComponentError` 是统一错误类型，跨进程时序列化为 FlatBuffers `ErrorPayload`（错误码 + 上下文 string）。

## ComponentError (thiserror)

```rust
#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("initialization failed: {0}")]
    InitFailed(String),
    #[error("RPC timeout after {0}ms")]
    RpcTimeout(u64),
    #[error("channel closed")]
    ChannelClosed,
    #[error("component shutting down")]
    Shutdown,
    #[error("internal error: {0}")]
    Internal(String),
    #[error("not authorized: {0}")]
    Unauthorized(String),
    #[error("configuration invalid: {0}")]
    ConfigInvalid(String),
}
```

## 错误码分层 (D-ERR-02)

| 范围 | 层 | 示例 |
|------|-----|------|
| 1xxx | 通用/Component | 1001 NotFound, 1002 InitFailed |
| 2xxx | 传输层 | 2001 ConnectionRefused, 2002 Timeout |
| 3xxx | 资源 | 3001 StreamNotFound, 3002 RoomFull |
| 4xxx | 发现 | 4101 CameraNotFound, 4102 ONVIFAuthFailed, 4103 DiscoveryTimeout |
| 5xxx | 认证授权 | 5001 TokenExpired, 5002 PermissionDenied |
| 6xxx | 媒体 | 6001 CodecUnsupported, 6002 EncoderFailed |

### SFU 媒体面错误 (40xx, 实时协议)

> 创建依据：doc-audit L6（错误码三处语义冲突合并）。SFU 码段为**线上信令协议实际使用**（signaling.rs），发现层已平移至 41xx。

| 码 | 含义 | 代码位置 |
|----|------|---------|
| 4000 | transport 连接失败 / Produce 需要 send transport | signaling.rs:675 |
| 4001 | Room join 失败 | signaling.rs:308 |
| 4002 | Room 已满 | signaling.rs:295 |
| 4003 | 认证失败（PSK）——语义属认证层，[规划] 迁移至 5003 | signaling.rs:215/232 |
| 4099 | SFU 内部错误——[规划] 由 5000 迁入 | signaling.rs:629/636/661/703/726 |

> ⚠️ 5xxx 段注明：**5000 已被 SFU 内部错误占用**（signaling.rs 5 处），与 5xxx=认证授权语义冲突；迁移计划见上表（5000→4099、4003→5003），需信令协议版本兼容评估（浏览器按 type 字段路由，PIT-06，code 迁移风险低）。

## HTTP 状态映射

```
ComponentError → gRPC status / HTTP status

NotFound         → 404
InitFailed       → 500
RpcTimeout       → 504
ChannelClosed    → 503
Shutdown         → 503
Internal(_)      → 500
Unauthorized(_)  → 401
ConfigInvalid(_) → 400
```

## 日志约定

- **ERROR**: InitFailed, Internal — 需要人工介入
- **WARN**: RpcTimeout, ChannelClosed — 可重试恢复
- **INFO**: Shutdown, NotFound — 正常操作流程
- **必须包含**: `component_id`, `request_id`, 上下文

## 运行时错误场景

### Signal 断连
- WebSocket 断开后保持现有 WebRTC 连接，ICE restart 尝试恢复信令。
- 超时 30s → 关闭连接。
- 错误码: 2004 SignalingDisconnected

### Transport 断开 (ICE)
- 优雅关闭 (正常挂断): 错误码 2005 ConnectionClosed。
- 异常断开 (网络中断): 错误码 2006 ConnectionLost。
- ICE restart 自动尝试恢复。

### Discovery 超时
- ONVIF/RTSP 设备发现超时 10s。
- 错误码: 4103 DiscoveryTimeout

### Codec 不匹配
- SDP 协商无共同 codec → 错误码 3005 CodecNegotiationFailed。

### Config 回滚屏障
- 新配置导致组件崩溃 → 回滚到旧配置。
- 二次失败 → 进入安全模式（最小化功能集）。
- 错误码: 9004 ConfigRollbackFailed

## 背压与溢出策略

### 编码队列满 (D-ERR-03)
- Ring buffer，drop oldest frame，保留关键帧。
- EncoderInputDropped 计数器。

### 推流发送缓冲满 (D-ERR-04)
- bounded mpsc channel (cap 32)，超出→ drop oldest P frame。
- signal lag 计数器。

### 背压传播
- Sink → Processor → Source，通过 bounded channel capacity 实现。
> 背压传播设计已实施完成（D127-D134 Component 架构，consolidated-mvp 计划）
