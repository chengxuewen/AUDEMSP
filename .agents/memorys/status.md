# AUDEMSP Status

**生成**: 2026-08-11| 决策: 190+ (D1-D216, 含跳号)| Phase: 3 完成 || 228 commits | 22 skills | mediasoup 0.24.1 | PIT-77 | 分支: main (VideoSource 统一接口已合入) || Crate | Lib Tests | Integration | 备注 |
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
| Host SFU Produce | ✅ (标准 answerer 协商, squares) |
| Integration E2E | ✅ (4/4 纯外部模式 + 浏览器渲染) |

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

## VideoSource 统一帧源接口 (2026-08-11, 计划 video-source-unification T1-T4)

- WebRtcTrackSink (audemsp-webrtc): 同步 VideoSource 广播 → bounded(3) channel → 异步 write_raw_i420_with_ts (c56bd87)
- Host B5 手写循环 → VideoFrameGenerator + TimestampOverlay (Combined/TopLeft) — 时间戳水印修复 (acd28d9)
- PIT-81: generator 绑定 main 级作用域修复 (7642960); e2e 脚本 headless shell (6268cf4)
- 验证: 关键帧 2.0s 不回归 + 浏览器首帧渲染 + 水印像素确认 + e2e_sfu 4/4

## setCodecPreferences 实现与验证 (2026-08-11, 计划 set-codec-preferences T1-T5)

- transceiver_set_codec_preferences: track_id 定位（mid 协商前不存在）+ fmtp 双向映射 (fc49f07)
- 6 场景矩阵 e2e_sfu_codec_prefs + offerer 机制验证 (732845e)
- 实证结论: ① offerer 偏好生效（offer codec 序重排, H264>VP8）② answerer(SFU) 偏好对
  answer 无效（libwebrtc 按 offer 序取交集）→ SFU 固定 codec 走 reduceCodecs
  ③ VP9/AV1 负向: InvalidAccessError 语义（set 拒绝/空列表）

## 编码器软/硬后端 + codec 配置 (2026-08-11, 计划 encoder-backend-codec-config T1-T7)

- set_video_encoder_backend: PcBackend track_id 分派 → SetEncoderSelector (d4e641e)
- offer codec 参数化 (config.encoder.codec) + backend 接线 + EncoderConfig.codec (78d95c4)
- H264 42e01f 全链路: router profile 统一 + produce parameters + 浏览器 consume 双 codec (75a849a)
- 验证: auto→VP8 / h264→浏览器 1280x720 渲染 / vp8 / vp9→Error 5000 / backend=software

## Web 端编码状态展示 (2026-08-11, 计划 web-stream-stats T1-T6)

- EncoderStatus 信令 + webrtc-sys get_stats 接线（ToJson 解析, encoder_implementation）(1a46296)
- Host 2s 周期上报 + server room 广播 relay（should_relay + DeviceStream 放行）(1678e8c)
- sfu-client StreamMetrics 扩展 + VideoPlayer ToDesk 风格分组面板（连接质量/编解码器/系统性能）(da16c33)
- 验证: 面板显示"软编/OpenH264/H264/30fps/1280x720" + encoder_status 4 次接收 + 全量回归

## stats 面板修复 (2026-08-11)

- 闪烁: 双数据源交替覆盖 → mergedMetrics 合并累加器（6df4630）
- 码率: 累计 bytesReceived 当瞬时 → 增量计算
- 验证: 3 采样稳定（libvpx/VP8/30fps/软编）
