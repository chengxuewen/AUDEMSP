//! MediaServo link IPC.
//!
//! 设备侧多进程本地 IPC（D235-D243）：
//! - [`bus::framebus::FrameBus`] — iceoryx2 SHM 零拷贝帧总线（latest-frame 覆盖语义）
//! - [`registry::Registry`] — attach 即注册 + 发现 + 活性（iceoryx2 内建）
//! - [`acl::NodeAcl`] — 静态 ACL（role 预置 + 节点覆盖，D237）
//! - [`token::CapabilityToken`] — Ed25519 能力令牌（ACL 签进 JWT，D238）
//!
//! 不含 SignalClient（对 server 的 WS 信令）——拆到 Phase 1b。

pub mod acl;
pub mod error;
pub mod frame;
pub mod id;

pub use acl::{NodeAcl, Role};
pub use error::LinkError;
pub use frame::{FrameMeta, FrameRef, FrameStream};
pub use id::{FrameTopic, NodeId};
