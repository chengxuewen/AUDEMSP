# mediasoup 官方参考源码导航

> 创建: 2026-08-05 | 用途: AI 分析参考的官方源码基准（跨平台/跨主机开发）
> 源码目录: `.refinfo/`（git-ignored，脚本管理）| 脚本: `scripts/sync-official-refs.sh`

## 为何存在

AUDEMSP Host SFU 集成需要权威的 mediasoup 客户端协商流程基准。官方客户端源码（mediasoup-client JS / libmediasoupclient C++ / mediasoup-demo）是**标准 offer/answer 协商流程的唯一权威对照**（C18 官方用法优先）。这些源码通过脚本拉取到 git-ignored 的 `.refinfo/`，不依赖 `/tmp`（跨主机/重启不丢失），浅克隆只取源码。

**刻意排除**:
- `mediasoup`（server）— 我们只调 API 不改它，不需要源码
- `mediasoup-client-android` — 第三方社区 fork（非 versatica 官方），且非 Rust

## 使用

```bash
# 首次 / 换机 / 需要最新
bash scripts/sync-official-refs.sh          # 幂等, 已存在跳过
bash scripts/sync-official-refs.sh --force  # 强制重拉

# 清理
rm -rf .refinfo
```

网络说明: 国内 GitHub 干扰（PIT-14）→ 脚本自动 `--http1.1` 回退；失败时设 `HTTPS_PROXY` 重试。

## 仓库清单

| 仓库 | 目录 | 版本@commit | 用途 |
|------|------|-------------|------|
| mediasoup-client (JS) | `.refinfo/mediasoup-client` | @ 1d4d597 | `Chrome74.ts send()` — 标准 produce 协商流程 JS 基准 |
| libmediasoupclient (C++) | `.refinfo/libmediasoupclient` | @ 8f08b4a | `Handler.cpp Send()` — 官方 C++ 客户端基准 |
| mediasoup-demo | `.refinfo/mediasoup-demo` | @ 6a578ac | `RoomClient.js` — 官方 demo 端到端流程 |

## 关键文件导航（分析用）

### mediasoup-client (JS)
| 文件 | 内容 | 对应 AUDEMSP 问题 |
|------|------|------------------|
| `src/handlers/Chrome74.ts:335-563` | `send()` 完整 produce 协商 | Host produce 标准流程基准 |
| `src/handlers/Chrome74.ts:360-364` | `addTransceiver(sendonly, sendEncodings)` | 标准方向 + 编码参数 |
| `src/handlers/Chrome74.ts:394-397` | `ortc.getSendingRtpParameters` | rtpParameters 从 offer 推导 |
| `src/handlers/Chrome74.ts:542-560` | 客户端自构 answer + `setRemoteDescription` | **mediasoup 不返回 SDP**（纯 ORTC） |
| `src/handlers/ortc/RTCRtpParameters.ts` | ortc 参数构造 | ssrc/PT/fmtp 推导 |

### libmediasoupclient (C++)
| 文件 | 内容 | 对应 AUDEMSP 问题 |
|------|------|------------------|
| `src/Handler.cpp:185-381` | `SendHandler::Send()` 完整 produce | Host produce 标准流程 C++ 基准 |
| `src/Handler.cpp:191-200` | `RtpTransceiverInit{kSendOnly, send_encodings}` | 标准 direction + encodings |
| `src/Handler.cpp:306-308` | `getRtpEncodings` 从 offer SDP 解析 ssrc | **ssrc 来源**（非手工） |
| `src/Handler.cpp:365-369` | `remoteSdp->GetSdp()` 自构 answer | 客户端自构 answer |
| `src/sdp/RemoteSdp.cpp:157-185` | `RemoteSdp::Send` → `AnswerMediaSection` | answer 如何用服务端 rtpParameters |
| `src/sdp/MediaSection.cpp:255-277` | codecOptions → fmtp (x-google-bitrate) | **官方 keyframe/码率参数路径** |

### mediasoup-demo
| 文件 | 内容 | 对应 AUDEMSP 问题 |
|------|------|------------------|
| `app/src/RoomClient.js` | 端到端 produce/consume | 完整信令 + 媒体协商流程 |

## 相关文档

- `docs/reference/webrtc/mediasoup-client.md` — 官方客户端架构/用法/案例沉淀（画像）
- `docs/reference/webrtc/webrtc-w3c-alignment.md` — 对齐 W3C API 重构分析（含 §0 团队审核修正）
- `docs/reference/webrtc/keyframe-black-screen-analysis.md` — PIT-65 关键帧黑屏根因分析
- `.agents/memorys/` — C18 官方用法优先约束、PIT-46/54/56/65 教训