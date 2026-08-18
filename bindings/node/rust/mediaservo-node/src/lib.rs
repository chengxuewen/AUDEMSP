//! MediaServo Node.js 绑定 — napi-rs 直绑 Rust SDK（livekit rtc-ffi-bindings 同构）。
//!
//! 分层: mediaservo-{field,link,deck} (Rust async API) → napi .node 二进制 → TS 薄包装。
//! 事件模型: Rust broadcast/channel → napi ThreadsafeFunction → JS 主线程回调（livekit async_queue 同构）。

pub mod field;
pub mod link;
pub mod deck;

// 模块导出（napi 命名空间合并）
pub use field::*;
pub use link::*;
pub use deck::*;
