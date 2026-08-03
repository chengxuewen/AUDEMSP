# mediasoup SFU Verification Guide

> Phase 2 SFU | Last updated: 2026-07-24

## macOS (compile-only)

```bash
# All tests pass without SFU
cargo test -p audemsp-server --tests  # 42 pass

# SFU feature compiles but no runtime (mediasoup C++ Worker unavailable)
cargo check -p audemsp-server --features sfu-mediasoup  # clean
```

## Linux (full verification)

### Native

```bash
# Install mediasoup dependencies
sudo apt-get install -y pkg-config cmake ninja-build libssl-dev libuv1-dev python3 python3-pip
pip3 install meson

# Full test suite (4 new SFU E2E tests)
cargo test -p audemsp-server --features sfu-mediasoup  # expect 44+ pass

# All workspace tests
cargo test --workspace --features sfu-mediasoup
```

### Docker

```bash
# Build dev image
docker compose build server

# Run SFU tests
docker compose run --rm server cargo test -p audemsp-server --features sfu-mediasoup

# Run full workspace tests
docker compose run --rm server cargo test --workspace --features sfu-mediasoup
```

## Verification Checklist

- [ ] `cargo check -p audemsp-server --features sfu-mediasoup` — clean
- [ ] `cargo test -p audemsp-server --tests --features sfu-mediasoup` — all pass (44+)
- [ ] `e2e_sfu_lifecycle` — create room, transports, produce, consume, cleanup
- [ ] `e2e_sfu_cleanup_on_disconnect` — WS close triggers SFU room destruction
- [ ] No mediasoup-specific warnings in clippy
- [ ] CI `test-mediasoup` job green

## Known Limitations

- DTLS fingerprint conversion (serde gap in mediasoup)
- Single Worker only (Phase 3+: N Workers + PipeTransport)
- No simulcast/SVC support yet
