# 15. Component Architecture — 组件架构

> 状态：Phase 1 设计 | 关联决策：D126–D134 | **Phase 编号说明**: 本文档使用 consolidated-mvp/plan.md 的 Phase 编号。Component 框架 = Phase 3, Plugin = Phase 5。

## 三层逻辑抽象模型

AUDEMSP 在代码层面采用三层逻辑抽象（D126），与 D1 的部署拓扑三层互补：

```
┌─────────────────────────────────────────────────────────────┐
│ Layer 3 — Process (部署层)                                   │
│  OS 进程，承载 Component 运行。Phase 1 单进程，              │
│  Phase 2 多进程（systemd/K8s 外层监督）                      │
├─────────────────────────────────────────────────────────────┤
│ Layer 2 — Component (服务层)                                 │
│  有独立生命周期的服务级单元。通过 ComponentBus 通信。        │
│  Phase 1：Gateway / Signaling / Relay / Admin / Auth / Monitor│
├─────────────────────────────────────────────────────────────┤
│ Layer 1 — Plugin (管线层)                                    │
│  媒体管线元素（capture/encode/decode/render）。              │
│  通过 MediaPort（帧队列）通信，由 PipelineEngine 管理。      │
│  Phase 1 仅控制面 Component，不涉及 Plugin 管线。            │
└─────────────────────────────────────────────────────────────┘
```

## Component trait

```rust
/// 服务层组件的基础 trait。
/// 每个 Component 有独立的 init → run → shutdown 生命周期。
#[async_trait]
pub trait Component: Send + Sync {
    /// 组件唯一标识
    fn id(&self) -> ComponentId;

    /// 初始化（Phase 1 同步，Phase 2 可异步加载资源）
    async fn init(&mut self, ctx: &ComponentContext) -> Result<(), ComponentError>;

    /// 主运行循环（由 ComponentManager spawn 为独立 tokio task）
    async fn run(self: Arc<Self>, ctx: ComponentContext) -> Result<(), ComponentError>;

    /// 优雅关闭
    async fn shutdown(&self) -> Result<(), ComponentError>;
}
```

### ComponentContext

传递给每个 Component 的运行时上下文：

```rust
pub struct ComponentContext {
    /// 消息总线（RPC + pub/sub）
    pub bus: Arc<dyn ComponentBus>,
    /// 组件自身 ID
    pub self_id: ComponentId,
}
```

**Phase 1 说明**：ComponentContext 不持有 `PipelineEngine` 引用。Phase 1 的 6 个 Component 全部为控制面，不需要创建媒体管线节点。Phase 2 引入 RecordingComponent 等数据面 Component 时，通过 `PluginManager::create_node()` 创建节点（届时 `create_node` 桩已实现）。

## ComponentBus

Component 间的统一通信总线。

### 双模式路由（D132）

| 模式 | 方法 | 用途 | 示例 |
|------|------|------|------|
| RPC (1:1) | `send_rpc::<Q,R>(query) → Result<R>` | 请求-响应，类型安全 | Gateway→Auth: 验证 token |
| Pub/Sub (1:N) | `publish(topic, event)` / `subscribe(topic)` | 事件广播 | Signaling→Monitor: peer_joined |

### InProcessBus 实现（Phase 1）

```rust
pub struct InProcessBus {
    rpc_handlers: DashMap<TypeId, DashMap<ComponentId, Box<dyn AnyChannel>>>,
    subscribers: DashMap<String, Vec<broadcast::Sender<ComponentEvent>>>,
}
```

- **Channel-per-type**（D133）：`register_rpc_handler::<Query, Reply>()` 编译期类型安全，零序列化开销
- **零序列化**：Phase 1 进程内直接传递 Rust 类型。Phase 2 切换到 ZenohBus 时由 Bus 实现层决定序列化策略
- **Arc\<Component\> 绕过**：WebSocket 连接无法通过 Bus 序列化，Gateway 直接持有 `Arc<SignalingComponent>` 并调用 `handle_socket()`——这是特例，不鼓励泛化

## ComponentManager 监督树

```
ComponentManager
├── GatewayComponent     ─── crash-loop: max_restarts=5, window=60s
├── SignalingComponent   ─── crash-loop: max_restarts=5, window=60s
├── AdminComponent       ─── crash-loop: max_restarts=3, window=60s
├── RelayComponent       ─── crash-loop: max_restarts=3, window=60s
├── AuthComponent        ─── crash-loop: max_restarts=5, window=60s
└── MonitorComponent     ─── crash-loop: max_restarts=5, window=60s
```

状态机：`Created → Initializing → Running → Crashed | Stopped`

**Phase 1**：单进程内监督（D134）。ComponentManager 监控 JoinHandle，crash-loop 防护（max_restarts + 时间窗口）。

**Phase 2**：多进程部署时，每个进程内部保留 ComponentManager（内层监督）；进程间由 systemd Restart=on-failure 或 K8s restartPolicy 负责（外层监督）。

## GatewayComponent 路由（D128）

Phase 1 统一 HTTP Gateway，单一端口 `:9800`。路由表硬编码（YAGNI，Phase 2 引入 mount API）：

```
:9800
├── /health              → MonitorComponent (状态检查)
├── /metrics             → MonitorComponent (Prometheus metrics)
├── /api/auth/login      → AuthComponent (JWT 签发)
├── /api/auth/validate   → AuthComponent (Token 验证)
├── /ws                  → SignalingComponent (WebSocket 升级)
├── /admin/api/*         → AdminComponent (REST API)
├── /admin/*             → Phase 2 占位 (SPA 静态文件)
└── /api/relay/*         → RelayComponent (Relay 管理)
```

**与 D52 的关系**：D128 的 Gateway 是 D52 单端口信令服务模式的演进——D52 的 axum 服务被 Gateway 吸收，SignalingComponent 负责 WS 信令内部处理。

## AuthProvider 鉴权（D130）

```rust
#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn login(&self, credentials: LoginRequest) -> Result<AuthToken, AuthError>;
    async fn validate(&self, token: &AuthToken) -> Result<UserInfo, AuthError>;
    async fn authorize(&self, user: &UserInfo, permission: Permission) -> Result<bool, AuthError>;
}
```

**RBAC 对齐（D88）**：预定义 3 角色（admin / operator / auditor）和功能权限枚举（host:read、host:control、server:config、record:read、record:delete、user:manage 等）。

**与 PskAuthenticator 的关系**：`PskAuthenticator`（core/auth.rs）是 Phase 1 过渡实现——用于 WebSocket PSK 握手。`AuthProvider` 处理 HTTP JWT 认证。两者共存但职责不重叠。

## Phase 阶段划分

### Phase 1（当前）

| 项目 | 范围 |
|------|------|
| Component 类型 | 仅控制面：Gateway / Signaling / Admin / Relay / Auth / Monitor |
| 通信 | InProcessBus（tokio::mpsc） |
| 序列化 | 零（Rust 类型直传） |
| Plugin 管线 | 不涉及。Phase 1 Component 不需要 PipelineEngine |
| 部署 | 单进程 |
| 路由注册 | 硬编码在 Gateway |
| 监督 | ComponentManager 单层 |

### Phase 2（计划）

| 项目 | 范围 |
|------|------|
| Component 类型 | 数据面：RecordingComponent、RTMPComponent 等 |
| 通信 | ZenohBus（网络透明） |
| Plugin 管线 | PluginManager::create_node() 实现，Component 可创建 PipelineNode |
| 部署 | 多进程（systemd/K8s 外层监督） |
| 序列化 | 由 ZenohBus 实现决定 |
| 路由注册 | mount_routes() API |
| Admin UI | React 19 + Ant Design 5 SPA |

## 关联决策

| 决策 | 内容 |
|------|------|
| D126 | 三层抽象模型（Plugin / Component / Process） |
| D127 | Component trait 独立 crate `audemsp-component` |
| D128 | 统一 HTTP Gateway 模式 |
| D129 | tokio::mpsc → Zenoh 通信中间件 |
| D130 | AuthProvider trait 鉴权架构 |
| D131 | Component 三阶段生命周期（init→run→shutdown） |
| D132 | 双模式路由（send_rpc + publish/subscribe） |
| D133 | Channel-per-type 类型安全 RPC |
| D134 | 简化监督树（ComponentManager single-level） |
| D88 | RBAC 角色权限模型 |
| D52 | 信令服务架构 |
| D10 | FlatBuffers 内部协议 |
| D-ERR-01/02 | 错误模型（Component 使用 ThisError 枚举，不加入 5 位错误码体系） |

## 现有模块迁移

| 现有文件 | 迁移方式 |
|----------|----------|
| `signaling.rs` | SignalingServer 保持无变更，被 SignalingComponent 包装 |
| `monitor.rs` | monitor_router() 被 Gateway 吸收，MonitorComponent 替代 |
| `relay.rs` | Relay 保持无变更，被 RelayComponent 包装 |
| `main.rs` | 重写为 ComponentManager 启动入口 |
