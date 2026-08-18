#!/usr/bin/env python3
"""deck 闭环 — 采集 → 录制 → 回放（枚举 → CameraSource → Recorder → Player）。

运行（deck 库需 FFmpeg 已构建 + MEDIASERVO_LIB_DIR）:
  export MEDIASERVO_LIB_DIR=$PWD/target/debug
  python3 examples/record_playback.py
"""

import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))  # 开发模式

import mediaservo
from mediaservo.deck import (
    CameraSource, CaptureOptions, DeviceKind, Player, Recorder, enumerate_devices,
)


def main() -> int:
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "recorded.mp4")
    print("mediaservo-deck %s" % mediaservo.deck.version())

    # 1. 枚举设备（双调用封装）→ stub:test-camera
    devs = enumerate_devices(DeviceKind.Camera)
    if not devs:
        print("no camera devices")
        return 1
    print("devices: %s" % devs)

    # 2. 采集 320x240@30（stub 彩条）
    frames = []

    def on_frame(f) -> None:
        frames.append(f)  # Frame.data 已拷贝为 bytes，可安全保留

    cam = CameraSource.open(devs[0], CaptureOptions(320, 240, 30))
    cam.on_frame(on_frame)
    cam.start()

    # 3. 录制 2 秒（关闭顺序: recorder stop/close 先于 camera stop/close）
    rec = Recorder.open(out)
    rec.record(cam)
    time.sleep(2.0)
    rec.stop()
    rec.close()
    cam.stop()
    cam.close()
    print("recorded %d frames -> %s (%d bytes)" % (len(frames), out, os.path.getsize(out)))

    # 4. 回放（on_frame 解码泵运行至 EOF；close 阻塞 join）
    played = [0]

    def on_playback(f) -> None:
        played[0] += 1

    player = Player.open(out)
    player.on_frame(on_playback)
    player.close()
    print("played back %d frames" % played[0])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
