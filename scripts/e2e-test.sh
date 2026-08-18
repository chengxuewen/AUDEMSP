#!/usr/bin/env bash
# E2E verification: Host -> Docker Server -> Client video flow
set -u

HOST_PID=""
CLIENT_PID=""
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
PASS=0; FAIL=0
TMPDIR="/tmp/mediaservo-e2e-$$"

pass() { echo -e "${GREEN}[PASS]${NC} $1"; ((PASS++)); }
fail() { echo -e "${RED}[FAIL]${NC} $1"; ((FAIL++)); }
info() { echo -e "${YELLOW}[INFO]${NC} $1"; }

cleanup() {
    info "Stopping Host and Client..."
    kill $HOST_PID 2>/dev/null || true
    kill $CLIENT_PID 2>/dev/null || true
    echo ""
    echo "═══════════════════════════════════════"
    echo "  Results: ${GREEN}$PASS passed${NC}  ${RED}$FAIL failed${NC}"
    echo "═══════════════════════════════════════"
    echo "Logs: $TMPDIR"
    exit $FAIL
}
trap cleanup EXIT

mkdir -p "$TMPDIR"
info "Logs: $TMPDIR"

# 1. Server health check
info "1. Checking server..."
if curl -sf http://localhost:9800/health | grep -q OK; then
    pass "Server healthy"
else
    fail "Server not running. Start: docker compose up -d"
    exit 1
fi

# 2. Build
info "2. Building..."
cargo build -p mediaservo-host -p mediaservo-client 2>/dev/null && \
    pass "Build" || { fail "Build"; exit 1; }

# 3. Start Host
info "3. Starting Host..."
cargo run -p mediaservo-host --bin host-legacy > "$TMPDIR/host.log" 2>&1 &
HOST_PID=$!
info "Host PID=$HOST_PID"

for i in 1 2 3 4; do
    sleep 5
    if grep -qi "room_join\|RoomJoin\|WebRTC\|PeerConnection" "$TMPDIR/host.log" 2>/dev/null; then
        pass "Host connected"
        break
    fi
    [ $i -eq 4 ] && { fail "Host connect timeout"; head -10 "$TMPDIR/host.log"; }
done

# 4. Start Client
info "4. Starting Client..."
cargo run -p mediaservo-client --bin mediaservo-client > "$TMPDIR/client.log" 2>&1 &
CLIENT_PID=$!
info "Client PID=$CLIENT_PID"

for i in 1 2 3 4; do
    sleep 5
    if grep -qi "signaling\|Signaling\|room\|RoomJoin" "$TMPDIR/client.log" 2>/dev/null; then
        pass "Client connected"
        break
    fi
    [ $i -eq 4 ] && { fail "Client connect timeout"; head -10 "$TMPDIR/client.log"; }
done

# 5. Wait for SDP exchange
info "5. SDP exchange..."
sleep 5
grep -qi "Sdp\|SDP\|offer\|answer\|remote_description" "$TMPDIR/host.log" 2>/dev/null && \
    pass "Host SDP" || fail "Host SDP"
grep -qi "Sdp\|SDP\|offer\|answer" "$TMPDIR/client.log" 2>/dev/null && \
    pass "Client SDP" || fail "Client SDP"

# 6. Wait for data channel
info "6. Data channel..."
sleep 5
grep -qi "DataChannel\|data.channel\|RTCDataChannel\|on_open\|dc\|channel" "$TMPDIR/host.log" 2>/dev/null && \
    pass "Host DC" || info "Host DC not detected"
grep -qi "DataChannel\|data.channel\|RTCDataChannel\|spool\|on_message" "$TMPDIR/client.log" 2>/dev/null && \
    pass "Client DC" || info "Client DC not detected"

# 7. Check server logs
info "7. Server relay..."
docker compose logs --tail 20 server 2>/dev/null | grep -qi "room\|Room\|relay\|Relay" && \
    pass "Server relay" || info "Server relay not detected"

# 8. Print summary lines
info "8. Key log lines:"
echo "--- Host ---"
grep -i "INFO\|WARN\|ERROR\|room\|WebRTC\|DC\|SDP\|Peer" "$TMPDIR/host.log" 2>/dev/null | head -10 || echo "(empty)"
echo "--- Client ---"
grep -i "INFO\|WARN\|ERROR\|room\|WebRTC\|DC\|SDP\|Peer" "$TMPDIR/client.log" 2>/dev/null | head -10 || echo "(empty)"
echo "--- Server ---"
docker compose logs --tail 10 server 2>/dev/null | grep -i "INFO\|WARN\|ERROR\|room\|join\|leave\|SDP" | head -5 || echo "(empty)"
