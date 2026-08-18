"""MediaServo ctypes 共享加载层 — field/link/deck 三 SDK 共用。

加载顺序（审核 H3）:
  1. MEDIASERVO_LIB_DIR 环境变量指向的目录（开发模式: target/debug）
  2. 包内 mediaservo/_libs/（打包分发时随 wheel 携带）
  3. ctypes.util.find_library("mediaservo_<sdk>")（系统安装路径）

FFI 安全（H3）:
  - 每个导出函数必须声明 restype + argtypes（64 位指针截断防护）；
    各 SDK 模块经 _api() 注册到 _API 表，测试断言全覆盖。
  - CFUNCTYPE 回调对象必须由持有句柄的 Python 对象保存引用（防 GC）,
    各模块在 close() 的 C 调用（join 泵线程）之后才释放。
  - C 结构体逐字段对齐 C 头文件（bindings/c/include/mediaservo/*.h）;
    mediaservo_frame_meta_t 必须 _pack_ = 1（36B 线格式 D243，自然对齐 40B 会错帧）。
"""

import ctypes
import ctypes.util
import os
import sys

__all__ = [
    "MediaServoError",
    "load",
    "cstr",
    "mediaservo_frame_meta_t",
    "mediaservo_frame_t",
    "mediaservo_push_config_t",
    "mediaservo_link_signal_config_t",
    "mediaservo_deck_capture_options_t",
]


class MediaServoError(Exception):
    """SDK 调用失败（code 为对应头文件 MEDIASERVO_<SDK>_ERR_* 值，message 读自 last_error）。"""

    def __init__(self, code: int, message: str):
        super().__init__("%s (code %d)" % (message, code))
        self.code = code
        self.message = message


# ── 共享 C 类型（common.h）──────────────────────────────────────────

# 定长 36B 线格式（D243）: seq(8)+width(4)+height(4)+format(1)+version(1)
# +is_keyframe(1)+reserved(1)+ts_mono_ns(8)+ts_epoch_ns(8)
class mediaservo_frame_meta_t(ctypes.Structure):
    _pack_ = 1
    _fields_ = [
        ("seq", ctypes.c_uint64),
        ("width", ctypes.c_uint32),
        ("height", ctypes.c_uint32),
        ("format", ctypes.c_uint8),        # 0=未知, 1=I420, 2=NV12, 3=RGBA
        ("version", ctypes.c_uint8),
        ("is_keyframe", ctypes.c_uint8),
        ("reserved", ctypes.c_uint8),      # 必须填 0
        ("ts_mono_ns", ctypes.c_uint64),
        ("ts_epoch_ns", ctypes.c_uint64),
    ]


# 内存帧描述（deck 采集/回放回调，I420 三平面；data_* 指针仅回调内有效）
class mediaservo_frame_t(ctypes.Structure):
    _fields_ = [
        ("width", ctypes.c_uint32),
        ("height", ctypes.c_uint32),
        ("pts_us", ctypes.c_uint64),
        ("stride_y", ctypes.c_uint32),
        ("stride_u", ctypes.c_uint32),
        ("stride_v", ctypes.c_uint32),
        ("data_y", ctypes.POINTER(ctypes.c_uint8)),
        ("data_u", ctypes.POINTER(ctypes.c_uint8)),
        ("data_v", ctypes.POINTER(ctypes.c_uint8)),
    ]


# field.h — 推流配置（struct_size 首字段 R3，调用方必须填 sizeof）
class mediaservo_push_config_t(ctypes.Structure):
    _fields_ = [
        ("struct_size", ctypes.c_size_t),
        ("url", ctypes.c_char_p),
        ("psk", ctypes.c_char_p),
        ("room", ctypes.c_char_p),
        ("width", ctypes.c_uint32),        # 默认 1280
        ("height", ctypes.c_uint32),       # 默认 720
        ("framerate", ctypes.c_uint32),    # 默认 30
        ("bitrate_kbps", ctypes.c_uint32), # 默认 2000
        ("keyframe_interval", ctypes.c_uint64),  # 默认 2
    ]


# link.h — 信令配置
class mediaservo_link_signal_config_t(ctypes.Structure):
    _fields_ = [
        ("struct_size", ctypes.c_size_t),
        ("url", ctypes.c_char_p),
        ("psk", ctypes.c_char_p),
        ("room", ctypes.c_char_p),
        ("role", ctypes.c_char_p),  # "Host"/"Pusher"→Host, "Client"/"Puller"→Remote; NULL=Host
    ]


# deck.h — 采集选项（全 0 字段 = C 默认 1280x720@30）
class mediaservo_deck_capture_options_t(ctypes.Structure):
    _fields_ = [
        ("struct_size", ctypes.c_size_t),
        ("width", ctypes.c_uint32),
        ("height", ctypes.c_uint32),
        ("framerate", ctypes.c_uint32),
    ]


# ── 加载 ────────────────────────────────────────────────────────────

_LIBS = {}  # sdk -> CDLL 缓存


def _lib_filename(sdk: str) -> str:
    if sys.platform == "darwin":
        return "libmediaservo_%s.dylib" % sdk
    if os.name == "nt":
        return "mediaservo_%s.dll" % sdk
    return "libmediaservo_%s.so" % sdk


def _find_library(sdk: str):
    """按 H3 顺序定位 libmediaservo_<sdk>，返回路径或 None。"""
    name = _lib_filename(sdk)
    env_dir = os.environ.get("MEDIASERVO_LIB_DIR")
    if env_dir:
        cand = os.path.join(env_dir, name)
        if os.path.exists(cand):
            return cand
        cand_ver = os.path.join(env_dir, name + ".0")
        if os.path.exists(cand_ver):
            return cand_ver
    pkg_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "_libs")
    cand = os.path.join(pkg_dir, name)
    if os.path.exists(cand):
        return cand
    found = ctypes.util.find_library("mediaservo_%s" % sdk)
    if found:
        return found
    return None


def load(sdk: str):
    """加载并缓存 libmediaservo_<sdk>（field/link/deck）。找不到抛 ImportError 带指引。"""
    if sdk not in _LIBS:
        path = _find_library(sdk)
        if path is None:
            raise ImportError(
                "cannot load libmediaservo_%s: set MEDIASERVO_LIB_DIR to the build output "
                "dir (e.g. export MEDIASERVO_LIB_DIR=$PWD/target/debug) or add the dir "
                "containing it to LD_LIBRARY_PATH" % sdk
            )
        _LIBS[sdk] = ctypes.CDLL(path)
    return _LIBS[sdk]


def cstr(s) -> bytes:
    """str → bytes；空串 → None（C 契约: 可选/必填字符串空串传 NULL）。"""
    return s.encode("utf-8") if s else None
