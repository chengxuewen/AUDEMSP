# SFU 配置与调优

## 1. mediasoup Worker 配置

```javascript
// worker.js — mediasoup Worker 配置
const worker = await mediasoup.createWorker({
  logLevel: "warn",
  logTags: [
    "info",
    "ice",
    "dtls",
    "rtp",
    "srtp",
    "rtcp",
  ],
  rtcMinPort: 40000,
  rtcMaxPort: 49999,
  dtlsCertificateFile: "/etc/audemsp/certs/dtls.pem",
  dtlsPrivateKeyFile: "/etc/audemsp/certs/dtls-key.pem",
});
```

### Worker 分配策略

```
auto 策略 (默认):
  workers = ceil((nproc * 0.8) + (max(0, nproc - 32) / 2))
  例: 8 核 →  ceil(6.4 + 0) = 7 Workers
  例: 64 核 → ceil(51.2 + 16) = 68 Workers

按媒体类型分配 (大部署):
  ┌──────────────┬─────────┬────────┐
  │ Worker Pool  │ Workers │ 用途   │
  ├──────────────┼─────────┼────────┤
  │ audio        │ 2       │ 纯音频 │
  │ video        │ nproc-4 │ 视频   │
  │ screen       │ 2       │ 屏幕共享│
  └──────────────┴─────────┴────────┘
```

## 2. Router 配置

```javascript
const router = await worker.createRouter({
  mediaCodecs: [
    {
      kind: "audio",
      mimeType: "audio/opus",
      clockRate: 48000,
      channels: 2,
      parameters: {
        useinbandfec: 1,
        usedtx: 1,
      },
    },
    {
      kind: "video",
      mimeType: "video/VP8",
      clockRate: 90000,
      parameters: {
        "x-google-start-bitrate": 1000,
      },
    },
    {
      kind: "video",
      mimeType: "video/VP9",
      clockRate: 90000,
      parameters: {
        "profile-id": 2,
      },
    },
    {
      kind: "video",
      mimeType: "video/H264",
      clockRate: 90000,
      parameters: {
        "packetization-mode": 1,
        "profile-level-id": "42e01f",
        "level-asymmetry-allowed": 1,
      },
    },
  ],
});
```

## 3. Transport 配置

```javascript
const transport = await router.createWebRtcTransport({
  listenIps: [
    { ip: "0.0.0.0", announcedIp: "public-ip" },  // 公网 IP
  ],
  enableUdp: true,
  enableTcp: true,
  preferUdp: true,
  initialAvailableOutgoingBitrate: 1_000_000,       // 1 Mbps
  minimumAvailableOutgoingBitrate: 100_000,          // 100 Kbps
  maxSctpMessageSize: 262_144,                       // 256 KB
  maxIncomingBitrate: 5_000_000,                     // 5 Mbps limit
});
```

## 4. Producer/Consumer 配置

### Producer (发布者)

```javascript
const producer = await transport.produce({
  kind: "video",
  rtpParameters: {
    mid: "0",
    codecs: [...],
    headerExtensions: [...],
    encodings: [
      { rid: "r0", maxBitrate: 150_000, scaleResolutionDownBy: 4 },
      { rid: "r1", maxBitrate: 400_000, scaleResolutionDownBy: 2 },
      { rid: "r2", maxBitrate: 1_200_000, scaleResolutionDownBy: 1 },
    ],
    rtcp: { reducedSize: true },
  },
  appData: { participantId: "xxx" },
});
```

### Consumer (订阅者)

```javascript
const consumer = await transport.consume({
  producerId: "producer-id",
  rtpCapabilities: clientRtpCapabilities,
  paused: false,
  preferredLayers: {
    spatialLayer: 2,    // SVC 空域层
    temporalLayer: 2,   // SVC 时域层
  },
  appData: { participantId: "yyy" },
});
```

## 5. 带宽管理

### 5.1 Dynacast (按需编码)

Dynacast 监控每个 Producer 的订阅情况。如果某编码层无人订阅，通知发布者停止发送该层，节省上行带宽。

```javascript
// 在 Transport 上启用
transport.enableDynacast();

// 切换订阅层时自动触发
await consumer.setPreferredLayers({ rid: "r1" });
// Dynacast 检测到 r0 和 r2 无人订阅 → 通知发布者停止
```

### 5.2 PLI/FIR 聚合

多个 Consumer 同时请求关键帧会导致编码器 burst。PLI/FIR 聚合将多个请求合并为一个。

```javascript
// 默认行为 (mediasoup 内置)
// 聚合窗口: 500ms
// 窗口内多个请求 → 1 个上游 PLI/FIR

// 配置
const consumer = await transport.consume({
  producerId: "...",
  rtpCapabilities: "...",
  // 启用 PLI/FIR 聚合 (默认开启)
});
```

### 5.3 带宽估计

```
发送端估计 (Sender-Side BWE):
  transport-cc 反馈 → per-packet RTT → 带宽估计

接收端估计 (Receiver-Side BWE):
  TWCC (Transport Wide Congestion Control) 反馈 → REMB

AUDEMSP 策略:
  TWCC 优先 (更准确)
  REMB 作为 fallback
  delay-based 算法用于延迟敏感网络
  loss-based 算法用于丢包敏感网络 (跨运营商)
```

### 5.4 码率表

| 场景 | 分辨率 | 帧率 | 码率参考 | 编码层 |
|------|--------|------|---------|--------|
| 多人小窗 | 320×180 | 15fps | 150 Kbps | Simulcast r0 |
| 单人中窗 | 640×360 | 24fps | 400 Kbps | Simulcast r1 |
| 全屏视频 | 1280×720 | 30fps | 1200 Kbps | Simulcast r2 |
| 屏幕共享 | 1920×1080 | 15fps | 2000 Kbps | Simulcast r2 |
| 音频 | - | - | 30 Kbps | Opus |

## 6. NAT 穿透

### ICE 配置

```javascript
const transport = await router.createWebRtcTransport({
  listenIps: [
    { ip: "0.0.0.0", announcedIp: "203.0.113.1" },  // 公网 IP
  ],
  enableUdp: true,
  enableTcp: true,
});
```

### TURN 集成

当 P2P 打洞失败时使用 TURN 中继：

```typescript
const client = new ConferenceClient({
  iceServers: [
    { urls: "stun:stun.l.google.com:19302" },
    {
      urls: "turn:turn.audemsp.io:3478",
      username: "user",
      credential: "password",
    },
  ],
});
```

## 7. 性能调优

### 7.1 系统参数

```bash
# /etc/sysctl.conf — 网络调优
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
net.ipv4.udp_mem = 4096 87380 16777216
net.ipv4.tcp_congestion_control = bbr
net.core.netdev_max_backlog = 5000

# UDP 缓冲区
net.ipv4.udp_rmem_min = 65536
net.ipv4.udp_wmem_min = 65536

# 连接跟踪
net.netfilter.nf_conntrack_max = 1048576
```

### 7.2 ulimit

```bash
# /etc/security/limits.conf
audemsp  hard  nofile  1048576
audemsp  soft  nofile  1048576
```

### 7.3 Worker 调优

| 参数 | 推荐值 | 说明 |
|------|--------|------|
| Worker per CPU | 1 | 每个 Worker 绑定一个 CPU 核 |
| Room per Worker | <500 | 单个 Worker 的房间数上限 |
| Participant per Worker | <2000 | 单个 Worker 的参与者上限 |
| Consumer per Participant | <50 | 单个参与者最多订阅流数 |
| Transport per Worker | <2000 | 单个 Worker 的 Transport 上限 |
| 大房间 Worker 独占 | - | >16 人的房间独占 Worker |

### 7.4 内存估算

```
per Participant:
  Transport: ~100 KB
  Audio Producer: ~200 KB
  Video Producer (Simulcast): ~500 KB
  Consumer (每路): ~300 KB

例: 16 人会议室 (每人订阅 15 路视频 + 15 路音频):
  per participant: 100KB + 200KB + 500KB + 30 × 300KB = 9.8 MB
  16 人: ~157 MB + Worker 基础开销 (~50 MB) = ~207 MB
  100 间 16 人会议室: ~20.7 GB
```

## 8. 故障排查

### 8.1 常见问题

| 症状 | 可能原因 | 解法 |
|------|---------|------|
| 连接失败 | ICE/防火墙端口 | 检查 UDP 40000-49999 是否开放 |
| 视频卡顿 | 带宽不足 | 降码率 / 切编解码 |
| 高延迟 | NAT/TURN 中继 | 检查 relay 路径，优化 ICE 候选 |
| 音频断流 | FEC 不足 | 启用 Opus inband FEC |
| 内存泄漏 | Worker 未回收 | 检查 Worker 生命周期管理 |
| SFU 崩溃 | Worker 负载过重 | 增加 Worker 数，限制 per-Worker 负载 |

### 8.2 诊断命令

```bash
# 检查 Worker 状态
curl http://localhost:9090/metrics | grep conference_

# 检查 WebRTC 端口
lsof -i :40000-49999

# 检查 UDP 缓冲区使用
ss -uap | wc -l

# 实时日志
journalctl -u audemsp-conference -f

# 内存使用
ps aux | grep mediasoup-worker | awk '{sum+=$6} END {print sum/1024 " MB"}'
```