"""field SDK 绑定测试（unittest，零依赖）。

运行（工作区根）:
  export PATH="$HOME/.pixi/bin:$PATH"
  export MEDIASERVO_LIB_DIR=$(pwd)/target/debug
  python3 -m unittest discover -s bindings/python/mediaservo/tests
"""

import ctypes
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from mediaservo import _ffi, field  # noqa: E402


class FieldVersionTest(unittest.TestCase):
    def test_version(self):
        self.assertTrue(field.version().startswith("0.1."))


class FieldErrorPathTest(unittest.TestCase):
    def test_connect_empty_required_invalid_arg(self):
        # url/psk/room 必填 → C 校验 → INVALID_ARG
        with self.assertRaises(field.FieldError) as cm:
            field.PushSession.connect(field.PushConfig(url="", psk="", room=""))
        self.assertEqual(cm.exception.code, field.ERR_INVALID_ARG)
        self.assertIsInstance(cm.exception.message, str)

    def test_unconnected_call_state(self):
        # 默认构造会话（未连接）调用 → STATE
        s = field.PushSession()
        with self.assertRaises(field.FieldError) as cm:
            s.publish_video()
        self.assertEqual(cm.exception.code, field.ERR_STATE)
        with self.assertRaises(field.FieldError) as cm:
            s.start_video_frames()
        self.assertEqual(cm.exception.code, field.ERR_STATE)

    def test_struct_size_invalid_arg(self):
        # R3: struct_size 首字段必须 >= sizeof(已知结构)；手动设 1 → INVALID_ARG
        cfg = _ffi.mediaservo_push_config_t()
        cfg.struct_size = 1
        out = ctypes.c_void_p()
        rc = field._push_connect(ctypes.byref(cfg), ctypes.byref(out))
        self.assertEqual(rc, field.ERR_INVALID_ARG)

    def test_close_idempotent_and_del_safe(self):
        s = field.PushSession()
        s.close()  # 已关闭 close → OK 不抛
        s.close()
        del s  # __del__ 兜底不抛


class FieldFfiSafetyTest(unittest.TestCase):
    def test_all_functions_declared_restype_argtypes(self):
        # H3: 每个导出函数都必须声明 restype + argtypes（64 位指针截断防护）
        self.assertGreaterEqual(len(field._API), 7)
        for name, (fn, restype, argtypes) in field._API.items():
            self.assertEqual(fn.restype, restype, name)
            self.assertEqual(fn.argtypes, argtypes, name)
            self.assertIsNotNone(argtypes, name + " missing argtypes")


if __name__ == "__main__":
    unittest.main()
