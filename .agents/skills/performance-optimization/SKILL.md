---
name: performance-optimization
description: "OMSPBase performance profiling and optimization. WebRTC latency tracing, mediasoup SFU throughput, Admin UI React render profiling, cargo bench regression detection. Use when latency spikes, after media pipeline changes, or before release."
---

# performance-optimization — 性能优化

> WebRTC 延迟 + mediasoup 吞吐 + React 渲染 + cargo bench.
> 先测量，再优化。不优化猜测。

## 触发条件

- 延迟 >100ms (OMSPBase 目标: <100ms E2E)
- 吞吐下降 >10%
- Admin UI 渲染掉帧
- 媒体管线代码变更后
- 用户说 "performance" / "latency" / "slow" / "优化性能"
- 发布前基准回归

## 黄金法则

```
1. 测量基线 (cargo bench / Playwright trace)
2. 定位瓶颈 (flamegraph / perf / Chrome DevTools)
3. 单变量优化 (一次只改一个变量)
4. 验证回归 (重新测量，对比基线)
5. 记录决策 (decisions.md)
```

## Phase 1: Rust 层 — 基准测试

### cargo bench

```bash
# 运行所有基准
cargo bench --workspace

# 特定 crate
cargo bench -p omspbase-codec
cargo bench -p omspbase-webrtc

# 对比 pre/post-change
cargo bench -- --save-baseline before
# ... make changes ...
cargo bench -- --baseline before
```

### 关键基线设置

```rust
// crates/omspbase-media/benches/pipeline_bench.rs
use criterion::{black_box, Criterion};

fn bench_encode_1080p(c: &mut Criterion) {
    c.bench_function("encode_h264_1080p30", |b| {
        let frame = generate_test_frame(1920, 1080);
        let encoder = Encoder::new(CodecConfig::h264());
        b.iter(|| encoder.encode(black_box(&frame)))
    });
}
```

### 性能剖析

```bash
# flamegraph (Linux, 需要 perf)
cargo flamegraph --bin omspbase-host -- --capture test-video

# valgrind / cachegrind
valgrind --tool=cachegrind cargo run -p omspbase-host --release

# 代码级计时 (项目中已有 metrics 模块)
# crates/omspbase-common/src/metrics.rs
use std::time::Instant;
let start = Instant::now();
// ... operation ...
metrics::record("encode_latency_us", start.elapsed().as_micros());
```

## Phase 2: WebRTC 延迟分析

### 测量点

```
[Host 采集] ──t1──> [编码] ──t2──> [RTP打包] ──t3──> [网络发送]
                                                             │
[Client 渲染] <──t6── [解码] <──t5── [RTP解包] <──t4── [网络接收]
```

```bash
# 检查现有的 metrics 打点
grep -rn 'metrics::record\|latency\|Instant::now' crates/omspbase-webrtc/src/ --include='*.rs'
grep -rn 'metrics::record\|latency\|Instant::now' crates/omspbase-media/src/ --include='*.rs'
```

### DataChannel 延迟

```rust
// DataChannel echo 测试
// ponytail: 使用已有的 E2E 测试脚本
// scripts/e2e/macos-e2e.sh 已验证 DataChannel relay
// 延迟: 574 bytes, <1ms localhost
```

```bash
# 运行 E2E DataChannel 延迟测试
bash scripts/e2e/macos-e2e.sh  # macOS
pixi run test-sfu               # Linux/Docker SFU
```

### 优化模式

```rust
// BEFORE: 每个 RTP 包 alloc
fn send_frame(&mut self, frame: &Frame) {
    let rtp_packet = self.packetizer.packetize(frame); // alloc
    self.transport.send(rtp_packet);
}

// AFTER: 预分配 buffer pool
// chesterton: buffer pool 是 PIT-01/PIT-03 分析后的必要权衡
fn send_frame(&mut self, frame: &Frame) {
    let buf = self.buffer_pool.acquire(frame.len());
    self.packetizer.packetize_into(frame, &mut buf);
    self.transport.send(&buf);
}
```

## Phase 3: mediasoup SFU 吞吐

### 基线命令

```bash
# Docker 环境
docker compose exec server cargo bench -p omspbase-server -- --bench sfu_throughput

# 或使用 mediasoup 内置 stats
# 检查 worker resource usage
grep -rn 'rtp_listener\|max_income_bitrate\|producer.*stats' crates/omspbase-server/src/sfu/ --include='*.rs'
```

### mediasoup 调优参数

```javascript
// mediasoup WebRtcTransport 配置参考
{
  initialAvailableOutgoingBitrate: 1_000_000,  // 1Mbps
  maxIncomingBitrate: 1_500_000,               // 1.5Mbps
  // 降低以获得更低延迟:
  // initialAvailableOutgoingBitrate: 300_000,  // 300kbps
}
```

```bash
# 检查当前 OMSPBase transport 配置
grep -rn 'availableOutgoingBitrate\|maxIncomingBitrate\|initial' crates/omspbase-server/src/sfu/ --include='*.rs'
```

### SFU 健康监控

```bash
# 运行时指标 (通过 Admin WebSocket)
# GET /api/admin/sfu/stats
curl -s http://localhost:9800/api/admin/sfu/stats | jq '.rooms[].peers[].transports[]'

# 关键指标:
# - bytesReceived / bytesSent (带宽使用)
# - producerScore (编码器质量, 0-10)
# - packetLoss (丢包率, target <0.1%)
# - roundTripTime (RTT, target <50ms)
```

## Phase 4: Admin UI — React 渲染优化

### Playwright 性能追踪

```bash
# 启动 Admin UI + Playwright trace
# 使用 local-playwright MCP 工具:
# 1. browser_navigate → Admin UI
# 2. browser_evaluate → performance.mark('start')
# 3. 交互操作
# 4. browser_evaluate → performance.measure('render', 'start')

# 代码级检查
grep -rn 'useState\|useEffect\|useMemo\|useCallback\|React.memo' crates/omspbase-server/src/admin/ --include='*.tsx' --include='*.ts'
```

### React 优化清单

| 问题 | 检测 | 修复 |
|------|------|------|
| 递归重渲染 | React DevTools Profiler → 火焰图 | `React.memo` + `useCallback` |
| 昂贵计算 in render | `console.time` 包裹 render 体 | `useMemo` |
| 大列表无虚拟化 | Items >100 且未使用 `react-window` | `FixedSizeList` |
| WebSocket 消息轰炸 | 每秒 >60 条更新 | 批量更新 (requestAnimationFrame throttle) |
| 未清理的订阅 | 无 useEffect cleanup | return `() => ws.close()` |
| 不必要 Context 扩散 | Context value 包含频繁变化对象 | 拆分 Context / 用 ref |

### 检查命令

```bash
# 检查 React 项目中重组件
grep -rn 'export.*function.*Component\|export.*default function' crates/omspbase-server/src/admin/ --include='*.tsx' | wc -l

# 检查缺少 memoization
grep -rn 'useState\|useEffect' crates/omspbase-server/src/admin/ --include='*.tsx' | wc -l
grep -rn 'useMemo\|useCallback\|memo' crates/omspbase-server/src/admin/ --include='*.tsx' | wc -l
# ponytail: ratio useMemo+useCallback+React.memo / useState+useEffect 应 >0.5
```

### Playwright 验证脚本

```javascript
// 在 Playwright MCP 中执行:
// 1. browser_navigate → http://localhost:9800/admin
// 2. browser_evaluate:
() => {
  const observer = new PerformanceObserver((list) => {
    for (const entry of list.getEntries()) {
      if (entry.duration > 16) { // >1 frame (60fps)
        console.warn('Long task:', entry.duration.toFixed(1) + 'ms', entry.name);
      }
    }
  });
  observer.observe({ entryTypes: ['measure', 'longtask'] });
  performance.mark('monitoring-start');
}
// 3. 操作 UI
// 4. browser_evaluate: () => performance.getEntriesByType('measure')
```

## Phase 5: CI 回归检测

### 添加 CI 性能门禁

```yaml
# .github/workflows/ci.yml 添加:
perf-regression:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - run: cargo bench --workspace -- --output-format bencher | tee bench-output.txt
    - uses: benchmark-action/github-action-benchmark@v1
      with:
        tool: 'cargo'
        output-file-path: bench-output.txt
        github-token: ${{ secrets.GITHUB_TOKEN }}
        auto-push: true
        alert-threshold: '130%'  # >30% 退化则告警
```

### 本地 pre-bench 检查

```bash
# 快速检查是否有明显退化
cargo check --workspace --all-features  # 编译检查
cargo clippy --workspace --all-features -- -D warnings  # 不引入不必要的 alloc/clone
cargo bench --workspace -- --quick  # 快速基准 (采样不足，仅快速验证)
```

## 热点参考 (OMSPBase 已知)

| 热点 | 位置 | 预期 | 监控 |
|------|------|------|------|
| H.264 编码 | `omspbase-codec/src/ffmpeg/encoder.rs` | <5ms 1080p | metrics: `encode_latency_us` |
| RTP 打包 | `omspbase-webrtc/src/backend/*/track.rs` | <1ms | metrics: `rtp_packetize_us` |
| WebSocket relay | `omspbase-server/src/signaling/ws.rs` | <1ms per message | metrics: `ws_relay_us` |
| GStreamer appsink | `omspbase-host/src/capture/gst.rs` | <3ms frame pull | 丢帧计数器 |
| mediasoup transport | `omspbase-server/src/sfu/transport.rs` | <50ms connect | room stats |

## 报告格式

```
## 性能分析报告 — [日期]

### 基准对比
| 基准 | Before | After | Delta |
|------|--------|-------|-------|
| encode_h264_1080p30 | 3.2ms | 3.1ms | -3% |
| rtp_packetize | 0.8ms | 0.4ms | -50% ✅ |
| ws_relay_1kb | 0.3ms | 0.3ms | 0% |

### 发现
- [热点] rtp_packetize 优化: 预分配 buffer pool, -50%
- [回归] 无
- [瓶颈] mediasoup transport connect ~45ms (PTH-07 已知, deferred)

### 建议
- [P0] 无
- [P1] SFU transport.connect actual call (见 pitfall PIT-07)
```

## 禁止

- 不测量就优化 (猜测优化通常引入新问题)
- 多变量同时优化 (无法归因)
- 优化牺牲可读性而无显著收益
- micro-benchmark 脱离真实使用场景
- 忽略 CI 性能退化告警
- `unsafe` 换性能而无验证
