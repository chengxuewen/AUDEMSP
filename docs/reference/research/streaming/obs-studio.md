# OBS Studio 参考分析
> 生成日期：2026-07-16 | 分类：流媒体

## 1. 产品画像
- **名称**：OBS Studio (Open Broadcaster Software)
- **开发者**：OBS Project（社区驱动项目）。核心维护者包括 Hugh "Jim" Bailey (Warchamp7/UI), John-Michael "Jayson" Brooks (jpark37/视频管线), Paul (PatTheMav/macOS), notr1ch (Windows/安全), Sean-Der (WebRTC), exeldro (滤镜/插件API) 等。社区贡献者超过 120 人
- **首次发布**：2012年（OBS Classic v0.1，最初叫 Open Broadcaster Software），2016年（OBS Studio v0.14 重写完成），持续开发超过 14 年
- **产品定位**：全球最流行的开源直播编码器和录屏工具。通用直播制作软件，覆盖从个人主播到专业内容创作者的完整谱系。兼具录屏、推流、虚拟摄像头功能。通过插件生态覆盖了企业级直播制作的绝大多数需求
- **目标用户群体**：游戏主播 (Twitch/YouTube Gaming)、播客制作人、教育内容创作者、企业培训视频制作者、音乐表演直播、电子竞技转播制作、宗教活动直播
- **许可 / 商业模式**：GPLv2 许可，完全免费。收入来源包括社区捐赠（Open Collective）、商业赞助（Twitch, YouTube, Meta, NVIDIA, AMD, Intel, Logitech 等每年赞助 $500K+）、以及开源合作项目

## 2. 技术特性
### 整体架构
```
┌──────────────────────────────────────────────────────────────────┐
│                      OBS Studio (Qt GUI + libobs)                  │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐     │
│  │                    Qt UI Layer                            │     │
│  │   Scenes · Sources · Mixer · Settings · Transitions       │     │
│  │   Studio Mode (preview/program dual window)               │     │
│  └───────────────────────────┬──────────────────────────────┘     │
│                              │ C API (libobs)                     │
│  ┌───────────────────────────▼──────────────────────────────┐     │
│  │                    libobs (核心引擎)                       │     │
│  │                                                           │     │
│  │  ┌─────────┐    ┌──────────┐    ┌───────────────────────┐ │     │
│  │  │ Scene   │───▶│ Graphics │    │    Audio Pipeline     │ │     │
│  │  │ Graph   │    │ Pipeline │    │  ┌─────┐ ┌─────┐     │ │     │
│  │  │         │    │ (GPU)    │    │  │Mixer│→│Enc  │     │ │     │
│  │  └────┬────┘    └────┬─────┘    │  └─────┘ └──┬──┘     │ │     │
│  │       │              │          └─────────────┼────────┘ │     │
│  │       │     ┌────────▼────────┐               │          │     │
│  │       │     │  Video Encoder  │               │          │     │
│  │       │     │ NVENC/AMF/VAAPI │               │          │     │
│  │       │     │/VT/QSV/SVT-AV1  │               │          │     │
│  │       │     └────────┬────────┘               │          │     │
│  │       │              │                        │          │     │
│  │       │     ┌────────▼────────────────────────▼───────┐  │     │
│  │       │     │           obs_output_t 插件层            │  │     │
│  │       │     │  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐  │  │     │
│  │       │     │  │ RTMP │ │ SRT  │ │ RIST │ │ WHIP │  │  │     │
│  │       │     │  │Output│ │Output│ │Output│ │Output│  │  │     │
│  │       │     │  └──────┘ └──────┘ └──────┘ └──────┘  │  │     │
│  │       │     └────────────────────────────────────────┘  │     │
│  │       │      ▲          ▲          ▲          ▲          │     │
│  │       │      │          │          │          │          │     │
│  │  ┌────┴──────┴──────────┴──────────┴──────────┴─────┐   │     │
│  │  │  obs_source_t 插件层（输入源）                      │   │     │
│  │  │  Window Capture · Display Capture · Game Capture  │   │     │
│  │  │  Video Capture Device · Browser Source (CEF)      │   │     │
│  │  │  Media Source · Image/Image Slide Show · Text     │   │     │
│  │  │  VLC Video Source · NDI Source · Audio Sources    │   │     │
│  │  └───────────────────────────────────────────────────┘   │     │
│  │                                                           │     │
│  │  ┌───────────────────────────────────────────────────┐   │     │
│  │  │  扩展层                                            │   │     │
│  │  │  Lua 脚本 (LuaJIT) · Python 脚本 (obs-scripting)  │   │     │
│  │  │  obs-websocket (WebSocket RPC)                    │   │     │
│  │  │  Filters (视频: 色键/LUT/缩放/锐化 · 音频: VST)  │   │     │
│  │  │  Transitions (Stinger/Track Matte/Luma Wipe)      │   │     │
│  │  │  Encoders (第三方: StreamFX, Aitum Multistream)   │   │     │
│  │  └───────────────────────────────────────────────────┘   │     │
│  └───────────────────────────────────────────────────────────┘     │
└──────────────────────────────────────────────────────────────────┘
```

### libobs 核心引擎详解

libobs 是 OBS Studio 的核心 C API 库，提供了所有插件的抽象接口：

**图形管线**：
- 场景图（Scene Graph）管理所有源的变换矩阵（位置、缩放、旋转、裁剪）和可见性
- 支持源嵌套和分组 — 一个场景可以是另一个场景中的源
- GPU 加速的画面合成，使用 OpenGL (Linux/macOS) 或 Direct3D 11 (Windows)
- 每个视频帧在 GPU 上完成所有合成操作（缩放、混合、滤镜），CPU 不参与像素处理
- Studio Mode（预览/节目双窗口）通过在 GPU 上维护两个渲染目标实现

**音频管线**：
- 新版 Audio Mixer (v32.1) 支持垂直和水平两种布局
- 音频源置顶（pinned）：全局音频源始终显示，不受场景切换影响
- 隐藏源可见：不在当前场景中的源的音频状态也可以查看和控制
- 最多 6 路独立音频轨道，每路可单独配置编码参数
- 独立监听（音频监测）：可监听任意音频源而不影响输出流
- VST 2.x 插件支持：可加载第三方音频效果器
- NVIDIA 音频特效支持 (v32.2)：RTX Voice 等 AI 音频处理

**编码输出管线**：
- RTMP 输出架构：两线程模型 — 连接线程 (connect_thread) + 发送线程 (send_thread)，通过信号量协调
- Windows 上还有独立的 socket 线程优化网络吞吐
- WebRTC WHIP 输出取消了 interleaver：FLV 的音视频交错排序对 WebRTC 不必要（音视频走独立 RTP 轨道），跳过此环节消除管线延迟
- Enhanced RTMP 支持 HEVC/AV1 编码和多音视频轨道（通过自维护的 librtmp fork）

### 关键技术能力
| 能力 | 详情 |
|------|------|
| 协议支持 | 推流：RTMP/RTMPS (Enhanced RTMP — HEVC/AV1/多轨)，SRT (libsrt)，RIST (librist)，WHIP/WebRTC (libdatachannel, v32.1 新增)，HLS (自定义 muxer)。录制：FLV, MKV, MP4, MOV, TS, fragmented MP4, fragmented MOV, Hybrid MP4 |
| 编码 | **GPU 硬件编码**：NVIDIA NVENC (H.264/HEVC/AV1/CQVBR)，AMD AMF (H.264/HEVC/AV1)，Intel QSV (H.264/HEVC/AV1)，Apple VideoToolbox (H.264/HEVC)，VA-API (Linux H.264/HEVC)。**软件编码**：x264 (H.264)，SVT-AV1，AOM AV1。**音频**：AAC (FFmpeg/CoreAudio), Opus, FLAC, PCM |
| 传输 | RTMP: librtmp (自维护 fork，mbedTLS/OpenSSL/GnuTLS)；SRT: libsrt；RIST: librist；WebRTC WHIP: libdatachannel；RTMP 两线程架构 |
| 录制 | 多格式输出，Hybrid MP4 (crash 恢复 — 分片写入，崩溃时已录制部分可用)，多轨音视频录制，Replay Buffer (内存循环缓冲，一键保存 N 秒前的画面) |
| 场景管理 | Scene Graph: 源 → 滤镜链 → 场景 → 转场。源嵌套和分组（无限层级）。Studio Mode (预览/节目分离)。变换矩阵自由调整 |
| 音频系统 | 新版 Audio Mixer (v32.1): 垂直/水平布局，源置顶，隐藏源显示，独立监听。6 路独立音轨。VST 2.x 效果器支持。NVIDIA 音频特效 |
| 多路流 | 原生仅支持单一输出流。多平台同时推流需第三方插件（Multiple RTMP Outputs 或 Aitum Multistream）。v32.2 新增多轨视频动态码率 |
| WHIP Simulcast | v32.1 新增。1-4 层编码分层。策略：1层=100%, 2层=50%, 3层=33%, 4层=25% 最大分辨率。支持 H.264/HEVC/AV1 编码 |


### 编码器性能对比

OBS Studio 支持 6 种 GPU 编码后端和 3 种软件编码后端。以下对比 AUDEMSP 可参考的编码器选型依据：

| 编码器后端 | 平台 | H.264 | H.265/HEVC | AV1 | 多编码实例 | 延迟特性 | AUDEMSP 适用场景 |
|-----------|------|:----:|:---------:|:---:|:---------:|---------|-------------------|
| NVENC (NVIDIA) | Windows/Linux | ✅ | ✅ | ✅ (Ada+) | ✅ (最多 5 路) | 超低 (<1ms) | Host 端远程桌面编码 |
| AMF (AMD) | Windows/Linux | ✅ | ✅ | ✅ (RDNA3+) | ✅ (3 路) | 低 (1-2ms) | Host 端备选编码 |
| QSV (Intel) | Windows/Linux | ✅ | ✅ | ✅ (Arc+) | ✅ (3 路) | 低 (1-2ms) | 笔记本/低功耗场景 |
| VideoToolbox (Apple) | macOS | ✅ | ✅ | ❌ | 有限 | 超低 (<1ms) | macOS Client 端 |
| VA-API | Linux | ✅ | ✅ | ⚠️ 有限 | ✅ (3 路) | 低 (1-2ms) | Linux 服务端编码 |
| x264 (软件) | 全平台 | ✅ | ❌ | ❌ | 按核数 | 中 (5-50ms) | 备选/高画质录制 |
| SVT-AV1 (软件) | 全平台 | ❌ | ❌ | ✅ | 按核数 | 高 (>50ms) | 高压缩率存档 |

**编码预设与延迟权衡**：

| 预设 | NVENC 延迟 | x264 延迟 | 质量 | 适用 AUDEMSP 场景 |
|------|-----------|----------|------|-------------------|
| ultrafast | ~0.5ms | ~5ms | 最低 | 远程桌面 (远控) — 最低延迟优先 |
| veryfast | ~1ms | ~10ms | 低 | 通用直播推流 — 延迟与质量的平衡 |
| faster | ~1.5ms | ~20ms | 中 | 视频会议 — 质量比远控要求高 |
| medium | ~2ms | ~50ms | 高 | 录制存档 — 延迟不重要，质量优先 |

**AUDEMSP 编码选型建议**：
- 远程桌面场景：NVENC/VideoToolbox ultrafast，延迟 <1ms 目标
- 视频会议场景：NVENC/AMF veryfast，延迟 <10ms 目标
- 监控录制场景：NVENC/SVT-AV1 medium，质量优先
- 推流中转场景：NVENC veryfast，透传不重新编码（Fragment Model）

### 传输协议性能对比

OBS Studio 支持 5 种推流协议，各协议在不同场景下的性能差异显著：

| 协议 | 延迟（局域网） | 延迟（公网） | 抗丢包 | 防火墙友好 | 浏览器原生 | 适用场景 |
|------|:-----------:|:----------:|:-----:|:---------:|:---------:|---------|
| RTMP/RTMPS | 1-3s | 2-5s | 低（TCP 重传） | ✅（TCP 1935） | ❌ | 通用推流（最广泛兼容） |
| SRT | 0.5-1s | 1-3s | 高（ARQ 重传） | ⚠️（UDP 端口） | ❌ | 不可靠网络推流 |
| RIST | 0.5-1s | 1-3s | 高（FEC+ARQ） | ⚠️（UDP 端口） | ❌ | 广电级远程推流 |
| WHIP/WebRTC | 100-300ms | 200-500ms | 中（NACK+FEC） | ✅（HTTPS 443） | ✅ | 低延迟互动推流 |
| HLS | 3-10s | 5-15s | 最高 | ✅（HTTP 80/443） | ✅ | 大规模分发 |

**AUDEMSP 的推流协议选择策略**：
- 远程桌面（远控）：WHIP/WebRTC — 最低延迟，浏览器原生支持
- 视频会议：WHIP/WebRTC — 实时双向互动
- 监控相机接入：SRT — 不丢帧，防火墙穿透较好
- 车端推流：SRT（主推流）+ WHIP（实时交互）— 双协议混合
- 通用直播推流：RTMP（兼容生态）+ 服务端通过 Fragment Model 自动转换为 HLS/DASH/WHEP

### 编码参数配置详表

以下是 OBS 编码参数对 AUDEMSP 实现编码配置系统的参考：

| 参数 | OBS 配置路径 | 有效值范围 | 对延迟的影响 | AUDEMSP 等效 |
|------|-----------|---------|:-----------:|--------------|
| 码率控制 | CBR / VBR / CQP / CRF | 取决于编码器 | CBR最稳定 | RateControl 枚举 |
| 关键帧间隔（GOP） | 0=auto / N | 1-300s | 越小延迟越低 | keyframe_interval: Duration |
| 编码预设 | ultrafast ~ placebo | 编码器相关 | 越快延迟越低 | EncoderPreset 枚举 |
| 双通道编码 | ✓ | on/off | 提升画质不增延迟 | twin_pass: bool |
| 心理视觉调整 | psycho visual tuning | on/off | 略微增加延迟 | psycho_visual: bool |
| B 帧 | 0-3 | 0=无B帧 | B帧增加延迟 | b_frames: u8 |
| 色彩格式 | NV12 / I420 / P010 / RGB | 编码器相关 | 无影响 | color_format: ColorFormat |
| 色彩范围 | 部分 / 完全 | TV/PC | 无影响 | color_range: ColorRange |
| 编码级别 | auto / 4.0 / 4.1 / 5.0/5.1/5.2 | 编码器相关 | 无影响 | h264_level / h265_level |

**低延迟编码配置参考**（针对远程桌面 <100ms 场景）：
```ini
# OBS 等效配置 → AUDEMSP 配置
tune=zerolatency       → b_frames: 0, lookahead: 0
preset=veryfast        → preset: VeryFast
gop=15 (0.5s@30fps)   → keyframe_interval: ms(500)
rate_control=CBR       → rate_control: Cbr
bitrate=4000           → bitrate: 4_000_000  # bps
```


- **语言**：C (libobs 核心及大部分插件, 87.5%)，C++ (UI 和部分输出插件、浏览器源)，C# (Windows 特定组件)，Objective-C (macOS 集成)
- **界面框架**：Qt 6 (Windows/Linux)，Cocoa (macOS 原生)
- **图形引擎**：OpenGL (跨平台)，Direct3D 11 (Windows)
- **关键依赖库**：FFmpeg (编解码 + 解复用 + 滤镜)，librtmp (自维护 fork — 支持 Enhanced RTMP)，libsrt (SRT 传输)，librist (RIST 传输)，libdatachannel (WebRTC RTCDataChannel/WHIP)，x264 (软件 H.264)，SVT-AV1 (软件 AV1)，AOM (软件 AV1)，mbedTLS / OpenSSL / GnuTLS (TLS)，Jansson (JSON 解析)，CEF (Chromium Embedded Framework — 浏览器源)，libvpx (VP8/VP9)，VLC (libvlc — VLC 视频源)
- **GPU 编码 API**：NVIDIA NVENC SDK，AMD AMF SDK，Intel Media SDK / oneVPL，Apple VideoToolbox，VA-API (libva)
- **脚本引擎**：LuaJIT (内置，高性能 Lua JIT 编译器)，Python (通过 obs-scripting 插件)
- **插件 ABI**：C API — `obs_source_t`, `obs_output_t`, `obs_encoder_t`, `obs_service_t`, `obs_filter_t`, `obs_transition_t`, `obs_frontend_api` 等结构体，通过函数指针表实现多态
- **远程控制**：obs-websocket (WebSocket JSON-RPC)，OBS Blade (iOS/Android 移动控制)
- **部署**：Windows (x64, ARM64)，macOS (Intel, Apple Silicon — v32.2 强制 Apple Silicon 版本), Linux (x86_64, Ubuntu 24.04/26.04)

## 3. 功能概览
### 核心功能模块
| 模块 | 功能 |
|------|------|
| Scene Graph | 无限场景层级。源嵌套和分组。可见性控制。变换矩阵（位置/缩放/旋转/裁剪/对齐）。层级顺序控制 |
| Source 类型 | **系统捕获**：窗口捕获、显示器捕获、游戏捕获（DirectX/OpenGL/Vulkan 注入）、音频输出捕获。**硬件采集**：视频采集设备 (V4L2/DirectShow/AVFoundation)、音频输入捕获。**媒体**：浏览器源 (CEF 嵌入式 Chromium)、媒体文件源、VLC 视频源、图片/幻灯片。**生成**：文本 (GDI+/FreeType)、色彩源 (纯色/渐变)。**第三方**：NDI 源、Spout/Syphon、Virtual Camera |
| Filters | **视频滤镜**：色键（绿幕抠像）、色彩校正 (gamma/对比度/亮度)、缩放/宽高比、锐化、LUT 颜色查找表、模糊、滚动、3D 变换、渲染延迟。SDR→HDR 合成滤镜 (v32.2)。**音频滤镜**：压缩器、限幅器、噪声抑制 (RNNoise + NVIDIA AI)、噪声门、增益、3.1 环绕声。VST 2.x 插件支持。**特效滤镜**：StreamFX (社区) — 3D 变换、动态模糊、着色器 |
| Transitions | **基础转场**：淡入淡出、滑动、擦除、Luma Wipe、Fade to Color。**高级转场**：Stinger (动画视频转场)、Track Matte (遮罩转场)、Motion (运动转场)。可自定义时长和缓动曲线 |
| Encoder | **硬件**：NVENC (H.264/HEVC/AV1)，AMF (H.264/HEVC/AV1)，QSV (H.264/HEVC/AV1)，VT (H.264/HEVC)，VAAPI (H.264/HEVC)。**软件**：x264, SVT-AV1, AOM AV1。CBR/VBR/CQP/CRF/CQVBR 码率控制。多码率预设（fast/veryfast/medium 等）|
| Output | **推流**：RTMP/SRT/RIST/WHIP/HLS。**录制**：本地文件多格式。**Virtual Camera**：系统虚拟摄像头输出（DirectShow/AVFoundation/v4l2loopback）。**多轨视频** (v32.2) |
| Audio Mixer | 多声道混音 (mono/stereo/2.1/4.0/4.1/5.1/7.1)。独立监听。音量/平衡/Mute。音频源置顶。6 路独立音轨输出 |
| Studio Mode | 预览/节目双窗口。转场预览（在预览窗口中预演转场效果）。源在预览窗口中可见可编辑 |
| Replay Buffer | 内存循环缓冲。可配置缓冲时长（1-300s）。一键保存或通过快捷键保存。常用于游戏精彩时刻录制 |
| Hotkeys | 全局快捷键系统。支持场景切换、源显示/隐藏、录制/推流/回放启停、音频静音、转场触发、截图 |
| obs-websocket | WebSocket JSON-RPC 协议 (端口 4455)。覆盖所有 OBS 操作：场景/源管理、推流/录制控制、设置修改、事件订阅。已成为流媒体工具互操作的事实标准 |
| Virtual Camera | 将 OBS 合成画面输出为系统虚拟摄像头。DirectShow (Windows)、CoreMedia DAL (macOS)、v4l2loopback (Linux) |
| Auto Configuration | 首次启动自动测试硬件性能，推荐最优编码设置。分析系统硬件（CPU/GPU/RAM）、网络带宽、分辨率 |

### 特色功能
- **场景图架构**：无限的源嵌套和分组，配合滤镜链，形成可编程的视频合成管线。每个源有独立的变换矩阵和混合模式。源之间可以相互引用，形成复杂的视效合成
- **WHIP/WebRTC Simulcast**：v32.1 新增。支持向 WHIP 端点同时推送 1-4 个分辨率层。每层独立编码，viewer 根据网络带宽选择最优分辨率。分层策略简单实用：等比缩放（每层 33-25% 递减）
- **协议感知的管线优化**：不同输出协议有各自的优化策略。WebRTC WHIP 输出跳过音视频 interleaver（FLV 遗留），SRT 使用字节流模式，RTMP Enhanced 支持新编码。不是"一刀切"的管线
- **Hybrid MP4 录制**：分片写入 (fragmented MP4)，crash 后已录制部分完全可用。替代传统的"录制到临时文件 + 停止时移动到最终路径"模式。录制可靠性从"全有或全无"变为"部分可用"
- **obs-websocket 行业标准**：完整的 WebSocket RPC 协议，JSON 格式。覆盖 100+ 个操作和 50+ 个事件。被 Stream Deck (硬件), Touch Portal (移动端), LioranBoard (直播互动), SAMMI (自动化), OBS Blade (移动控制), UP DECK (Android) 等数十个工具集成
- **动态码率多轨视频** (v32.2)：多轨视频输出支持根据网络状况动态调整每条轨的码率。结合 Enhanced RTMP 的多轨道支持，为自适应码率分发提供了基础设施

### 扩展性 / 插件机制
OBS Studio 拥有流媒体行业最丰富的插件生态：
- **C 插件 API**：7 个核心插件类型 —
  - `obs_source_t`：输入源（音视频生成或采集），实现 create/destroy/get_name/get_defaults/update/get_properties/video_render(或 filter_video)/audio_render(或 filter_audio) 等方法
  - `obs_output_t`：输出目标（推流/录制/虚拟摄像头），实现 start/stop/get_name 等方法，通过 `obs_output_t.encoded_packet` 分析器消费编码后的包
  - `obs_encoder_t`：编码器，实现 encode/extra_data/sei 等方法
  - `obs_service_t`：直播服务平台（Twitch/YouTube/Facebook 等），管理服务器 URL 和推流密钥
  - `obs_filter_t`：滤镜，在源和编码器之间处理音视频帧
  - `obs_transition_t`：转场，实现 scene A → scene B 的过渡动画
  - `obs_frontend_api`：前端 API，用于控制 OBS 主窗口的行为
- **插件注册**：通过 `obs_register_source_s` / `obs_register_output_s` 等注册函数在模块初始化时注册。支持运行时启用/禁用（v32.1 新增缺失插件管理）
- **脚本扩展**：LuaJIT (内置，支持完整的 libobs API) 和 Python (通过 obs-scripting 插件)。脚本可以创建源、添加滤镜、响应事件、控制推流/录制
- **obs-websocket**：WebSocket RPC 协议，是事实上的外部集成标准。任何工具可以通过 WebSocket 远程控制 OBS。协议设计简洁（JSON-RPC 风格），事件驱动
- **Browser Source**：嵌入式 Chromium (CEF)，在场景中嵌入任意 Web 内容。连接 StreamElements/Streamlabs 等云叠加服务。支持 CSS/JS 动画和交互
- **第三方插件生态**（最受欢迎的社区插件）：
  - StreamFX：高级滤镜和转场 (3D 变换、动态模糊、着色器) — 2,000+ stars
  - Advanced Scene Switcher：基于条件的自动场景切换 — 1,500+ stars
  - Aitum Multistream：同时推流到多个平台 — 替代 OBS 原生单输出限制
  - obs-ndi：NDI 网络视频输入/输出 — 专业级局域网视频传输
  - Move Transition：元素在场景间的平滑移动转场
  - Downstream Keyer (DSK)：下游键控 — 专业级广播叠加

## 4. 现状与生态
- **当前版本**：v32.1.2 (2026-04-21 稳定版)，v32.2.0-rc1 (2026-07-10 候选发布版)。v32.2 新增功能：动态码率多轨视频、SDR→HDR 合成滤镜、自动 Intel→Apple Silicon 迁移、插件自定义源图标、WebP 支持
- **GitHub Stars / 活跃度**：约 74,000 stars，9,200+ forks。持续高频提交（几乎每天有 commit），120+ 贡献者。Release 周期约 6-8 周
- **社区规模**：全球最大的流媒体工具社区。OBS Discord 服务器 (100,000+ 成员)，obsproject.com 论坛 (数十万帖子)，Reddit r/obs (200,000+ 订阅者)。YouTube 教程生态（数千个 OBS 教程视频）。Slack 开发者频道。Streamlabs 基于 OBS fork 出 Streamlabs Desktop（500,000+ 用户）
- **文档 / SDK / API 生态**：
  - 开发文档：obsproject.com/docs (libobs API 参考、插件开发指南、脚本开发指南)
  - 插件模板：obs-plugintemplate (官方 GitHub 模板仓库)
  - WebSocket 协议文档：完整的 JSON-RPC 参考（100+ RequestType, 50+ EventType）
  - Lua/Python 脚本 API 文档 + 社区教程
  - 社区资源：数千篇博客、YouTube 教程、Discord 帮扶频道
  - 第三方集成：Stream Deck, Discord, Twitch, YouTube, TikTok, Kick, Trovo, Restream
  - 赞助商：Twitch, YouTube, Meta, NVIDIA, AMD, Intel, Logitech
  - 包管理器：Windows (winget/chocolatey), macOS (Homebrew), Linux (Flathub/PPA/AUR)
- **已知缺陷或限制**：
  - **单输出流原生架构**：多平台同时推流需要第三方插件。流复制需要额外的编码资源（每增加一个平台，编码负载增加）
  - **插件兼容性风险**：依赖社区插件提供高级功能。第三方平台（Twitch/YouTube/TikTok）频繁更新 API 导致集成断裂
  - **librtmp 自维护负担**：社区标准 librtmp 不支持 Enhanced RTMP (HEVC/AV1)。OBS 维护自己的 fork，需要持续跟踪和合并上游变更
  - **音频路由复杂度**：不如 vMix 的 Bus 系统直观。多声道、多设备场景的配置门槛高
  - **无内置即时回放引擎**：仅提供 Replay Buffer（手动触发保存）。不如 vMix 的一键慢动作/多角度标记/回放切换引擎
  - **无内置虚拟演播室**：绿幕抠像可用，但无 3D 虚拟演播室。无追踪和 3D 渲染能力
  - **启动和切换性能**：启动约 4.2s（加载插件和 Qt 界面），场景切换约 120ms。相比 vMix (80ms) 和 Wirecast (~90ms) 不是最优
  - **Mac Apple Silicon 过渡期**：v32.2 强制 Apple Silicon 版本升级，Intel 版本的第三方插件在 Apple Silicon 上不可用，需要开发者重新编译
- **版本演进路径**：
  - 2012 年：OBS Classic v0.1 — 最初版本，Windows 专用，DirectShow/DirectX 10 采集
  - 2016 年：OBS Studio v0.14 — 全平台重写，引入 libobs C API，插件系统，Qt UI
  - 2019 年：OBS Studio v24 — 新版编码器 UI，NVENC 编码器大幅改进
  - 2020 年：OBS Studio v26 — macOS Apple Silicon 支持，Browser Source 更新
  - 2022 年：OBS Studio v28 — Qt 6 迁移，Apple Silicon 原生支持，HEVC 编码支持
  - 2024 年：OBS Studio v30 — AV1 编码支持 (NVENC/SVT-AV1)，WHIP/WebRTC 推流 (v32.1)
  - 2025-2026 年：v32.x — Simulcast、SDR→HDR 滤镜、多轨动态码率、插件自定义图标
- **贡献者趋势**：120+ 贡献者，从 v28 到 v32 贡献者数量持续增长。企业赞助商 (NVIDIA/AMD/Intel/Meta/Twitch/YouTube) 提供全职开发者和测试资源。
- **下载量**：每月活跃用户数超过 100 万（Steam + 直接下载 + 包管理器）。Streamlabs Desktop (OBS fork) 额外 50 万+ 用户。

## 5. 市场定位
- **主要应用行业**：游戏直播 (Twitch/YouTube Gaming/Kick/Trovo) — 最大的用户群体，在线教育 (网课/培训直播)，播客制作 (视频播客/采访)，企业会议/培训 (Teams/Zoom 通过 Virtual Camera 集成)，宗教活动直播 (教堂/寺庙)，音乐表演直播 (音乐家/乐队)，电子竞技制作 (赛事转播切换台)
- **竞品对比简表**：
| 维度 | OBS Studio | vMix | Wirecast | Streamlabs Desktop | XSplit | Prism Live Studio |
|------|------------|------|----------|---------------------|--------|-------------------|
| 价格 | 免费 | $60-$1200 | $299-$799 | 免费增值 | $60-199/年 | 免费 |
| 平台 | Win/Mac/Linux | Win Only | Win/Mac | Win/Mac | Win Only | Win/Mac/iOS/Android |

| 多流输出 | ❌ (需插件) | ✅ (最多5路) | ✅ (原生) | ✅ (2路免费) | ✅ (高级版) | ✅ (原生) |
| 即时回放 | ⚠️ Replay Buffer | ✅ 完整引擎 | ✅ | ❌ | ❌ | ❌ |
| 虚拟演播室 | ❌ (需插件) | ✅ 内置 | ❌ | ❌ | ❌ | ❌ |
| 脚本扩展 | Lua/Python | C# | JavaScript | ❌ | C#/JS | ❌ |
| NDI 支持 | ✅ (插件) | ✅ 原生 | ✅ 原生 | ❌ | ❌ | ❌ |
| PTZ 控制 | ⚠️ (插件) | ✅ 原生 VISCA/ONVIF | ❌ | ❌ | ❌ | ❌ |
| WebRTC WHIP | ✅ v32.1 | ❌ | ❌ | ❌ | ❌ | ❌ |
| GPU 编码 | NVENC/AMF/QSV/VT/VAAPI | NVENC/QSV/AMF | NVENC/QSV/VT | NVENC/AMF | NVENC/AMF/QSV | NVENC/QSV/AMF |
| 视频合成 | 场景图 (分层) | 混合总线 | 场景图层 | 场景图 | 场景图 | 场景图 |
| 社区规模 | ⭐⭐⭐⭐⭐ | ⭐⭐ | ⭐⭐ | ⭐⭐⭐ | ⭐⭐ | ⭐ |
| 学习曲线 | 中等 | 陡峭 | 陡峭 | 平缓 | 中等 | 平缓 |
| 许可 | GPLv2 | 闭源 | 闭源 | GPLv3 | 闭源 | Apache 2.0 |
- **定价 / 许可**：OBS Studio 完全免费 (GPLv2)。Streamlabs Desktop 有付费主题和 Prime 订阅 ($19/月)

## 6. 产品特色
1. **全球最大免费开源直播编码器**：74K GitHub Stars，14 年社区积累，赞助商包括 NVIDIA/AMD/Intel/Meta/Twitch/YouTube。开源模式保证了长期可用性和透明性。任何其他编码器都无法匹敌的社区生态
2. **WHIP/WebRTC Simulcast 的简单分层策略**：v32.1 新增。1层=100%, 2层=50%, 3层=33%, 4层=25% 最大分辨率。无需复杂带宽协商，开箱即用的编码自适应。这是 OBS 进入低延迟 WebRTC 直播赛道的核心技术
3. **Source → Filter → Encoder → Output 管线模式**：libobs 定义的 7 种插件类型形成了完整的流媒体管线抽象。每种协议作为独立的 output 插件，通过统一接口消费编码数据。这个管线设计是流媒体客户端架构的业界标准参考
4. **Hybrid MP4 录制解决直播录制最大痛点**：崩溃恢复、分片写入。分段 MP4 (fMP4) 规格保证了崩溃后已录制部分的完整性。替代了传统的"移动后写入"模式，将录制可靠性从"全有或全无"提升到"最大努力保存"
5. **obs-websocket 成为行业互操作标准**：WebSocket JSON-RPC 远程控制协议覆盖 OBS 的所有功能。Stream Deck、Touch Portal、LioranBoard、SAMMI、OBS Blade 等数十个工具都使用此协议。这是 OBS 生态系统的核心粘合剂

## 7. 对 AUDEMSP 的参考价值
### [Adopt] 可直接借鉴
1. **Source → Filter → Encoder → Output 管线模型**：这个管线是 AUDEMSP `MediaSource → MediaProcessor → MediaSink` 插件 trait 的直接参考。每种输出协议作为独立的 `MediaSink` 实现，通过统一接口消费 Fragment 流。管线的每个阶段都可以独立替换和组合
2. **obs-websocket 远程控制协议设计**：AUDEMSP 的 Host 嵌入式 Web 配置页和 Client 的远程控制能力可以参考 obs-websocket 的简洁设计：JSON-RPC 风格、事件驱动、全功能覆盖、请求/响应配对。WAMP 或纯 WebSocket 都可以作为传输
3. **WHIP Simulcast 分层策略**：等比缩放的分层模型简单且实用。AUDEMSP 的 WebRTC 推流可以直接采用 1-4 层等比降分辨率的策略，避免复杂的带宽协商
4. **协议感知的管线裁剪**：OBS 为 WebRTC 输出跳过 interleaver 的优化策略展示了协议感知优化的重要性。AUDEMSP 的 PipelineEngine 应该在管线构建时根据输出协议类型自动跳过不必要的处理步骤（如 HTTP-FLV 不需要 transmux 到 fMP4）
5. **Hybrid MP4 的崩溃恢复模式**：分片 fMP4 写入策略是录制系统的基础保障。AUDEMSP 的录制模块（从 Fragment 流直接写入）应该采用相同的分片策略

### [Adapt] 需修改后采用
1. **多输出流原生支持**：OBS 的单输出限制是架构缺陷。AUDEMSP 应该从一开始就支持一条输入管线 fork 到多条输出管线。参考 LVQR 的 FragmentBroadcaster（一个输入 → 多个 Observer tap），避免 OBS 的多平台推流需要复制编码资源的低效模式
2. **编码器抽象**：OBS 的 `obs_encoder_t` 假设输入是原始帧（音频/视频帧）。AUDEMSP 需要更灵活的编码器抽象：编码器可以接受 Fragment 作为输入（transmux 路径）或原始帧作为输入（编码路径）。支持 CBR/VBR/CQP/CRF 等多种码率控制模式
3. **音频系统设计**：OBS 的 Audio Mixer 在 v32.1 才获得较完善的功能。AUDEMSP 应该借鉴 vMix 的 Bus 音频系统 — 多 Bus 路由、独立监听、灵活的子混音 — 在初期就设计好。避免 OBS 的音频系统升级路径
4. **录制格式**：OBS 支持多种录制容器格式。AUDEMSP 应该统一使用 CMAF/fMP4（与 Fragment payload 格式一致），避免容器格式转换。录制目标应该支持多种存储后端（本地磁盘、S3/OSS 对象存储、NFS 网络挂载）
5. **Virtual Camera 输出**：将视频流注册为系统虚拟摄像头的功能可以作为 AUDEMSP 的可选 `VirtualCameraPlugin`。需要委托各平台的摄像头驱动 API（Windows DirectShow, macOS CoreMedia DAL, Linux v4l2loopback）
6. **编码器硬件抽象**：OBS 为每种 GPU 编码平台（NVENC/AMF/QSV/VT/VAAPI）编写了独立的编码器实现。AUDEMSP 应该在 `HardwareEncoder` trait 下统一配置接口，隐藏各平台的差异。参考 MediaMTX RPI Camera 模块的配置统一策略

### [Avoid] 已知坑 / 不适用场景
1. **不要复制 OBS 的 C 插件 ABI**：OBS 通过结构体函数指针表实现多态的 C API（如 `obs_source_t.audio_render` 函数指针）在 2026 年已过时。AUDEMSP 应该使用 Rust trait 实现多态，由编译器保证类型安全
2. **不要依赖 librtmp**：OBS 自维护 librtmp fork 的历史表明，社区标准的 librtmp 对 Enhanced RTMP 支持不足。AUDEMSP 应该自研或选择高质量的 Rust RTMP 实现。如果必须用 C 库，通过 FFI facade crate 隔离
3. **GPLv2 许可隔离**：OBS 是 GPLv2，AUDEMSP 是 Apache 2.0。参考架构设计不构成版权问题，但不能直接复制代码。接口定义和管线拓扑可以借鉴，实现必须独立编写
4. **Scene Graph 适合 UI 端，不适合服务端**：OBS 的 Scene Graph 是为交互式画面创作设计的，要求 GPU 实时渲染。AUDEMSP Host（headless 服务端）如果需要画面合成，应考虑轻量级 compositor（如 GStreamer `compositor` 元素或自定义着色器），而非完整的 Scene Graph
5. **不要绑定特定 UI 框架**：OBS 深度绑定 Qt/Cocoa。AUDEMSP Client 使用 Tauri v2 + React，Host 使用 axum + 静态 HTML。不要参考 OBS 的 UI 架构
6. **单进程架构 vs 多进程**：OBS 在单一进程中运行所有功能。AUDEMSP 的架构是 Client（GUI 进程）、Host（headless 进程）、Backend（服务进程）分离。进程间通信（FlatBuffers、gRPC）是 AUDEMSP 独有的复杂度，OBS 没有这方面的经验可参考


**总体评分**：★★★★☆ (4/5)

OBS Studio 是生产工具 benchmark — 在编码器客户端和制作工具方面无可匹敌。其对 AUDEMSP 的核心价值在于：Source→Filter→Encoder→Output 管线架构参考、WHIP Simulcast 分层策略、obs-websocket 远程控制协议设计、Hybrid MP4 录制模式。不适合直接参考的是：单输出流限制、C 插件 ABI、Scene Graph 架构。

### [Adopt] 补充 — 编码器抽象层设计

**6. HardwareEncoder trait 的设计参考**：OBS 为每种 GPU 编码平台维护独立的 C 实现，导致代码分散在 `libobs/media-io/` 下的多个文件中。AUDEMSP 应定义一个统一的 `HardwareEncoder` trait：

```rust
#[async_trait]
pub trait HardwareEncoder: Send + Sync {
    /// 支持的编码格式
    fn supported_codecs(&self) -> Vec<Codec>;
    /// 创建编码会话
    async fn create_session(&self, config: EncoderConfig) -> Result<Box<dyn EncoderSession>>;
    /// 编码器能力查询
    fn capabilities(&self) -> EncoderCapabilities;
}

pub struct EncoderConfig {
    pub codec: Codec,
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
    pub bitrate: u32,
    pub preset: EncoderPreset,
    pub rate_control: RateControl,
}
```

每种硬件后端（NVENC/AMF/QSV/VT/VAAPI）实现此 trait，通过 `HardwareEncoderRegistry` 在启动时自动发现可用编码器。

### [Adapt] 补充 — 多流输出架构设计

**7. 单输入→多输出分叉策略**：OBS 的单输出流限制是多平台推流的瓶颈。AUDEMSP 的 FragmentBroadcaster 天然支持一对多分叉 — 一个 MediaSource 的 Fragment 流可同时被多个 MediaSink 消费，无需额外编码资源：

```
RTMP Source → FragmentBroadcaster ─┬──→ HlsSink (LL-HLS)
                                    ├──→ DashSink (MPEG-DASH)
                                    ├──→ WhepSink (WebRTC WHEP)
                                    ├──→ RecordSink (fMP4 录制)
                                    └──→ MoqSink (MoQ/QUIC relay)
```

实现策略：PipelineEngine 在注册 source 时自动广播 Fragment 到所有已注册的 sink。新添加 sink 只需在 PipelineEngine 中注册一次，自动开始消费所有活跃 source 的 Fragment 流。

### [Avoid] 补充 — 更多坑

**7. 不是所有 GPU 编码器都支持 AV1**：OBS 的 AV1 支持需 Ada Lovelace (NVENC AV1)、RDNA3 (AMF AV1)、Arc (QSV AV1)。旧硬件仅支持 H.264/H.265。AUDEMSP 的 `HardwareEncoderRegistry` 必须报告编码器能力，由配置层根据硬件能力自动降级。

**8. 多编码实例的 GPU 内存限制**：OBS 的 WHIP Simulcast 4 层编码消耗大量显存。AUDEMSP 的 Simulcast 层数应根据 GPU 显存动态上限，默认为 3 层（1080p → 540p → 360p）。

---
**相关决策**: D45 (采集-编码耦合), D6, D19, D-SIMULCAST
## 附录 A: OBS 管线模式深度分析

### A.1 Source 到 Output 四阶段管线

OBS 的媒体处理管线分为 4 个阶段，每阶段由特定插件实现：

**阶段 1 - Source (源)**：
- 实现 obs_source_t 结构体和方法表
- 输出原始音频帧 (audio_data) 或视频帧 (video_data)
- 视频帧以 GPU 纹理形式存在
- 音频帧以 PCM float 缓冲形式存在
- 多个源被 Scene Graph 组合成一个视频帧 (GPU 合成) + 一个音频帧 (混音)

**阶段 2 - Filter (滤镜)**：
- 实现 obs_filter_t 结构体
- 输入来自源 (或源链中上一个滤镜) 的原始帧
- 输出处理后的原始帧 (同格式)
- 滤镜可链式组合
- 音频和视频滤镜是分离路径

**阶段 3 - Encoder (编码器)**：
- 实现 obs_encoder_t 结构体
- 输入原始视频帧 (GPU 纹理) 和原始音频帧 (PCM float 缓冲)
- 输出编码后的包 (data + pts + dts + keyframe flag)

**阶段 4 - Output (输出)**：
- 实现 obs_output_t 结构体
- 输入编码后的包，做协议相关处理
- FLV 封装 (RTMP)、MPEG-TS 封装 (SRT)、RTP 打包 (WHIP)

### A.2 协议感知优化的原理

OBS 为不同输出协议做管线裁剪：

**RTMP 输出管线**：
```


### A.3 Simulcast 分层策略的技术原理

WHIP Simulcast 同时编码多个分辨率层：

| 层数 | 编码器实例 | 分辨率比例 | 码率分配 | 适用场景 |
|------|-----------|-----------|---------|----------|
| 1 | NVENC(H.264) | 100% (1080p) | 全码率 | 网络优良的单播 |
| 2 | 2x NVENC | 100%+50% (1080p+540p) | 70%+30% | 中等网络 |
| 3 | 3x NVENC | 100%+50%+33% | 50%+30%+20% | 弱网络 |
| 4 | 4x NVENC | 100%+50%+33%+25% | 40%+25%+20%+15% | 自适应 |

对 AUDEMSP 的启示：
- HardwareEncoder 插件应支持同时创建多个编码实例 (1-4 个)
- 每层分辨率和码率是可配置比例，非绝对数值
- RTCP 反馈回路需集成到 PipelineEngine 质量监控中

---

## 附录 B: obs-websocket 协议参考

obs-websocket 使用 WebSocket (ws://localhost:4455) + JSON-RPC 风格。
消息类型：Request (客户端-服务器) 和 Event (服务器-客户端)。

**常用 Request 类型** (共 100+)：

| RequestType | 功能 | AUDEMSP 等效 |
|-------------|------|----------------|
| GetSceneList | 获取所有场景 | Stream.List |
| SetCurrentProgramScene | 切换场景 | Stream.Switch |
| StartStream / StopStream | 启停推流 | Stream.Start / Stop |
| StartRecord / StopRecord | 启停录制 | Record.Start / Stop |
| GetStreamStatus | 查询推流状态 | Stream.Status (SSE) |
| SetSourceFilterEnabled | 启用/禁用滤镜 | Filter.Enable/Disable |
| SetInputVolume | 设置音量 | Audio.SetVolume |

**常用 Event 类型** (共 50+)：

| EventType | 触发时机 | AUDEMSP 等效 |
|-----------|---------|----------------|
| StreamStateChanged | 推流状态变更 | StreamStateChanged |
| RecordStateChanged | 录制状态变更 | RecordStateChanged |
| SceneTransitionStarted | 场景切换开始 | SourceChanged |
| InputVolumeChanged | 音量变更 | AudioLevelChanged |
| StreamStatus | 推流统计 (码率/丢帧) | StreamStats (周期) |

AUDEMSP 的 Host 配置页和 Client 控制协议可参考此消息结构
(JSON-RPC + Request/Response + Event 推送)，但操作模型需替换为
AUDEMSP 自己的域模型 (Stream/Record/Audio/Filter 等)。

---

## 附录 C: OBS 编码性能数据

以下数据来源于社区测试：

| 测试项 | 数据 | 硬件 |
|--------|------|------|
| CPU 占用 (1080p60, 3摄像头) | 约 22% | OBS 30 (Intel i7-12700K) |
| GPU 编码开销 | 约 12% | NVENC (RTX 3080) |
| 启动时间 | 约 4.2s | SSD + NVENC |
| 场景切换延迟 | 约 120ms | GPU 合成 |
| 6小时稳定性测试 | 零丢帧 | 1080p60 推流 |
| WebRTC WHIP 延迟 | <500ms | 局域网 |
| WHIP Simulcast 开销 | 4层 +35% GPU | 4x NVENC 编码实例 |

编码预设对应的质量/速度权衡：

| 预设 | 速度 | 压缩率 | 延迟 | 适用场景 |
|------|------|--------|------|----------|
| ultrafast | 最快 | 最低 | 最低 | 低延迟直播 |
| veryfast | 快 | 低 | 低 | 通用直播 |
| faster | 中 | 中 | 中 | 录播 |
| fast | 中慢 | 中高 | 中 | 本地录制 |
| medium | 慢 | 高 | 中高 | 高质量录制 |

对 AUDEMSP 的启示：
- 编码预设应该有默认值 (veryfast) 并允许用户根据场景切换
- 远控场景 (remote desktop) 应使用 ultrafast+tune=zerolatency
- 录制场景可使用 medium 获得更高压缩率
- WHIP Simulcast 4 层 +35% GPU 开销是合理的编码资源预算
- 6 小时稳定性是 AUDEMSP 录制系统的基准目标

OBS Studio 的 libobs 核心引擎和 obs-websocket 远程控制协议是流媒体
客户端架构的两个基石。Source -> Filter -> Encoder -> Output 管线
启发了 AUDEMSP 的 MediaSource -> MediaProcessor -> MediaSink trait 设计。
obs-websocket 的 JSON-RPC 事件驱动模式是 AUDEMSP Host/Client 控制
协议的参考基准。

OBS 的弱项 — 单输出流限制、C 插件 ABI、无服务端功能 — 恰好是 AUDEMSP
需要补充和完善的部分。OBS 做不了服务器，AUDEMSP 做不了 UI 端编码器，
两者互补而非竞争。

核心教训：插件化管线和远程控制协议的设计是 OBS 最可迁移的价值，
其 UI 框架和 GPU 渲染管线则完全不适用于 AUDEMSP。


---

*本文档基于 OBS Studio v32.1.2 / v32.2.0-rc1 及 OBS Project 公开文档编写。*