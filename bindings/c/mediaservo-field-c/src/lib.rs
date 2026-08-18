//! MediaServo Field C ABI — 推流面（车端 SDK 消费）。
//!
//! 契约 §7（D109/D240/D241）：opaque handle + int 错误码 + 回调。
//! 同步阻塞式（内部 current_thread runtime）— 车端嵌入式场景简化集成。
//!
//! # C ABI 面（cbindgen 导出, MAJOR 内稳定）
//! ```c
//! typedef struct ms_field_push_t ms_field_push_t;   /* opaque */
//! typedef int ms_err_t;                              /* 0=ok, <0=error */
//!
//! ms_err_t ms_field_push_connect(const ms_push_config_t* cfg, ms_field_push_t** out);
//! ms_err_t ms_field_push_publish_video(ms_field_push_t* s, ms_track_id_t* out_track);
//! ms_err_t ms_field_push_start_video_frames(ms_field_push_t* s);
//! void     ms_field_push_stop_video_frames(ms_field_push_t* s);
//! ms_err_t ms_field_push_close(ms_field_push_t* s);
//! ms_err_t ms_last_error(char* buf, size_t len);
//! ```

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use mediaservo_field::{PublishOptions, PushConfig, PushSession};

/// 错误码（<0；0 = ok）。
pub const MS_OK: c_int = 0;
pub const MS_ERR_INVALID_ARG: c_int = -1;
pub const MS_ERR_CONNECT: c_int = -2;
pub const MS_ERR_PUBLISH: c_int = -3;
pub const MS_ERR_STATE: c_int = -4;
pub const MS_ERR_INTERNAL: c_int = -5;

/// 全局最近错误信息（ms_last_error 读取）。
static LAST_ERROR: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn set_last_error(msg: impl Into<String>) {
    if let Ok(mut guard) = LAST_ERROR.lock() {
        *guard = Some(msg.into());
    }
}

/// 推流配置（C 结构 — 与 PushConfig 一一映射）。
#[repr(C)]
pub struct ms_push_config_t {
    /// 信令 WS 地址（如 "ws://host:9800/ws"）。
    pub url: *const c_char,
    /// PSK 认证密钥。
    pub psk: *const c_char,
    /// 房间 ID。
    pub room: *const c_char,
    /// 视频宽（默认 1280）。
    pub width: u32,
    /// 视频高（默认 720）。
    pub height: u32,
    /// 帧率（默认 30）。
    pub framerate: u32,
    /// 编码码率 kbps（默认 2000）。
    pub bitrate_kbps: u32,
    /// 关键帧间隔秒（默认 2）。
    pub keyframe_interval: u64,
}

impl Default for ms_push_config_t {
    fn default() -> Self {
        Self {
            url: ptr::null(),
            psk: ptr::null(),
            room: ptr::null(),
            width: 1280,
            height: 720,
            framerate: 30,
            bitrate_kbps: 2000,
            keyframe_interval: 2,
        }
    }
}

/// 推流会话 opaque handle。
pub struct ms_field_push_t {
    inner: std::sync::Mutex<Option<PushSession>>,
    cfg: PushConfig,
}

// SAFETY: handle 内部为 Mutex<Option<PushSession>>（线程安全）+ 不可变 cfg。
unsafe impl Send for ms_field_push_t {}
// SAFETY: 所有方法经 Mutex 序列化访问内部会话。
unsafe impl Sync for ms_field_push_t {}

// ── 内部辅助 ──

/// 提取 C 字符串（null → None）。非法 UTF-8 → 错误。
fn cstr<'a>(ptr: *const c_char) -> Result<Option<&'a str>, ()> {
    if ptr.is_null() {
        return Ok(None);
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().map(Some).map_err(|_| ())
}

/// 单线程 runtime（同步阻塞完成 async 操作）。
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("field-c runtime")
}

// ── C ABI ──

/// 连接信令并创建推流会话（阻塞）。
///
/// `cfg` 不可为 null；`url/psk/room` 必填。成功后 `*out` 指向新 handle
/// （调用方负责 `ms_field_push_close`）。
#[unsafe(no_mangle)]
pub extern "C" fn ms_field_push_connect(
    cfg: *const ms_push_config_t,
    out: *mut *mut ms_field_push_t,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if cfg.is_null() || out.is_null() {
            set_last_error("ms_field_push_connect: null cfg/out");
            return MS_ERR_INVALID_ARG;
        }
        let cfg_ref = unsafe { &*cfg };
        let (url, psk, room) = match (
            cstr(cfg_ref.url),
            cstr(cfg_ref.psk),
            cstr(cfg_ref.room),
        ) {
            (Ok(Some(u)), Ok(Some(p)), Ok(Some(r))) => (u, p, r),
            (Ok(None), _, _) => {
                set_last_error("ms_field_push_connect: url required");
                return MS_ERR_INVALID_ARG;
            }
            (_, Ok(None), _) => {
                set_last_error("ms_field_push_connect: psk required");
                return MS_ERR_INVALID_ARG;
            }
            (_, _, Ok(None)) => {
                set_last_error("ms_field_push_connect: room required");
                return MS_ERR_INVALID_ARG;
            }
            _ => {
                set_last_error("ms_field_push_connect: invalid UTF-8 in config");
                return MS_ERR_INVALID_ARG;
            }
        };

        let mut push_cfg = PushConfig::new(url, psk, room);
        if cfg_ref.width > 0 {
            push_cfg.width = cfg_ref.width;
        }
        if cfg_ref.height > 0 {
            push_cfg.height = cfg_ref.height;
        }
        if cfg_ref.framerate > 0 {
            push_cfg.framerate = cfg_ref.framerate;
        }
        if cfg_ref.bitrate_kbps > 0 {
            push_cfg.bitrate_kbps = cfg_ref.bitrate_kbps;
        }
        if cfg_ref.keyframe_interval > 0 {
            push_cfg.keyframe_interval = cfg_ref.keyframe_interval;
        }

        let rt = runtime();
        match rt.block_on(PushSession::connect(push_cfg.clone())) {
            Ok((session, _events)) => {
                let handle = Box::new(ms_field_push_t {
                    inner: std::sync::Mutex::new(Some(session)),
                    cfg: push_cfg,
                });
                unsafe { *out = Box::into_raw(handle) };
                MS_OK
            }
            Err(e) => {
                set_last_error(format!("ms_field_push_connect: {e}"));
                MS_ERR_CONNECT
            }
        }
    }))
    .unwrap_or_else(|_| {
        set_last_error("ms_field_push_connect: panic");
        MS_ERR_INTERNAL
    })
}

/// 发布视频轨（阻塞协商；成功返回 track id 字符串到 `out_track` 缓冲）。
///
/// `out_track` 需至少 `out_track_len` 字节（track id 短, 64 足够）。
#[unsafe(no_mangle)]
pub extern "C" fn ms_field_push_publish_video(
    s: *mut ms_field_push_t,
    out_track: *mut c_char,
    out_track_len: usize,
) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if s.is_null() || out_track.is_null() {
            set_last_error("ms_field_push_publish_video: null handle/out_track");
            return MS_ERR_INVALID_ARG;
        }
        let handle = unsafe { &*s };
        let mut guard = match handle.inner.lock() {
            Ok(g) => g,
            Err(_) => {
                set_last_error("ms_field_push_publish_video: lock poisoned");
                return MS_ERR_INTERNAL;
            }
        };
        let Some(session) = guard.as_mut() else {
            set_last_error("ms_field_push_publish_video: session closed");
            return MS_ERR_STATE;
        };

        let rt = runtime();
        let opts = PublishOptions::default();
        match rt.block_on(session.publish_video(&handle.cfg, &opts)) {
            Ok(track_id) => {
                let bytes = track_id.as_bytes();
                let n = bytes.len().min(out_track_len.saturating_sub(1));
                unsafe {
                    ptr::copy_nonoverlapping(bytes.as_ptr(), out_track as *mut u8, n);
                    *out_track.add(n) = 0;
                }
                MS_OK
            }
            Err(e) => {
                set_last_error(format!("ms_field_push_publish_video: {e}"));
                MS_ERR_PUBLISH
            }
        }
    }))
    .unwrap_or_else(|_| {
        set_last_error("ms_field_push_publish_video: panic");
        MS_ERR_INTERNAL
    })
}

/// 启动视频帧生成（Squares + 时间戳水印；阻塞仅本地启动）。
#[unsafe(no_mangle)]
pub extern "C" fn ms_field_push_start_video_frames(s: *mut ms_field_push_t) -> c_int {
    catch_unwind(AssertUnwindSafe(|| {
        if s.is_null() {
            set_last_error("ms_field_push_start_video_frames: null handle");
            return MS_ERR_INVALID_ARG;
        }
        let handle = unsafe { &*s };
        let mut guard = match handle.inner.lock() {
            Ok(g) => g,
            Err(_) => {
                set_last_error("ms_field_push_start_video_frames: lock poisoned");
                return MS_ERR_INTERNAL;
            }
        };
        let Some(session) = guard.as_mut() else {
            set_last_error("ms_field_push_start_video_frames: session closed");
            return MS_ERR_STATE;
        };
        match session.start_video_frames(&handle.cfg) {
            Ok(()) => MS_OK,
            Err(e) => {
                set_last_error(format!("ms_field_push_start_video_frames: {e}"));
                MS_ERR_STATE
            }
        }
    }))
    .unwrap_or_else(|_| {
        set_last_error("ms_field_push_start_video_frames: panic");
        MS_ERR_INTERNAL
    })
}

/// 停止视频帧生成（幂等）。
#[unsafe(no_mangle)]
pub extern "C" fn ms_field_push_stop_video_frames(s: *mut ms_field_push_t) {
    if s.is_null() {
        return;
    }
    let handle = unsafe { &*s };
    if let Ok(mut guard) = handle.inner.lock() {
        if let Some(session) = guard.as_mut() {
            session.stop_video_frames();
        }
    }
}

/// 关闭推流会话并释放 handle。
#[unsafe(no_mangle)]
pub extern "C" fn ms_field_push_close(s: *mut ms_field_push_t) -> c_int {
    if s.is_null() {
        return MS_OK;
    }
    let handle = unsafe { Box::from_raw(s) };
    let session = handle.inner.lock().ok().and_then(|mut g| g.take());
    if let Some(session) = session {
        let rt = runtime();
        match rt.block_on(session.close()) {
            Ok(()) => MS_OK,
            Err(e) => {
                set_last_error(format!("ms_field_push_close: {e}"));
                MS_ERR_INTERNAL
            }
        }
    } else {
        MS_OK
    }
}

/// 最近一次错误的详情（线程安全；无错误时返回空串）。
#[unsafe(no_mangle)]
pub extern "C" fn ms_last_error(buf: *mut c_char, len: usize) -> c_int {
    if buf.is_null() || len == 0 {
        return MS_ERR_INVALID_ARG;
    }
    let msg = LAST_ERROR
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default();
    let bytes = msg.as_bytes();
    let n = bytes.len().min(len - 1);
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
        *buf.add(n) = 0;
    }
    MS_OK
}

/// 版本信息（MAJOR.MINOR.PATCH — D241 soname 语义）。
#[unsafe(no_mangle)]
pub extern "C" fn ms_field_version(buf: *mut c_char, len: usize) -> c_int {
    if buf.is_null() || len == 0 {
        return MS_ERR_INVALID_ARG;
    }
    let ver = CString::new(env!("CARGO_PKG_VERSION")).unwrap_or_default();
    let bytes = ver.as_bytes();
    let n = bytes.len().min(len - 1);
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, n);
        *buf.add(n) = 0;
    }
    MS_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_error_roundtrip() {
        // 全局状态跨测试竞争: 先清空再设（不依赖其他测试未写）
        set_last_error("");
        set_last_error("test error");
        let mut buf = [0u8; 64];
        let rc = ms_last_error(buf.as_mut_ptr() as *mut c_char, buf.len());
        assert_eq!(rc, MS_OK);
        let s = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) }
            .to_str()
            .unwrap();
        assert_eq!(s, "test error");
    }

    #[test]
    fn version_roundtrip() {
        let mut buf = [0u8; 32];
        let rc = ms_field_version(buf.as_mut_ptr() as *mut c_char, buf.len());
        assert_eq!(rc, MS_OK);
        let s = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) }
            .to_str()
            .unwrap();
        assert!(s.starts_with("0.1."), "version: {s}");
    }

    #[test]
    fn connect_null_cfg_fails() {
        let rc = ms_field_push_connect(ptr::null(), ptr::null_mut());
        assert_eq!(rc, MS_ERR_INVALID_ARG);
    }

    #[test]
    fn publish_null_handle_fails() {
        let mut track = [0u8; 64];
        let rc = ms_field_push_publish_video(ptr::null_mut(), track.as_mut_ptr() as *mut c_char, 64);
        assert_eq!(rc, MS_ERR_INVALID_ARG);
    }
}
