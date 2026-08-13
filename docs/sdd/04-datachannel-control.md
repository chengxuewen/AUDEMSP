# SDD 04: DataChannel Control

## 1. 概述

DataChannel 控制模块。通过 WebRTC DataChannel 双向传输控制指令（键盘、鼠标、触控），附带 HMAC 安全签名。

**决策引用**: D65 (field DataChannel), D66 (remote DataChannel), D117 (控制安全)

## 2. 接口定义

```rust
/// 控制指令类型 (二进制协议)
pub enum ControlCommand {
    // 键盘
    KeyDown { key: KeyCode, modifiers: Modifiers },
    KeyUp { key: KeyCode },
    // 鼠标
    MouseMove { x: f32, y: f32 },
    MouseDown { button: MouseButton },
    MouseUp { button: MouseButton },
    MouseWheel { delta: f32 },
    // 触控
    Touch { id: u32, x: f32, y: f32, phase: TouchPhase },
    // 遥操作专有
    Steering(f32),         // -1.0 ~ 1.0
    Throttle(f32),         // 0.0 ~ 1.0
    Brake(f32),            // 0.0 ~ 1.0
    // 系统
    Heartbeat(u64),
    EmergencyStop,
}
```

### 二进制帧格式

```
| length: u32 (4) | seq: u32 (4) | cmd: u8 (1) | payload (var) | hmac: [u8; 8] |
```

- `length`: 整帧长度 (包括 hmAc)
- `seq`: 递增序列号, 防重放
- `cmd`: ControlCommand 枚举 tag
- `payload`: 指令具体参数 (bincode 序列化)
- `hmac`: 8 字节 truncated HMAC-SHA256

### DataChannel 配置

```
unordered: true
maxRetransmits: 0
label: "control"
```

## 3. HMAC 安全 (D117)

- 密钥: `DTLS-SRTP export_keying_material("mediaservo-control")`
- 算法: HMAC-SHA256, truncate 前 8 字节
- 接收端: 验证 HMAC, 序列号单调递增, seq > last_seq + 窗口
- 发送端: 队列 > 3 帧时丢弃最旧帧, 只发最新

```rust
pub fn sign_command(key: &[u8], seq: u32, cmd: &ControlCommand) -> Vec<u8>;
pub fn verify_command(key: &[u8], data: &[u8]) -> Result<ControlCommand>;
```

## 4. 紧急停止通道 (D117)

独立于 DataChannel 的 UDP 紧急停止路径:

```rust
pub trait EmergencyControl: Send {
    fn bind(&mut self, addr: SocketAddr) -> Result<()>;
    fn send_stop(&self) -> Result<()>;
    fn poll_stop(&mut self) -> Result<Option<()>>;
}
```

- 协议: 纯 UDP, 不含 WebRTC 栈
- 数据: 单字节 `0xFF` = 紧急停止
- 目标延迟: <20ms
- 车控器独立 listener, 不经过 Host 进程 (Phase 2)

## 5. 错误处理

| 条件 | 分类 | 错误码 | 恢复 |
|------|------|--------|------|
| HMAC 验证失败 | Recoverable | 4002 | 丢弃指令, 记录告警 |
| 序列号乱序 | Recoverable | 4002 | 丢弃旧 seq 指令 |
| DataChannel 断连 | Recoverable | 1003 | 等待 WebRTC 恢复 |
| 紧急停止 UDP 丢包 | Transient | - | 重复发送 3 次, 50ms 间隔 |

## 6. 测试计划

| 测试 | 类型 | 描述 |
|------|------|------|
| hmac_sign_verify | 单元 | 签名后验证通过, 篡改后验证失败 |
| seq_replay_protection | 单元 | 旧 seq 指令被丢弃 |
| control_latency | 集成 | 端到端控制延迟 <50ms |
| queue_overflow_drop | 单元 | 队列 > 3 帧时丢弃最旧帧 |
| emergency_stop_latency | 集成 | UDP 紧急停止 <20ms |
| emergency_independence | 集成 | WebRTC 断开时 UDP 停止仍可用 |
| keyboard_mouse_roundtrip | E2E | 键盘按下 → 发送 → 接收 → 事件注入 |