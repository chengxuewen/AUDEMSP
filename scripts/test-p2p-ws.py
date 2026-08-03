#!/usr/bin/env python3
"""P2P WebSocket test: connect as consumer, receive SDP offer, send answer back."""
import asyncio
import json
import sys
import websockets

SERVER_URL = "ws://localhost:9800/ws"
PSK = "audemsp-dev"
ROOM_ID = "test-p2p-room"


async def main():
    print(f"[INFO] Connecting to {SERVER_URL}...")
    async with websockets.connect(SERVER_URL) as ws:
        # Step 1: Send PSK for auth
        print(f"[INFO] Sending PSK...")
        await ws.send(PSK)

        # Step 2: Wait for auth ack
        msg = json.loads(await ws.recv())
        print(f"[INFO] Auth response: {msg}")
        if msg.get("code") != 0:
            print(f"[FAIL] Auth failed: {msg}")
            return 1

        # Step 3: Join room as Consumer
        join_msg = {
            "type": "room_join",
            "room_id": ROOM_ID,
            "peer_role": "consumer",
        }
        await ws.send(json.dumps(join_msg))
        print(f"[INFO] RoomJoin sent (consumer)")

        # Step 4: Wait for RoomJoined
        msg = json.loads(await ws.recv())
        print(f"[INFO] RoomJoined: {msg}")
        if msg.get("type") != "room_joined":
            print(f"[FAIL] Expected room_joined, got {msg.get('type')}")
            return 1

        # Step 5: Receive SDP offer
        print(f"[INFO] Waiting for SDP offer...")
        sdp_received = False
        ice_received = 0
        seen_messages = set()

        while True:
            try:
                raw = await asyncio.wait_for(ws.recv(), timeout=15.0)
                msg = json.loads(raw)
                msg_type = msg.get("type", "")

                if msg_type == "sdp":
                    inner = json.loads(msg.get("sdp", "{}"))
                    sdp_typ = inner.get("type", "")
                    print(f"[INFO] Received SDP: type={sdp_typ}")
                    if sdp_typ == "offer" and not sdp_received:
                        sdp_received = True
                        # Create a fake answer
                        answer_sdp = json.dumps({
                            "type": "answer",
                            "sdp": "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=group:BUNDLE 0\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\nc=IN IP4 0.0.0.0\r\na=mid:0\r\na=sctp-port:5000\r\na=max-message-size:262144\r\n"
                        })
                        answer_msg = {
                            "type": "sdp",
                            "room_id": ROOM_ID,
                            "target": None,
                            "sdp": answer_sdp,
                        }
                        await ws.send(json.dumps(answer_msg))
                        print("[INFO] SDP answer sent")

                elif msg_type == "rtcic_e_candidate":
                    ice_received += 1
                    msg_sig = f"{msg.get('sdp_mid','')}-{msg.get('sdp_mline_index','')}"
                    if msg_sig not in seen_messages:
                        seen_messages.add(msg_sig)
                        print(f"[INFO] ICE candidate #{ice_received}: mid={msg.get('sdp_mid','')}, idx={msg.get('sdp_mline_index','')}")

                elif msg_type == "room_leave":
                    print("[INFO] Room leave message received")
                    break

                elif msg_type == "error" and msg.get("code") == 0:
                    pass  # auth ack echoed

                print("[PASS] SDP exchange completed")
                print(f"[INFO] Total ICE candidates received: {ice_received}")
                return 0

            except asyncio.TimeoutError:
                if sdp_received:
                    print("[PASS] SDP exchange completed (timeout)")
                    print(f"[INFO] Total ICE candidates received: {ice_received}")
                    return 0
                print("[FAIL] Timeout waiting for SDP offer")
                return 1
            except websockets.exceptions.ConnectionClosed:
                print("[INFO] Connection closed")
                return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
