#!/usr/bin/env python3
"""field 推流骨架 — 车端视频推流到云端（连接 → 发布视频轨 → 帧生成 → 关闭）。

运行（需信令 server + MEDIASERVO_LIB_DIR）:
  export MEDIASERVO_LIB_DIR=$PWD/target/debug
  python3 examples/vehicle_push.py ws://127.0.0.1:9800/ws mediaservo-dev room-1 10
"""

import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))  # 开发模式

import mediaservo
from mediaservo.field import PushConfig, PushSession


def main() -> int:
    if len(sys.argv) < 4:
        print("usage: vehicle_push.py <ws_url> <psk> <room> [seconds]")
        return 1
    url, psk, room = sys.argv[1], sys.argv[2], sys.argv[3]
    seconds = float(sys.argv[4]) if len(sys.argv) > 4 else 10.0

    print("mediaservo-field %s" % mediaservo.field.version())
    cfg = PushConfig(url=url, psk=psk, room=room,
                     width=1280, height=720, framerate=30,
                     bitrate_kbps=2000, keyframe_interval=2)

    # 连接信令 + 创建会话（阻塞；失败抛 FieldError）
    session = PushSession.connect(cfg)
    try:
        # 发布视频轨（阻塞协商）→ track id
        track = session.publish_video()
        print("track: %s" % track)

        # 启动视频帧生成（Squares + 时间戳水印）
        session.start_video_frames()
        print("pushing for %.1fs ..." % seconds)
        time.sleep(seconds)

        session.stop_video_frames()  # 幂等
    finally:
        session.close()  # 显式关闭（__del__ 兜底，但显式 close 优先）
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
