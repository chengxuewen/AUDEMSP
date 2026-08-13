---
name: source-driven-development
description: "Ground every external dependency decision in official docs. Use when integrating mediasoup, webrtc-rs, React, Docker, GStreamer, FFmpeg, or any crate/framework. Verifies Cargo.toml deps against upstream docs. Triggers: 'check the docs', 'is this API correct', 'what does the spec say', 'verify against upstream', crate upgrade, new dependency."
---

# Source-Driven Development — MediaServo

## Overview

Every external dependency decision must be backed by official documentation. MediaServo integrates C/C++ FFI (mediasoup, libwebrtc), three WebRTC backends, two codec backends, and Docker tooling. Training data goes stale — documentation doesn't lie. Verify every API call, every feature flag, every build step against upstream docs.

## When to Use

- Adding or upgrading a crate dependency (check changelog, migration guide, MSRV)
- Implementing mediasoup API calls (Worker, Router, Transport, Producer, Consumer)
- Using webrtc-rs or webrtc-sys APIs (SDP, ICE, DataChannel)
- Writing Docker/CI configs (Dockerfile, docker-compose, GitHub Actions)
- Integrating GStreamer pipelines, FFmpeg codec flags
- Building Admin Dashboard with React/TS patterns
- Any time you're about to write framework-specific code from memory

**When NOT to use:**
- Pure Rust logic (ownership, iterators, error handling) — stdlib patterns don't change
- Renaming variables, fixing typos, moving files
- Tests that exercise project-internal APIs (no external dep involved)

## The Process

```
DETECT ──→ FETCH ──→ IMPLEMENT ──→ CITE
  │          │           │            │
  ▼          ▼           ▼            ▼
 Cargo.toml  Upstream    Follow the   Show your
 version     docs        documented   sources
                         patterns
```

### Step 1: Detect Dependency Versions

From `Cargo.toml`:

```toml
[dependencies]
mediasoup-sys = "0.13"
webrtc = { version = "0.14", optional = true }
gstreamer = "0.24"
```

State what you found:

```
DEPENDENCIES DETECTED:
- mediasoup-sys 0.13 (from crates/mediaservo-server/Cargo.toml)
- webrtc 0.14 (from crates/mediaservo-webrtc/Cargo.toml)
- gstreamer 0.24 (from crates/mediaservo-codec/Cargo.toml)
→ Fetching upstream docs for the relevant APIs.
```

For Docker/CI: check Dockerfile base image (`ubuntu:22.04`), docker-compose service configs, GitHub Actions `runs-on`.

For TS: check `package.json` for React, Playwright, and other TS deps.

### Step 2: Fetch Official Documentation

**Source hierarchy (in order of authority):**

| Priority | Source | Example |
|----------|--------|---------|
| 1 | Upstream crate docs / API reference | docs.rs/mediasoup-sys, docs.rs/webrtc |
| 2 | Upstream repo README + CHANGELOG | github.com/versatica/mediasoup/CHANGELOG.md |
| 3 | Official guides / migration docs | webrtc.rs/book, gstreamer.freedesktop.org |
| 4 | Language/framework official docs | rust-lang.org, react.dev, docs.docker.com |
| 5 | Web standards (for WebRTC, media) | w3.org/TR/webrtc, datatracker.ietf.org |

### Using Context7 MCP for Documentation

Context7 provides a fast MCP-based query interface to official library documentation. Use it as a caching layer over the sources in tiers 1-4 above.

**Context7 is already configured** in `.opencode/opencode.json` and available via two tools:

**Step 2a: Resolve library name to ID**
```
# Tool: context7_resolve-library-id
# Search for a library; select the best match by snippet count + source reputation + benchmark score.
#
# Example — find the mediasoup docs:
context7_resolve-library-id(libraryName: "mediasoup", query: "WebRtcTransport connect")
# → Returns libraryId: "/versatica/mediasoup"
```

**Step 2b: Query the library's documentation**
```
# Tool: context7_query-docs
# Pass the exact libraryId from step 2a. Be specific — one concept per query.
#
# Example — query mediasoup transport connect API:
context7_query-docs(libraryId: "/versatica/mediasoup", query: "WebRtcTransport connect dtlsParameters")
# → Returns code snippets ranked by relevance + benchmark score
```

**When to use Context7 vs. direct docs:**

| Scenario | Use |
|----------|-----|
| Quick API signature lookup | Context7 (faster, code-snippet ranked) |
| Full method docs with examples | Context7 first, fall back to docs.rs if incomplete |
| Version-specific migration guides | Direct upstream repo + Context7 |
| First-time crate exploration | Context7 (gets you oriented fast) |
| API that changed recently (last 6 months) | Context7 (indexed from live docs, not training data) |

**Workflow:**
```
1. context7_resolve-library-id → get libraryId
2. context7_query-docs(libraryId, "specific API question") → code examples
3. If answer is incomplete, fall back to docs.rs / upstream repo (tiers 1-2)
4. Cite: "Source: Context7 /{org}/{project} + docs.rs/{crate}/{version}"
```

**Not authoritative:**
- Stack Overflow, blog posts, tutorials
- AI training data (that's what we're verifying)
- Random GitHub issues without upstream confirmation

### Step 3: Implement Following Documented Patterns

For Rust crate APIs:

```rust
// mediasoup-sys 0.13: Transport::connect() signature
// Source: https://docs.rs/mediasoup-sys/0.13/mediasoup/transport/struct.Transport.html#method.connect
transport.connect(&dtls_parameters)
    .map_err(|e| SfuError::ConnectFailed(e.to_string()))?;

// webrtc 0.14: RTCPeerConnection::create_offer
// Source: https://docs.rs/webrtc/0.14/webrtc/api/struct.RTCPeerConnection.html#method.create_offer
let offer = pc.create_offer().await?;
```

For mediasoup C++ Worker API (via mediasoup-sys bindings):

```
VERIFICATION: mediasoup Worker::CreateWebRtcTransport
Source: https://mediasoup.org/documentation/v3/mediasoup/api/#WebRtcTransportOptions
Required fields: listenIps, enableUdp, enableTcp
Optional: initialAvailableOutgoingBitrate, maxIncomingBitrate
→ Confirmed against mediasoup-sys 0.13 binding signatures
```

### Step 4: Cite Your Sources

Every non-trivial external dependency usage gets a citation:

```rust
// PIT-07: Transport connect must call actual mediasoup API
// Source: https://docs.rs/mediasoup-sys/0.13/mediasoup/transport/struct.Transport.html#method.connect
// Decision: D198 (SFU Server-Offer architecture)
transport.connect(&dtls_params)?;
```

In conversation:

```
Using mediasoup Transport::connect() from mediasoup-sys 0.13.
Source: https://docs.rs/mediasoup-sys/0.13/mediasoup/transport/struct.Transport.html
This replaces the previous stub that only logged and returned "transport_connected"
(see PIT-07 in .agents/memorys/pitfalls.md).
```

## MediaServo-Specific Verification

### Rust Crate Dependencies

Before adding/changing a dep:

```bash
# 1. Check if the crate is already in the workspace
grep -r "crate_name" Cargo.toml crates/*/Cargo.toml

# 2. Verify version compatibility
cargo tree -p mediaservo-server -i mediasoup-sys

# 3. Check MSRV against rust-toolchain.toml
cat rust-toolchain.toml

# 4. Verify license (cargo-deny)
cargo deny check
```

### Feature Flag Dependencies

MediaServo feature flags are interdependent. Verify:

```bash
# Each flag combination must cargo check
cargo check -p mediaservo-webrtc --no-default-features --features backend-stub
cargo check -p mediaservo-webrtc --features backend-webrtc-rs
cargo check -p mediaservo-webrtc --features backend-webrtc-sys
cargo check -p mediaservo-codec --features backend-ffmpeg
cargo check -p mediaservo-codec --features backend-gstreamer
cargo check -p mediaservo-server --features sfu-mediasoup
```

### mediasoup Version Constraints

mediasoup-sys 0.13 binds to mediasoup C++ Worker v3. Key constraints:
- Linux x86_64 only (no macOS ARM64 build)
- Requires meson + ninja + libuv
- Python 3.9+ for worker build scripts
- Ubuntu 22.04 LTS recommended base

Source: `mediasoup-sys` README + PIT-11 (meson buildtype conflict)

### Docker/CI Verification

```bash
# Dockerfile base image should match CI
grep "FROM" docker/Dockerfile
grep "runs-on" .github/workflows/ci.yml

# Both should be ubuntu-22.04 / ubuntu:22.04
```

## Common MediaServo Conflicts

### Conflict: Crate version != binding version

```
CONFLICT: Cargo.toml has mediasoup-sys = "0.13" but the C++ Worker seems to expect
API from v3.14.x (based on CHANGELOG.md).
→ Check mediasoup-sys CHANGELOG for which Worker version 0.13 targets.
```

### Conflict: Feature flag mutual exclusion

```
CONFLICT: Both backend-webrtc-rs and backend-webrtc-sys are enabled.
Source: crates/mediaservo-webrtc/Cargo.toml + PIT-04
→ compile_error! is expected. Only one backend per build.
```

### Conflict: Platform constraint

```
CONFLICT: This mediasoup API call needs to build. But mediasoup only builds on Linux.
Options:
A) cargo check --features sfu-mediasoup (type-check on macOS, no build)
B) Build in Docker (pixi run check)
C) Test on Linux CI
→ Choose based on what we're verifying.
```

## Verification Checklist

- [ ] Dependency version identified from Cargo.toml / package.json
- [ ] Official docs fetched for any new/modified external API usage
- [ ] API signatures match the detected version (not training data)
- [ ] Feature flag combinations all `cargo check` clean
- [ ] Platform constraints documented (Linux-only, macOS-only)
- [ ] Non-trivial decisions cite upstream docs + MediaServo decision record
- [ ] Deprecated APIs not used (checked migration guides)
- [ ] Conflicts between docs and existing code surfaced
- [ ] Anything unverified is explicitly flagged

## Red Flags (MediaServo)

- Writing `mediasoup::Transport::connect()` without checking the 0.13 API signature
- Using webrtc-rs APIs from memory (v0.11 vs v0.14 differ significantly)
- Adding a Cargo.toml dep without checking if it's already in the workspace
- Enabling two mutually exclusive features (see PIT-04)
- Docker base image mismatch with CI (ubuntu:22.04 vs ubuntu-latest)
- Not checking `cargo deny` after adding a new dependency

## See Also

- `.agents/memorys/pitfalls.md` — PIT-04 (mutual exclusion), PIT-11 (mediasoup build)
- `.agents/memorys/conventions.md` — C5 (GStreamer boundary), C6 (webrtc naming)
- `.agents/memorys/decisions.md` — D198 (SFU Server-Offer), D155 (GStreamer interface)
- `.agents/rules/common/constraints.md` — Platform constraints, Docker constraints
- `crates/mediaservo-webrtc/Cargo.toml` — Feature flag matrix
