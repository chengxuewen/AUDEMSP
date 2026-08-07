# AUDEMSP Status

**生成**: 2026-08-06 | 决策: 190+ (D1-D215, 含跳号) | Phase: 3 完成 | 222 commits | 22 skills | mediasoup 0.24.1 | PIT-69

**Agent 上下文治理 D213 完成** — instructions 瘦身（pitfalls.md 59KB 移出 instructions → 按需读取，18 文件 ~70KB）+ 六模型 premium/1024K + .agents 精简（删 zh/ 翻译副本 + book-to-skill 瘦身 956K→192K，非项目语言规则保留）。**W3C API 补全 D214 完成** — audemsp-webrtc 补全所有 W3C API（transceiver/parameters/capabilities/sender-receiver 对象方法 + 三后端），Host SFU produce 走标准协商（get_sending_rtp_parameters 推导替代手工 SDP/硬编码），C18 检查 src/ 无残留。**client P2P 迁移 D215 完成** — webrtc_transport 迁移到通用 W3C API（on_data_channel 三后端），修复 client feature 不匹配，5 crate 全编译通过。**待办**: admin dist 修复；e2e_sfu.rs 需在真实 mediasoup 环境更新标准协商（C13 原生编译受 flatbuffers wrapdb 网络限制）

## 测试

| Crate | Lib Tests | Integration | 备注 |
|-------|:---------:|:------------:|------|
| audemsp-common | 68 | — | +backup +logging +auth tests |
| audemsp-media | 54 | — | |
| audemsp-webrtc (stub) | 11 | 67+ | |
| audemsp-webrtc (webrtc-sys) | 11 | 49 (4 ICE 预存) | |
| audemsp-webrtc (webrtc-rs) | 11 | 29 (9 SDP/ICE 预存) | |
| audemsp-codec (stub) | 0 | 32 | |
| audemsp-codec (FFmpeg) | 0 | 35 | |
| audemsp-codec (GStreamer) | 0 | 27 | pixi 环境 |
| audemsp-server | 12 | 32 (27 e2e + 5 integration) | +3 SFU E2E (Linux only) |
| audemsp-host | — | E2E 脚本 9/9 ✅ | macOS native |
| audemsp-client | — | E2E 脚本 9/9 ✅ | macOS native |

### macOS E2E 验证 (2026-07-24)
```
Host (macOS) → WS :9800 → Docker Server → WS :9800 → Client (macOS)
                         └── P2P WebRTC (574 bytes relayed) ──┘
9/9 tests pass: Server health → Build → Host connect → Client connect → SDP → DC → Relay
```

| Phase | 状态 |
|-------|:----:|
| 0-1 基础设施 | ✅ |
| 2a-2d mediasoup SFU | ✅ |
| 3A P0 安全+容错 | ✅ |
| 3B P1 日志+文档 | ✅ |
| 3C P2 高级特性 | ✅ |
| Docker/CI/DevContainer | ✅ |
| macOS E2E 验证 | ✅ |
| Admin Dashboard (P1-P5) | ✅ |
| Admin Dashboard (P6) | 🟡 |
| OpenCode 配置优化 | ✅ |
| Doc-Audit 完整审计 | ✅ |
| OMO 插件版本审计 | ✅ (4.19.2→4.19.3 patch) |

## 决策状态

| 决策 | 内容 | 状态 | Phase |
|------|------|:----:|:-----:|
| D124-D190 | (见 decisions.md) | ✅ | 0-3 |
| D196 | Admin Dashboard 架构 | ✅ | 4 |
| D197 | D87 范围限定 (Client GUI only) | ✅ | 4 |
| D198 | SFU Server-Offer 架构 | ✅ | 4 |
| D199 | Instructions 精简化 | ✅ | Config |
| D200 | OMO Agent 模型分配优化 | ✅ | Config |
| D201 | Pre-commit Hook | ✅ | Config |
| D202 | Global Provider Config 修复 | ✅ | Config |
| D203 | Agent 模型层级最终确认 | ✅ | Config |
| D204 | ecosystem-scan 技能体系 | ✅ | Config |
| D205 | skill-router 技能创建 | ✅ | Config |
| D206 | Docker 国内镜像加速（部分修订: cargo tuna→rsproxy） | 🟡 | Config |
| D207 | 预构建 dev 镜像（机制修订: compose pull） | 🟡 | Config |
| D208 | 构建优化策略实施（详见 docs/reference/codec/build-optimization-strategy.md） | 🟡 | Config |
| D209 | 项目重命名 OMSPBase→AUDEMSP（217 文件/2363 处） | ✅ | Config |
| D210 | 帧时间戳锚定单调真实时钟（11s→2.35s 关键帧间隔） | ✅ | Pipeline |
| D211 | 帧率必须匹配 libwebrtc 编码器配置 — 帧循环绝对时间轴 | ✅ | Pipeline |
| D212 | docs/reference Diátaxis 重组 + 计划体系清理（C19） | ✅ | Docs |
| D213 | Agent 上下文爆炸治理 — instructions 瘦身 + 六模型 1024K + .agents 精简 | ✅ | Config |
| D214 | audemsp-webrtc 补全 W3C API 面 + Host SFU 标准协商（C18） | ✅ | WebRTC |
| D215 | client P2P 迁移到通用 W3C API — 修复 feature 不匹配 | ✅ | WebRTC |

## Admin Dashboard 测试

| Crate | Lib Tests |
|-------|:---------:|
| audemsp-common | 71 (+3) |
| audemsp-server | 32 (新增 admin) |
| audemsp-server e2e | 25 |
| audemsp-server integration | 5 |

## SFU Video Playback

| Phase | 状态 |
|-------|:----:|
| Docker SFU Foundation | ✅ |
| Browser SFU Client | ✅ (Server-Offer) |
| Admin WS SFU Routing | ✅ |
| Web UI (Video Grid + Metrics) | 🟡 |
| Host SFU Produce | ✅ (VideoFrameGenerator squares) |
| Integration E2E | ✅ (transport connect + consume pipeline) |

### SFU 已完成

- ✅ `connect_transport()` 实现 (sfu.rs:331-371)
- ✅ signaling.rs ConnectWebRtcTransport handler 调用实际连接
- ✅ admin.rs 同步修复
- ✅ 浏览器 sfu-client.ts consume 消息补充 rtp_capabilities
- ✅ `default_router_options()` — Router 默认 codec (Opus+VP8+H264)
- ✅ signaling.rs peer_id 一致性修复 — 统一使用 session peer_id
- ✅ `e2e_sfu_consume_pipeline` 测试 — Host produce → Consumer consume 全链路
- ✅ SDP BUNDLE MID 修复 — `a=mid:video`/`a=mid:audio`
- ✅ Consumer late-joiner sync — `list_producers()` + pending producer queuing
- ✅ Host RTP parameters 修复 — payloadType + H264 codec
- ✅ WebRtcServer 单端口 — port 20000

### 下一步

1. Host RTP 发送 — 需要 ICE/DTLS 握手完成（当前 webrtc-rs PeerConnection 无 candidate pairs）
2. Playwright 端到端验证
3. 浏览器 ontrack → video.srcObject → 视频帧渲染
