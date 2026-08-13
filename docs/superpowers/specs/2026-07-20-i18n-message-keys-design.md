# i18n Message Key Infrastructure — Design Spec

**Date:** 2026-07-20
**Status:** draft
**Decision:** D-AUDIT-02 派生

## Motivation

MediaServo 所有用户面向文本当前硬编码为英文字符串。初期不实施 i18n，但为每种错误类型添加一个零成本的 `locale_key()` 方法，返回已有的数字错误码作为稳定标识符。后续添加多语言支持时，翻译层只需映射 `"1001"` → 区域文本，无需修改任何错误产生点。

## Scope

### In scope (Phase 1)

- `CoreError` (14 variants) — 添加 `locale_key()` 方法，返回错误码（如 `"1001"`）
- `RTCError` (5 variants) — 添加 `locale_key()` 方法，返回错误码（如 `"RTCPC"`）

### Out of scope

- 运行时翻译 — 零依赖。英文 `#[error("...")]` 文本保留作为默认回退
- `AuthResult` — 程序化枚举，非显示消息，不需要键
- HTTP 状态字符串 — 协议值，非用户面向文本
- Prometheus 指标描述 — 运维人员阅读的英文，不会被翻译
- tracing 日志消息、配置 doc 注释、Web UI、SDK
- fluent-rs 或其它 i18n crate

## Design

### Key Strategy: Reuse Existing Error Codes

`CoreError` 已有数字错误码 `[1001]`–`[9003]`，已嵌入每条 `#[error("...")]` 显示消息，已在模块头部文档中定义。这些是稳定的、唯一的、已文档化的标识符。`locale_key()` 返回相同的码作为 `&'static str`，零重复。

同样，`RTCError` 当前没有数字码，因此分配短字符串标识符。

### Inherent Methods (No Trait)

`mediaservo-core` 和 `mediaservo-webrtc` 无相互依赖关系，因此每种错误类型实现直接方法：

```rust
// crates/mediaservo-core/src/error.rs
impl CoreError {
    /// Return the numeric error code as a stable, context-free key.
    /// Used by future i18n layers to look up locale-specific text.
    pub fn locale_key(&self) -> &'static str {
        match self {
            CoreError::WebSocketDisconnect(_) => "1001",
            CoreError::IceTimeout => "1003",
            CoreError::PeerConnectionFailure(_) => "1004",
            CoreError::EncoderInit(_) => "2001",
            CoreError::CaptureSourceNotFound(_) => "3001",
            CoreError::CaptureDisconnected => "3002",
            CoreError::RelayTrackBind(_) => "4001",
            CoreError::RoomFull => "4002",
            CoreError::PskAuthFailed => "4003",
            CoreError::DecoderInit(_) => "5001",
            CoreError::ControlHmacFailed => "6001",
            CoreError::OutOfMemory => "9001",
            CoreError::ConfigParse(_) => "9002",
            CoreError::Unknown(_) => "9003",
        }
    }
}
```

```rust
// crates/mediaservo-webrtc/src/lib.rs
impl RTCError {
    /// Return a stable, context-free identifier for this error.
    pub fn locale_key(&self) -> &'static str {
        match self {
            RTCError::RTCPeerConnection(_) => "RTCPC",
            RTCError::RTCDataChannel(_) => "RTCDC",
            RTCError::Sdp(_) => "RTCSD",
            RTCError::Track(_) => "RTCTK",
            RTCError::Internal(_) => "RTCIN",
        }
    }
}
```

### Key Assignment

**CoreError** — 数字码直接来自现有错误码范围：

| Variant | Key | Error Code Range |
|----------|-----|-------------------|
| WebSocketDisconnect | `"1001"` | 1xxx Connectivity |
| IceTimeout | `"1003"` | 1xxx Connectivity |
| PeerConnectionFailure | `"1004"` | 1xxx Connectivity |
| EncoderInit | `"2001"` | 2xxx Encoding |
| CaptureSourceNotFound | `"3001"` | 3xxx Capture |
| CaptureDisconnected | `"3002"` | 3xxx Capture |
| RelayTrackBind | `"4001"` | 4xxx Relay/Server |
| RoomFull | `"4002"` | 4xxx Relay/Server |
| PskAuthFailed | `"4003"` | 4xxx Relay/Server |
| DecoderInit | `"5001"` | 5xxx Decode/Render |
| ControlHmacFailed | `"6001"` | 6xxx Control |
| OutOfMemory | `"9001"` | 9xxx System |
| ConfigParse | `"9002"` | 9xxx System |
| Unknown | `"9003"` | 9xxx System |

**RTCError** — 短标识符分配：

| Variant | Key |
|----------|-----|
| PeerConnection | `"RTCPC"` |
| DataChannel | `"RTCDC"` |
| Sdp | `"RTCSD"` |
| Track | `"RTCTK"` |
| Internal | `"RTCIN"` |

### Future Translation Layer (Not in Scope)

当需要 i18n 时，消费者可构建最简单的查找：

```rust
pub fn localized_display(err: &CoreError, locale: &Locale) -> String {
    let key = err.locale_key();
    TRANSLATION_TABLE.get(locale).get(key).unwrap_or_else(|| err.to_string())
}
```

已存在的 `#[error("...")]` thiserror 文本保持原样，作为默认回退。

## Files Changed

| File | Action | Description |
|------|--------|-------------|
| `crates/mediaservo-core/src/error.rs` | Modify | Add `CoreError::locale_key()` — 14-arm match |
| `crates/mediaservo-webrtc/src/lib.rs` | Modify | Add `RTCError::locale_key()` — 5-arm match |

## Non-Goals

- No runtime translation — zero dependencies
- No new files, no new modules, no constant modules
- No trait — inherent methods only
- No `format!()` or lookup tables — keys are `&'static str`
- No changes to log messages, config docs, Web UI, SDK
- No `fluent-rs` or other i18n crates
- No `AuthResult`, status strings, or Prometheus keys

## Testing

No dedicated tests needed. Existing tests for `CoreError::to_string()` continue to pass (no behavioral change). The `locale_key()` match is exhaustive — the compiler enforces coverage of all variants.

## Effort

~25 lines of match arms across 2 files. 1 commit, ~10 minutes.
