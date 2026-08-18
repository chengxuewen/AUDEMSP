//! MediaServo Node.js 绑定 — napi-rs 直绑 Rust SDK（livekit rtc-ffi-bindings 同构）。
//!
//! 分层: mediaservo-{field,link,deck} (Rust async API) → napi .node 二进制 → TS 薄包装。
//! 事件模型: Rust broadcast/channel → napi ThreadsafeFunction → JS 主线程回调（livekit async_queue 同构）。

pub mod field;
pub mod link;
pub mod deck;

/// 共享事件泵 runtime（同步 napi 方法无 tokio 上下文——field-c 同款全局 runtime 模式）。
pub(crate) fn event_runtime() -> &'static tokio::runtime::Runtime {
    use std::sync::OnceLock;
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("mediaservo-node event runtime")
    })
}

// 模块导出（napi 命名空间合并）
pub use field::*;
pub use link::*;
pub use deck::*;
