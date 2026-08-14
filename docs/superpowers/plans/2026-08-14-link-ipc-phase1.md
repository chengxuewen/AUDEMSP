# Phase 1 — link IPC 实施计划（iceoryx2 底座 + FrameBus 薄层）【修订版 v2】

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **v2 修订**（团队审核后）：重排任务序（消 stub/回填）· 补 `enable_safe_overflow` · 补 D239 单发布者强制 · attach 即注册接线 · 令牌 Ed25519 · FrameStream 改 latest-slot · Registry 用 iceoryx2 内建活性/发现 · 补审计日志/close/kill-9 · SignalClient 拆出 · FrameMeta 加 format+version。

**Goal:** 建立 `mediaservo-link` crate：基于 iceoryx2 的跨进程零拷贝帧总线（FrameBus）+ 去中心化注册（Registry）+ 静态 ACL 权限 + 能力令牌，支撑车端多进程拓扑（出图/推流/ROS 拼接）的本地 IPC。**不含 SignalClient（对 server 的 WS 信令）——拆到 Phase 1b 后续 plan。**

**Architecture:** iceoryx2 0.9.3 作 SHM 传输底座（零拷贝 pub/sub、服务发现、节点活性现成）；FrameBus 为其上薄层（latest-frame 覆盖语义、`camera/*` topic、帧元数据、ACL 强制点、能力令牌 attach、单发布者强制）。去中心化无 daemon（D235），权限靠签名能力令牌本地校验（D237/D238）。

**Tech Stack:** Rust edition **2024**、iceoryx2 `=0.9.3`、jsonwebtoken（Ed25519/EdDSA）、tokio、thiserror、tracing、serde、mediaservo-common。

**依据决策:** D235/D236/D237/D238/D239/D242 + D243（帧元数据定长 LE，FlatBuffers 推迟）。

## Global Constraints

- **Rust edition `2024`**（iceoryx2 0.9.3 硬性要求；`edition.workspace = true`）
- **iceoryx2 锁 `=0.9.3`**（pre-1.0 防漂移，D242）
- **latest-frame 机制**：所有 topic service 经**统一 `topic_service()` helper** 创建，service builder 必须显式 `.subscriber_max_buffer_size(1).enable_safe_overflow(true)`（默认 safe_overflow=false 会导致缓冲满时 publish 报 ReceiverCacheIsFull 停摆——C1 审核实证）
- **FrameStream = latest-slot**（bounded-1 替换，慢消费者跳到最新帧），**禁用无界队列**（无界会重新引入积压，违背 latest-frame）
- **单发布者（D239）**：创建 publisher 前查 Registry，同 topic 已有发布者 → `LinkError::TopicConflict`
- **attach 即注册（D235）**：`FrameBus::attach` 内完成 验签 → 载 ACL → `Registry::register`
- **令牌 Ed25519 非对称**（D238）：设备私钥签、各节点公钥验；**验签公钥来源 = 设备配置**（`MEDIASERVO_DEVICE_PUBKEY` env 或 config 文件路径）；**禁止 HS256 对称**（持钥即可伪造）
- **令牌 TTL**：attach 时一次性验签；7x24 节点用长 TTL（或到期重 attach），已 attach 节点不受中途过期影响（静态 ACL 语义）
- **审计日志**：所有 ACL deny 路径 `tracing::warn!`（D237 + C15）
- **错误**：thiserror、`Result<T, LinkError>`、禁生产路径 `unwrap()`
- **零拷贝纪律**：订阅方只读 iceoryx2 sample 视图，禁 memcpy 帧
- **威胁模型（一行）**：ACL 是**库级检查**，防"诚实但配置错误"的节点；恶意进程可直接用 iceoryx2 打开同名 service 绕过（底座无权限层，D237/D238 已知限制）
- **测试**：TDD；多进程集成测试必须真跑；`cargo test` 不编 examples，子进程 helper 用 `cargo build --example` 预编 + 路径经 `env!("CARGO_BIN_EXE_...")` 或环境变量传递
- **提交**：每任务一个 commit；conventional commits；**建 crate/改 workspace = 结构变更，需用户确认（执行门禁）**

---

## File Structure

```
crates/mediaservo-link/                 # 新 crate（结构变更，需确认）
├── Cargo.toml                          # edition.workspace; iceoryx2=0.9.3, jsonwebtoken, tokio, thiserror, tracing, serde; 依赖 common
├── src/
│   ├── lib.rs                          # pub mod + re-exports
│   ├── error.rs                        # LinkError（含 TopicConflict）
│   ├── id.rs                           # NodeId, FrameTopic（品牌化 + 通配匹配）
│   ├── frame.rs                        # FrameMeta(含 format+version) + FrameRef + FrameStream(latest-slot)
│   ├── acl.rs                          # Role, NodeAcl, can_publish/can_subscribe（审计日志）
│   ├── token.rs                        # CapabilityToken（Ed25519 签发/验签，ACL claims）
│   ├── registry/mod.rs                 # Registry（attach 即注册 + Service::list 发现 + Node::list 活性）
│   └── bus/framebus.rs                 # FrameBus（topic_service helper + safe_overflow + 单发布者 + attach 接线）
└── tests/
    ├── frame.rs / acl.rs / token.rs / registry.rs
    ├── framebus.rs                     # 单进程回环
    ├── framebus_multiproc.rs           # 多进程零拷贝（cargo build --example helper）
    └── e2e_link.rs                     # 出图→拼接→推流 + ACL 负例 + 单发布者冲突 + kill-9 崩溃重启
examples/
    └── framebus_pub.rs                 # 多进程测试的子进程 publisher
```

**执行顺序（v2 重排，消除 stub/回填）：** Task 0 → 1 → 2(ACL) → 3(Token) → 4(Registry) → 5(FrameBus) → 6(e2e)。ACL/Token 是纯数据+加密、不依赖 iceoryx2，先做；FrameBus 建在其上，一次到位。

---

## Task 0: 依赖接入 + crate 骨架

**Files:**
- Modify: 根 `Cargo.toml`（workspace members + `[workspace.dependencies]` 加 iceoryx2/jsonwebtoken）
- Create: `crates/mediaservo-link/Cargo.toml`、`src/lib.rs`、`src/error.rs`
- ⚠️ **结构变更（新 crate）— 执行前需用户确认**

**Interfaces:**
- Produces: `mediaservo-link` crate 可编译；`LinkError`（含 `TopicConflict`）

- [ ] **Step 1: 用户确认创建 `mediaservo-link` crate（执行门禁）**
- [ ] **Step 2: 根 Cargo.toml 加 workspace 依赖**

```toml
# [workspace.dependencies]
iceoryx2 = "=0.9.3"
jsonwebtoken = "9"
```
并把 `crates/mediaservo-link` 加入 `members`。

- [ ] **Step 3: 写 crates/mediaservo-link/Cargo.toml**（edition 2024，去掉 bytes）

```toml
[package]
name = "mediaservo-link"
version = "0.1.0"
edition.workspace = true   # = 2024（iceoryx2 0.9.3 硬性要求）

[dependencies]
mediaservo-common = { path = "../mediaservo-common" }
iceoryx2 = { workspace = true }
jsonwebtoken = { workspace = true }   # v9 支持 EdDSA(Ed25519)
thiserror = "2"
serde = { version = "1", features = ["derive"] }
tracing = "0.1"
tokio = { version = "1", features = ["rt", "sync", "time", "macros"] }

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
```

- [ ] **Step 4: 写 src/error.rs + src/lib.rs 骨架**

```rust
// src/error.rs
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LinkError {
    #[error("attach failed: {0}")]
    Attach(String),
    #[error("acl denied: {topic}")]
    AclDenied { topic: String },
    #[error("topic already has a publisher: {topic}")]
    TopicConflict { topic: String },          // D239 单发布者
    #[error("token invalid: {0}")]
    Token(String),
    #[error("registry error: {0}")]
    Registry(String),
    #[error("bus error: {0}")]
    Bus(String),
    #[error("closed")]
    Closed,
}
```
```rust
// src/lib.rs
pub mod error; pub mod id; pub mod frame; pub mod acl; pub mod token; pub mod registry; pub mod bus;
pub use error::LinkError;
pub use id::{NodeId, FrameTopic};
pub use frame::{FrameMeta, FrameRef, FrameStream};
pub use acl::{Role, NodeAcl};
pub use token::CapabilityToken;
pub use registry::Registry;
pub use bus::framebus::FrameBus;
```

- [ ] **Step 5: 验证编译 + cargo deny 审计 iceoryx2（D242 影响）**
Run: `cargo check -p mediaservo-link` 然后 `cargo deny check`（确认 iceoryx2 ~40 传递依赖过审计）
Expected: check PASS；deny 无新增 CRITICAL

- [ ] **Step 6: Commit**
```bash
git add Cargo.toml Cargo.lock crates/mediaservo-link deny.toml
git commit -m "feat(link): mediaservo-link crate 骨架 + LinkError(含 TopicConflict) (D235/D242)"
```

---

## Task 1: 品牌化 ID + FrameMeta(含 format+version) + FrameStream(latest-slot)

**Files:**
- Create: `src/id.rs`、`src/frame.rs`
- Test: `tests/frame.rs`

**Interfaces:**
- Produces: `NodeId`、`FrameTopic(matches 通配)`、`FrameMeta(seq/width/height/format/ts_mono_ns/ts_epoch_ns/is_keyframe/version)`、`FrameRef`、`FrameStream(latest-slot)`
- Consumes: 无

- [ ] **Step 1: 写失败测试**

```rust
// tests/frame.rs
use mediaservo_link::{FrameTopic, FrameMeta};

#[test]
fn topic_wildcard_match() {
    let t = FrameTopic::new("camera/front/raw");
    assert!(t.matches("camera/*"));
    assert!(!t.matches("video/*"));
}

#[test]
fn frame_meta_has_format_and_version_roundtrip() {
    let m = FrameMeta { seq: 7, width: 1920, height: 1080, format: 1 /* I420 */,
                        ts_mono_ns: 123, ts_epoch_ns: 456, is_keyframe: true, version: 1 };
    let d = FrameMeta::decode(&m.encode()).unwrap();
    assert_eq!((d.seq, d.format, d.version), (7, 1, 1));
    assert!(d.is_keyframe);
}
```

- [ ] **Step 2: 跑测试确认失败**
Run: `cargo test -p mediaservo-link --test frame`
Expected: FAIL

- [ ] **Step 3: 实现 id.rs + frame.rs**

```rust
// src/id.rs — 品牌化 ID + 通配匹配
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NodeId(String);
impl NodeId { pub fn new(s: impl Into<String>) -> Self { Self(s.into()) }
              pub fn as_str(&self) -> &str { &self.0 } }

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FrameTopic(String);
impl FrameTopic {
    pub fn new(s: impl Into<String>) -> Self { Self(s.into()) }
    pub fn as_str(&self) -> &str { &self.0 }
    /// `camera/*` 前缀通配匹配 `camera/front/raw`
    pub fn matches(&self, pattern: &str) -> bool {
        match pattern.strip_suffix("/*") {
            Some(pfx) => self.0.starts_with(&format!("{pfx}/")),
            None      => self.0 == pattern,
        }
    }
}
```
```rust
// src/frame.rs — FrameMeta 定长 LE（含 format+version，M4/D243）+ FrameStream latest-slot（H5）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub struct FrameMeta {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    pub format: u8,        // 像素格式（0=未知,1=I420,2=NV12,3=RGBA...）
    pub version: u8,       // 元数据版本（演进用，D243）
    pub is_keyframe: bool,
    pub ts_mono_ns: u64,
    pub ts_epoch_ns: u64,
}
impl FrameMeta {
    pub const WIRE_LEN: usize = 8+4+4+1+1+1+1+8+8; // 定长 LE（含对齐 padding）
    pub fn encode(&self) -> [u8; Self::WIRE_LEN] { /* LE 编码 */ todo_impl() }
    pub fn decode(b: &[u8]) -> Result<Self, crate::LinkError> { todo_impl() }
}

/// FrameRef = 对 iceoryx2 sample 的零拷贝视图（FrameMeta + payload &[u8]，持有 sample 防过早释放）
pub struct FrameRef { /* meta + payload 视图 */ }
impl FrameRef { pub fn meta(&self) -> &FrameMeta { todo_impl() }
                pub fn payload(&self) -> &[u8] { todo_impl() } }

/// FrameStream = latest-slot（H5：慢消费者跳到最新帧，非无界队列）
/// 实现：Arc<Mutex<Option<FrameRef>>> + tokio::sync::Notify；
/// 后台线程每收到 sample 就替换槽内帧并 notify_waiters；消费者 await notify 后 take 最新。
pub struct FrameStream { /* slot + notify */ }
impl FrameStream {
    pub async fn recv(&self) -> Option<FrameRef> { todo_impl() } // await notify -> take latest
    pub(crate) fn spawn(/* iceoryx2 subscriber */) -> Self { todo_impl() }
}
```
> **FlatBuffers 推迟为定长 LE 已记为 D243**（跨语言 ROS 消费者按定长布局解析；未来需演进再上 FlatBuffers，靠 version 字段兼容）。

- [ ] **Step 4: 跑测试确认通过**
Run: `cargo test -p mediaservo-link --test frame`
Expected: PASS

- [ ] **Step 5: Commit**
```bash
git add crates/mediaservo-link/src/id.rs crates/mediaservo-link/src/frame.rs crates/mediaservo-link/tests/frame.rs crates/mediaservo-link/src/lib.rs
git commit -m "feat(link): 品牌化 ID + FrameMeta(format/version) + FrameStream latest-slot (D243/H5)"
```

---

## Task 2: ACL（静态 ACL + 通配 + 审计日志）

**Files:**
- Create: `src/acl.rs`
- Test: `tests/acl.rs`

**Interfaces:**
- Consumes: `FrameTopic`（Task 1）
- Produces: `Role`、`NodeAcl { node_id, role, publish_allow, subscribe_allow }`、`for_role/can_publish/can_subscribe`

- [ ] **Step 1: 写失败测试**（D237 权限矩阵逐行）

```rust
// tests/acl.rs
use mediaservo_link::{NodeAcl, Role, FrameTopic};

#[test]
fn capture_pub_camera_not_control_no_sub() {
    let acl = NodeAcl::for_role(Role::Capture);
    assert!(acl.can_publish(&FrameTopic::new("camera/front/raw")));
    assert!(!acl.can_publish(&FrameTopic::new("control/cmd")));
    assert!(!acl.can_subscribe(&FrameTopic::new("camera/front/raw")));
}

#[test]
fn processor_sub_camera_pub_video() {
    let acl = NodeAcl::for_role(Role::Processor);
    assert!(acl.can_subscribe(&FrameTopic::new("camera/front/raw")));
    assert!(acl.can_publish(&FrameTopic::new("video/stitched"))); // 派生 topic (D239)
    assert!(!acl.can_publish(&FrameTopic::new("control/cmd")));
}
```

- [ ] **Step 2: 跑测试确认失败**
Run: `cargo test -p mediaservo-link --test acl`
Expected: FAIL

- [ ] **Step 3: 实现 acl.rs**（role 预置矩阵 + 节点覆盖 + deny 审计日志 M1）

```rust
// src/acl.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum Role { Capture, Processor, Pusher, Puller, Recorder, Control, Perception }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeAcl {
    pub node_id: crate::NodeId,
    pub role: Role,
    pub publish_allow: Vec<String>,   // 通配模式，如 "camera/*"
    pub subscribe_allow: Vec<String>,
}
impl NodeAcl {
    /// D237 权限矩阵预置：capture pub camera/*；processor sub camera/* + pub video/*；
    /// pusher sub camera/*,video/*；recorder sub camera/*,video/*；
    /// control pub control/cmd + sub control/telemetry,status/*；perception pub perception/* + sub camera/*；puller 无。
    pub fn for_role(role: Role) -> Self { todo_impl() }
    pub fn can_publish(&self, topic: &FrameTopic) -> bool {
        let ok = self.publish_allow.iter().any(|p| topic.matches(p));
        if !ok { tracing::warn!(node=%self.node_id.as_str(), topic=%topic.as_str(), "ACL deny publish"); } // M1 审计
        ok
    }
    pub fn can_subscribe(&self, topic: &FrameTopic) -> bool {
        let ok = self.subscribe_allow.iter().any(|p| topic.matches(p));
        if !ok { tracing::warn!(node=%self.node_id.as_str(), topic=%topic.as_str(), "ACL deny subscribe"); }
        ok
    }
}
```

- [ ] **Step 4: 跑测试确认通过**
Run: `cargo test -p mediaservo-link --test acl`
Expected: PASS

- [ ] **Step 5: Commit**
```bash
git add crates/mediaservo-link/src/acl.rs crates/mediaservo-link/tests/acl.rs crates/mediaservo-link/src/lib.rs
git commit -m "feat(link): 静态 ACL role 矩阵 + 通配 + deny 审计日志 (D237)"
```

---

## Task 3: 能力令牌（Ed25519 签发/验签，ACL claims）

**Files:**
- Create: `src/token.rs`
- Test: `tests/token.rs`

**Interfaces:**
- Consumes: `NodeAcl`、`Role`、`NodeId`（Task 2）；jsonwebtoken EdDSA
- Produces: `CapabilityToken::sign/verify`、`Claims { node_id, role, acl, exp }`、`test_keypair()`

- [ ] **Step 1: 写失败测试**（roundtrip + 篡改拒绝 + 过期拒绝）

```rust
// tests/token.rs
use mediaservo_link::{CapabilityToken, NodeAcl, Role};

#[test]
fn sign_verify_roundtrip_ed25519() {
    let acl = NodeAcl::for_role(Role::Processor);
    let (sk, vk) = CapabilityToken::test_keypair();   // Ed25519 随机密钥对
    let tok = CapabilityToken::sign(&acl, 3600, &sk).unwrap();
    let claims = tok.verify(&vk).unwrap();
    assert_eq!(claims.role, Role::Processor);
    assert!(claims.acl.can_publish(&"video/stitched".into()));
}

#[test]
fn tampered_rejected() {
    let acl = NodeAcl::for_role(Role::Capture);
    let (sk, vk) = CapabilityToken::test_keypair();
    let mut tok = CapabilityToken::sign(&acl, 3600, &sk).unwrap();
    tok.tamper();
    assert!(tok.verify(&vk).is_err());
}
```

- [ ] **Step 2: 跑测试确认失败**
Run: `cargo test -p mediaservo-link --test token`
Expected: FAIL

- [ ] **Step 3: 实现 token.rs**（Ed25519，禁 HS256；公钥来源见 Global Constraints）

```rust
// src/token.rs — D238：ACL 签进 JWT；Ed25519 非对称（设备私钥签、各节点公钥验）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    pub node_id: String,
    pub role: Role,
    pub acl: NodeAcl,      // ACL 签进令牌
    pub exp: u64,
}
pub struct CapabilityToken(String); // JWT
impl CapabilityToken {
    /// Ed25519 签名（jsonwebtoken Algorithm::EdDSA）。禁止 HS256（对称，持钥可伪造）。
    pub fn sign(acl: &NodeAcl, ttl_secs: u64, signing_key: &Ed25519SigningKey) -> Result<Self, LinkError> { todo_impl() }
    /// 公钥验签 + 校验 exp。验签公钥来源：MEDIASERVO_DEVICE_PUBKEY env / 设备配置文件。
    pub fn verify(&self, verifying_key: &Ed25519VerifyingKey) -> Result<Claims, LinkError> { todo_impl() }
    #[cfg(test)] pub fn test_keypair() -> (Ed25519SigningKey, Ed25519VerifyingKey) { todo_impl() }
    #[cfg(test)] pub fn tamper(&mut self) { /* 改 payload 一位 */ todo_impl() }
}
```
> TTL 策略：attach 时一次性验签；7x24 节点用长 TTL 或到期重 attach，已 attach 节点不受中途过期影响。

- [ ] **Step 4: 跑测试确认通过**
Run: `cargo test -p mediaservo-link --test token`
Expected: PASS

- [ ] **Step 5: Commit**
```bash
git add crates/mediaservo-link/src/token.rs crates/mediaservo-link/tests/token.rs crates/mediaservo-link/src/lib.rs
git commit -m "feat(link): 能力令牌 Ed25519 签发/验签 + ACL claims (D238)"
```

---

## Task 4: Registry（attach 即注册 + 内建发现/活性）

**Files:**
- Create: `src/registry/mod.rs`
- Test: `tests/registry.rs`

**Interfaces:**
- Consumes: `NodeId`、`Role`（Task 1/2）；iceoryx2 `Node::list`、`ipc::Service::list`
- Produces: `Registry::register/discover_topics/discover_nodes/topic_publisher`、`NodeInfo { id, role, publishes, subscribes }`、`TopicInfo { topic, publisher }`

- [ ] **Step 1: 写失败测试**（注册后可发现 topic/publisher）

```rust
// tests/registry.rs
use mediaservo_link::{Registry, registry::NodeInfo, NodeId, Role};

#[test]
fn register_then_discover() {
    let info = NodeInfo { id: NodeId::new("capture-0"), role: Role::Capture,
                          publishes: vec!["camera/front/raw".into()], subscribes: vec![] };
    Registry::register(&info).unwrap();
    let topics = Registry::discover_topics("camera/").unwrap();
    assert!(topics.iter().any(|t| t.topic.as_str() == "camera/front/raw"));
}
```

- [ ] **Step 2: 跑测试确认失败**
Run: `cargo test -p mediaservo-link --test registry`
Expected: FAIL

- [ ] **Step 3: 实现 registry**（用 iceoryx2 内建能力，删手写 heartbeat — M5）
> **发现**：`ipc::Service::list(Config::global_config(), cb)` 列出活跃 service（= topic）。
> **活性**：`Node::<ipc::Service>::list(Config::global_config(), cb)` 返回 `NodeState::Alive/Dead`——iceoryx2 文件系统锁机制内建，**无需手写 heartbeat/lease**。`DeadNodeView::try_remove_stale_resources()` 清理僵尸资源。
> **节点自描述**（publishes/subscribes/role）：写入 iceoryx2 service attributes 或一个约定注册 service；**register 由 `FrameBus::attach` 调用（Task 5 接线，D235 attach 即注册）**。
> `topic_publisher(topic)`：查该 topic service 的发布者（供 Task 5 单发布者检查）。

- [ ] **Step 4: 跑测试确认通过**
Run: `cargo test -p mediaservo-link --test registry`
Expected: PASS

- [ ] **Step 5: 补活性测试**（kill 一个节点进程 → `Node::list` 报 Dead）+ 跑通
- [ ] **Step 6: Commit**
```bash
git add crates/mediaservo-link/src/registry crates/mediaservo-link/tests/registry.rs crates/mediaservo-link/src/lib.rs
git commit -m "feat(link): Registry attach 即注册 + Service::list 发现 + Node::list 活性 (D235)"
```

---

## Task 5: FrameBus（核心：safe_overflow + 单发布者 + attach 接线 + latest-slot）

**Files:**
- Create: `src/bus/mod.rs`、`src/bus/framebus.rs`
- Create: `examples/framebus_pub.rs`（多进程测试子进程）
- Test: `tests/framebus.rs`（单进程）、`tests/framebus_multiproc.rs`（多进程零拷贝）

**Interfaces:**
- Consumes: `FrameTopic/FrameMeta/FrameRef/FrameStream`（Task 1）、`NodeAcl`（Task 2）、`CapabilityToken`（Task 3）、`Registry`（Task 4）
- Produces: `FrameBus::attach/publish/subscribe/close/node_id`

- [ ] **Step 1: 写单进程回环失败测试**

```rust
// tests/framebus.rs
use mediaservo_link::{FrameBus, FrameTopic, FrameMeta, CapabilityToken, NodeAcl, Role};

#[tokio::test]
async fn pubsub_roundtrip_latest_frame() {
    let (sk, vk) = CapabilityToken::test_keypair();
    let tok = CapabilityToken::sign(&NodeAcl::for_role(Role::Processor), 3600, &sk).unwrap();
    let bus = FrameBus::attach("test", &tok, &vk).unwrap(); // 验签 vk（测试用本地公钥）
    let topic = FrameTopic::new("camera/test/raw");
    let stream = bus.subscribe(&topic).unwrap();
    bus.publish(&topic, &[1u8,2,3], &FrameMeta::default()).unwrap();
    let frame = stream.recv().await.unwrap();   // latest-slot recv
    assert_eq!(frame.payload(), &[1u8,2,3]);
}
```

- [ ] **Step 2: 跑测试确认失败**
Run: `cargo test -p mediaservo-link --test framebus`
Expected: FAIL

- [ ] **Step 3: 实现 framebus.rs**

```rust
// src/bus/framebus.rs
use iceoryx2::prelude::*;

pub struct FrameBus {
    node: Node<ipc::Service>,
    acl: NodeAcl,
    node_id: crate::NodeId,
}
impl FrameBus {
    /// attach 即注册（D235）：验签 → 载 ACL（fail-closed：验签失败拒绝）→ Registry::register
    pub fn attach(endpoint: &str, token: &CapabilityToken, vk: &Ed25519VerifyingKey) -> Result<Self, LinkError> {
        let claims = token.verify(vk)?;                 // 验签失败 → LinkError::Token（fail-closed）
        let acl = claims.acl;                            // ACL 来自签名令牌
        let node = NodeBuilder::new().create::<ipc::Service>().map_err(/*...*/)?;
        let info = registry::NodeInfo::from_claims(&claims);
        Registry::register(&info)?;                      // attach 即注册接线（H3）
        Ok(Self { node, acl, node_id: crate::NodeId::new(claims.node_id) })
    }

    /// 统一 topic service 构造（pub/sub 必须同配置，C1）：
    /// buffer_size=1 + enable_safe_overflow=true → latest-frame 覆盖（不显式开 safe_overflow 会停摆）
    fn topic_service(&self, topic: &FrameTopic) -> Result</* PortFactory */ ..., LinkError> {
        self.node.service_builder(&topic.as_str().try_into()?)
            .publish_subscribe::<[u8]>()
            .subscriber_max_buffer_size(1)
            .enable_safe_overflow(true)                  // ★ C1 关键
            .open_or_create()
            .map_err(/*...*/)
    }

    pub fn publish(&self, topic: &FrameTopic, payload: &[u8], meta: &FrameMeta) -> Result<(), LinkError> {
        if !self.acl.can_publish(topic) { return Err(LinkError::AclDenied { topic: topic.as_str().into() }); } // 内部已 warn 审计
        // D239 单发布者：该 topic 已有发布者 → TopicConflict
        if Registry::topic_publisher(topic)?.is_some() {
            return Err(LinkError::TopicConflict { topic: topic.as_str().into() });
        }
        let svc = self.topic_service(topic)?;
        let publisher = svc.publisher_builder()
            .initial_max_slice_len(FrameMeta::WIRE_LEN + payload.len()) // Static：须 >= 最大帧
            .allocation_strategy(AllocationStrategy::Static)
            .create().map_err(/*...*/)?;
        let mut buf = Vec::with_capacity(FrameMeta::WIRE_LEN + payload.len());
        buf.extend_from_slice(&meta.encode());
        buf.extend_from_slice(payload);
        let sample = publisher.loan_slice_uninit(buf.len()).map_err(/*...*/)?;
        let sample = sample.write_from_slice(&buf);
        sample.send().map_err(/*...*/)?;                 // 帧就地写入 SHM（零拷贝源）
        Ok(())
    }

    pub fn subscribe(&self, topic: &FrameTopic) -> Result<FrameStream, LinkError> {
        if !self.acl.can_subscribe(topic) { return Err(LinkError::AclDenied { topic: topic.as_str().into() }); }
        let svc = self.topic_service(topic)?;            // 与 publish 同配置（C1）
        let subscriber = svc.subscriber_builder().buffer_size(1).create().map_err(/*...*/)?;
        Ok(FrameStream::spawn(subscriber))               // 后台线程 receive → latest-slot
    }

    pub fn node_id(&self) -> &crate::NodeId { &self.node_id }
    pub fn close(self) -> Result<(), LinkError> { /* Registry 注销 + drop node，M6 */ todo_impl() }
}
```
> **约束**：`AllocationStrategy::Static` 要求 `initial_max_slice_len` >= 最大帧，否则 `LoanError::ExceedsMaxLoanSize`；变长帧改 `PowerOfTwo`。FrameStream 用 Task 1 的 latest-slot（替换语义）。

- [ ] **Step 4: 跑单进程测试确认通过**
Run: `cargo test -p mediaservo-link --test framebus`
Expected: PASS

- [ ] **Step 5: 写多进程零拷贝集成测试**（子进程 helper：`cargo build --example framebus_pub` + 路径经 env 传递 — L4）

```rust
// tests/framebus_multiproc.rs
// 主进程 subscribe camera/mp/raw；spawn examples/framebus_pub 子进程发布 1080p 帧；
// 断言收到 payload.len()==3_110_400，且为 SHM 视图（零拷贝）
```
```rust
// examples/framebus_pub.rs — 子进程：attach(role=capture) → publish 3_110_400B 帧
```

- [ ] **Step 6: 跑多进程测试（必须真跑）**
Run: `cargo build --example framebus_pub -p mediaservo-link && cargo test -p mediaservo-link --test framebus_multiproc -- --nocapture`
Expected: PASS，收到 1080p 帧

- [ ] **Step 7: Commit**
```bash
git add crates/mediaservo-link/src/bus crates/mediaservo-link/examples crates/mediaservo-link/tests/framebus*.rs crates/mediaservo-link/src/lib.rs
git commit -m "feat(link): FrameBus on iceoryx2 — safe_overflow latest-frame + 单发布者 + attach 接线 (D242/D239/D235/C1)"
```

---

## Task 6: e2e（出图→拼接→推流 + 负例 + 崩溃）

**Files:**
- Create: `tests/e2e_link.rs`

**Interfaces:**
- Consumes: FrameBus/Registry/ACL/CapabilityToken（Task 2-5）
- Produces: 场景级验收（对应 21-link-ipc.md §11）

- [ ] **Step 1: 写集成测试**
```rust
// tests/e2e_link.rs（多进程）
// capture(role=capture) pub camera/front/raw
// processor(role=processor) sub camera/* → 拼接 → pub video/stitched（D239 派生）
// pusher(role=pusher) sub video/stitched
// 断言：pusher 收到派生帧
```
- [ ] **Step 2: ACL 负例**（processor 试发 control/cmd → `AclDenied`）
- [ ] **Step 3: 单发布者冲突**（第二个 capture 对同 topic publish → `TopicConflict`）
- [ ] **Step 4: kill-9 崩溃重启**（kill capture 进程 → `Node::list` 报 Dead + 资源清理 → 重启恢复，M6）
- [ ] **Step 5: 跑测试（必须真跑，多进程）**
Run: `cargo test -p mediaservo-link --test e2e_link -- --nocapture`
Expected: PASS（含负例与崩溃恢复）
- [ ] **Step 6: Commit**
```bash
git add crates/mediaservo-link/tests/e2e_link.rs
git commit -m "test(link): e2e 出图->拼接->推流 + ACL/单发布者负例 + kill-9 崩溃恢复 (D236/D239/D235)"
```

---

## 拆出：SignalClient（对 server 的 WS 信令）→ Phase 1b 后续 plan

21-link-ipc.md 明确把 SignalClient 划出本 IPC 范围；本 plan 目标是本地 IPC。**SignalClient（WS 连 server、复用 SignalingMessage、需 tokio-tungstenite）拆到 Phase 1b 独立 plan**，不阻塞 Task 0-6；拆出后 link 对 ROS 轻量消费者更精简。

## 收尾：文档同步（M3）

- [ ] 同步 `docs/modules/20-sdk-api-contract.md §3` 的 link 节：`attach(endpoint, token, vk)`、`LinkError`(含 TopicConflict)、`Registry` API、`FrameRef` 统一、去掉"Phase 3 定传输"（D242 已定 iceoryx2）
- [ ] `docs/modules/21-link-ipc.md §4` 的 `FrameStream` 由 `UnboundedReceiver` 改为 **latest-slot**（H5，规范同源修正）
- [ ] `decisions.md` 记 **D243**（FrameMeta 定长 LE + format/version，FlatBuffers 推迟）

---

## Self-Review（v2）

- **决策覆盖**：D235→Task4+5(attach 接线)；D236→Task5/6；D237→Task2(+审计)；D238→Task3(Ed25519)；D239→Task5(单发布者)+error TopicConflict；D242→Task5(safe_overflow)；D243→Task1(format/version) ✅
- **顺序**：0→1→2→3→4→5→6，FrameBus 建在 ACL/Token/Registry 之上，无 stub/回填 ✅
- **C1 修复**：`enable_safe_overflow(true)` + `topic_service()` 统一构造 ✅
- **H5 修复**：FrameStream latest-slot（Task 1 定义、Task 5 用），spec 同步改 ✅

## 风险与去险

| 风险 | 去险 |
|---|---|
| iceoryx2 latest-frame（buffer=1+safe_overflow）行为 | Task 5 单进程回环先验证，再多进程 |
| 多进程测试 CI 不稳 | 显式子进程 + 超时 + `cargo build --example` 预编 + env 传路径 |
| 0.9.3 与 spike（main 0.9.999）API 差异 | 锁 =0.9.3；Task 0 `cargo check` 先确认依赖解析；实现前对照 0.9.3 tag |
| 恶意进程绕过 ACL | 威胁模型已声明（库级检查，底座无权限层）；非本 phase 解决 |

## 执行门禁提醒

- **Task 0 创建 `mediaservo-link` crate = 结构变更，需用户确认后才动手**
- 本 plan 仅为文档；写码前逐项确认
