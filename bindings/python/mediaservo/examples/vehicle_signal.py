#!/usr/bin/env python3
"""link 信令 + 事件 — 设备侧信令会话骨架（连接 → 事件 → 发送 → 关闭）。

运行（需信令 server + MEDIASERVO_LIB_DIR）:
  export MEDIASERVO_LIB_DIR=$PWD/target/debug
  python3 examples/vehicle_signal.py ws://127.0.0.1:9800/ws mediaservo-dev room-1
"""

import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))  # 开发模式

import mediaservo
from mediaservo.link import SignalConfig, SignalSession


def on_event(event_json: str) -> None:
    # 泵线程触发；事件 JSON 已拷贝为 str，可安全保留
    print("event: %s" % event_json)


def main() -> int:
    if len(sys.argv) < 4:
        print("usage: vehicle_signal.py <ws_url> <psk> <room> [seconds]")
        return 1
    url, psk, room = sys.argv[1], sys.argv[2], sys.argv[3]
    seconds = float(sys.argv[4]) if len(sys.argv) > 4 else 5.0

    print("mediaservo-link %s" % mediaservo.link.version())
    cfg = SignalConfig(url=url, psk=psk, room=room, role="Pusher")  # 空 role = Host

    # 连接信令 + 创建会话（阻塞；失败抛 LinkError）
    session = SignalSession.connect(cfg)
    try:
        session.on_event(on_event)  # 注册事件回调（首次注册泵补发 {"type":"connected"}）
        time.sleep(0.5)
        session.send('{"type":"ping"}')
        time.sleep(seconds)
    finally:
        session.close()  # C close join 事件泵
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
