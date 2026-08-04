# AUDEMSP Status

**生成**: 2026-08-03 | 决策: 184 (D1-D209, 含跳号) | Phase: 3 完成 | 217 commits | 22 skills | mediasoup 0.24.1 | PIT-53

**当前**: 7 crate workspace (audemsp-*)。Phase 3 全部完成。**Host SFU 全链路实现完成** (host-webrtc-sfu-web-client 50/50: audemsp-webrtc 抽象层补全 + ICE/DTLS + E2E)。**统一 Docker 构建策略完成** (docker-build-strategy 29/29: 层缓存 + compose 分离 + Caddy 代理 + CI 镜像)。**OpenVidu 参考文档 3 篇**。Docker 环境本机已装 (29.1.3 + 镜像加速 + daemon 代理)。**构建优化 D208 已实施验证** — 本周 9 项 P0 修复全部完成：builder/dev/runtime 三 target 冒烟 EXIT 0 + runtime health 200；docker-cargo.sh 全链路实测通过（C13 check-server 首次可用，3m27s）；CI 门禁升级（PIT-39 gate 冒烟 + PR build-only）；沉淀 PIT-36~41（Docker dev 链路历史故障 + 编排/编辑教训）。**项目重命名 D209 完成**（OMSPBase→AUDEMSP，217 文件/2363 处）。**Host SFU Produce 打通 (PIT-54)**：根因 Host 手工 rtp_parameters 缺 H264 parameters → match_codecs 严格匹配失败；修复后 Producer 创建 + NewProducer 广播 + I420 帧循环运行。**浏览器 consume 渲染打通 (PIT-56/57)**：E2E videoWidth=640x480 + 棋盘格截图——全链路 Host 编码→mediasoup→浏览器完成；修复链=rtp_capabilities kind 字段+candidates 传递+setup:passive+本地指纹+offer sendonly（PIT-56 六连）+VideoFrame 时间戳递增（PIT-57 编码极小帧）。**部署策略**: 优先本地构建（cargo-cache 卷 + 层缓存，初次 15-30min 一次性）；ghcr 预烘焙按需启动（团队 >1 人/换机频繁时再做，占位符 ghcr.io/org/audemsp-server 已就绪）。**待办**: admin dist 修复；audemsp-webrtc 补 `get_rtp_parameters(track_id)` API（PIT-54，Host 手工 rtp_parameters 双硬编码消除）

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
| D208 | 构建优化策略实施（详见 docs/reference/build-optimization-strategy.md） | 🟡 | Config |
| D209 | 项目重命名 OMSPBase→AUDEMSP（217 文件/2363 处） | ✅ | Config |

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
