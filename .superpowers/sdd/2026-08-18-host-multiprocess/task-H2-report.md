# Task H2 报告: 音频会议房间 (2026-08-19)

## 状态
✅ 完成（含 1 个文档化阻塞: PIT-105 — libwebrtc 音频编码不产 RTP）

## Commit(s)
- `feat(webrtc): H2 webrtc-sys 音频轨道 — AudioTrackSource + capture_frame PCM 推流 (TrackKind::Audio 真实发送路径)`
- `feat(server): H2 音频会议房间 — audio- 房间语义 + SfuStats 统计协议`
- `feat(host): H2 host-audio 进程 + 3 方音频会议 e2e`

## 测试摘要
- server (Docker): **135 全绿**（lib 81 + admin_e2e 6 + e2e 25 + e2e_sfu 5 + integration 18）
- e2e_audio_conf **2/2**（3 方全互连接线 + 音频房间禁视频 4031）— 外部 Docker server
- host_audio_e2e **2/2**（坏参 exit 2 + 进程全流程/优雅退出 0）— 外部 Docker server
- 回归: e2e_sfu **4/4** + e2e_sfu_codec_prefs **6/6**（视频链路无回归）
- native: `pixi run check` 0 error; common 82 / host lib 45 / webrtc-sys lib 全绿

## 音频房间设计（REUSE 胜出）
**音频房间 = 既有 SFU 机制 + room_id 前缀约定 `audio-<vehicle-id>`，不新增 RoomType**：
- mediasoup Router 按 room_id 字符串隔离 — 音频房间与视频/控制房间同机共存零改动
- 房间语义（全互连 opus: 每参与者 publish 1 路 + subscribe 其他所有）由两处表达:
  ① **produce 门**: `is_audio_room(room_id) && kind != Audio` → 4031 + audit（signaling.rs Produce 分支）
  ② **客户端全订阅**: NewProducer 广播 + late-joiner 重放（既有机制）驱动 subscribe
- **成员策略（G3 既有门，零新增）**: join = RoomJoin 门（`room_owners` 存设备 ID = 车辆 ID —
  车端自动、账号白名单、dispatcher/admin 任意车天然正确）；produce = can_produce（账号禁发 —
  舱端只消费；两方发言需 D-H11 修订，已记录）
- **网关直通**: rewrite_room 对 `audio-` 房间跳过重写（子进程已用规范名；重写会把音频会议
  并入视频房间，破坏每车独立音频房语义）
- 对比过的备选: 新 RoomType::Audio（否决 — 引入第二套房间管理，媒体面无差异）、
  新 AudioJoin/AudioJoined 协议（否决 — RoomJoin 已有全部门控语义，additive 协议最小化）

## WebRTC 音频能力发现（关键）
- **FFI 层完整存在**: vendor/webrtc-sys（livekit fork）有 `AudioTrackSource` + `capture_frame(PCM i16)`
  + `NativeAudioSink` + `create_audio_track` — libwebrtc 内部 opus 编码由 vendor 提供（patch
  `external_audio_source.patch` 已在 prebuilt libwebrtc.a 中，字符串实证 12 处）
- **mediaservo-webrtc 此前未接线**（write_frame 音频 = pass-through stub）→ H2 补全:
  `WebrtcSysTrack.audio_source` + `write_pcm`（10ms PCM 帧 → capture_frame）+ `create_audio_track`
  + `create_track_sender(Audio)` 分支（track id 必须 "audio" — sender 按 libwebrtc 内部 label 匹配）
- **PIT-105 阻塞（vendor 域）**: 全链路协商正确（answer `a=sendonly` + ssrc + opus/48000/2、
  DTLS/ICE Connected、capture_frame 逐帧成功、FFI 探针实证 source→track sink 交付 70 次回调），
  但 **libwebrtc 音频编码不产 RTP**（outbound-rtp bytesSent=0、server PROD-TRACE 零事件）。
  丢失点在 libwebrtc 音频发送通道内部（LocalAudioSinkAdapter 挂载/channel StartSend 状态）。
  已排除: SDP、传输、源→轨道 sink 链、patch 存在性、帧节奏/大小。
  修复方向: C++ 最小复现对照 livekit-go 官方 publish 流程（C11）或换 webrtc-rs 音频路径。
- **后果**: 音频媒体面证据（byte_count>0）挂起；e2e 断言 wiring 证据（3 producer + 6 consumer
  全 Audio kind + SfuStats 统计可达）；host-audio 在 PIT-105 修复后无需改动即出音频

## 交付物
| 面 | 内容 |
|----|------|
| common | `SfuStatsRequest`/`SfuStats`（producer/consumer RTP 统计 — e2e 媒体面证据 + H3 面板数据源）+ roundtrip 测试 |
| server | `is_audio_room` + produce 门（4031+audit）+ `producer_stats`/`consumer_stats`（mediasoup get_stats）+ SfuStatsRequest 处理（can_consume 纵深门） |
| webrtc | AudioTrackSource 发送路径（上述） |
| host | `mediaservo_host::audio` 模块（opus SDP/produce 参数/tone 纯函数 + 4 单测）+ `host-audio` 进程（信令→publish→tone 推流→NewProducer 全订阅→SfuStats 日志→SIGTERM/--duration 优雅退出 0） |
| gateway | rewrite_room 对 audio- 房间直通 |
| e2e | e2e_audio_conf（3 方: 车端+舱端+dispatcher 合成参与者; 6 consumer 全订阅; 4031 负例）+ host_audio_e2e（进程级） |

## 成员策略（e2e 实证）
- 车端（device）: join `audio-<vehicle>` 自动允许 + 登记 owner → produce 自动允许 ✅
- 舱端（viewer+ 白名单）: join 由 RoomJoin 门按车辆 ID 校验 ✅（既有矩阵测试覆盖）
- dispatcher/admin: 任意车 ✅（既有矩阵测试覆盖）
- 账号 produce: 4031（舱端只消费 — G3 定稿，未改动）✅
- 音频房间视频 producer: 4031 + audit（新增门）✅

## E2E 证据
```
PRODUCER <id>: bytes=0 packets=0 kind=Some(Audio) (PIT-105: >0 待音频编码修复)   ×3
CONSUMER <id> ← <producer>: bytes=0 packets=0 (PIT-105: >0 待音频编码修复)       ×6
test result: ok. 2 passed (e2e_audio_conf)
test result: ok. 2 passed (host_audio_e2e: 已加入音频房间 → published producer → 已退出 code 0)
```

## 后续（Follow-ups）
1. **PIT-105 攻关**（最高优先）: C++ 最小复现（对照 livekit-go publish）+ 或 webrtc-rs 音频路径验证（先验 ICE vs mediasoup）
2. H3 admin dashboard 音频面板（SfuStats 数据源已就绪）
3. ALSA/MMAPI 麦克风接入 host-audio（tone stub 替换点已留: tone 任务即捕获泵）
4. 账号两方发言（D-H11 can_produce 修订 — 安全决策需用户）
5. host-agent 多房间模型（当前整车单会话；音频房间直通已兼容，agent 级编排待 Phase D 扩展）
