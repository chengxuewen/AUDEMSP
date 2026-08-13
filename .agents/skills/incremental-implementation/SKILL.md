---
name: incremental-implementation
description: "Thin vertical slices across Rust+TS. Implement→test→verify→commit per slice. Use when implementing multi-file Rust crate changes, Admin Dashboard TS features, or any change touching 2+ crates. Triggers: 'incremental', 'slice by slice', 'one crate at a time', multi-crate refactor."
---

# Incremental Implementation — MediaServo

## Overview

Build in thin vertical slices — one crate, one module, one function at a time. Implement → test → verify → commit. Each slice leaves the workspace compilable and tests green. This is how 7-crate workspaces stay manageable.

## When to Use

- Multi-crate changes (e.g., new protocol message in common → server handler → client UI)
- Admin Dashboard TS feature (component → API route → server handler)
- SFU/WebRTC pipeline changes (codec → webrtc → server → browser test)
- Any change touching `Cargo.toml` deps or features

**When NOT to use:** Single-function bugfix in one crate where `cargo check -p <crate>` is the only verification needed.

## The MediaServo Slice Cycle

```
┌──────────────────────────────────────────────────┐
│   Implement → cargo check → cargo test → Commit  │
│       ▲                                        │
│       └────────── Next slice ──────────────────┘ │
└──────────────────────────────────────────────────┘
```

For Rust code, each slice's verification gates:

| Slice Type | Verify Commands |
|------------|----------------|
| Single crate lib change | `cargo check -p <crate> && cargo test -p <crate>` |
| Multi-crate change | `cargo check --workspace && cargo test --workspace` |
| Feature-gated change | `cargo check -p <crate> --features <feat> && cargo test -p <crate> --features <feat>` |
| TS Admin Dashboard | `npx tsc --noEmit` (from admin dir) + browser verify |
| SFU/mediasoup | `pixi run check` (Linux) or `cargo check -p mediaservo-server --features sfu-mediasoup` |

### Rust Crate Slice Example

```
Slice 1: Add protocol enum variant to mediaservo-common
  → cargo check -p mediaservo-common && cargo test -p mediaservo-common
  → 68 tests pass ✓ → commit

Slice 2: Handle new variant in mediaservo-server
  → cargo check -p mediaservo-server && cargo test -p mediaservo-server
  → 32 tests pass ✓ → commit

Slice 3: Wire up in Admin Dashboard TS
  → npx tsc --noEmit (types check) + browser verify
  → commit

Slice 4: Integration test (WS relay)
  → cargo test -p mediaservo-server --test '*' 
  → 25 e2e + 5 integration pass ✓ → commit
```

### SFU/WebRTC Vertical Slice

```
Slice 1: Define SFU message type in mediaservo-common (protocol.rs)
  → cargo test -p mediaservo-common ✓

Slice 2: Add handler in mediaservo-server (admin/sfu_handler.rs)
  → cargo test -p mediaservo-server ✓

Slice 3: Implement browser-side SFU client (admin/src/sfu-client.ts)
  → npx tsc --noEmit + browser console verify ✓

Slice 4: Docker integration test
  → docker compose up -d && pixi run test-sfu ✓
```

### Risk-First Slicing (MediaServo)

For SFU/media pipeline work, prove the riskiest piece first:

```
Slice 1: mediasoup transport connect (highest risk — PIT-07)
  → Verify DTLS/ICE handshake completes ✓

Slice 2: Producer → Consumer video relay
  → Verify video frames reach browser ✓

Slice 3: Admin Dashboard video grid
  → Verify multi-peer layout renders ✓
```

## MediaServo-Specific Rules

### Rule 0: Feature Flag Awareness

MediaServo has many feature flags. Each slice must specify which features it touches:

```
Always document the feature set:
  cargo check -p mediaservo-webrtc --features backend-webrtc-rs
  cargo check -p mediaservo-server --features sfu-mediasoup

A slice that accidentally breaks a feature behind a flag is a broken slice.
```

### Rule 1: Crate Boundary Discipline

Don't cross crate boundaries in one slice without verification:

```rust
// BAD: One slice that adds a type to common AND uses it in server
// GOOD: Slice 1: add to common (test it). Slice 2: use in server (test it).
```

### Rule 2: Keep Workspace Compilable

`cargo check --workspace` must pass after every slice. If a slice breaks another crate, it's too big.

### Rule 3: macOS vs Linux Awareness

Some slices only verify on Linux (mediasoup). Document expected platform:

```
Slice: SFU transport connect
Platform: Linux only (cargo check OK on macOS)
Verify: pixi run check (Linux Docker)
```

### Rule 4: Commit Atomicity

Each commit should be independently revertable:
- `feat(common): add SfuTransportStatus enum`
- `feat(server): handle ConnectWebRtcTransport`
- `feat(admin): wire SFU transport status in dashboard`

## Verification Checklist (MediaServo)

After each slice, verify with these commands (run only what changed):

- [ ] `cargo check -p <changed-crate>` passes
- [ ] `cargo test -p <changed-crate>` passes (all tests green)
- [ ] `cargo clippy -p <changed-crate> -- -D warnings` clean
- [ ] `cargo fmt --check` (only on changed files)
- [ ] Feature-gated crates: check with all relevant `--features` combos
- [ ] TS changes: `npx tsc --noEmit` passes
- [ ] Browser changes: Playwright verify (console clean, no errors)
- [ ] Docker changes: `docker compose up -d` succeeds
- [ ] Commit with conventional commit message

## Slice Size Limits

| Metric | Max | Red Flag |
|--------|-----|----------|
| Lines per slice | 150 | > 200 = split |
| Crates touched | 2 | > 2 = too wide |
| Files changed | 5 | > 5 = slice deeper |
| Time to verify | 60s | > 2 min = too much |

## Common MediaServo Rationalizations

| Rationalization | Reality |
|---|---|
| "I'll test all 7 crates at the end" | A type change in common cascades. Test each crate that changed. |
| "`cargo check` is enough, skip tests" | `check` catches types, not logic. Run tests on the changed crate. |
| "It's a small TS change, skip browser verify" | TS compiles ≠ DOM renders. Playwright verification catches runtime. |
| "mediasoup only builds on Linux, I'll test later" | At minimum `cargo check --features sfu-mediasoup` on macOS. |
| "I'll add the feature flag later" | If Slice 1 breaks `sfu-mediasoup` feature, Slices 2-5 are built on sand. |

## Red Flags (MediaServo)

- > 150 lines in one commit
- Workspace `cargo check` broken between slices
- Unverified TS changes ("looks right in code")
- SFU/media changes not tested with actual mediasoup transport
- Feature flag combinations not checked
- macOS-only testing for Linux-only features

## See Also

- `.agents/memorys/pitfalls.md` — PIT-07 (SFU connect), PIT-11 (mediasoup build)
- `.agents/memorys/conventions.md` — C5 (GStreamer ↔ WebRTC boundary), C6 (naming)
- `.agents/rules/rust/coding-style.md` — Rust conventions
- `docs/modules/development/docker-workflow.md` — Docker dev workflow
