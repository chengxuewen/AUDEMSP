"""MediaServo link Python 绑定 — 信令 + 帧总线（镜像 C++ mediaservo::link 类结构）。

API: SignalConfig + SignalSession（connect/send/on_event/close）、Bus（attach/
publish/subscribe/close）、Stream（recv/close）、FrameMeta（9 字段 dataclass
↔ ctypes 结构逐字段互转，mediaservo_frame_meta_t _pack_=1 36B）。

线程语义（C 契约）: on_event 回调在内部泵线程触发，事件 JSON 字符串仅回调内
有效——Python str 已拷贝，可安全保留。回调内禁止调用本对象任何方法。
"""

import ctypes
import traceback
from dataclasses import dataclass

from . import _ffi
from ._ffi import MediaServoError, cstr

__all__ = [
    "LinkError", "SignalConfig", "SignalSession", "Bus", "Stream", "FrameMeta", "version",
    "ERR_OK", "ERR_INVALID_ARG", "ERR_CONNECT", "ERR_SEND", "ERR_BUS",
    "ERR_STATE", "ERR_INTERNAL", "ERR_CLOSED",
]

# 错误码（link.h）
ERR_OK = 0
ERR_INVALID_ARG = -1
ERR_CONNECT = -2
ERR_SEND = -3
ERR_BUS = -4
ERR_STATE = -5
ERR_INTERNAL = -6
ERR_CLOSED = -7


class LinkError(MediaServoError):
    """link SDK 调用失败。"""


_lib = _ffi.load("link")

# 事件回调签名（link.h）: void (*)(mediaservo_link_signal_t*, const char*, void*)
_EVENT_CB_TYPE = ctypes.CFUNCTYPE(None, ctypes.c_void_p, ctypes.c_char_p, ctypes.c_void_p)


def _last_error() -> str:
    buf = ctypes.create_string_buffer(512)
    _last_error_fn(buf, 512)
    return buf.value.decode("utf-8", errors="replace")


def _check(rc: int) -> None:
    if rc != ERR_OK:
        raise LinkError(rc, _last_error())


# ── FFI 声明（H3: restype + argtypes 全覆盖，64 位指针截断防护）──────

_API = {}  # name -> (fn, restype, argtypes)，测试断言全覆盖


def _api(name, restype, argtypes):
    fn = getattr(_lib, name)
    fn.restype = restype
    fn.argtypes = argtypes
    _API[name] = (fn, restype, argtypes)
    return fn


_signal_connect = _api(
    "mediaservo_link_signal_connect", ctypes.c_int,
    [ctypes.POINTER(_ffi.mediaservo_link_signal_config_t), ctypes.POINTER(ctypes.c_void_p)],
)
_signal_send = _api(
    "mediaservo_link_signal_send", ctypes.c_int,
    [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t],
)
_signal_on_event = _api(
    "mediaservo_link_signal_on_event", None,
    [ctypes.c_void_p, _EVENT_CB_TYPE, ctypes.c_void_p],
)
_signal_close = _api("mediaservo_link_signal_close", ctypes.c_int, [ctypes.c_void_p])
_bus_attach = _api(
    "mediaservo_link_bus_attach", ctypes.c_int,
    [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)],
)
_bus_publish = _api(
    "mediaservo_link_bus_publish", ctypes.c_int,
    [ctypes.c_void_p, ctypes.c_char_p, ctypes.POINTER(ctypes.c_uint8), ctypes.c_size_t,
     ctypes.POINTER(_ffi.mediaservo_frame_meta_t)],
)
_bus_subscribe = _api(
    "mediaservo_link_bus_subscribe", ctypes.c_int,
    [ctypes.c_void_p, ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)],
)
_bus_recv = _api(
    "mediaservo_link_bus_recv", ctypes.c_int,
    [ctypes.c_void_p, ctypes.POINTER(_ffi.mediaservo_frame_meta_t),
     ctypes.POINTER(ctypes.c_uint8), ctypes.c_size_t, ctypes.POINTER(ctypes.c_size_t)],
)
_stream_close = _api("mediaservo_link_stream_close", ctypes.c_int, [ctypes.c_void_p])
_bus_close = _api("mediaservo_link_bus_close", ctypes.c_int, [ctypes.c_void_p])
_last_error_fn = _api("mediaservo_link_last_error", ctypes.c_int, [ctypes.c_char_p, ctypes.c_size_t])
_version_fn = _api("mediaservo_link_version", ctypes.c_int, [ctypes.c_char_p, ctypes.c_size_t])


def version() -> str:
    """SDK 版本 (MAJOR.MINOR.PATCH)，去尾 NUL。"""
    buf = ctypes.create_string_buffer(64)
    rc = _version_fn(buf, 64)
    if rc != ERR_OK:
        raise LinkError(rc, _last_error())
    return buf.value.decode()


@dataclass
class SignalConfig:
    """信令配置（对应 mediaservo_link_signal_config_t）。"""

    url: str
    psk: str
    room: str
    role: str = ""  # "Host"/"Pusher" → Host, "Client"/"Puller" → Remote；空 = Host


@dataclass
class FrameMeta:
    """帧元数据（9 字段，36B 线格式 D243；format: 0=未知 1=I420 2=NV12 3=RGBA）。"""

    seq: int = 0
    width: int = 0
    height: int = 0
    format: int = 0
    version: int = 0
    is_keyframe: int = 0
    reserved: int = 0
    ts_mono_ns: int = 0
    ts_epoch_ns: int = 0

    def to_c(self) -> _ffi.mediaservo_frame_meta_t:
        """→ ctypes 结构（逐字段赋值；禁 struct.pack 猜测——线格式由 _pack_=1 结构保证）。"""
        c = _ffi.mediaservo_frame_meta_t()
        c.seq = self.seq
        c.width = self.width
        c.height = self.height
        c.format = self.format
        c.version = self.version
        c.is_keyframe = self.is_keyframe
        c.reserved = self.reserved
        c.ts_mono_ns = self.ts_mono_ns
        c.ts_epoch_ns = self.ts_epoch_ns
        return c

    @classmethod
    def from_c(cls, c: _ffi.mediaservo_frame_meta_t) -> "FrameMeta":
        return cls(
            seq=c.seq, width=c.width, height=c.height, format=c.format,
            version=c.version, is_keyframe=c.is_keyframe, reserved=c.reserved,
            ts_mono_ns=c.ts_mono_ns, ts_epoch_ns=c.ts_epoch_ns,
        )


class SignalSession:
    """信令会话（对应 C++ mediaservo::link::SignalSession）。默认构造 = 已关闭。"""

    def __init__(self, h=None):
        self._h = h
        self._cb_ref = None  # CFUNCTYPE 引用（H3: 防 GC，C close join 泵线程后才释放）

    @classmethod
    def connect(cls, cfg: SignalConfig) -> "SignalSession":
        """连接信令 + 创建会话（阻塞）。失败抛 LinkError，不返回半开会话。"""
        c = _ffi.mediaservo_link_signal_config_t()
        c.struct_size = ctypes.sizeof(_ffi.mediaservo_link_signal_config_t)  # R3: 自动填充
        c.url = cstr(cfg.url)
        c.psk = cstr(cfg.psk)
        c.room = cstr(cfg.room)
        c.role = cstr(cfg.role)  # 空 → NULL = Host
        out = ctypes.c_void_p()
        rc = _signal_connect(ctypes.byref(c), ctypes.byref(out))
        if rc != ERR_OK:
            raise LinkError(rc, _last_error())
        return cls(out.value)

    def _require_open(self) -> None:
        if not self._h:
            raise LinkError(ERR_STATE, "session not connected")

    def send(self, json_str: str) -> None:
        """发送一条信令消息（JSON；SignalingMessage type 标签 snake_case）。"""
        self._require_open()
        payload = json_str.encode("utf-8")
        rc = _signal_send(self._h, payload, len(payload))
        if rc != ERR_OK:
            raise LinkError(rc, _last_error())

    def on_event(self, callback) -> None:
        """注册事件回调（connect 后任意时刻；重复注册替换；泵线程触发）。

        callback(event_json: str) -> None；回调内禁止调用本对象任何方法。
        已关闭会话: no-op（C++ parity）。
        """
        if not self._h:
            return

        def _trampoline(_h, event_json, _user):
            try:
                callback(event_json.decode("utf-8", errors="replace") if event_json else "")
            except Exception:
                traceback.print_exc()  # 泵线程异常不得越过 C 边界（UB）

        self._cb_ref = _EVENT_CB_TYPE(_trampoline)  # H3: 保存引用防 GC
        _signal_on_event(self._h, self._cb_ref, None)

    def close(self) -> None:
        """关闭会话并释放 handle（幂等；C close join 事件泵后释放回调引用）。"""
        h, self._h = self._h, None
        if h is None:
            return
        try:
            rc = _signal_close(h)
        finally:
            self._cb_ref = None
        if rc != ERR_OK:
            raise LinkError(rc, _last_error())

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass


class Bus:
    """帧总线（对应 C++ mediaservo::link::Bus）。默认构造 = 已关闭。"""

    def __init__(self, h=None):
        self._h = h

    @classmethod
    def attach(cls, endpoint: str, token_pem: str, vk_pem: str) -> "Bus":
        """附加帧总线（验签 + ACL + iceoryx2 节点，阻塞）。endpoint 为 Phase 1 预留（空串即可）。"""
        out = ctypes.c_void_p()
        rc = _bus_attach(cstr(endpoint), cstr(token_pem), cstr(vk_pem), ctypes.byref(out))
        if rc != ERR_OK:
            raise LinkError(rc, _last_error())
        return cls(out.value)

    def _require_open(self) -> None:
        if not self._h:
            raise LinkError(ERR_STATE, "bus not attached")

    def publish(self, topic: str, payload: bytes, meta: FrameMeta) -> None:
        """发布一帧（ACL 检查 + SHM loan + send，阻塞）。payload 空 = 纯元数据帧。"""
        self._require_open()
        # C 契约: payload NULL 当且仅当 len == 0
        buf = ctypes.create_string_buffer(payload) if payload else None
        cmeta = meta.to_c()
        rc = _bus_publish(self._h, cstr(topic), buf, len(payload), ctypes.byref(cmeta))
        if rc != ERR_OK:
            raise LinkError(rc, _last_error())

    def subscribe(self, topic: str) -> "Stream":
        """订阅 topic，创建帧流（阻塞）。"""
        self._require_open()
        out = ctypes.c_void_p()
        rc = _bus_subscribe(self._h, cstr(topic), ctypes.byref(out))
        if rc != ERR_OK:
            raise LinkError(rc, _last_error())
        return Stream(out.value)

    def close(self) -> None:
        """关闭帧总线并释放 handle（幂等；shutdown 全部流，stream recv 返回 CLOSED）。"""
        h, self._h = self._h, None
        if h is None:
            return
        rc = _bus_close(h)
        if rc != ERR_OK:
            raise LinkError(rc, _last_error())

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass


class Stream:
    """帧流（对应 C++ mediaservo::link::Stream）。默认构造 = 已关闭。"""

    # ponytail: 单缓冲 16MiB 覆盖 4K I420（12.4MiB）；C ABI 无法探测截断，
    # 更大帧需扩缓冲 —— 升级路径: 按 meta 缓存上次帧尺寸自适应增长（同 C++）。
    _CAP = 16 * 1024 * 1024

    def __init__(self, h=None):
        self._h = h
        self._buf = ctypes.create_string_buffer(self._CAP)

    def _require_open(self) -> None:
        if not self._h:
            raise LinkError(ERR_STATE, "stream not subscribed")

    def recv(self):
        """阻塞取帧 → (FrameMeta, bytes)。关停（stream/bus close）抛 LinkError CLOSED。"""
        self._require_open()
        meta = _ffi.mediaservo_frame_meta_t()
        out_len = ctypes.c_size_t()
        rc = _bus_recv(self._h, ctypes.byref(meta), self._buf, self._CAP, ctypes.byref(out_len))
        if rc != ERR_OK:
            if rc == ERR_CLOSED:
                raise LinkError(rc, "stream closed")
            raise LinkError(rc, _last_error())
        return FrameMeta.from_c(meta), bytes(self._buf[: out_len.value])

    def close(self) -> None:
        """关闭帧流并释放 handle（幂等；唤醒阻塞中的 recv 使其返回 CLOSED）。"""
        h, self._h = self._h, None
        if h is None:
            return
        rc = _stream_close(h)
        if rc != ERR_OK:
            raise LinkError(rc, _last_error())

    def __del__(self):
        try:
            self.close()
        except Exception:
            pass
