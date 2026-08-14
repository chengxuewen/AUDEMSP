# link IPC 设计（总线 / 注册 / 权限 / 令牌）

> **状态**: 设计定稿（未实现）| **日期**: 2026-08-14
> **关联决策**: D235（注册中心）、D236（ROS 集成）、D237（权限载体）、D238（令牌机制）、D239（派生 topic）
> **上游**: 20-sdk-api-contract.md §3（link）、04-sdk-layers.md、D222-D232（四 SDK 架构）
> **范围**: link 的**设备侧多进程 IPC**——FrameBus 帧总线、Registry 注册/发现、ACL 权限、能力令牌。信令（SignalClient，对 server 的 WS）见 20-sdk-api-contract.md §3，本文不重复。

---

## 0. 五项决策速览

| 决策 | 结论 |
|---|---|
| **D235 注册中心** | 去中心化 SHM service registry + **attach 即注册**，无专用 daemon；数据面 SHM 零拷贝直连 |
| **D236 ROS 集成** | 帧路径 ROS 节点**直连 FrameBus**（link SDK + 能力令牌）；桥接仅非帧场景逃生舱 |
| **D237 权限载体** | **静态 ACL**（role 预置 + 节点覆盖）；attach + 每次操作双重强制 |
| **D238 令牌机制** | **能力令牌**（ACL 签进 JWT，设备私钥签/公钥验）；复用 link JWT，不建独立 PKI |
| **D239 派生 topic** | **自由创建 + ACL 兜底**；registry 只登记不批准；单 topic 单发布者 |

## 1. 总体架构：数据面与控制面解耦

```
┌───────────────────── 控制面（注册/发现/权限）─────────────────────┐
│  Registry: 去中心化 SHM service registry，attach 即注册，无 daemon   │
│  Permission: 静态 ACL + 能力令牌，attach 验签 + 逐次操作校验          │
└──────────────────────────────────────────────────────────────────┘
                               │ 约束/记录
┌───────────────────── 数据面（帧传输）─────────────────────────────┐
│  FrameBus: SHM 零拷贝直连——发布写 SHM，订阅直读，不经任何 broker      │
└──────────────────────────────────────────────────────────────────┘
```
- **数据面**：帧走 SHM 零拷贝直连（发布写 SHM，订阅直读）——**零拷贝是硬需求**（车端多路高清），排除 broker 型中转
- **控制面**：注册/发现/权限独立于数据路径；**去中心化、无中央 daemon**（车端 7x24 可靠性，避免单点）
- **前提**：底层 SHM IPC（iceoryx2 或自研）**不提供权限**——Permission 层必须自建，与底层选型无关

## 2. 节点模型与角色

每个 worker / ROS 节点 = 独立进程，持 **NodeId + Role + capabilities + 能力令牌**。

| role | 典型进程 | publish 允许 | subscribe 允许 |
|---|---|---|---|
| `capture` | 出图节点 | `camera/*` | —（不订阅） |
| `perception` | ROS 感知节点 | `perception/*` | `camera/*` |
| `processor` | ROS 拼接节点 | `video/*` | `camera/*` |
| `pusher` | 推流节点 | —（只出 WebRTC，不进总线） | `camera/*`、`video/*` |
| `recorder` | 录制节点 | — | `camera/*`、`video/*` |
| `control` | 控制节点 | `control/cmd` | `control/telemetry`、`status/*` |
| `puller` | 舱端拉流 | —（经 field 远端） | —（本地总线外） |

> 读法：拼接节点 = `sub camera/* + pub video/*`；推流节点只订阅不发布（其"发布"是 WebRTC）；控制节点独占 `control/cmd` 写权限。最小权限 + 隔离（感知节点被攻破也不能 publish `control/cmd`）。

## 3. Topic 命名空间

```
camera/{front,rear,cabin}/raw        出图节点原始帧
camera/*/encoded                     编码帧
video/stitched                       拼接派生流（processor 产出）
perception/{objects,lane}            感知结果（perception 产出）
control/{cmd,telemetry,emergency}    控制/遥测
status/*                             节点状态
```
- 层级命名 + 通配（`camera/*`）
- **派生 topic**（processor 产出）用层级命名 + producer 语义前缀，降低冲突

## 4. FrameBus 发布订阅

**语义**（沿用已定）：
- **最新帧覆盖**（免疫积压/启动顺序）；多订阅者独立消费
- **SHM 零拷贝**：元数据 FlatBuffers + 像素裸内存
- **单 topic 单发布者**（D239）：同名 topic 已有发布者则后到者拒绝

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LinkError {
    #[error("attach failed: {0}")]   Attach(String),
    #[error("acl denied: {0}")]      AclDenied(String),
    #[error("bus error: {0}")]       Bus(String),
    #[error("topic conflict: {0}")]  TopicConflict(FrameTopic),
    #[error("closed")]               Closed,
}

pub struct FrameBus;
impl FrameBus {
    /// attach 即注册：验签能力令牌 + 自描述注册 + 载入 ACL
    pub fn attach(endpoint: &str, credential: CapabilityToken) -> Result<FrameBus, LinkError>;
    /// publish 前查 ACL（越权 → AclDenied + 审计）
    pub fn publish(&self, topic: &FrameTopic, frame: FrameRef) -> Result<(), LinkError>;
    /// subscribe 前查 ACL
    pub fn subscribe(&self, topic: &FrameTopic) -> Result<FrameStream, LinkError>;
    pub fn node_id(&self) -> NodeId;
    pub fn close(self) -> Result<(), LinkError>;
}
pub struct FrameTopic(String);
pub type FrameStream = UnboundedReceiver<FrameRef>;
```

## 5. Registry（去中心化 SHM，attach 即注册）

- **注册**：`FrameBus::attach` 时自动——节点将 `{id, role, caps, publishes, subscribes}` 写入 SHM 注册区
- **发现**：查 SHM 注册区（谁发布 `camera/*`、哪些 processor 在线）
- **活性**：heartbeat/lease 写入注册区，掉线自动摘除 → 订阅方收到 `TopicLost`
- **无 daemon**：iceoryx2 式去中心化发现（若用 iceoryx2 则现成）；自研则 SHM 注册区从简（扁平 topic 表 + 节点表）

```rust
pub struct Registry;
impl Registry {
    pub fn discover_topics(prefix: &str) -> Result<Vec<TopicInfo>, LinkError>;
    pub fn discover_nodes(role: Role) -> Result<Vec<NodeInfo>, LinkError>;
    pub fn topic_publisher(topic: &FrameTopic) -> Result<Option<NodeId>, LinkError>;
}
pub struct NodeInfo { pub id: NodeId, pub role: Role, pub caps: Capabilities,
                      pub publishes: Vec<TopicPattern>, pub subscribes: Vec<TopicPattern> }
pub struct TopicInfo { pub topic: FrameTopic, pub publisher: NodeId }
```

## 6. 权限：静态 ACL + 强制点

**ACL 结构**（topic 级 + 通配）：
```rust
pub struct NodeAcl {
    pub node_id: NodeId,
    pub role: Role,
    pub publish_allow:  Vec<TopicPattern>,   // 如 ["camera/*"]
    pub subscribe_allow: Vec<TopicPattern>,  // 如 ["video/*", "camera/*"]
}
impl NodeAcl {
    pub fn can_publish(&self, topic: &FrameTopic) -> bool;
    pub fn can_subscribe(&self, topic: &FrameTopic) -> bool;
}
pub struct TopicPattern(String);   // 支持通配 camera/*
```

**强制点**：
1. `FrameBus::attach`：验签能力令牌 → 提取 ACL → 载入
2. **每次 `publish`/`subscribe`**：查 ACL，越权拒绝 + 审计日志

**ACL 源配置**：role 预置（§2 矩阵固化）+ 按节点覆盖；纳入设备配置统一管理 + 版本控制。权限变更走"改源配置→离线重签令牌→节点重启"（低频可接受）。

## 7. 能力令牌（ACL 签进 JWT）

```rust
pub struct CapabilityToken;   // 签名 JWT：node_id/role/acl
impl CapabilityToken {
    /// 设备公钥验签 → 提取 claims
    pub fn verify(&self, device_pubkey: &DevicePublicKey) -> Result<Claims, LinkError>;
}
pub struct Claims { pub node_id: NodeId, pub role: Role, pub acl: NodeAcl, pub exp: u64 }
```
- **签发**：设备权威组件（provisioning / host supervisor）在节点部署时**离线签发**；**设备私钥签、公钥验**（非对称 Ed25519/ES256）
- **复用 link JWT**（claims 装 `node_id/role/acl`），不建独立节点证书 PKI
- **PSK 不参与令牌签名**（PSK 对称、用于对 server 认证，属另一关注点）
- **审计**：ACL 源配置一处编写，令牌是签名快照
- **去中心化自洽**：校验只需令牌 + 公钥，无中央、无配置分发依赖

## 8. 处理节点模式（订阅→加工→再发布）

"ROS 拼接节点发布拼接视频" = 节点同时是消费者 + 生产者，创建**派生 topic**：
```
camera/front/raw ─┐
camera/rear/raw  ─┼─► [ros-stitcher 节点] ─► video/stitched ─► pusher / recorder
camera/cabin/raw ─┘     (订阅 3 路→拼接)      (派生 topic)
```
- 节点可同时持 `subscribe` + `publish`（非互斥）
- 派生 topic 是**新注册实体**：Registry 记录 `video/stitched` 由 stitcher 发布、依赖 `camera/*`
- 形成 **topic DAG / 处理图**（ROS node graph 同款）
- **治理（D239）**：自由创建 + ACL 兜底——令牌允许 `pub video/*` 即放行 `video/stitched`，无需批准；registry 只登记不批准；单 topic 单发布者

## 9. ROS 集成（直连）

- **帧路径 ROS 节点**（感知/拼接）link link-SDK 直接 `FrameBus::attach(endpoint, credential)`：
  - py 节点 → ctypes/pyo3（D227 两步走）；C++ 节点 → cxx
  - endpoint 从设备配置/环境变量/约定 SHM 路径取
  - 持 role 令牌（perception/processor），ACL 在 attach 强制
- **非帧 ROS 子系统**：可用桥接（bridge 进程转 ROS2 topic ↔ FrameBus）保持纯 ROS——仅逃生舱
- **双协议栈**：ROS 节点同时持 ROS-DDS（对 ROS 内部）+ FrameBus（对 MediaServo），职责清晰

## 10. C ABI 绑定形态

延续 D109（opaque handle + int 错误码 + 回调）：
```c
typedef struct ms_link_bus_t ms_link_bus_t;                 /* opaque */
typedef int ms_err_t;                                        /* 0=ok, <0=error */

ms_err_t ms_link_bus_attach(const char* endpoint, const ms_capability_token_t* tok,
                            ms_link_bus_t** out);
ms_err_t ms_link_bus_publish(ms_link_bus_t*, const char* topic, const ms_frame_t* frame);
ms_err_t ms_link_bus_subscribe(ms_link_bus_t*, const char* topic, ms_frame_stream_t** out);
void     ms_link_bus_on_frame(ms_frame_stream_t*, ms_frame_cb cb, void* user);  /* 帧回调 */
ms_err_t ms_link_bus_close(ms_link_bus_t*);

ms_err_t ms_link_registry_discover_topics(const char* prefix, ms_topic_info_t** out, size_t* n);
ms_err_t ms_last_error(char* buf, size_t len);
```
- 帧跨 FFI：指针 + 元数据 struct（零拷贝优先）
- py 走 ctypes 加载 cdylib；C++ 走 header-only RAII

## 11. 场景全链路走查（拼接视频）

1. **出图节点** attach（role=capture，令牌 `pub camera/*`）→ `CameraSource.frames()` → `publish("camera/front/raw")`（SHM 零拷贝）
2. **ROS 拼接节点** attach（role=processor，令牌 `sub camera/*` + `pub video/*`）→ `subscribe` 3 路 → 拼接 → `publish("video/stitched")`（自由创建，ACL 放行，registry 自描述登记，单发布者）
3. **推流节点** attach（role=pusher，令牌 `sub video/*`）→ `subscribe("video/stitched")` → `field::PushSession` 推 WebRTC

权限全程：attach 验签 → publish/subscribe 查 ACL → 越权拒绝 + 审计。

## 12. 向后兼容与演进

| 项 | 规则 |
|---|---|
| 公开 enum `#[non_exhaustive]` | `LinkError`、`Role` |
| 新 role/topic 追加 | 加法式，不改既有 |
| 演进（不预埋） | 节点规模/一致性需求上升 → 可引入轻量 registry；运行时吊销 → 轻量 revoke list；派生 topic 强治理 → ACL 源配置加白名单/配额 |
