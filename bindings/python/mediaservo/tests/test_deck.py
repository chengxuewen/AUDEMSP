"""deck SDK 绑定测试（unittest，零依赖）。

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

from mediaservo import _ffi, deck  # noqa: E402


class DeckVersionTest(unittest.TestCase):
    def test_version(self):
        self.assertTrue(deck.version().startswith("0.1."))


class DeckEnumerateTest(unittest.TestCase):
    def test_enumerate_camera_double_call(self):
        # 双调用封装: 第一次查长度，第二次填缓冲 → stub 设备
        devs = deck.enumerate_devices(deck.DeviceKind.Camera)
        self.assertEqual(devs, ["stub:test-camera"])

    def test_enumerate_invalid_kind(self):
        # C 层 kind 越界 → INVALID_ARG（Python Enum 已在类型层拦截 99）
        n = ctypes.c_size_t()
        rc = deck._devices_enumerate(99, None, 0, ctypes.byref(n))
        self.assertEqual(rc, deck.ERR_INVALID_ARG)


class DeckErrorPathTest(unittest.TestCase):
    def test_open_nonexistent_device_device_error(self):
        with self.assertRaises(deck.DeckError) as cm:
            deck.CameraSource.open("no-such-device", deck.CaptureOptions())
        self.assertEqual(cm.exception.code, deck.ERR_DEVICE)

    def test_unconnected_call_state(self):
        cam = deck.CameraSource()
        with self.assertRaises(deck.DeckError) as cm:
            cam.start()
        self.assertEqual(cm.exception.code, deck.ERR_STATE)
        rec = deck.Recorder()
        with self.assertRaises(deck.DeckError) as cm:
            rec.record(cam)
        self.assertEqual(cm.exception.code, deck.ERR_STATE)

    def test_struct_size_invalid_arg(self):
        # R3: struct_size 首字段必须 >= sizeof(已知结构)；手动设 1 → INVALID_ARG
        opts = _ffi.mediaservo_deck_capture_options_t()
        opts.struct_size = 1
        out = ctypes.c_void_p()
        rc = deck._camera_open(b"stub:test-camera", ctypes.byref(opts), ctypes.byref(out))
        self.assertEqual(rc, deck.ERR_INVALID_ARG)

    def test_open_close_success(self):
        # 正例: 枚举到的设备可 open（仅本地初始化）→ close 幂等
        devs = deck.enumerate_devices(deck.DeviceKind.Camera)
        cam = deck.CameraSource.open(devs[0], deck.CaptureOptions(320, 240, 30))
        cam.close()
        cam.close()


class DeckFfiSafetyTest(unittest.TestCase):
    def test_all_functions_declared_restype_argtypes(self):
        # H3: 每个导出函数都必须声明 restype + argtypes（64 位指针截断防护）
        self.assertGreaterEqual(len(deck._API), 14)
        for name, (fn, restype, argtypes) in deck._API.items():
            self.assertEqual(fn.restype, restype, name)
            self.assertEqual(fn.argtypes, argtypes, name)
            self.assertIsNotNone(argtypes, name + " missing argtypes")


if __name__ == "__main__":
    unittest.main()
