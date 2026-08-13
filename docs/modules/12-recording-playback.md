# 录制与回放架构

> 状态: Phase 0 架构设计
> 关联决策: D34–D40
> 依赖模块: 08-pipeline-model.md, 09-transport-architecture.md

## 概述

录制与回放是 MediaServo 跨场景的基础能力，覆盖远程桌面、视频会议、监控相机、推拉流、遥操作五个产品领域。录制回放复用 PipelineEngine 的 `MediaSource → MediaProcessor → MediaSink` 模型——Recording 是 Sink 的一种实现，Playback 是 Source 的一种实现。

## 1. 录制位置三形态

```
                 录制位置
                 ════════

  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
  │ Pipeline 内       │  │ SFU Egress       │  │ 客户端本地        │
  │ (RecordingSink)   │  │ (SFU 旁路 hook)  │  │ (本地编码+存储)   │
  ├──────────────────┤  ├──────────────────┤  ├──────────────────┤
  │ 复用 PipelineEngine│  │ SFU 内部直接旁路  │  │ 录制不依赖服务端   │
  │ 任何经过管线的流   │  │ 同进程零开销拷贝  │  │ 本地磁盘优先      │
  │ 都能录制          │  │ 不走 WebRTC 握手  │  │ 可选异步上传 S3   │
  ├──────────────────┤  ├──────────────────┤  ├──────────────────┤
  │ 适用:             │  │ 适用:             │  │ 适用:             │
  │ 远程桌面录屏      │  │ 视频会议录制      │  │ 远程桌面本地录屏  │
  │ 遥操作记录        │  │ 推流 DVR          │  │ 遥操作本地记录    │
  │ 监控相机录制      │  │                  │  │                  │
  └──────────────────┘  └──────────────────┘  └──────────────────┘
```

**SFU 旁路录制架构** (区别于 LiveKit 的 "隐藏 Peer" 模式):

```
SfuRelayPlugin (PipelineNode)
    │
    ├──▶ 转发给各参会者 WebRTC peer
    │
    └──▶ RecordingEgress (同 Pipeline)
         │
         ├── Compositor (合流场景)  → GStreamer compositor
         ├── Encoder               → NVENC/VAAPI H.264
         └── splitmuxsink          → fMP4 segments → S3/MinIO
```

## 2. 录制粒度

| 场景 | 默认粒度 | 可选 |
|------|---------|------|
| 视频会议 | 合流 MP4（画廊/演讲者布局） | 单流原始 |
| 远程桌面 | 单流录屏 | — |
| 监控相机 | 单流 per-camera | — |
| 推流接收 | 单流 DVR | — |
| 遥操作 | MP4 视频 + JSONL 控制日志 | SEI 嵌入（证据级） |

## 3. 容器格式: fMP4 + splitmuxsink

全线使用 fMP4 (Fragmented MP4)，由 GStreamer `splitmuxsink` 原生支持。

```
Pipeline 输出 → splitmuxsink
                    │
                    ├── max-size-time: 3600s (1h Segment)
                    ├── muxer-factory: mp4mux
                    ├── sink-factory: filesink
                    ├── async-finalize: true (零间隙分片)
                    └── location: session_%d.mp4
```

### Part-Segment 两层模型 (借鉴 MediaMTX)

```
session_1730000000/
├── 001.mp4    ← Segment 1 (0:00–1:00)
├── 002.mp4    ← Segment 2 (1:00–2:00)
└── 003.mp4    ← Segment 3 (2:00–3:00)
```

- **Part**: 最小录制单元 (~1s)，崩溃后最多丢失 1 秒
- **Segment**: 文件组织单元 (~1h)，方便清理、上传和回放
- **splitmuxsink 行为**: 每个 Segment 结束时 async-finalize 当前 muxer，启动新 muxer，零间隙

## 4. 合流架构

```
参会者 A ──▶ SFU ─┐
参会者 B ──▶ SFU ─┤
参会者 C ──▶ SFU ─┼──▶ GStreamer compositor
                   │     ├── 解码 N 路 H.264 → YUV
                   │     ├── 画面合成（画廊/演讲者布局）
                   │     ├── 叠加水印（时间戳、参会者名）
                   │     └── NVENC/VAAPI → H.264 合流
                   │
                   └──▶ splitmuxsink → fMP4 segments
```

- `compositor` 元素支持 GPU 加速（CUDA 零拷贝路径：decode→compose→encode 全程 GPU 显存）
- 合成延迟：~10ms（硬件路径）
- 布局配置：画廊 (N×N grid) / 演讲者 (speaker + thumbnails)

## 5. WebRTC 录制截取点

录制发生在 **MediaTransport trait 输出之后**，即媒体层而非传输层。

```
MediaTransport::poll_output()
    │
    ▼ TransportOutput::Media(frame)
    │
    ▼ PipelineEngine
        ├── MediaProcessor: 缩放/颜色转换（如果需要）
        ├── MediaProcessor: SEI 注入（遥操作场景）
        └── RecordingSink: MediaSink 实现
```

**选择媒体层的理由**:
- 与传输后端解耦（str0m / libwebrtc / webrtc-rs 无感）
- 自动处理 Simulcast（只录最高层）、FEC/RTX（不录重传包）
- SVC 流保留完整 bitstream（回放时可降层）
- 录制从 keyframe 对齐开始（前几秒丢弃，标准行为）

## 6. 遥操作多轨录制

### 默认: 分离文件

```
session_001/
├── video_001.mp4          ← H.264 视频流
└── control_001.jsonl      ← 控制指令日志
```

JSONL 格式:
```jsonl
{"ts": 1730000000001, "type": "steering", "angle": 15.2, "speed": 30.0}
{"ts": 1730000000050, "type": "brake", "pressure": 0.7}
{"ts": 1730000000100, "type": "throttle", "value": 45.0}
```

### 可选: SEI 嵌入 (证据级)

```
H.264 bitstream:
  [SPS][PPS][SEI: control_state][IDR][SEI: control_state][P][SEI: control_state]...
      ↑                      ↑
  每帧附带控制状态快照       帧精确绑定，无法分离篡改
```

- SEI NAL 单元类型 6，标准播放器忽略
- 专用回放工具可提取 SEI 实现逐帧同步
- SEI 注入作为 `SeiInjector` MediaProcessor，不改架构

## 7. 回放架构

```
录制产物                    回放方式                    播放端
────────                   ────────                   ──────
fMP4 segments ──────▶ HLS playlist ─────────▶ 浏览器 <video>
                      (nginx/S3 静态服务)      管理后台 web 端

本地 fMP4 文件 ────▶ FrameServer ──────────▶ Client App
                      (精确 seek)             远程桌面录屏回放
                                              遥操作事故回溯
```

- **HLS 流式**: segments 即 HLS 分片，`m3u8` playlist 由 nginx/S3 拼接，浏览器原生 `<video>` 播放
- **本地回放**: FrameServer 提供时间轴 seek，支持变速播放和帧步进
- **不需要按需转码**: Phase 1 无此需求，未来可用 Cloudflare Stream 等方案

## 8. 存储与生命周期

```
存储: S3 / MinIO / 本地磁盘
路径模板: recordings/{room_id}/{timestamp}_{seq}.mp4
保留策略: 按场景可配置
  - 视频会议: 默认 30 天
  - 监控: 7 天循环覆盖
  - 遥操作: 事故会话永久保留
  - 远程桌面: 用户自行管理
```

## 9. 项目参考

| 参考产品 | 借鉴点 |
|---------|--------|
| LiveKit Egress | GStreamer compositor 合流、模板文件名、多输出格式 |
| mediasoup | PlainTransport 录制模式（SFU 旁路验证） |
| GStreamer splitmuxsink | 零间隙分片、async-finalize、mp4mux 工厂 |
| MediaMTX | Part-Segment 模型、路径模板、自动清理 |
| OBS Studio | 子进程隔离 muxer、replay buffer（未来参考） |

**Phase 2+ 补充**：fMP4 + splitmuxsink recording 作为一等公民的 pipeline 路径（D34-D40）。Phase 1 录制为 MediaSink 简单实现；Phase 2+ 将录制提升为独立的、可组合的一级管道节点，支持合流、SEI 嵌入、多轨并行录制等高级能力。
