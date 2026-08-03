# Troubleshooting Guide

Common issues encountered during AUDEMSP development and deployment, with diagnostics and fixes.

## Docker Networking Debug

### Symptom
Containers can't reach each other or host services.

### Check
- Container network: `docker network ls`
- Container IP: `docker inspect <container> | grep -A5 "NetworkSettings"`
- DNS resolution inside container: `docker exec <container> nslookup <hostname>`
- Bridge vs host network mode

### Common Causes
1. **Port conflict** — Container port already in use. Run `lsof -i :<port>` on host.
2. **Docker bridge not reachable** — Host firewall blocking `docker0`. Add `iptables -P FORWARD ACCEPT` if needed.
3. **Multiple Docker bridges** — Custom networks isolate containers. Use `docker network connect <network> <container>` to link.
4. **macOS networking** — Docker Desktop on macOS uses a VM backend; `localhost` in container ≠ host `localhost`. Use `host.docker.internal` instead.

### Quick Test
```bash
docker run --rm --network host alpine wget -qO- http://localhost:<port>/health
```

---

## Cargo Cache Issues

### Symptom
`cargo build` fails with "unrecognized toolchain" or dependency resolution errors after OS upgrades.

### Fix
```bash
# Clear all cargo cache (nuclear option)
cargo clean && rm -rf ~/.cargo/registry ~/.rustup/toolchains

# Or selectively clear cache
cargo clean
rm -rf ~/.cargo/registry/cache/
```

### When to Rebuild
- After `rustup update` (MSRV bumps)
- After package index corruption (`git reset --hard HEAD` in `~/.cargo/registry`)
- After cross-compiling to a new target

---

## Port Conflicts

### Symptom
`Address already in use` errors on startup.

### AUDEMSP Ports
| Service | Default Port |
|---------|-------------|
| audemsp-server | 9800 |
| audemsp-host | 9801 |
| audemsp-client | 9101 |

### Find and Kill
```bash
# Find what's using a port
lsof -ti :<port>

# Kill it
kill -9 $(lsof -ti :<port>)

# Or on macOS/Linux
ss -tlnp | grep <port>
```

### Docker Port Mappings
Check `docker-compose.yml` for conflicting `ports:` entries. Use `docker ps --format "table {{.Names}}\t{{.Ports}}"` to verify.

---

## STUN/ICE Troubleshooting

### Symptom
WebRTC peers can't establish a connection despite successful SDP exchange.

### Debug Levels
Enable verbose ICE logging:
```bash
RUST_LOG=webrtc=debug,ice=trace ./target/debug/audemsp-server
```

### Common Causes

1. **STUN server unreachable**
   - Test: `curl -v stun:stun.l.google.com:19302` or use `stunclient` CLI tool
   - Verify public IP returned matches your actual IP

2. **NAT/ Firewall blocking UDP**
   - Check outbound UDP on 3478 (STUN default) and ICE candidates' ports
   - Try symmetric NAT mode: increase `iceServers` list with multiple STUN servers

3. **ICE candidate exchange timing**
   - ICE candidates must be exchanged via signaling channel BEFORE setting remote description
   - Ensure `setRemoteDescription` happens AFTER all ICE candidates are added

4. **Localhost testing**
   - Use `stun:stun.l.google.com:19302` as STUN server
   - Verify both peers are on the same local network or have proper NAT traversal

### WebRTC Debug Checklist
- [ ] Both peers successfully connected to signaling server (check WebSocket logs)
- [ ] SDP offer/answer exchanged without errors
- [ ] ICE candidates logged with types (host/candidate/relay)
- [ ] DataChannel open event fires on both sides
- [ ] No NAT loopback issues (test from different networks)

---

## General Debugging Tips

### Enable Verbose Logging
```bash
# All crates at debug level
RUST_LOG=debug cargo run

# Specific module
RUST_LOG=audemsp-webrtc=debug,audemsp-server=trace cargo run

# JSON format for structured analysis
RUST_LOG_FORMAT=json
```

### Core Dump Analysis (Linux)
```bash
# Enable core dumps
ulimit -c unlimited
echo '/tmp/core.%e.%p' | sudo tee /proc/sys/kernel/core_pattern

# Analyze after crash
gdb ./target/debug/audemsp-server /tmp/core.audemsp-server.1234
(gdb) bt
```

### Memory Profiling
```bash
# Valgrind for memory leaks
valgrind --leak-check=full ./target/debug/audemsp-server

# heaptrack for Rust (Linux)
cargo install heaptrack
heaptrack ./target/debug/audemsp-server
```

---

## Quick Reference: System Checks

```bash
# Network connectivity
curl -v ws://localhost:9800/ws

# Port availability
netstat -tlnp | grep -E '9800|9801|9101'

# Running processes
ps aux | grep audemsp

# Disk space (cargo cache grows large)
du -sh ~/.cargo/registry/

# Rust toolchain version
rustc --version

# Check for stale locks
rm -rf /tmp/.cargo-lock
```
