# Testing Strategy — 测试策略

> 状态：Phase 3 前设计 | 关联决策：D113 (4-layer), D-TEST-01 | 创建依据：doc-audit M6

## 4 层测试架构 (D113)

```
Layer 1: Unit         → per-crate #[cfg(test)] + #[test]
Layer 2: Component    → crate 内集成测试，依赖被 mock
Layer 3: Integration  → 多 crate 组合，真实 transport
Layer 4: E2E          → Host→Server→Remote 全链路
```

当前测试分布 (2026-07-20): 147 workspace tests。Unit: core 58, webrtc 21 (w3c-api)。Integration: webrtc-sys loopback 13, host E2E 21, server 37, remote 13, PipelineEngine 11。

### Layer 1: Unit

- 位置：`crates/*/src/**/*.rs` 内 `#[cfg(test)] mod tests`
- 覆盖：纯函数、类型转换、序列化、配置解析
- 框架：`#[test]` + `assert_eq!`，无外部依赖

### Layer 2: Component

- 位置：`crates/*/tests/component_*.rs`
- 覆盖：单个 crate 内部模块集成（Pipeline 链路、信令状态机、编解码器封装）
- Fixtures：
  - `MockTrackLocal` — 模拟本地视频轨道，注入固定帧
  - `LoopbackTransport` — 内存 channel 模拟 RTP 收发
  - `TestConfig` — 内联 toml，不依赖文件系统

### Layer 3: Integration

- 位置：`crates/audemsp-server/tests/integration_*.rs`（server 作为协调点）
- 场景：
  - Host 注册 → Server 确认 → Remote 发现 Host 列表
  - RTP relay 环路：Host → Server → Remote → Server → Host
  - 断线重连：kill transport → 自动 reconnect + 会话恢复
- 需要：tokio runtime + 真实 WebSocket/QUIC transport（localhost）

### Layer 4: E2E

- 位置：`tests/e2e/`（workspace root）
- 场景：
  - 远程桌面全链路：Host 采集 mock 帧 → Remote 渲染 mock 帧
  - 多方会议：1 Server + 3 Host 推流 + 3 Remote 拉流
  - mediasoup SFU 多对等点：4+ producer/consumer 并发
- 基础设施：docker-compose 编排，Playwright 验证 Remote GUI

## 覆盖率目标 (D-TEST-01)

| Crate | Unit | Component | 合计 |
|-------|------|-----------|------|
| audemsp-host | 60% | 80% | 80% |
| audemsp-client | 50% | 75% | 75% |
| audemsp-server | 60% | 80% | 80% |
| audemsp-common (Phase 2) | 70% | 85% | 85% |

工具：`cargo tarpaulin --workspace --out xml`

## SDD 追溯

每个 SDD 验收标准 → 对应 `#[test]`。用例命名：`test_{sdd_id}_{scenario}`。AAA 模式 (Arrange/Act/Assert) 强制。

> 详见 `.sisyphus/plans/phase3-production/plan.md` Phase 5
