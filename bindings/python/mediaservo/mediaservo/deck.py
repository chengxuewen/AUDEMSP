"""MediaServo deck Python 绑定 — 采集/录制/回放（镜像 C++ mediaservo::deck 类结构）。

API: DeviceKind(Enum) + enumerate_devices、CaptureOptions + CameraSource（open/
start/on_frame/stop/close）、Recorder（open/record/stop/close）、Player（open/
on_frame/close）。帧回调收 Frame dataclass（data 已拷贝为 bytes——C 指针仅
回调内有效）。

线程语义（C 契约）: 帧回调在泵线程触发，回调内禁止调用任何 deck API。
关闭顺序（录制场景）: recorder stop/close 先于 camera stop/close。
"""

import ctypes
import enum
import traceback
from dataclasses import dataclass

from . import _ffi
from ._ffi import MediaServoError, cstr

__all__ = [
    "DeckError", "DeviceKind", "CaptureOptions", "Frame",
    "CameraSource", "Recorder", "Player", "enumerate_devices", "version",
    "ERR_OK", "ERR_INVALID_ARG", "ERR_DEVICE", "ERR_RECORDER",
    "ERR_PLAYER", "ERR_STATE", "ERR_INTERNAL",
]

# 错误码（deck.h）
ERR_OK = 0
ERR_INVALID_ARG = -1
ERR_DEVICE = -2
ERR_RECORDER = -3
ERR_PLAYER = -4
ERR_STATE = -5
ERR_INTERNAL = -6


class DeckError(MediaServoError):
    """deck SDK 调用失败。"""


_lib = _ffi.load("deck")

# 帧回调签名（deck.h）: void (*)(const mediaservo_frame_t*, void*)
_FRAME_CB_TYPE = ctypes.CFUNCTYPE(None, ctypes.POINTER(_ffi.mediaservo_frame_t), ctypes.c_void_p)


def _last_error() -> str:
    buf = ctypes.create_string_buffer(512)
    _last_error_fn(buf, 512)
    return buf.value.decode("utf-8", errors="replace")


def _check(rc: int) -> None:
    if rc != ERR_OK:
        raise DeckError(rc, _last_error())


# ── FFI 声明（H3: restype + argtypes 全覆盖，64 位指针截断防护）──────

_API = {}  # name -> (fn, restype, argtypes)，测试断言全覆盖


def _api(name, restype, argtypes):
    fn = getattr(_lib, name)
    fn.restype = restype
    fn.argtypes = argtypes
    _API[name] = (fn, restype, argtypes)
    return fn


_devices_enumerate = _api(
    "mediaservo_deck_devices_enumerate", ctypes.c_int,
    [ctypes.c_int, ctypes.c_char_p, ctypes.c_size_t, ctypes.POINTER(ctypes.c_size_t)],
)
_camera_open = _api(
    "mediaservo_deck_camera_open", ctypes.c_int,
    [ctypes.c_char_p, ctypes.POINTER(_ffi.mediaservo_deck_capture_options_t),
     ctypes.POINTER(ctypes.c_void_p)],
)
_camera_start = _api("mediaservo_deck_camera_start", ctypes.c_int, [ctypes.c_void_p])
_camera_frames_cb = _api(
    "mediaservo_deck_camera_frames_cb", ctypes.c_int,
    [ctypes.c_void_p, _FRAME_CB_TYPE, ctypes.c_void_p],
)
_camera_stop = _api("mediaservo_deck_camera_stop", ctypes.c_int, [ctypes.c_void_p])
_camera_close = _api("mediaservo_deck_camera_close", ctypes.c_int, [ctypes.c_void_p])
_recorder_new = _api(
    "mediaservo_deck_recorder_new", ctypes.c_int,
    [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)],
)
_recorder_record = _api("mediaservo_deck_recorder_record", ctypes.c_int, [ctypes.c_void_p, ctypes.c_void_p])
_recorder_stop = _api("mediaservo_deck_recorder_stop", ctypes.c_int, [ctypes.c_void_p])
_recorder_close = _api("mediaservo_deck_recorder_close", ctypes.c_int, [ctypes.c_void_p])
_player_open = _api(
    "mediaservo_deck_player_open", ctypes.c_int,
    [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)],
)
_player_frames_cb = _api(
    "mediaservo_deck_player_frames_cb", ctypes.c_int,
    [ctypes.c_void_p, _FRAME_CB_TYPE, ctypes.c_void_p],
)
_player_close = _api("mediaservo_deck_player_close", ctypes.c_int, [ctypes.c_void_p])
_last_error_fn = _api("mediaservo_deck_last_error", ctypes.c_int, [ctypes.c_char_p, ctypes.c_size_t])
_version_fn = _api("mediaservo_deck_version", ctypes.c_int, [ctypes.c_char_p, ctypes.c_size_t])


def version() -> str:
    """SDK 版本 (MAJOR.MINOR.PATCH)，去尾 NUL。"""
    buf = ctypes.create_string_buffer(64)
    rc = _version_fn(buf, 64)
    if rc != ERR_OK:
        raise DeckError(rc, _last_error())
    return buf.value.decode()


class DeviceKind(enum.Enum):
    """设备种类（对应 enumerate 的 kind 参数）。"""

    Camera = 0
    Audio = 1
    Screen = 2


@dataclass
class CaptureOptions:
    """采集选项（对应 mediaservo_deck_capture_options_t）。"""

    width: int = 1280
    height: int = 720
    framerate: int = 30


@dataclass
class Frame:
    """一帧（I420 三平面拼接: data = y + u + v，各平面行宽 = stride_*）。

    data 已拷贝为 bytes —— C 的 data_* 指针仅回调内有效。
    平面切片: y = data[:stride_y*height]; u = data[stride_y*height:...]。
    """

    width: int
    height: int
    pts_us: int
    stride_y: int
    stride_u: int
    stride_v: int
    data: bytes


def enumerate_devices(kind: DeviceKind) -> list:
    """枚举设备 id（双调用模式；多设备 '\n' 分隔；失败 = 空列表，C++ parity）。"""
    first = ctypes.c_size_t()
    rc = _devices_enumerate(int(kind.value), None, 0, ctypes.byref(first))
    if rc < 0 or first.value == 0:  # rc 为正 = 所需长度（snprintf 约定）
        return []
    buf = ctypes.create_string_buffer(first.value + 1)  # C 写 cap-1 字符 + NUL
    out_len = ctypes.c_size_t()
    rc = _devices_enumerate(int(kind.value), buf, first.value + 1, ctypes.byref(out_len))
    if rc < 0:
        return []
    ids = buf.value.decode("utf-8", errors="replace")
    return [d for d in ids.split("\n") if d] if ids else []


def _make_frame_trampoline(cb):
    """Python 帧回调 → CFUNCTYPE（H3: 返回对象由调用方保存在持有句柄的对象上）。

    拷贝 I420 三平面为 bytes（指针仅回调内有效）；泵线程异常不得越过 C 边界（UB）。
    """

    def _trampoline(frame_ptr, _user):
        try:
            f = frame_ptr.contents
            n_y = f.stride_y * f.height
            n_uv = f.stride_u * (f.height // 2)
            n_v = f.stride_v * (f.height // 2)

            def _copy(p, n):
                return ctypes.string_at(p, n) if p else b""

            cb(Frame(
                width=f.width, height=f.height, pts_us=f.pts_us,
                stride_y=f.stride_y, stride_u=f.stride_u, stride_v=f.stride_v,
                data=_copy(f.data_y, n_y) + _copy(f.data_u, n_uv) + _copy(f.data_v, n_v),
            ))
        except Exception:
            traceback.print_exc()

    return _FRAME_CB_TYPE(_trampoline)


class CameraSource:
    """相机采集（对应 C++ mediaservo::deck::CameraSource）。默认构造 = 已关闭。"""

    def __init__(self, h=None):
        self._h = h
        self._cb_ref = None  # CFUNCTYPE 引用（H3: 防 GC，C close join 帧泵后才释放）

    @classmethod
    def open(cls, dev_id: str, opts: CaptureOptions = None) -> "CameraSource":
        """打开相机（仅本地初始化；dev_id 必须存在于枚举结果）。"""
        opts = opts or CaptureOptions()
        c = _ffi.mediaservo_deck_capture_options_t()
        c.struct_size = ctypes.sizeof(_ffi.mediaservo_deck_capture_options_t)  # R3: 自动填充
        c.width = opts.width
        c.height = opts.height
        c.framerate = opts.framerate
        out = ctypes.c_void_p()
        rc = _camera_open(cstr(dev_id), ctypes.byref(c), ctypes.byref(out))
        if rc != ERR_OK:
            raise DeckError(rc, _last_error())
        return cls(out.value)

    def _require_open(self) -> None:
        if not self._h:
            raise DeckError(ERR_STATE, "camera not open")

    def start(self) -> None:
        """开始产帧（用 open 时的 opts；只允许一次，重复调用 → STATE）。"""
        self._require_open()
        rc = _camera_start(self._h)
        if rc != ERR_OK:
            raise DeckError(rc, _last_error())

    def on_frame(self, callback) -> None:
        """注册帧回调（泵线程逐帧触发；重复注册替换；回调收 Frame）。

        回调内禁止调用任何 deck API；帧数据已拷贝，可安全保留。
        """
        self._require_open()
        self._cb_ref = _make_frame_trampoline(callback)  # H3: 保存引用防 GC
        rc = _camera_frames_cb(self._h, self._cb_ref, None)
        if rc != ERR_OK:
            self._cb_ref = None
            raise DeckError(rc, _last_error())

    def stop(self) -> None:
        """停止产帧（幂等）。"""
        self._require_open()
        rc = _camera_stop(self._h)
        if rc != ERR_OK:
            raise DeckError(rc, _last_error())

    def close(self) -> None:
        """关闭相机并释放 handle（幂等；C close join 帧泵后释放回调引用）。"""
        h, self._h = self._h, None
        if h is None:
            return
        try:
            rc = _camera_close(h)
        finally:
            self._cb_ref = None
        if rc != ERR_OK:
            raise DeckError(rc, _last_error())

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass


class Recorder:
    """录制器（对应 C++ mediaservo::deck::Recorder）。默认构造 = 已关闭。"""

    def __init__(self, h=None):
        self._h = h

    @classmethod
    def open(cls, path: str) -> "Recorder":
        """创建录制器（默认 h264/mp4；父目录必须已存在）。"""
        out = ctypes.c_void_p()
        rc = _recorder_new(cstr(path), ctypes.byref(out))
        if rc != ERR_OK:
            raise DeckError(rc, _last_error())
        return cls(out.value)

    def _require_open(self) -> None:
        if not self._h:
            raise DeckError(ERR_STATE, "recorder not open")

    def record(self, camera: CameraSource) -> None:
        """桥接录制: camera 帧泵 → recorder。camera 必须已 start 且活到录制结束。"""
        self._require_open()
        if not camera._h:
            raise DeckError(ERR_INVALID_ARG, "camera closed")
        rc = _recorder_record(self._h, camera._h)
        if rc != ERR_OK:
            raise DeckError(rc, _last_error())

    def stop(self) -> None:
        """请求停止录制（幂等；flush + trailer 收尾在 close 时完成）。"""
        self._require_open()
        rc = _recorder_stop(self._h)
        if rc != ERR_OK:
            raise DeckError(rc, _last_error())

    def close(self) -> None:
        """关闭录制器并释放 handle（幂等；join 录制任务完成 flush）。"""
        h, self._h = self._h, None
        if h is None:
            return
        rc = _recorder_close(h)
        if rc != ERR_OK:
            raise DeckError(rc, _last_error())

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass


class Player:
    """回放器（对应 C++ mediaservo::deck::Player）。默认构造 = 已关闭。"""

    def __init__(self, h=None):
        self._h = h
        self._cb_ref = None

    @classmethod
    def open(cls, path: str) -> "Player":
        """打开媒体文件（demux + 解码器就绪）。"""
        out = ctypes.c_void_p()
        rc = _player_open(cstr(path), ctypes.byref(out))
        if rc != ERR_OK:
            raise DeckError(rc, _last_error())
        return cls(out.value)

    def _require_open(self) -> None:
        if not self._h:
            raise DeckError(ERR_STATE, "player not open")

    def on_frame(self, callback) -> None:
        """逐帧解码回调泵（运行至 EOF 自然结束；只允许一次）。

        close 为阻塞 join（等待解码完成）—— 长文件需等待，无法中途中止（YAGNI）。
        """
        self._require_open()
        self._cb_ref = _make_frame_trampoline(callback)  # H3: 保存引用防 GC
        rc = _player_frames_cb(self._h, self._cb_ref, None)
        if rc != ERR_OK:
            self._cb_ref = None
            raise DeckError(rc, _last_error())

    def close(self) -> None:
        """关闭回放器并释放 handle（幂等；join 解码泵至完成后释放回调引用）。"""
        h, self._h = self._h, None
        if h is None:
            return
        try:
            rc = _player_close(h)
        finally:
            self._cb_ref = None
        if rc != ERR_OK:
            raise DeckError(rc, _last_error())

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass
