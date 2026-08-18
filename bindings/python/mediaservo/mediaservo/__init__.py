"""MediaServo Python 绑定 — 设备侧三 SDK（field / link / deck）。

ctypes 加载 libmediaservo_<sdk>.so（D227 第一步；D228: 非 cargo workspace member）。
加载目录顺序（审核 H3）: MEDIASERVO_LIB_DIR → 包内 _libs/ → find_library。
开发模式: export MEDIASERVO_LIB_DIR=$PWD/target/debug
"""
__version__ = "0.1.0"

from . import _ffi  # noqa: F401  (共享 ctypes 层)
from . import field, link, deck  # noqa: F401

__all__ = ["field", "link", "deck", "MediaServoError", "__version__"]

MediaServoError = _ffi.MediaServoError
