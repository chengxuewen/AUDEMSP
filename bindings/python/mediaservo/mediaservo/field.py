"""MediaServo Field Python 绑定 — 推流会话（镜像 C++ mediaservo::field 类结构）。

API: PushConfig(dataclass) + PushSession（connect/publish_video/start_video_frames/
stop_video_frames/close）。错误抛 FieldError（code + message），close 幂等，
__del__ 兜底 close（不抛异常）。
"""

import ctypes
from dataclasses import dataclass

from . import _ffi
from ._ffi import MediaServoError, cstr

__all__ = [
    "FieldError", "PushConfig", "PushSession", "version",
    "ERR_OK", "ERR_INVALID_ARG", "ERR_CONNECT", "ERR_PUBLISH", "ERR_STATE", "ERR_INTERNAL",
]

# 错误码（field.h）
ERR_OK = 0
ERR_INVALID_ARG = -1
ERR_CONNECT = -2
ERR_PUBLISH = -3
ERR_STATE = -4
ERR_INTERNAL = -5


class FieldError(MediaServoError):
    """field SDK 调用失败。"""


_lib = _ffi.load("field")


def _last_error() -> str:
    buf = ctypes.create_string_buffer(512)
    _last_error_fn(buf, 512)
    return buf.value.decode("utf-8", errors="replace")


def _check(rc: int) -> None:
    if rc != ERR_OK:
        raise FieldError(rc, _last_error())


# ── FFI 声明（H3: restype + argtypes 全覆盖，64 位指针截断防护）──────

_API = {}  # name -> (fn, restype, argtypes)，测试断言全覆盖


def _api(name, restype, argtypes):
    fn = getattr(_lib, name)
    fn.restype = restype
    fn.argtypes = argtypes
    _API[name] = (fn, restype, argtypes)
    return fn


_push_connect = _api(
    "mediaservo_field_push_connect", ctypes.c_int,
    [ctypes.POINTER(_ffi.mediaservo_push_config_t), ctypes.POINTER(ctypes.c_void_p)],
)
_push_publish_video = _api(
    "mediaservo_field_push_publish_video", ctypes.c_int,
    [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t],
)
_push_start_video_frames = _api("mediaservo_field_push_start_video_frames", ctypes.c_int, [ctypes.c_void_p])
_push_stop_video_frames = _api("mediaservo_field_push_stop_video_frames", None, [ctypes.c_void_p])
_push_close = _api("mediaservo_field_push_close", ctypes.c_int, [ctypes.c_void_p])
_last_error_fn = _api("mediaservo_field_last_error", ctypes.c_int, [ctypes.c_char_p, ctypes.c_size_t])
_version_fn = _api("mediaservo_field_version", ctypes.c_int, [ctypes.c_char_p, ctypes.c_size_t])


def version() -> str:
    """SDK 版本 (MAJOR.MINOR.PATCH)，去尾 NUL。"""
    buf = ctypes.create_string_buffer(64)
    rc = _version_fn(buf, 64)
    if rc != ERR_OK:
        raise FieldError(rc, _last_error())
    return buf.value.decode()


@dataclass
class PushConfig:
    """推流配置（对应 mediaservo_push_config_t；默认值与 C DEFAULT 一致）。"""

    url: str
    psk: str
    room: str
    width: int = 1280
    height: int = 720
    framerate: int = 30
    bitrate_kbps: int = 2000
    keyframe_interval: int = 2


class PushSession:
    """推流会话（对应 C++ mediaservo::field::PushSession）。默认构造 = 已关闭。"""

    def __init__(self, h=None):
        self._h = h

    @classmethod
    def connect(cls, cfg: PushConfig) -> "PushSession":
        """连接信令 + 创建会话（阻塞）。失败抛 FieldError，不返回半开会话。"""
        c = _ffi.mediaservo_push_config_t()
        c.struct_size = ctypes.sizeof(_ffi.mediaservo_push_config_t)  # R3: 自动填充
        c.url = cstr(cfg.url)
        c.psk = cstr(cfg.psk)
        c.room = cstr(cfg.room)
        c.width = cfg.width
        c.height = cfg.height
        c.framerate = cfg.framerate
        c.bitrate_kbps = cfg.bitrate_kbps
        c.keyframe_interval = cfg.keyframe_interval
        out = ctypes.c_void_p()
        rc = _push_connect(ctypes.byref(c), ctypes.byref(out))
        if rc != ERR_OK:
            raise FieldError(rc, _last_error())
        return cls(out.value)

    def _require_open(self) -> None:
        if not self._h:
            raise FieldError(ERR_STATE, "session not connected")

    def publish_video(self) -> str:
        """发布视频轨（阻塞协商）。返回 track id。"""
        self._require_open()
        track = ctypes.create_string_buffer(64)
        rc = _push_publish_video(self._h, track, 64)
        if rc != ERR_OK:
            raise FieldError(rc, _last_error())
        return track.value.decode()

    def start_video_frames(self) -> None:
        """启动视频帧生成（Squares + 时间戳水印）。"""
        self._require_open()
        rc = _push_start_video_frames(self._h)
        if rc != ERR_OK:
            raise FieldError(rc, _last_error())

    def stop_video_frames(self) -> None:
        """停止视频帧生成（幂等；C ABI 为 void 无错误通道）。"""
        if self._h:
            _push_stop_video_frames(self._h)

    def close(self) -> None:
        """关闭会话并释放 handle（幂等）。"""
        h, self._h = self._h, None
        if h is None:
            return
        rc = _push_close(h)
        if rc != ERR_OK:
            raise FieldError(rc, _last_error())

    def __del__(self):
        try:  # 兜底 close；__del__ 内禁止抛异常（解释器关停期 ctypes 可能已卸载）
            self.close()
        except Exception:
            pass
