"""link SDK 绑定测试（unittest，零依赖）。

运行（工作区根）:
  export PATH="$HOME/.pixi/bin:$PATH"
  export MEDIASERVO_LIB_DIR=$(pwd)/target/debug
  python3 -m unittest discover -s bindings/python/mediaservo/tests

注: 不触碰 iceoryx2 SHM（无 attach 正例）；FrameMeta 36B 往返为纯 Python 级验证。
"""

import ctypes
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

from mediaservo import _ffi, link  # noqa: E402


class LinkVersionTest(unittest.TestCase):
    def test_version(self):
        self.assertTrue(link.version().startswith("0.1."))


class LinkErrorPathTest(unittest.TestCase):
    def test_connect_empty_required_invalid_arg(self):
        with self.assertRaises(link.LinkError) as cm:
            link.SignalSession.connect(link.SignalConfig(url="", psk="", room=""))
        self.assertEqual(cm.exception.code, link.ERR_INVALID_ARG)

    def test_unconnected_call_state(self):
        s = link.SignalSession()
        with self.assertRaises(link.LinkError) as cm:
            s.send('{"type":"ping"}')
        self.assertEqual(cm.exception.code, link.ERR_STATE)
        b = link.Bus()
        with self.assertRaises(link.LinkError) as cm:
            b.publish("camera/0", b"", link.FrameMeta())
        self.assertEqual(cm.exception.code, link.ERR_STATE)
        st = link.Stream()
        with self.assertRaises(link.LinkError) as cm:
            st.recv()
        self.assertEqual(cm.exception.code, link.ERR_STATE)

    def test_struct_size_invalid_arg(self):
        # R3: struct_size 首字段必须 >= sizeof(已知结构)；手动设 1 → INVALID_ARG
        cfg = _ffi.mediaservo_link_signal_config_t()
        cfg.struct_size = 1
        out = ctypes.c_void_p()
        rc = link._signal_connect(ctypes.byref(cfg), ctypes.byref(out))
        self.assertEqual(rc, link.ERR_INVALID_ARG)

    def test_close_idempotent(self):
        s = link.SignalSession()
        s.close()
        s.close()
        b = link.Bus()
        b.close()
        st = link.Stream()
        st.close()


class LinkFrameMetaTest(unittest.TestCase):
    def test_frame_meta_36_bytes(self):
        # R4: _pack_=1 → 36B 线格式；自然对齐是 40B 会错帧
        self.assertEqual(ctypes.sizeof(_ffi.mediaservo_frame_meta_t), 36)

    def test_frame_meta_roundtrip(self):
        # 构造 → to_c → from_c，9 字段逐字段一致
        meta = link.FrameMeta(
            seq=0x0102030405060708, width=1280, height=720, format=1,
            version=0, is_keyframe=1, reserved=0,
            ts_mono_ns=1234567890123, ts_epoch_ns=9876543210987,
        )
        back = link.FrameMeta.from_c(meta.to_c())
        self.assertEqual(meta, back)


class LinkFfiSafetyTest(unittest.TestCase):
    def test_all_functions_declared_restype_argtypes(self):
        # H3: 每个导出函数都必须声明 restype + argtypes（64 位指针截断防护）
        self.assertGreaterEqual(len(link._API), 11)
        for name, (fn, restype, argtypes) in link._API.items():
            self.assertEqual(fn.restype, restype, name)
            self.assertEqual(fn.argtypes, argtypes, name)
            self.assertIsNotNone(argtypes, name + " missing argtypes")


if __name__ == "__main__":
    unittest.main()
