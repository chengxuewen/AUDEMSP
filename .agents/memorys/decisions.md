# OMSPBase 架构决策记录

## D1: 三层部署拓扑架构

**决策**: 后台服务控制面 + Client/Host 数据面 + SDK 层
**日期**: 2026-07-16
**原因**: 
- 控制面与数据面分离，后台集中管理权限/License/信令
- Client 和 Host 分别针对有/无 GUI 环境
- SDK 分层实现核心共享、领域特化

## D2: Client + Host 双应用

**决策**: 双应用而非单应用
**日期**: 2026-07-16
**原因**:
- Host 需要运行在无桌面环境的平台（Linux 服务器、车端边缘设备）
- 双应用可减少 Host 的依赖体积（无需 GUI 框架）
- 主流的单应用模式（TeamViewer/RustDesk）无法满足 headless 部署需求

## D3: 微内核 + 插件体系

**决策**: omspbase-core 微内核 + 插件层按领域划分
**日期**: 2026-07-16
**原因**:
- 不同部署形态编译不同插件子集（Embed ~5 个，Standalone 全量）
- 插件 trait 清晰定义边界（MediaSource/Processor/Sink）
- 三层 trait 层次：Plugin → 生产/消费/协议 → 具体实现

## D4: Auth 双模式

**决策**: AuthProvider trait 支持 Local 和 AUDEBase 两种实现
**日期**: 2026-07-16
**原因**:
- 独立部署时需要完整账户系统（Local: SQLite + JWT）
- 作为 AUDEBase 模块时委托平台 RBAC/LDAP
- 参考模型：群晖 Surveillance Station、Jira on DSM
- 未来可扩展 LDAP/OIDC/OAuth2 实现

## D5: Unified Fragment Model

**决策**: 采用 LVQR 的 Unified Fragment Model 而非 GStreamer 有向图
**日期**: 2026-07-16
**原因**:
- 避免 N×M 协议转换矩阵
- 所有输入协议适配到统一 MediaFragment，所有输出协议从中投影
- GStreamer 用在其擅长的协议解析/编解码/打包
- Rust 自定义热路径（GPU 编码桥接、sans-I/O WebRTC）

## D6: GStreamer + Rust 混合管线

**决策**: GStreamer 处理协议和标准编解码，Rust 处理热路径
**日期**: 2026-07-16
**原因**:
- GStreamer 覆盖 100+ 编解码器和协议（减少开发量）
- webrtcbin2 (Rust) 每会话节省 5 线程
- videoconvertscale 比 videoconvert+scale 快 2.6×
- Rust libloading 桥接避免编译时绑定 GPU SDK
- str0m / webrtc-rs/rtc sans-I/O 设计灵活

## D7: SDK 分层（core + 领域 SDK）

**决策**: omspbase-core 共享 + 按领域分层 SDK
**日期**: 2026-07-16
**原因**:
- AUDESYS 只需编译 ~2 个领域 SDK（remote + teleop）
- AUDEBase 可加载全部 SDK
- 领域 SDK 是薄封装，组合 core 的插件

## D8: 四形态部署

**决策**: Embed / Sidecar / Standalone / AUDEBase 模块
**日期**: 2026-07-16
**原因**:
- Embed: AUDESYS 静态链接 Rust crate
- Sidecar: AUDEBase 通过 napi-rs 调用容器
- Standalone: 独立进程 + 完整后端
- 模块: Docker 容器安装到 AUDEBase

## D9: 与 AUDEBase 零硬依赖

**决策**: OMSPBase 不依赖 AUDEBase，可完全独立部署
**日期**: 2026-07-16
**原因**:
- 独立场景不强制引入 AUDEBase 依赖
- 作为模块时通过 AuthProvider trait 委托平台服务
- 参考群晖 DSM 模块化架构

## D10: FlatBuffers 内部协议

**决策**: 插件间通信使用 FlatBuffers
**日期**: 2026-07-16
**原因**:
- 零拷贝反序列化
- 多语言支持（Rust/C/TypeScript）
- AUDESYS 已使用 FlatBuffers (D19)


## D11: 客户端传输三后端架构

**决策**: 三种 WebRTC 后端，编译期 `#[cfg(feature)]` 切换
**日期**: 2026-07-16 (更新)
**原因**:
- str0m: sans-I/O 极致轻量，Embed/局域网场景，无运行时依赖
- backend-webrtc-sys (via webrtc-sys): 完整 GCC + FEC + NetEQ，公网弱网/遥控场景 (因 D139 更名自 backend-libwebrtc)
- webrtc-rs (v0.20+ sans-I/O): 未来 Embed 升级目标，W3C API 兼容 + 可选运行时
- 三后端共享同一 `MediaTransport` trait，编译期 feature gate 互斥
- 参考 webrtc-kit 的 `compile_error!` + `RtcEngine::create_factory()` 模式

- Phase 1: 单一 webrtc-sys 后端。str0m/webrtc-sys/webrtc-rs 三后端为 Phase 2+ 架构
## D12: 信令服务混合架构

**决策**: 独立 axum/tonic 信令服务 + LAN P2P fallback
**日期**: 2026-07-16
**原因**:
- 主路径: 独立信令服务 (axum/tonic + WebSocket)，房间管理、SDP/ICE 交换
- Fallback: 局域网内 P2P 信令（mDNS 发现），无服务器场景可用
- 信令服务不绑定任何 SFU 框架

## D13: Plugin 加载混合模式

**决策**: 核心插件编译期 feature flags + 扩展插件运行时 dlopen
**日期**: 2026-07-16
**原因**:
- 必需插件（WebRTC、ScreenCapture、DataChannel）编译进去，零运行时开销
- 第三方/实验性插件走 dlopen 动态加载，ABI 稳定 trait 接口
- 平衡性能与灵活性

## D14: LiveKit 集成深度

**决策**: LiveKit 仅作为纯 SFU 转发插件，可替换
**日期**: 2026-07-16
**原因**:
- OMSPBase 自管信令、房间、认证、权限，LiveKit 只转发 RTP
- 不依赖 LiveKit 信令、Egress、Ingress、录音
- 不影响未来换 mediasoup 或自研 SFU

## D15: Phase 1 里程碑 (⚠️ 范围变更 D118: P2P → Server relay)

**决策**: 第一个里程碑为遥控座舱推拉流
**日期**: 2026-07-16
**原因**:
- P2P 优先，不需要 SFU，最小化第一阶段
- 验证 libwebrtc P2P + DataChannel 控制 + 信令服务
- 双向验证：推流 + 控制，一个场景验证两个核心能力

## D16: Cargo Workspace 混合结构

**决策**: 核心 2-3 crate + 领域 domain SDK crates
**日期**: 2026-07-16
**原因**:
- omspbase-core: 微内核 + 公共 trait + PluginManager + PipelineEngine
- omspbase-transport: 传输 trait + 三后端 feature gate
- omspbase-streaming/remote/teleop/surveillance/capture: 按产品能力展开

## D17: TURN/STUN 方案

**决策**: Phase 1 使用 coturn 外部部署，后续评估自研
**日期**: 2026-07-16
**原因**:
- coturn 是业界标准，Docker 部署即用
- P2P 必须 NAT 穿透，先用 coturn 顶
- Phase 2+ 根据实际需求评估自研 Rust TURN

## D18: Web 客户端

**决策**: 自研薄层 JS（~500行），浏览器原生 RTCPeerConnection，不绑 SFU 框架
**日期**: 2026-07-16
**原因**:
- 浏览器已内置 libwebrtc (RTCPeerConnection API)
- 只需要 JS 桥接层将信令消息翻译为浏览器 API
- 不依赖 LiveKit JS SDK，SFU 可随时替换
- Web Viewer 只看流不控车，Native Client 负责完整控制

## D19: GStreamer 渐进 Rust 化

**决策**: Phase 1 用 GStreamer 处理协议转换和编解码，后续逐步 Rust 化核心管线节点
**日期**: 2026-07-16
**原因**:
- GStreamer 覆盖全协议 (RTMP/RTSP/SRT/HLS) 和全编解码器
- Phase 1 快速验证，Phase 2+ 将热路径替换为纯 Rust
- 最终目标: GStreamer 作为可插拔协议适配器，核心管线纯 Rust

## D20: TextureHandle 所有权 + 借用模型

**决策**: GStreamer 式所有权 + 显式借用 (map_readable/map_writable)，内部 Arc 引用计数
**日期**: 2026-07-16
**原因**:
- 远程桌面需要同时渲染 + 编码（管道分叉），Parsec 的独占所有权模型无法满足
- 写入路径（颜色空间转换等）需要独占访问，OBS 的 Arc 共享不处理写入
- GStreamer 的 `map_readable()/map_writable()` 经过 20 年 GPU 路径验证
- 管道节点按需借用 TextureView，释放时自动减引用
- `RawFrame.texture: Option<TextureHandle>` 内部 Arc，克隆开销 O(1)

**对比方案**:
- A. 唯一所有权 (Parsec 模型): 仅一个消费者，编码器消费后销毁 → 不适用分叉场景
- B. 共享引用 (OBS 模型): Arc<TextureHandle> 多消费者只读 → 不支持写入路径

## D21: 时间戳混合方案

**决策**: 轨道时间刻度 (dts/pts/duration) + 可选 NTP 挂钟时间 (wall_clock: Option<Instant>)
**日期**: 2026-07-16
**原因**:
- 推拉流场景: RTMP/SRT 源携带 RTCP SR → NTP 可用 → 精确唇音同步
- 远程桌面场景: 屏幕捕获无 NTP → 仅轨道时间刻度 → 接收端 jitterbuffer 重建播放
- 遥操作场景: 4G 弱网 RTCP SR 不可靠 → 优雅回退到轨道时间刻度
- TrackMeta 携带 timescale (视频 90kHz，音频 48000) 和 start_ntp (第一个样本的挂钟时间)

**对比方案**:
- A. 纯轨道刻度 (LVQR 模型): 简单但无法混合多源 → 适用于单源推流，不适用多源 A/V 同步
- B. 全局 NTP (MediaMTX 模型): 精确但需要 NTP 基础设施 → AUDESYS 嵌入式环境可能无 NTP

## D22: FragmentFlags::discardable 保留

**决策**: 保留 `discardable` 标志位（delta 帧标记为 discardable）
**日期**: 2026-07-16
**原因**:
- HLS/DASH 分段需要区分独立片段 vs 部分片段 (CmafPolicy 检查 discardable 决定段边界)
- SFU 服务器端帧级 QoS: 拥塞时优先丢弃 delta 帧，保留关键帧
- 接收端弱网 jitterbuffer: 延迟峰值时可跳过 discardable 帧追赶播放
- 成本: 1 个布尔位，关键帧/音频为 false — 最便宜的复杂性
- LivKit 的调度器在拥塞期间确实执行帧级优先级，自定义 SFU 插件需要此标志

**参考项目**:
- LVQR: 定义 FragmentFlags::discardable → CmafPolicyState::step() 检查
- GStreamer: GST_BUFFER_FLAG_DELTA_UNIT — 由 rtpjitterbuffer 在接收端使用
- mediasoup: 不使用帧级标记，但 RtpStreamSend::RequestKeyFrame() 在流级别操作

## D23: 管道节点基础 trait

**决策**: NodeInfo（元数据 + 能力声明）+ PipelineNode（生命周期 on_start/on_stop）
**日期**: 2026-07-16
**原因**:
- NodeInfo 声明节点能力（输入/输出格式、资源需求），PipelineEngine 据此做连接决策
- on_start/on_stop 映射 GStreamer 的 NULL→READY→PLAYING 状态转换，处理资源分配/释放
- 不采用 GStreamer 完整 4 状态机，Phase 1 简化：Started/Stopped 两态
- 参考 LVQR 的 Agent/Transcoder trait 的 on_start/on_stop 生命周期

## D24: 三层管道角色 (Source / Processor / Sink)

**决策**: MediaSource::poll_fragment → MediaProcessor::process → MediaSink::on_fragment
**日期**: 2026-07-16
**原因**:
- Producer (拉模式): poll_fragment 由引擎驱动，适合外部数据源（摄像头、网络流）
- Processor (同步变换): process 输入→输出，适合 CPU/GPU 工作（编码、解码、色彩转换）
- Sink (推模式): on_fragment 由上游推送，适合最终消费（渲染、推流、录制）
- 三重角色覆盖所有媒体管道场景，参考 GStreamer 的 BaseSrc/BaseTransform/BaseSink
- 参考 LVQR 的 Agent/Transcoder 模式

## D25: FragmentBroadcaster 扇出机制

**决策**: 基于 tokio::sync::broadcast 的单生产者/多订阅者广播，慢消费者跳过策略
**日期**: 2026-07-16
**原因**:
- 生产者永不阻塞：慢订阅者收到 RecvError::Lagged 并跳过（LVQR 反压策略）
- 订阅者自身是 MediaSource（BroadcastStream），可以链入管道
- 默认缓冲区 1024 个数据包（~1-2 秒 60fps），平衡内存与丢帧风险
- 单个管道节点可同时馈送多个下游（渲染 + 编码 + 录制）
- 参考 LVQR 的 FragmentBroadcaster (broadcast::Sender<Fragment>)

## D26: PipelineRegistry 生命周期钩子

**决策**: 中央注册表 + on_source_added/removed 回调（锁外触发）
**日期**: 2026-07-16
**原因**:
- get_or_create(source_id, track_id) 管理动态轨道（RTMP 重连、摄像头热插拔）
- 钩子在锁外触发：消费者可在回调中安全调用 registry.subscribe() 不会死锁
- 参考 LVQR 的 FragmentBroadcasterRegistry::on_entry_created/removed

## D27: 管道 async 模型

**决策**: poll_fragment 异步等待数据，process/on_fragment 同步执行
**日期**: 2026-07-16
**原因**:
- MediaSource 需要异步等待外部数据（网络、摄像头），poll_fragment 返回 futures
- MediaProcessor 是同步 CPU/GPU 工作（编码、解码），避免 async 开销和 PinBox 复杂性
- MediaSink 是同步消费（推送模式），无需 async
- 对于 CPU 密集型处理，节点内部可选 tokio::spawn 卸载到独立线程
- 参考 LVQR 的 Agent（同步 on_fragment）+ FragmentBroadcaster（异步 broadcast 订阅）
- 参考 GStreamer 的 BaseTransform::transform（同步，由框架线程调度）

## D28: Plugin trait 注册模型

**决策**: struct-of-callbacks 模式，参考 OBS 的 obs_source_info 注册
**日期**: 2026-07-16
**原因**:
- Plugin trait 声明 name/version/kind/capabilities + on_load/on_unload 生命周期
- PluginCapability 在注册时声明（非运行时探测）：node_type, media_type, codecs, pixel_formats
- 参考 OBS 的 obs_source_info（含 required + optional 字段的结构体）
- 参考 GStreamer 的 GST_ELEMENT_REGISTER + PadTemplate 能力声明

## D29: PluginManager 双模式加载

**决策**: 编译时 inventory (inventory crate) + 运行时 dlopen 双模式
**日期**: 2026-07-16
**原因**:
- 编译时：通过 inventory::submit! 在插件注册表中登记，零运行时开销
- 运行时：dlopen → extern "C" fn omspbase_plugin_register() → 添加到 PluginManager
- find_nodes() 按能力查询，PipelineEngine 按优先级选择（GPU > 硬件加速 > 软件）
- 参考 webrtc-kit 的 compile_error! 互斥后端 guard 模式
- 参考 OBS 的 obs_register_source_s（sizeof 大小检查，ABI 安全）

## D30: PluginCapability 声明式能力

**决策**: 注册时声明能力，非运行时协商（OBS output_flags 模式）
**日期**: 2026-07-16
**原因**:
- PluginCapability { node_type, media_type, codecs, pixel_formats }
- codecs 空 Vec = 通配符（穿透节点不关心 codec 类型）
- 避免 GStreamer 的复杂 Pad Caps 协商，Phase 1 保持简单
- 后续可选支持动态能力声明（类似 GStreamer 的 GST_QUERY_CAPS）

## D31: MediaTransport trait (sans-I/O)

**决策**: sans-I/O trait 设计，参考 str0m 的 handle_input/poll_output 模式
**日期**: 2026-07-16
**原因**:
- sans-I/O 使 Embed 场景（AUDESYS 无 tokio 运行时）可用 str0m 后端
- poll_output 返回 TransportOutput::Transmit/Event/Timeout/Nothing
- 三后端共享同一 trait：str0m (原生), libwebrtc (FFI), webrtc-rs (tokio 包装)
- 参考 webrtc-kit 的 PeerConnection trait（对象安全，Send trait）
- SDP：str0m 用类型化 SdpApi，libwebrtc 用不透明 SessionDescription 字符串

## D32: Transport 后端编译期分发

**决策**: 编译期 #[cfg(feature)] 分发，每次编译一个最优后端
**日期**: 2026-07-16
**原因**:
- backend-str0m (默认, Embed/LAN), backend-webrtc-sys (弱网/遥控, 因 D139 更名自 backend-libwebrtc), backend-webrtc-rs (未来)
- 参考 webrtc-kit 的 RtcEngine::create_factory() cfg 分发 + compile_error! 互斥 guard
- 非运行时切换：每个部署场景编译一个最优后端
- 只有 str0m 后端是纯 Rust 零外部依赖，适合 AUDESYS Embed

## D33: sans-I/O run loop 合约

**决策**: 每次变更后必须完全排空 poll_output，再执行下次变更
**日期**: 2026-07-16
**原因**:
- str0m 强制合约：handle_input/write/set_local_description 后必须 drain poll_output
- 保证状态机一致性：事件按序产生，ICE 状态变更不丢失
- 三后端统一行为：即使 libwebrtc 内部有回调线程，poll_output 也按合约排空
- run loop 模式：input → drain → write → drain → poll(timeout) → drain

---

## D34: 录制位置 — 全形态

**决策**: Pipeline 内 RecordingSink + SFU Egress + 客户端本地，三形态并行
**日期**: 2026-07-16
**原因**:
- 不同场景需求不同：远程桌面需要客户端本地录屏，会议需要云端录制
- RecordingSink 实现 MediaSink trait，复用 PipelineEngine 基础设施
- SFU Egress 走旁路 hook（同进程），不额外走 WebRTC 连接
- 客户端本地录制可选异步上传到云端

## D35: 录制粒度 — 合流为主，可选单流

**决策**: 会议场景默认合流录制（单 MP4），可选保留单流原始
**日期**: 2026-07-16
**原因**:
- 行业标准：Zoom、Teams、Meet 全部输出合流 MP4
- 用户体验：打开一个文件即可观看回放
- 存储效率：一路 H.264 合流 vs N 路原始流
- 单流原始作为高级功能可选（用于后期分析）
- 遥操作场景例外：视频+控制指令走分离文件模式

## D36: 合流实现 — GStreamer compositor + GPU 编码

**决策**: SFU 侧通过 GStreamer compositor 合流，NVENC/VAAPI 硬件编码
**日期**: 2026-07-16
**原因**:
- GStreamer compositor 成熟稳定，GPU 加速可用 CUDA 零拷贝路径
- 架构已选 GStreamer 作为编解码引擎，复用现有依赖
- 合流即 Pipeline 中的合成 Processor + 编码 Sink，模型一致
- LiveKit Egress 使用相同的 GStreamer compositor 管线，验证可行

## D37: 容器格式 — fMP4 + splitmuxsink

**决策**: 全线使用 fMP4 容器，GStreamer splitmuxsink 分片
**日期**: 2026-07-16
**原因**:
- 行业标准：所有主流产品输出 MP4/fMP4，无人用 WebM 做交付
- HLS/DASH 原生兼容：fMP4 segment 可直接播放，无需转码
- splitmuxsink 是 GStreamer 原生元素，零开发成本
- moov atom 前置（faststart），segment 写完即可播放
- MKV 仅用于遥操作多轨存档，不用于分发

## D38: Part-Segment 两层文件模型

**决策**: 借鉴 MediaMTX，文件组织为 Segment (1h) + Part (1s) 两层
**日期**: 2026-07-16
**原因**:
- Part 价值：崩溃时最多丢失最后 1 秒（RPO=1s），非整个 Segment
- Segment 价值：按小时组织文件，方便清理、上传、回放
- splitmuxsink 的 max-size-time 天然支持 Segment 边界
- 文件命名模板：session_1730000000/001.mp4 ↔ 002.mp4

## D39: 遥操作录制 — 分离文件 + 可选 SEI 嵌入

**决策**: 默认 MP4 视频 + JSONL 控制日志分离，证据级场景开启 SEI 嵌入
**日期**: 2026-07-16
**原因**:
- 分离文件调试友好（grep JSONL 查控制指令），日常场景足够
- SEI 嵌入提供帧精确绑定，证据级防篡改（SEI 不可与视频帧分离）
- SEI 注入为可选 MediaProcessor，不改架构
- 标准播放器可正常播放含 SEI 的 MP4（忽略 SEI NAL 单元）

## D40: 回放方式 — HLS 流式 + 本地直接回放

**决策**: HLS 流式回放用于 web，本地直接回放用于 Client
**日期**: 2026-07-16
**原因**:
- HLS 流式：录制分片即 HLS segments → nginx/S3 静态服务 → 浏览器 <video> 原生播放
- 本地回放：Client 直接读取本地 fMP4 文件，FrameServer 精确 seek
- 不需要按需转码（Phase 1 无此需求，未来可用 Cloudflare Stream 等方案）
- Phase 1 遥控座舱场景无回放需求，不过度设计


## D41: 零拷贝策略 — 零拷贝优先 + 系统内存 fallback

**决策**: GPU 零拷贝主路径 + 系统内存 fallback 降级
**日期**: 2026-07-16
**原因**:
- DMA-BUF (Linux) / NV12 Surface (Windows) / CVPixelBuffer (macOS) 实现 GPU→GPU 零拷贝
- 无可用 GPU 时 fallback 到 CPU 内存 + 软件编码
- GStreamer buffer pool + allocator 自动选择零拷贝路径
- Parsec 零拷贝全链路验证了延迟收益（7ms 端到端）

## D42: 采集层 — GStreamer 优先 + Rust 自研 fallback

**决策**: Phase 1 使用 GStreamer 插件采集，不支持平台用 Rust 自研 fallback
**日期**: 2026-07-16
**原因**:
- dxgiscreencapsrc (Windows), pipewiresrc (Linux), avfvideosrc (macOS) 覆盖主流平台
- GStreamer 采集插件成熟，直接提供 GstBuffer 给编码器，零拷贝路径最短
- Rust 自研 fallback 仅覆盖 GStreamer 不支持的平台（嵌入式、Wayland 非 PipeWire）

## D43: 编码后端 — GStreamer 统一

**决策**: Phase 1 编码后端全部走 GStreamer（nvh264enc / vah264enc / vtenc_h264）
**日期**: 2026-07-16

**修订 (2026-07-22)**: omspbase-codec 计划支持 FFmpeg 静态链接作为 Host 编码备选（backend-ffmpeg feature）。GStreamer 保留为默认（开发/桌面），FFmpeg 用于 edge embed 静态分发。详见 docs/modules/18-codec-crate.md。

**原因**:
- 单一 dxgiscreencapsrc → nvh264enc → appsink pipeline，零组装成本
- FFmpeg 软件 fallback 留给 Phase 2
- NVENC libloading 直调热路径留给 Phase 3（仅 GStreamer 开销 >3ms 时实施）
- GStreamer 已覆盖所有主流 GPU 编码 API

## D44: 编码器 trait 设计 — 两层抽象

**决策**: EncoderCaps bitflags + VideoEncoder trait（含工厂方法）
**日期**: 2026-07-16
**原因**:
- EncoderCaps 声明能力：GPU_TEXTURE, DMA_BUF, PARALLEL, DYNAMIC_RC, REQUEST_KF, YUV444, HDR10, AV1
- VideoEncoder trait: probe(), create(), caps(), codec(), encode(), set_bitrate(), request_keyframe()
- 工厂方法放 trait（create），不单独拆 EncoderFactory — Phase 1 只有一个后端

## D45: 采集-编码耦合 — 多通道输出

**决策**: ScreenCapture 声明可用格式列表，PipelineEngine 按需选择
**日期**: 2026-07-16
**原因**:
- ScreenCapture.available_formats() 返回支持的 GpuTextureFormat 列表
- PipelineEngine 根据下游编码器能力选择最佳格式
- GStreamer videoconvert 自动处理格式转换，trait 层仅透传
- 各平台 native 采集格式不同（Windows NV12, Linux DMA-BUF, macOS BiPlanar），统一抽象层处理

## D46: 解码架构 — 分层 trait（与编码镜像）

**决策**: DecoderCaps + VideoDecoder trait + GStreamer Phase 1 后端
**日期**: 2026-07-16
**原因**:
- 与 VideoEncoder (D43-D44) 完全对称，编码/解码 trait 层模式一致
- Phase 1 GStreamer decodebin 覆盖全平台硬件解码，实际等效于裸用 GStreamer
- trait 层提供显式能力声明：ADAPTIVE flag 用于丢包恢复、supported_codecs 用于编解码协商
- 未来发现 GStreamer decode 开销不可接受时，可替换为原生 D3D11VA/VAAPI/VideoToolbox 后端
- VideoDecoder 作为 MediaProcessor 插入 PipelineEngine，处理 EncodedFragment → RawFrame
- 跨场景复用：远程桌面、遥操作、会议、回放均通过同一 trait

核心 trait: DecoderCaps (GPU_TEXTURE, DMA_BUF, PARALLEL, ADAPTIVE, AV1, VP9, VP8, H264, HEVC)
接口: probe(), create(), caps(), supported_codecs(), decode(), request_keyframe(), reset()
Phase 1 后端: GStreamer pipeline (appsrc → h264parse → decodebin → videoconvert → appsink)
Phase 2+: 原生 D3D11VA / VAAPI / VideoToolbox / Vulkan Video 后端（仅 GStreamer 开销不可接受时）

## D47: 渲染架构 — Phase 1 极简 CPU buffer + Phase 2 GPU direct

**决策**: RenderCap + VideoRenderer trait，Phase 1 CPU 回读路径（appsink→CPU→Canvas），Phase 2 GPU interop
**日期**: 2026-07-16
**原因**:
- Phase 1 遥控座舱：Host 端 headless 不需渲染，Client 端只需功能验证
- appsink CPU buffer → webview Canvas 路径极简，无需处理 D3D11/wgpu interop 复杂度
- Phase 2 GPU direct 路径：D3D11 texture → wgpu DX12 interop (Win), DMA-BUF → wgpu Vulkan interop (Linux), CVPixelBuffer → wgpu Metal interop (macOS)
- Moonlight 像素着色器 YUV→RGB 模式作为延迟最优方案（+<1ms），Phase 2 实现
- Headless 场景（车端推流、监控录制）不需要渲染

核心 trait: RenderCap (GPU_DIRECT, GPU_INTEROP, YUV_CS_SHADER, CPU_FALLBACK)
接口: caps(), set_surface(target), render(frame)

## D48: 软件编解码策略 — VP8 优先 + H.264 fallback

**决策**: 默认 VP8 (libvpx)，H.264 (openh264) 作为兼容 fallback，AV1 (SVT-AV1+dav1d) 作为未来演进
**日期**: 2026-07-16
**原因**:
- OMSPBase 不走传统 CDN 分发路线，不需要 H.264 的极致兼容性
- VP8 免专利（Google 开放），H.264 有 MPEG LA 专利池风险
- WebRTC MTI 强制要求 VP8 + H.264，浏览器全兼容
- libvpx 压缩率优于 openh264，编码延迟相似（20-35ms）
- dav1d 软件解码极快 (3-8ms)，AV1 在弱网场景带宽节省 50%
- 三层策略: Layer1 openh264 基线 (能跑) → Layer2 VP8 默认 → Layer3 AV1 未来 (弱网)
- GStreamer 统一后端: openh264enc / vp8enc / vp9enc / svtav1enc element 切换
## D49: 音频前处理 — AudioProcessor trait + backend 自适应

**决策**: AudioProcessor trait 统一接口。backend-libwebrtc 透传（APM 内置），backend-str0m 走 GStreamer webrtcdsp
**日期**: 2026-07-16
**原因**:
- libwebrtc 自带 AudioProcessingModule (AEC+ANS+AGC)，20 年迭代的业界标准
- str0m 是 sans-I/O，不碰音频处理 → 需要外部 3A
- GStreamer webrtcdsp element 内嵌 Google audio_processing 相同代码，零额外依赖
- AudioProcessor trait: process_capture + process_playback/PLC，caps 返回 AEC/ANS/AGC/PLC/VAD bitflags
- AudioProcessor 作为 MediaProcessor 可插入任何 PipelineEngine 管线
- backend-webrtc-rs 当前版本依赖 libwebrtc → 等同透传；0.20+ sans-I/O → 等同 str0m
- Opus 内置 FEC 弥补部分 PLC 需求，webrtcdsp 的 opusdec 自动解码 FEC

## D50: 会议混音 — SFU 转发 + 客户端混音

**决策**: SFU 转发最响 N 路音频 + 客户端独立混音。录制时 MCU 混音为单轨
**日期**: 2026-07-16
**原因**:
- Google Meet 验证: SFU 转发最响 3 人 (CSRC 标记)，客户端处理标准 WebRTC 混音
- 小会: SFU 全转发，客户端混音负载可控
- 大会: SFU 转发最响 3-5 人 + VAD 静默检测 skip 静音流
- 客户端混音延迟更低（无需上行→混音→下行多一跳）
- 标准 WebRTC 客户端自带混音能力（浏览器 audio mixing 内置）
- 录制场景例外: 必须 MCU 混音为单轨
- LiveKit SFU 插件只转发 RTP 不做混音（决策 D14: 纯 SFU 插件）


## D51: 信令消息格式 — Protobuf 双格式

**决策**: WebSocket 信令使用 Protobuf 双格式（二进制 = Protobuf，文本 = JSON 调试）
**日期**: 2026-07-16
**原因**:
- 类型安全: oneof 消息分发，编译期保证完整性，不存在"忘记处理某条消息"
- 双格式: LiveKit 投产验证的 dual-format WS 模式，调试时 useJSON: true 即可
- RustDesk 一致性: RendezvousMessage oneof 已验证，26+ 消息类型管理清晰
- 代价极小: 一个 match(ws_msg.is_binary) 分支，Web 端一次性 protobufjs ~30KB gzip
- SDP 本身是文本，但消息封装层需要类型安全

## D52: 信令服务架构 — 单服务 + trait 留分离插口

**决策**: Phase 1 单 axum 服务（HTTP REST + WebSocket 同进程），trait 抽象留 Phase 3 分离后路
**日期**: 2026-07-16
**原因**:
- YAGNI: Phase 1 遥控座舱推拉流，信令负载可忽略，分离架构的独立扩缩容优势不存在
- 代码复用: JWT 验证逻辑 REST 和 WS upgrade 共享，同进程直接调用，无分布式一致性风险
- 留后路: SignalingTransport trait + MessageSink/MessageSource 抽象，为 Phase 3 跨进程 relay 做准备
- Jitsi 反面教材: Jicofo (Java) + Prosody (Lua) 分离导致两个语言、两个日志、两个部署的调试地狱
- RustDesk 一致性: hbbs 单二进制处理 UDP+TCP+WebSocket 全部信令

## D53: 房间模型 — 统一 Room + Topology 枚举

**决策**: 所有场景使用统一 Room 模型，topology 字段区分 P2P/SFU/PubSub
**日期**: 2026-07-16
**原因**:
- LiveKit 验证: 不区分 1:1/多人，所有场景都是 Room { name, participants, tracks }
- 本质分析: 6 场景 = 3 种拓扑 (P2P/SFU/PubSub) × 是否有 DataChannel × 方向
- 场景变迁: RemoteCall → StreamSession 只需改 topology 字段，不重写信令代码
- 场景专用模型的过度抽象: RemoteCall 和 TeleopSession 共享 90% SDP+ICE 逻辑却各自实现
- 升级路径统一: 新能力只需加枚举值，不改类型系统


## D54: 信令 Trait 架构 — 单层 sans-I/O SignalHandler

**决策**: Phase 1 单一 SignalHandler trait（sans-I/O 消息中继），砍掉 SignalingTransport 和 SignalingSession 两层
**日期**: 2026-07-16
**原因**:
- MediaTransport 已有 SDP/ICE API（create_offer/set_remote_description/add_ice_candidate），信令层不应重复
- 信令的本质 = 不透明消息中继，不解析 SDP 内容
- SignalingTransport trait 是 WebSocket 包装器过度抽象 — WS 由 axum/tungstenite 管理
- SignalingSession 的 send_offer/handle_answer 与 MediaTransport API 重复
- sans-I/O: handle_input/poll_output 模式，与 MediaTransport 一致
- Phase 1 遥控座舱 1:1 P2P，SignalHandler + HashMap 配对足够

SignalHandler trait:
  accept(conn_id, auth) → Result
  handle_input(conn_id, msg) → Result<Vec<SignalOutput>>
  handle_close(conn_id) → Result<Vec<SignalOutput>>
  poll_output() → Result<Vec<SignalOutput>>

Protobuf 消息分离:
- ClientSignal oneof: JoinRoom, SdpOffer, SdpAnswer, IceCandidate, PublishTrack, SubscribeTrack, DataChannelSignal, Ping
- ServerSignal oneof: JoinAccepted, RemoteOffer, RemoteAnswer, RemoteIce, ParticipantJoined, ParticipantLeft, Pong, Error, DataChannelSignal

Phase 2 预留:
- RoomRouter trait（提取到独立层）: 拓扑感知路由（P2P 单播/SFU 广播/PubSub 按订阅过滤）

## D55: napi-rs Binding API — Session 抽象

**决策**: 仅导出 Session 抽象到 Node.js（EventEmitter 模式），不暴露底层 trait
**日期**: 2026-07-16
**原因**:
- Session 高封装，Node.js 友好，与 AUDEBase PluginHost 风格一致
- 底层 MediaTransport/PeerConnection 保留在 Rust 侧，trait 体系不变
- EventEmitter: Node.js 原生无额外依赖
- 视频帧: 解码后 RGBA Buffer（napi-rs 拷贝），Phase 1 120MB/s 可接受
- Phase 2 可选 DMA-BUF 共享提升性能
- signal/control/stats 三组方法 + 6 种事件

### D56: SessionType 枚举

Phase 1: remote_desktop (host/client), teleop_cockpit, teleop_vehicle
Phase 2+: video_conference, live_streaming, surveillance_viewer
SessionRole: host | client | cockpit | vehicle

## D57: gRPC Auth 合约 — 最小验证

**决策**: gRPC AuthService 仅提供 validateToken + checkPermission，OMSPBase 自维护 Permission 字符串枚举
**日期**: 2026-07-16
**原因**:
- 最小合约: AUDEBase 维护全部用户/角色/权限逻辑，OMSPBase 只是客户端
- Permission = 字符串: OMSPBase 定义 StreamingStart/RemoteDesktopConnect 等枚举，AUDEBase RBAC 做字符串→角色映射
- validateToken 预解析权限列表: 减少 checkPermission gRPC 调用频率
- 错误通过 response.error 字段: gRPC status 留给传输层错误

## D58: 插件进程隔离 — 分阶段渐进策略

**决策**: 采用 C→A→A+B 渐进路径。Phase 1 inline (trait 调用)，Phase 2 iceoryx2 SHM (MEDIA 隔离)，Phase 3 iceoryx2+Zenoh (网络+第三方)
**日期**: 2026-07-17
**原因**:
- Phase 1 遥控座舱：3-4 个自研插件，rustc 编译期安全 = 隔离已足够
- 进程隔离成本：iceoryx2 带来 ~80ns 延迟 + 1.5MB 内存，Zenoh 带来 25MB zenohd
- 不做过早优化：Phase 1 无第三方插件、无跨机器通信，两套中间件都是负担
- AUDEBase 借鉴：接口先行（MediaHost trait）+ manifest 声明，为未来隔离做准备
- GPU 零拷贝：inline 模式可直接持有 GPU 帧引用，跨进程必须拷贝

### 三阶段路径

| 阶段 | 隔离层 | IPC | 适用场景 |
|------|--------|-----|---------|
| Phase 1 | inline (同进程) | dyn trait 调用 (0ns) | 遥控座舱 1:1 P2P |
| Phase 2 | SIGNALING+MEDIA (2 进程) | iceoryx2 SHM (80ns) | SFU 会议、多模块 |
| Phase 3 | +ISOLATED+Container | iceoryx2+Zenoh | 第三方编解码器、网络 |

### D59: Phase 1 插件 = 同进程 trait 调用

- 所有插件编译进 omspbase-core 二进制
- PluginManager 管理 lifecycle（on_load/on_unload）
- GPU 帧通过 Arc 共享，零拷贝
- MediaHost trait 预留 process 模式接口

### D60: Phase 2 iceoryx2 统一数据面

- 选 iceoryx2 而非 Zenoh SHM: 纯 SHM 零拷贝，无协议开销，无 TCP 会话维护
- 信令保持同进程，媒体处理（编码/解码/渲染）进 MEDIA 进程组
- 抛弃 Zenoh+iceoryx2 混合方案: 双中间件税（25MB+配置翻倍+两套 debug）
- 抛弃 AUDEBase stdin/stdout 模式: 8MB 视频帧 120ms vs iceoryx2 80ns，150 万倍差距

## D61: 录制回放 — Phase 1 不做

**决策**: Phase 1 不包含录制回放功能，Phase 2 再做
**日期**: 2026-07-17
**原因**:
- 遥控座舱推拉流 MVP 核心价值在实时双向通信，录制是附加功能
- Phase 2 场景（视频会议、监控）录制需求更明确，届时一起设计

## D62: Host 多进程架构 — 分阶段

**决策**: Phase 1 3 进程套件（hostd + capture-encode + push）→ Phase 2 外部 SDK (ROS2/自驾视觉订阅) + record-worker
**日期**: 2026-07-17
**修正**: 原 "Phase 1 单进程 → Phase 2 多进程" → Phase 1 提前引入子进程模型 (D102)
**原因**:
- Phase 1 验证 WebRTC 管线，单进程最快跑通 MVP
- 多进程价值：Capture 一帧多吃（Push/Record/ROS 共享）、进程隔离高可靠、外部 SDK 零拷贝订阅
- GPU 路径约束：iceoryx2 暂无 DMA-BUF，当前需 CPU 中转。车辆摄像头 V4L2 出 CPU 帧影响较小
- Phase 1 预留进程边界：定义 iceoryx2 message schema，确保拆分低成本

### D63: Phase 1 Host = 3 进程套件

- hostd: supervisor（信令/MQTT/Web 配置/进程管理）
- capture-encode worker: GStreamer 采集 + GPU 编码
- push worker: libwebrtc 推流 + DataChannel
- IPC: iceoryx2 SHM (D103)
**修正**: 原 "单二进制" → 3 进程套件 (D102)

### Host 进程分工 (Phase 2)

| 进程 | 职责 | 关键依赖 |
|------|------|---------|
| Daemon | 信令连接、配置分发、进程监督 | SignalHandler |
| Capture | 摄像头 V4L2 采集 → iceoryx2 发布 | GStreamer v4l2src |
| Push | 订阅 SHM 帧 → HW 编码 → WebRTC 推流 | libwebrtc |
| Record | 订阅 SHM 帧 → 本地 splitmuxsink 存盘 | GStreamer |
| External | ROS2 节点 / 自驾视觉，只读订阅 SHM | iceoryx2 C++ binding |

## D64: CameraCapture trait + GStreamer 默认实现

**决策**: CameraCapture trait（CameraCaps bitflags + CameraSource 枚举），Phase 1 单一 GStreamer 实现覆盖 USB/V4L2/Jetson/RTSP
**日期**: 2026-07-17
**原因**:
- GStreamer autovideosrc 已自动检测 USB/V4L2/Jetson，零额外工作
- 一个 impl 覆盖所有 CameraSource 变体，~100 行
- CameraSource 枚举区分 USB/V4L2/Jetson/RTSP，方便 Phase 2 替换特定实现
- 沿袭 VideoEncoder/Decoder 的 Caps+trait 模式

## D65: omspbase-vision-sdk — 纯 iceoryx2 通信

**决策**: vision-sdk 与 Daemon 全部通过 iceoryx2 通信（注册/发现/通知/视频帧/数据通道），不引入 gRPC
**日期**: 2026-07-17
**原因**:
- 统一中间件：一套 iceoryx2 覆盖所有 IPC 需求
- 零延迟：Service(注册) + Event(通知) + SHM(帧+元数据) 全部 80ns 级
- 简化部署：无需额外 gRPC server/client，无需 TLS 证书
- iceoryx2 的 Service 模式可替代 gRPC 的 req/resp

### SDK 功能
- Stream 注册/注销 (→ Daemon Registry)
- 视频帧发布/订阅 (iceoryx2 SHM pub/sub)
- DataChannel 绑定/消息收发 (iceoryx2 pub/sub per stream)
- 流发现 + 事件通知 (iceoryx2 event)
- sensor_msgs/Image + cv::Mat 零拷贝适配

## D66: omspbase-client-sdk — 远程收图+控制 SDK

**决策**: 远程 SDK 提供多流订阅、视频解码渲染、控制指令发送、遥测接收能力
**日期**: 2026-07-17
**原因**:
- 与 vision-sdk 配对：vision = 出图端，remote = 收图+控制端
- 不绑 cockpit 场景：覆盖遥控、远程桌面、远程监控
- subscribe + send_control + on_telemetry 三项核心能力
- 紧急通道：独立 DataChannel label="emergency" 不排队
- 多屏布局 Phase 2

## D67: omspbase-field-sdk — 轻量 IPC 层

**决策**: 更名为 omspbase-field-sdk，仅包含 IPC 能力（register/publish/subscribe/events/DataChannel），媒体处理（capture/encode/decode）在 omspbase-codec 和 omspbase-core 中，按需组合
**日期**: 2026-07-17
**原因**:
- field↔remote 对称命名，field=现场端，remote=远程端
- 分层清晰：field-sdk 做 IPC，core/codec 做媒体处理
- ROS 节点/感知模块只需 field-sdk，无需 GStreamer 依赖
- 模块按需组合：Capture=field-sdk+codec，Push=field-sdk+codec+transport，ROS node=field-sdk only

## D68: SDK 命名规范 — 去掉 -sdk 后缀

**决策**: Rust crate 命名 omspbase-field / omspbase-client，C/C++ FFI 加 -c 后缀（omspbase-field-c / omspbase-client-c）
**日期**: 2026-07-17
**原因**:
- Rust crate 本身即是库，-sdk 后缀冗余
- -c 后缀 = C binding 惯例（类比 -sys = FFI wrapper）
- field↔remote 对称，简洁

## D69: SDK Facade 模式 — 单一入口

**决策**: omspbase-client 和 omspbase-field 作为 facade crate，各自 re-export 子 crate 类型（core/codec/transport/signaling/pipeline），舱端只需一 dep
**日期**: 2026-07-17
**原因**:
- 舱端 app 只需 `omspbase-client = "0.1"` 一行依赖
- facade 内部传递依赖 core/codec/transport/signaling/pipeline
- pub use 暴露用户需要的类型（MediaFragment, VideoDecoder 等）
- 版本同步由 facade 内部管理，用户无感知
- field 端同样: `omspbase-field = "0.1"` 一行

## D70: omspbase-client-c 静态链接 FFmpeg

**决策**: 舱端 C 库 (omspbase-client-c) 静态链接 FFmpeg (.a)，支持 HW decode，field 端保持 GStreamer
**日期**: 2026-07-17
**原因**:
- FFmpeg 可完全静态链接 (--enable-static --disable-shared)，GStreamer 插件必须 .so
- 舱端只需 decode + colorspace convert，FFmpeg libavcodec 已足够
- HW decode (NVDEC/VAAPI/VideoToolbox/V4L2) 编译时注册，未装 GPU 自动 fallback 软解
- field 端需 GStreamer 复杂管线 (capture/RTSP/compositor)，不宜换 FFmpeg
- remote-c 静态库 ~15MB，零运行时依赖

## D71: 编解码框架策略 — GStreamer (Field) + FFmpeg (Remote)

**决策**: Field/Host 端使用 GStreamer（capture+encode+compositor+协议栈），Remote 端使用 FFmpeg（decode+colorspace），Browser 端使用 WebCodecs。trait 层统一
**日期**: 2026-07-17
**原因**:
- Field 需要 GStreamer 的 v4l2src/nvarguscamerasrc capture + NVENC encode + rtspsrc/webrtcbin 完整协议栈 + GPU 零拷贝
- Remote 只需 decode+colorspace，FFmpeg 可完整静态链接 (.a)，零运行时依赖
- GStreamer 插件体系无法静态链接，不适合 Remote 的零依赖部署要求
- 统一 VideoDecoder/VideoEncoder trait，编译期 cfg 选择后端实现
- Browser 端完全跳过 codec crate，直接用 WebCodecs API

## D72: remote 端编解码策略优化

**决策**: remote 默认 backend-libwebrtc（内置 codec），backend-str0m 时才静态链接 FFmpeg 解码。D70 修正
**日期**: 2026-07-17
**原因**:
- libwebrtc 内置 VP8/VP9/H.264 软解 + NVDEC/VAAPI 硬解，不需要外部 codec
- str0m 只传输原始 RTP 包，不解码，需要外部解码器
- 默认场景 (弱网遥控) 用 libwebrtc，FFmpeg 不参与
- FFmpeg 仅为 str0m (Embed/LAN 场景) 备选
- Web 端用 WebCodecs，两种后端都不需要的 codec crate

## D73: 最低系统支持 — macOS/Linux/Windows (⚠️ D118: 原 Ubuntu 20.04 only → 跨平台)

**决策**: Field 和 Remote 最低支持 Ubuntu 20.04，Jetson JetPack 基于 20.04
**日期**: 2026-07-17
**原因**:
- GStreamer 1.16 (Ubuntu 20.04 apt 默认) 已含 v4l2src/vp8enc/h264parse
- libwebrtc (C++17) + Rust edition 2024 均支持
- NVIDIA JetPack 基于 Ubuntu 20.04

## D74: 信令协议 — WebSocket (Phase 1) + MQTT 5.0 (Phase 2+)

⚠️ **MVP 修正**: Phase 1 全部使用 WebSocket。MQTT 5.0 延至 Phase 2（车端抗弱网场景）。D52/D54 信令架构均基于 WebSocket。

**原始决策** (Phase 2 保留): MQTT 5.0 作为主信令协议（抗弱网：QoS 1 + Persistent Session + Last Will），WebSocket 保留给浏览器客户端。

**决策**: MQTT 5.0 作为主信令协议（抗弱网），WebSocket 保留给浏览器客户端
**日期**: 2026-07-17
**原因**:
- Persistent Session: 车辆断网重连后自动恢复订阅，不丢消息
- QoS 1: 控制指令 at-least-once 保证
- Last Will: 座舱/车辆离线自动通知对端
- Keep Alive 内置，无需应用层心跳
- MQTT over WebSocket 兼容浏览器 (mqtt.js)
- Phase 1: MQTT Broker (emqx/mosquitto) + rumqttc Rust 客户端
- 新增订阅者（多屏/录制/监控）只需订阅 topic，信令代码零改动

## D75: 视频裸流标准格式 I420

**决策**: 裸流统一使用 I420 (YUV 4:2:0 planar)，GPU 路径通过 GStreamer videoconvert 自动转换
**日期**: 2026-07-17
**原因**:
- libwebrtc 内部原生格式
- 浏览器 WebCodecs + Canvas 直接消费
- FFmpeg swscale 最优路径
- GPU NV12→I420 转换 ~1ms (GStreamer videoconvert)
- 跨平台通用性最高，降低 N×M 格式转换矩阵

## D76: omspbase-client 与 omspbase-client 分离

**决策**: omspbase-client(-c) 为无 GUI SDK，omspbase-client 为带 Tauri GUI 的独立二进制。omspbase-client 可直接依赖底层 Rust crate
**日期**: 2026-07-17
**原因**:
- omspbase-client-c: .a 静态库，供 C/C++ 嵌入（ROS/自驾/移动端）
- omspbase-client: Rust 二进制，可直接 use omspbase_core::* 调底层 trait
- 分离 GUI 层和 SDK 层，omspbase-client 可跳过 C FFI 开销直接 Rust 调用

## D77: Host 跨平台支持 + P2P/中继自动切换

**决策**: Host 端跨平台 (Ubuntu 20.04/Windows/macOS/Jetson)，P2P 优先 + TURN 中继 fallback，由 libwebrtc ICE agent 自动选择 ⚠️ 默认策略由 D96 修正为 relay-default
**日期**: 2026-07-17
**原因**:
- Host 跨平台：GStreamer 原生支持 Windows (dshow/d3d11), macOS (avfvideosrc/VT), Linux (v4l2src/VAAPI)
- CameraSource 枚举 + GstCameraCapture impl 无需改动
- P2P/中继：libwebrtc ICE 已内置 host→srflx→relay 优先级
- coturn (D17) 同时提供 STUN + TURN，配置即可

## D78: P2P/中继强制指定 + 远程配置监控

**决策**: Host/Remote 支持配置文件强制 P2P 或 relay，后端通过 MQTT topic 远程配置/监控 Host 和 Remote ⚠️ 默认策略由 D96 修正为 relay-default
**日期**: 2026-07-17
**原因**:
- 强制指定：p2p_mode = auto | force_p2p | force_relay，解决特定网络环境需求
- 远程配置：MQTT cmd/host/{id}/config 下发 camera/encode/record/network 配置
- 远程监控：status/host/{id} 上报 CPU/GPU/fps/bitrate/rtt/camera 状态
- Remote 管理：cmd/remote/{id}/decode 指定编解码偏好
- 告警推送：MQTT status topic + 后端订阅，异常自动告警

## D79: Field/Remote 多语言绑定 (C/CXX/Python)

**决策**: omspbase-field 和 omspbase-client 提供 C (.h+.a)、C++ (header-only RAII wrapper)、Python (pyo3 native) 三套绑定
**日期**: 2026-07-17
**原因**:
- C: 最底层 FFI，所有语言可调用
- C++: header-only RAII 封装，ROS2/自驾节点标准语言
- Python: pyo3 原生绑定，ML/CV 生态（OpenCV/numpy 零拷贝桥接）

## D80: Remote 配置只读 — 仅监控+授权

**决策**: 后端对 Remote 只有监控（status/remote/{id}）和授权能力，不可修改 Remote 本地配置
**日期**: 2026-07-17
**原因**:
- Remote 配置（codec/render/layout）是用户本地偏好，不应远程强制
- 后端只需监控连接质量和授权访问
- 避免远程修改导致的意外体验变更

## D81: Host 内嵌 web 本机配置

**决策**: Host 运行 axum 嵌入式 Web 服务，默认 bind 127.0.0.1 (仅本地)，配置文件可改为 0.0.0.0 允许局域网访问，HTTP Basic Auth 保护 (D85)
**日期**: 2026-07-17
- 车辆端可能无显示器，需要通过其他设备浏览器访问本机配置（类似路由器管理界面）→ 0.0.0.0 模式
- axum + 静态 HTML 无前端框架依赖，~100KB
- 远程配置 (MQTT) 仍保留为批量管理通道，本地 web 为初始设置通道
- 参考 RustDesk 本地配置页模式
- 安全: HTTP Basic Auth (D85) 防止未授权局域网访问

**修正**: C1 (arch-reviewer) — 原 "127.0.0.1 仅本地" 与 "通过其他设备浏览器访问" 矛盾，修正为默认 localhost + 可配置 LAN

## D82: 最终 Crate 清单 — 10 crates + 3 binaries

**决策**: MVP 保持 10 crates + 3 binaries，职责单一，不合并。field/remote 作为 facade 保证轻量消费者最小依赖
**日期**: 2026-07-17
**原因**:
- 合并 pipeline+transport 会耦合帧路由与网络 I/O
- 合并 field+remote 会导致轻量消费者被迫引入 codec
- field 只需 core，remote 需 core+transport+signaling+pipeline

## D83: Field/Remote 双格式库输出 — .a + .so

**决策**: omspbase-field 和 omspbase-client 同时提供静态库 (.a) 和动态库 (.so/.dylib/.dll)
**日期**: 2026-07-17
**原因**:
- 静态链接 .a：嵌入车载控制器等资源受限环境，无运行时依赖，编译器可内联优化
- 动态链接 .so：用于插件系统（dlopen）和多进程共享（减少内存占用）
- C FFI 接口相同（omspbase-field-c / omspbase-client-c），链接方式不影响 API
- RustDesk 模式：核心 Rust crate 编译为 .a 供 C++ GUI 链接

### 完整清单

| # | Crate | 类型 | 外部依赖 | 预估行数 |
|---|------|:---:|------|:---:|
| 1 | omspbase-core | lib | bitflags, thiserror, bytes | ~800 |
| 2 | omspbase-transport | lib | core, webrtc-kit | ~300 |
| 3 | omspbase-pipeline | lib | core, tokio | ~400 |
| 4 | omspbase-signaling | lib | core, rumqttc | ~400 |
| 5 | omspbase-codec | lib | core, pipeline, gstreamer | ~600 |
| 6 | omspbase-field | facade | core (+ iceoryx2 Phase 2) | ~100 |
| 7 | omspbase-client | facade | core, transport, signaling | ~150 |
| 8 | omspbase-host | binary | field, codec, transport, signaling | ~80+web |
| 9 | omspbase-client | binary | remote, tauri, react | ~300+TSX |
| 10 | omspbase-server | binary | signaling, axum | ~200 |

## D84: Host Web 技术栈 — axum + 静态 HTML + SSE

**决策**: Host 嵌入式配置页使用 axum serve 静态 HTML + vanilla JS + SSE 实时推送，include_str! 嵌入二进制
**日期**: 2026-07-17
**原因**:
- 零构建工具：HTML/CSS/JS 由 include_str! 嵌入 Rust 二进制
- ~100KB 总大小
- SSE 实时推送状态（CPU/GPU/fps/bitrate/rtt 每秒更新）
- vanilla JS fetch() 提交配置表单
- 资源约束安全（Jetson Nano 4GB）

## D85: Host Web 访问策略 — 配置文件账户密码

**决策**: 初期 bind 地址 + 账户密码由配置文件控制，HTTP Basic Auth 验证
**日期**: 2026-07-17
**原因**:
- 可配置 bind: 127.0.0.1 (仅本机) 或 0.0.0.0 (允许局域网)
- 简单账户密码存配置文件，无需 token 生成逻辑
- HTTP Basic Auth: 所有浏览器原生支持，零 JS 代码
- Phase 2 可升级为 JWT 或 OAuth 集成

## D86: Server MVP 范围 — relay+信令+监控 (⚠️ D118: 原纯监控 → 全功能Server)

**决策**: Phase 1 Server 仅提供 Host/Remote 状态监控和 JWT 认证，不提供远程控制
**日期**: 2026-07-17
**原因**:
- MQTT Broker (emqx) + TURN (coturn) 为 Docker 部署，零自定义代码
- omspbase-server: axum API + MQTT subscriber 缓存状态 + WebSocket 实时推送
- REST API: /api/hosts, /api/remotes, /api/auth/login, /api/health
- 内存 HashMap 缓存最新状态，不持久化（Phase 2 可选 PostgreSQL）

## D87: Server Web UI — React + Ant Design

**决策**: Server 管理面板使用 React 19 + Ant Design 5 (ProTable/ProLayout)，与 omspbase-client 和 AUDEBase Admin UI 共享组件生态
**日期**: 2026-07-17
**原因**:
- 运营人员日常使用，需要专业 UI（ProTable 排序/筛选/分页）
- 复用 AUDEBase 和 omspbase-client 的 React + Ant Design 组件
- recharts 图表展示 fps/bitrate/rtt 趋势
- WebSocket 实时推送 Host 上下线和告警

## D88: Server 账户 — 角色级 RBAC

**决策**: SQLite users 表 + JWT，role 字段支持 admin / operator / auditor
**日期**: 2026-07-17
**修正**: 原 "单 admin + JWT" → 角色级 RBAC (D100)

## D89: Server 数据库 — SQLite

**决策**: SQLite 仅存 users 表。Host/Remote 状态存内存 HashMap，不持久化
**日期**: 2026-07-17

## D90: Server 插件 — 编译期 feature flags

**决策**: Server 扩展点使用编译期 #[cfg(feature)] 选择，不运行时加载。扩展点: auth-local|auth-audebase, storage-local|storage-s3, notify-ws, sfu-mediasoup
**日期**: 2026-07-17
**修正**: 原 "Server 无插件系统" → 编译期 feature flags 扩展点 (D98)

## D91: Server 日志 — tracing + stdout

**决策**: tracing + env_logger → stdout/stderr，Docker logs 收集
**日期**: 2026-07-17

## D92: MVP 功能清单 — 7 项核心功能

**决策**: 远程驾驶 MVP 包含摄像头采集、WebRTC推流(P2P+TURN)、座舱解码渲染、DataChannel控制指令、Host本地Web配置、Server监控面板、紧急停止通道
**日期**: 2026-07-17

## D93: MVP 验收标准

**决策**: 端到端视频延迟≤150ms、控制延迟≤50ms、720p@30fps Jetson Nano、P2P直连>90%、断线重连≤3s、Host Web可访问、Server面板可用、紧急停止独立通道
**日期**: 2026-07-17

## D94: SDD + TDD 策略

**决策**: 7 个核心模块 SDD 接口规格文档 + 4 层测试（单元/集成/E2E/实车），覆盖率目标 80-90%
**日期**: 2026-07-17

## D95: ⚠️ OBSOLETE — LiveKit SFU (已被 D97+D118 替代)

**决策**: MVP 使用 LiveKit Server 作为 SFU 媒体转发，MQTT 管理控制面（房间/权限），omspbase-server 签发 LiveKit JWT ⚠️ 已由 D97 替代为 mediasoup
**日期**: 2026-07-17
**原因**:
- MVP 有 N 个观看者（座舱+大屏+运维），需要 SFU 转发
- LiveKit: 成熟 SFU，VP8/H.264 原生支持，选择性转发
- MQTT 保持为信令控制面（房间管理/权限），不重复造轮子
- omspbase-server 签发 LiveKit JWT，完成权限校验
- 延续 D14 (LiveKit 纯 SFU 插件)，早于原计划启用

## D96: 传输策略翻转为默认中继

**决策**: 默认 p2p_mode = "relay"（走 SFU/TURN），仅 AUDESYS Studio LAN 场景 p2p_mode = "p2p"（跳过服务器直连）
**日期**: 2026-07-17
**原因**:
- 远程驾驶/会议等生产场景依赖中继保证可靠性和可达性
- P2P 穿透在生产网络 (NAT/防火墙/4G) 不可靠
- SFU/LiveKit 已部署，走中继无额外成本
- AUDESYS Studio 是唯一 P2P 场景：单机/LAN 离线开发，无服务器依赖
- 修正 D77/D78 的默认策略

## D97: mediasoup SFU 引擎 (Phase 2 启用)

⚠️ MVP 修正: Phase 1 P2P 直连，SFU 延至 Phase 2 (D15 "P2P优先无SFU")。mediasoup 保留给 Phase 2 多人观看和会议场景。

原内容: 覆盖全部阶段
**日期**: 2026-07-17
**原因**:
- mediasoup Rust crate (v0.22.12, 133K+ 下载) 运行 C++ Worker 为进程内线程，零外部运行时
- 信令不可知设计 — MQTT 5.0 直接集成，无需 WebSocket 桥接
- DataChannel subchannels 与 MQTT topic 路由完美映射
- Worker 进程隔离保证崩溃安全
- 替代 D95 (LiveKit) — mediasoup 与 MQTT 原生契合

## D-ERR-01: Error Model — 统一错误处理

**决策**: Phase 0 定义统一错误分类、传播链和恢复策略
**日期**: 2026-07-17
**原因**:
- 分类: Fatal (终止) / Recoverable (重试) / Transient (忽略)
- 传播链: 每层转换，不跨层透传原始错误
- 熔断器: 5 次失败 60s 内 → 熔断，指数退避
- 背压: PipelineEngine broadcast 溢出 Lagged skip (D25)
- WebSocket 重连: 指数退避 + jitter, min=1s, max=30s, max_retries=unlimited

## D-SEC-01: Security Architecture — 组件间安全

**决策**: Phase 1 MVP Docker 网络隔离，Phase 2 完整安全体系
**日期**: 2026-07-17
**原因**:
- IPC 认证: Phase 2 iceoryx2 SHM + token (D58)
- mTLS: gRPC Phase 1 启用，MQTT TLS 补充 D74
- 审计日志: 敏感操作记入结构化日志
- 静态加密: S3 SSE-KMS + AES-256 (Phase 2)
- Phase 1: mTLS + TLS + 审计日志基础
- Phase 2: 评估 vault 集成 (HashiCorp Vault) 用于 secret 管理
- Phase 2: 定义审计事件 schema (who/what/when/from/result)

## D-CI-01: CI/CD Pipeline — 构建与发布

**决策**: Phase 1 GitHub Actions 5 阶段流水线 (fmt → check → clippy → test)
**日期**: 2026-07-17 (更新 2026-07-19)
**原因**:
- 阶段: fmt (rustfmt check) → check (cargo check) → clippy (lint) → test
- 平台矩阵: Ubuntu + macOS (check + test jobs, 双平台并行)
- 缓存: Swatinem/rust-cache@v2 (check/clippy/test 三阶段)
- RUSTFLAGS: `-D warnings` (所有阶段)
- 暂缓: 跨编译 (cross + qemu, Phase 2)、tarpaulin (GStreamer tests 需 pixi env)、build/packaging (Phase 2)
**实现**: `.github/workflows/ci.yml` 已更新，覆盖 fmt/check/clippy/test 4 个独立 job。
## D-HW-01: 硬件基线 — 跨平台桌面 (⚠️ D118: 原 Jetson Nano → macOS/Win/Linux)

**决策**: Phase 1 MVP 目标平台硬件基线
**日期**: 2026-07-17
**原因**:
- **Jetson Nano**: 4GB RAM, 128 CUDA cores, JetPack 4.6+ (Ubuntu 20.04)
- **CSI Camera**: IMX219 8MP / IMX477 12MP via nvarguscamerasrc
- **编码**: nvv4l2h264enc 720p@30fps, GOP=30, bitrate=2Mbps
- **功耗**: MAXN mode (10W) for sustained encoding
- **存储**: 32GB+ SD/eMMC (系统+录制缓冲)
- **网络**: 4G Dongle / WiFi / Ethernet, RTT 50-200ms target
- **备用平台**: Jetson Orin Nano 8GB (Phase 2), x86 Ubuntu 20.04 (开发阶段)
- Phase 1 存储需求: ~25MB 二进制 + ~100MB 日志 (7 天轮转) + 配置文件。最低 4GB。

> **注意**: D-HW-01 scope is now embedded (Jetson) only. Desktop baseline covered by D-HW-03.
## D-TEST-01: 测试基础设施 — 框架 + Mock 策略

**决策**: Phase 1 测试基础设施设计
**日期**: 2026-07-17
**原因**:
- **单元测试**: cargo test (Rust 内置)，每个 crate 独立 #[cfg(test)]
- **集成测试**: tests/ 目录，cargo test --test integration
- **Mock 策略**: trait object mock（手动实现），不引入 mockall 依赖
  - MockMediaTransport: sans-I/O handle_input/poll_output 实现 P2P loopback
  - MockCameraCapture: 静态 YUV 帧回放
  - MockSignalHandler: 内存消息队列
- **E2E**: tokio 异步集成测试，loopback 模式（capture→encode→decode→render），验证端到端延迟
- **CI**: GitHub Actions Ubuntu x86_64，每个 PR 运行全量测试
- **覆盖率**: cargo-tarpaulin，目标 80%+ (D94)
- **不引入**: mockall (4KB macro 膨胀)，proptest (Phase 2)，docker-compose E2E (Phase 2)

## D-SAFETY-02: 遥操作安全架构 — SafetyEnvelope

**决策**: Phase 1 遥操作 SafetyEnvelope trait 设计
**日期**: 2026-07-17
**参考**: Vay SafetyEnvelope (边界计算模型), tether-rally ESP32 timeout levels (L0-L4), TUM three-level pipeline (FORWARD/LIMIT/OVERRIDE)
**原因**:
- Phase 1 MVP 的遥控座舱有 7 项核心功能，其中紧急停止需要独立通道 (D92-D93)
- SafetyEnvelope trait 定义分级安全响应模型，即使 Phase 2 实现、Phase 1 必须设计接口

```
trait SafetyEnvelope: Send {
    fn check(&self, state: &ControlState) -> SafetyLevel;
    fn limits(&self) -> &ControlLimits;
}

enum SafetyLevel {
    Normal,           // 控制指令正常转发
    Warning,          // 允许执行但发出警告（接近边界）
    Limit,            // 限速/限角：钳制到安全范围
    SoftStop,         // 渐进减速到零（2s 内）
    HardStop,         // 立即切断动力 + 独立紧急通道
}

struct ControlLimits {
    max_steering_angle: f32,  // 最大转向角 (度)
    max_speed: f32,           // 最大速度 (m/s)
    max_accel: f32,           // 最大加速度 (m/s²)
    timeout_ms: u32,          // 控制指令超时 (ms)
    rtt_warning_ms: u32,      // RTT 告警阈值 (ms)
    rtt_emergency_ms: u32,    // RTT 紧急停止阈值 (ms)
}
```

- 紧急停止: 独立 UDP 路径（不依赖 WebRTC DataChannel），QoS DSCP EF 标记 (D92)
- 超时分级: L0 Normal→L1 Warning(100ms)→L2 Limit(150ms)→L3 SoftStop(300ms)→L4 HardStop(500ms)
- 双验证: 操作员指令 + 车端 SafetyEnvelope 独立判定 = 两者一致才执行

## D98: 服务端插件 — 编译期 feature flags

**日期**: 2026-07-17
**状态**: 已决策

**决策**: 服务端扩展点使用编译期 #[cfg(feature)] 选择，不运行时加载。

- AuthProvider: feature = "auth-local" | "auth-audebase"
- StorageBackend: feature = "storage-local" | "storage-s3"
- NotificationChannel: feature = "notify-ws" | "notify-email" | "notify-webhook"
- SFU Engine: feature = "sfu-mediasoup" (Phase 1 唯一选项)
- LicenseChecker: feature = "license-none" (Phase 1 无)

**理由**: RustDesk hbbs 全编译单二进制模式验证可行。Phase 1 仅 3-4 个自研扩展点，不需要 dlopen 动态加载的灵活性成本。

**修正**: 更新 D90（原 "Server 无插件系统" 改为 "Server 编译期 feature flags 扩展点"）


## D99: 日志与可观测性

**日期**: 2026-07-17
**状态**: 已决策

**日志**:
- 框架: tracing + tracing-subscriber
- 格式: JSON (tracing-subscriber fmt::json)
- 输出: stdout/stderr, Docker logs driver 收集
- 轮转: Docker json-file log driver (max-size: 10m, max-file: 3)
- Phase 2: opentelemetry-otlp → collector

**Metrics**:
- 库: prometheus-client
- 端点: GET /metrics (Prometheus 格式)
- 暴露指标: HTTP 请求计数/延迟, WS 连接数, SFU 带宽, 会话时长

**链路追踪**:
- traceId: axum middleware 注入 (tower-http TraceLayer)
- 跨边界: HTTP header x-trace-id → 日志 JSON 字段
- mediasoup C++ worker: 不同运行时，暂不追踪


## D100: 用户权限 — 角色级 RBAC

**日期**: 2026-07-17
**状态**: 已决策

**决策**: SQLite users 表 role 字段，JWT 携带角色，axum middleware 提取。

- users 表: {id, username, password_hash (argon2), role, created_at}
- role enum: admin | operator | auditor
- JWT claims: {sub, exp, role, iat}
- axum middleware: Extensions 注入 CurrentUser
- Phase 1: admin 和 operator 均有全部权限（不做路由级 guard）
- Phase 2: permissions 表 + per-route guard

**修正**: 更新 D88（原 "单 admin" 改为 "角色级 RBAC"）

**权限定义 (Phase 2 预留)**:
- host:read (看车视频), host:control (控车), host:admin (管理车)
- server:config (服务器配置), record:read (回放), record:delete (删除)
- user:manage (用户管理)


## D101: 服务端技术栈补齐

**日期**: 2026-07-17
**状态**: 已决策

**数据库**: sqlx (SQLite)
- 编译期 SQL 校验 (sqlx::query!)
- 异步 (tokio)
- 迁移: sqlx migrate (手写 SQL)
- 依赖: sqlx = { features = ["runtime-tokio", "sqlite", "migrate"] }

**配置**: serde_yaml + 环境变量覆盖
- config.yaml: 静态配置 (port, db_path, mediasoup_worker)
- 环境变量: 敏感值 (JWT_SECRET, AUDEBASE_AUTH_URL)
- 优先级: env > config.yaml > default
- 依赖: serde + serde_yaml + dotenvy

**API 文档**: 手写 README + 模块文档
- REST endpoints: docs/server-api.md
- WebSocket 协议: docs/server-ws.md
- Phase 2: utoipa

**CI/CD**: 三阶段 GitHub Actions
- Stage 1 check: cargo fmt --check + clippy -- -D warnings + cargo deny check
- Stage 2 test: cargo test + cargo test --test integration + cargo tarpaulin
- Stage 3 build: cargo build --release + docker build + docker push
- 触发: push main, PR to main


## D102: Phase 1 Host 单进程模型

⚠️ **D118 修正**: Host push 目标变为 Server relay（非 Remote 直连）。单进程仍合理。
**决策**: Phase 1 Host 使用单进程模型。capture → encode → push 在同进程内通过内部 channel 传递帧。Phase 2 录制/ROS 消费者出现时拆分为多进程。

- 单一二进制 omspbase-host (~25 MB)：内部分为 capture 线程 + encode 线程 + push 线程 + signaling 任务
- 线程间通道: tokio::mpsc (I420 raw → H.264 encoded → RTP packets)
- 无进程间 IPC（省 iceoryx2 依赖 + SHM 管理代码）
- 信令/MQTT/web 配置均在主线程中的 tokio 任务
- systemd 直接管理（Type=notify），无需内部 supervisor

**设计理由**:
- Phase 1 是 1→1 线性管道 (capture→encode→push)，无 fan-out 消费者 (录制 Phase 2, ROS Phase 2)
- 3 进程模型节省: iceoryx2 crate、SHM 管理代码、进程生命周期代码、~10MB 进程开销
- Phase 2 录制/ROS 消费者引入真实 fan-out 时，按 D62 规划拆分为多进程

**Phase 2 拆分预案** (D62 保留): 录制 → capture-encode + record worker (iceoryx2)。ROS 订阅 → + ros-publish worker。座舱预览 → + preview worker。分裂条件: 2+ 消费者 || 1+ 外部消费者 || 进程隔离需求。

**修正**: 审计 C2(arch-reviewer) + simplifier 审查 — 原 3 进程对 Phase 1 为过早工程，回退为单进程。同时回退 D63 和 D103。

## D103: Host IPC — iceoryx2 一步到位

⚠️ MVP 修正 (D102): Phase 1 单进程 tokio::mpsc 内部通道，iceoryx2 延至 Phase 2。此决策仅保留 Phase 2 设计。

Phase 1 直接使用 Eclipse iceoryx2 (v0.9.x) 作为 Host 子进程间通信。

- capture-encode → push: I420 帧传输 (250 MB/s 带宽, ~80ns 延迟)
- hostd ↔ workers: 控制通道 (heartbeat, 配置下发, 状态上报)
- pub/sub 模型: capture-encode publish, push subscribe
- 零序列化: ZeroCopySend derive macro

**风险**: iceoryx2 v0.9.3 pre-1.0 API 可能变更。备选方案: Pinning 版本 + CI 固定。


## D104: Host 打包 — tarball + install.sh

**日期**: 2026-07-17
**状态**: 已决策

**决策**: Phase 1 使用 tarball 裸发到 GitHub Releases，含 install.sh 安装脚本。

**tarball 内容**:
- bin/omspbase-host (单二进制, ~25 MB)
- etc/omspbase/host.conf
- lib/ (GStreamer 插件子集)
- lib/systemd/system/omspbase-host.service
- install.sh (复制到 /opt/oomspbase, 注册 systemd)

**修正**: 审计 simplifier — Phase 1 单进程，单二进制
- bin/{hostd, capture, push}
- etc/omspbase/host.conf
- lib/ (GStreamer 插件子集)
- lib/systemd/system/omspbase-hostd.service
- install.sh (复制到 /opt/oomspbase, 注册 systemd)

**发布平台**: x86_64 (开发) + aarch64 (Jetson)
**Phase 2**: 加 .deb 包和 Docker 镜像
- Windows 策略: sc.exe 创建服务, MSI 打包 (Phase 2)


## D105: SDK 分发 — cargo-c tarball

**日期**: 2026-07-17
**状态**: 已决策

**决策**: 使用 cargo-c 构建 C SDK，产物以 tarball 发布到 GitHub Releases。

**产物**: cargo cbuild → .a + .so + .h (cbindgen) + .pc (pkg-config)
**附赠**: C++ header-only RAII wrapper (.hpp)
**CMake**: 手写 FindOMSPBase.cmake (包装 pkg_check_modules)
**Python**: Phase 2+ pyo3 wheel, Phase 1 随 tarball 附赠源码

**发布资产**:
- omspbase-field-sdk-v0.1.0-x86_64.tar.gz (~2 MB)
- omspbase-field-sdk-v0.1.0-aarch64.tar.gz (~2 MB)
- omspbase-client-sdk-v0.1.0-x86_64.tar.gz (~5 MB, FFmpeg)
- omspbase-client-sdk-v0.1.0-aarch64.tar.gz (~5 MB)

**SDK tarball 内容**:
- include/omspbase/{field,remote}.{h,hpp}
- lib/{libomspbase_{field,remote}.{a,so}}
- share/pkgconfig/omspbase-{field,remote}.pc
- share/cmake/OMSPBaseConfig.cmake


## D106: SDK 安装前缀 — /usr/local

**日期**: 2026-07-17
**状态**: 已决策

**决策**: 默认 /usr/local，用户可通过 --prefix 自定义。FHS 标准，pkg-config 默认搜索路径。

**install.sh 行为**:
- 默认: install to /usr/local/{include,lib,share}
- --prefix=/opt/project: install to /opt/project/{include,lib,share}
- 需要 root (或 sudo) 写入 /usr/local
- Phase 2 可选: --user 模式安装到 ~/.local

## D-HW-02: 编码模式 — 多设备自适应 + 按需推流状态机

**日期**: 2026-07-17
**状态**: 已决策

**决策**: Phase 1 单进程，编码为 2 态切换（IDLE ↔ PUSH）。

### Phase 1 编码模式

Phase 1 单进程无录制 (D61)，编码 = push_active 时启动，否则 IDLE。

```
State: IDLE  ←→  PUSH
  IDLE: 不编码, 仅 I420 采集循环, NVENC 暂停
  PUSH: GCC adaptive, 720p@0.5-8Mbps, GOP=30
  切换: MQTT push_request 到达/结束 → 启动/停止 NVENC
```

### NVENC 探测

启动时自动检测 (nvidia-smi / vainfo / GStreamer element-factory)。单方法失败时 fallback 到下一个。host.conf 可强制覆盖 mode: hw | sw。无 GPU 时使用 libvpx VP8 软件编码 (D48)。

### Phase 2 预留

录制 (D61 Phase 2) 和 ROS 消费者引入后，按 D62 拆分为 capture-encode + record worker (iceoryx2)。双 NVENC 设备可使用独立编码参数（推流 GCC，录制 CQP）。min_bitrate 下限在录制 Phase 2 重新评估。

**修正**: 审计 simplifier — 回退到 Phase 1 2 态。删除 4 态状态机、multi-device 表格、min_bitrate、dual_separate。

**修正**: 审计 simplifier — 回退到 Phase 1 2 态。删除 4 态状态机、multi-device 表格、min_bitrate、dual_separate。

## D-HW-03: 桌面硬件基线 (Desktop Hardware Baseline for MVP)

**决策**: Phase 1 MVP 桌面平台硬件基线
**日期**: 2026-07-17
**状态**: 已决策
**类型**: 硬件架构
**原因**:

| 平台 | CPU | RAM | GPU | 备注 |
|------|-----|-----|-----|------|
| macOS | Apple M1+ | 8GB | Integrated (VideoToolbox) | macOS 13+ (VideoToolbox hardware decoding supported since macOS 13) |
| Linux (x86_64) | 4-core 2GHz+ | 8GB | Intel QSV/AMD VAAPI | Ubuntu 22.04+ |
| Windows (x86_64) | 4-core 2GHz+ | 8GB | NVIDIA NVENC (Pascal+) / Intel QSV | Windows 10 22H2+ |

Phase 1 开发和测试最低需要：一台 macOS (M1+)、一台 Linux x86_64、一台 Windows x86_64。Jetson Nano 基线由 D-HW-01 覆盖。

**关联**: D-HW-01 (嵌入式硬件基线), D118 (MVP 三组件架构), D73 (跨平台支持)


## D-OPS-01: Host Prometheus — /metrics 端点

**日期**: 2026-07-17
**状态**: 已决策

**决策**: Host 暴露 GET /metrics (Prometheus 格式)，与 Server D99 一致。

**暴露指标**:
- capture_fps: 采集帧率 (gauge)
- encode_fps / encode_bitrate_kbps: 编码输出 (gauge)
- encode_latency_ms: NVENC 编码延迟直方图 (histogram)
- push_bitrate_kbps / push_rtt_ms: 推流质量 (gauge)
- gpu_util_pct / gpu_mem_mb: GPU 资源 (gauge)
- mqtt_connected: MQTT 连接状态 (gauge, 0/1)
- process_rss_mb: 进程内存 (gauge)

**库**: prometheus-client (Rust)
**来源**: gap-finder G1

## D-OPS-02: Host 日志轮转 — 裸金属

**日期**: 2026-07-17
**状态**: 已决策

**决策**: Phase 1 裸金属 (ubuntu, Jetson) 使用 tracing-appender 实现本地文件日志轮转。

- 框架: tracing + tracing-subscriber + tracing-appender
- 格式: JSON (同 D99)
- 输出: stdout + 文件双写
- 轮转: tracing-appender rolling::daily, max_log_files=7
- 路径: /var/log/omspbase/host.log
- systemd journal 自动收集 stdout
- Phase 2: 从编码器到采集源的可配置背压 (configurable backpressure)
- Phase 2 Docker 部署: 改用 Docker json-file log driver (D99)

**来源**: gap-finder G3

## D-OPS-03: GPU 显存管理

**日期**: 2026-07-17
**状态**: 已决策

**决策**: Phase 1 单进程，GPU 显存由 NVENC + GStreamer 管理，不做额外池化。

- NVENC 编码器分配: ~200MB (720p H.264 one session)
- GStreamer buffer pool: ~50MB (采集+转换)
- 总 GPU 预算: ~300-400MB / Jetson Nano 4GB 共享内存
- 启动时 probe: NVENC 创建 test session → 分配成功 → 释放 → 记录可用
- 失败: 降级到 software VP8 (D48)
- 运行时: 编码帧分配失败 → D-ERR-01 recoverable 错误 → 重启编码 pipeline

**来源**: gap-finder G4

## D-OPS-04: GStreamer 插件加载策略

**日期**: 2026-07-17
**状态**: 已决策

**决策**: Phase 1 Host 内嵌必需 GStreamer 插件子集，tarball lib/ 包含。

- 必需插件子集: nvcodec (NVENC), videoconvert, videoscale, appsink, v4l2src/avfvideosrc
- 加载策略:
  1. 环境变量: GST_PLUGIN_PATH=$INSTALL_DIR/lib/gstreamer-1.0
  2. fallback: 系统路径 (/usr/lib/x86_64-linux-gnu/gstreamer-1.0)
  3. 启动检查: 遍历必需插件列表，gst-inspect-1.0 验证存在
  4. 缺失插件: 打印错误 + 跳过该功能 + 继续启动
- verbose 模式下打印所有检测结果
- tarball 内插件: 从构建环境复制 .so 文件

**来源**: gap-finder G6

## D-OPS-05: 摄像头采集管道错误恢复

**日期**: 2026-07-17
**状态**: 已决策

**决策**: GStreamer pipeline 监听 bus messages，错误时自动重建 pipeline。

- GStreamer bus watch: 监听 GST_MESSAGE_ERROR / GST_MESSAGE_WARNING / GST_MESSAGE_EOS
- 恢复策略:
  1. v4l2src 超时/断开 → 等待 1s → 重建 pipeline → 重新 probe caps → 重启编码
  2. nvh264enc 错误 → 重建编码 pipeline → 如连续失败 3 次 → software fallback
  3. 摄像头热插拔: udev 监听 + 自动重连
  4. pipeline stall: appsink new-sample 回调设置 2s timeout → 超时视为 stall → 重建
- 恢复过程中 push worker 暂停推流（libwebrtc ice restart）

**来源**: gap-finder G7 + edge-case-finder #8

## D-OPS-06: 优雅关机序列

**日期**: 2026-07-17
**状态**: 已决策

**决策**: 单进程 Host shutdown 序列，确保数据不丢失。

```
systemd SIGTERM
    │
    ▼
1. MQTT: 发送 disconnect (Last Will 已预设)
    │
    ▼
2. libwebrtc: 发送 RTCP BYE → close DataChannel → close PeerConnection
    │
    ▼
3. GStreamer: send EOS → wait_state(NULL) → unref pipeline
    │
    ▼
4. 监控: emit final Prometheus snapshot
    │
    ▼
5. 日志: tracing flush → exit

超时: 每阶段 2s timeout (systemd TimeoutStopSec=10s)。超时跳过当前阶段继续。
systemd unit: Type=notify, sd_notify(READY=1) on start, sd_notify(STOPPING=1) on SIGTERM.

**来源**: gap-finder G8

## D-OPS-07: NVENC 多方法探测

**日期**: 2026-07-17
**状态**: 已决策

**决策**: 启动时用多方法探测 NVENC 可用性，级联 fallback。

探测链:
1. GStreamer: gst-inspect-1.0 nvh264enc → 首选
2. nvidia-smi: query encoder sessions → 次选
3. NVML: nvidia-ml API → 备选 (nvidia-smi 不在 PATH 时)
4. vainfo: VAAPI 探测 → non-NVIDIA fallback
5. 全失败: 降级 software VP8 (D48)

host.conf 可强制: mode: hw | sw

来源: edge-case-finder #7
## D107: Host 安装布局 — /opt/oomspbase

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 部署架构

**决策**: Host 安装到 /opt/oomspbase/，目录结构：

```
/opt/oomspbase/
├── bin/
│   └── omspbase-host          # 可执行文件
├── etc/
│   └── host.conf               # 默认配置文件
├── web/
│   └── index.html              # 内嵌 Web 配置页
└── logs/                       # 日志目录 (tracing-appender)
```

install.sh 脚本行为：
1. 创建 /opt/oomspbase 目录结构
2. 复制 omspbase-host → bin/
3. 复制 host.conf → etc/ (不覆盖已有配置)
4. 注册 systemd service → /etc/systemd/system/omspbase-host.service
5. systemctl enable omspbase-host
6. 提示用户编辑 /opt/oomspbase/etc/host.conf 后 systemctl start

**理由**: 独立目录不污染系统路径，AnyDesk/RustDesk 风格。非 root 可通过 --prefix 覆盖。

**关联**: D104 (tarball + install.sh), D108 (systemd), D73 (Ubuntu 20.04)

---

## D108: Host systemd 守护

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 运维架构

**决策**: Host 通过 systemd service 守护，Type=notify 配合 D-OPS-06 优雅关闭。

systemd service 文件模板：
```ini
[Unit]
Description=OMSPBase Host Service
After=network.target
Wants=network.target

[Service]
Type=notify
ExecStart=/opt/oomspbase/bin/omspbase-host --config /opt/oomspbase/etc/host.conf
Restart=always
RestartSec=5
WatchdogSec=30
StandardOutput=journal
StandardError=journal
Environment=RUST_LOG=info
Environment=GST_PLUGIN_PATH=/opt/oomspbase/lib/gstreamer-1.0

[Install]
WantedBy=multi-user.target
```

**关键参数**:
- Type=notify: 启动完成后 sd_notify(READY=1)，D-OPS-06 5 阶段关闭
- Restart=always: 崩溃后 5s 自动重启
- WatchdogSec=30: systemd watchdog 每 15s sd_notify(WATCHDOG=1)

**理由**: 所有 Host 平台 (Ubuntu 20.04/22.04) 均支持 systemd。RustDesk 模式。

**关联**: D-OPS-06 (优雅关闭), D107 (安装布局), D111 (健康检查)

---

## D109: C SDK 工具链 — cbindgen + 手写 CMake/pc

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 构建工具

**决策**: Phase 1 cbindgen (build.rs) 生成 .h，手写 FindOMSPBase.cmake + .pc 模板。

**工具链**:
- **头文件**: cbindgen v0.29+ (build.rs，参考 webrtc-kit 配置)
- **Crate 输出**: staticlib + cdylib (cargo build 直接产出 .a + .so)
- **pkg-config**: 手写模板，cargo build 后复制到 target/ (参考 webrtc-kit WKitTarget.pc.in)
- **CMake**: 手写 FindOMSPBase.cmake (pkg-config 桥接)，Phase 2 升级到 Corrosion + Config.cmake
- **C API 设计**: 不透明指针 (*mut c_void) + 整数错误码 (0 = ok, -1 = error) (webrtc-kit 模式)
- **C 测试**: Phase 1 不做（YAGNI），Phase 2 Unity 框架

**Phase 2 升级路径**: cargo-c 一站式 + Corrosion CMake 集成 (webrtc-kit 完整模式)

**理由**: webrtc-kit 已验证此模式可行。Phase 1 最小化不引入 cargo-c 和 Corrosion 额外依赖。

**关联**: D68 (命名 -c 后缀), D79 (C/C++/Python 三语言绑定), D83 (双格式 .a+.so)

---

## D110: Docker 镜像 — Phase 2 延后

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 部署架构

**决策**: Phase 1 仅 tarball + install.sh。Docker 镜像延至 Phase 2。

**Phase 2 Docker 规划**:
- 基础镜像: alpine:3.21 + GStreamer 运行时
- 多阶段构建: builder (rust:alpine) → runtime
- 静态链接 musl (禁用 GNU libc 依赖)
- Docker Hub: omspbase/host:latest

**理由**: Phase 1 MVP 遥控座舱，边缘设备 (Jetson/Ubuntu) 直接部署。Docker 增加复杂度（GPU 穿透、GStreamer 设备访问）且 4GB RAM Jetson Nano 不友好。

**关联**: D104 (tarball), D73 (Ubuntu 20.04)

---

## D111: Host 健康检查端点

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 运维架构

**决策**: Host 内嵌 Web 服务提供两个健康检查端点：

```
GET /health → 200 {"status": "ok"}
GET /ready  → 200 {"ready": true, "checks": {"camera": "ok", "encoder": "ok", "signaling": "connected"}}
```

**特性**:
- /health: 轻量存活探针 (K8s livenessProbe)，仅检查进程存活
- /ready: 就绪探针 (K8s readinessProbe)，检查所有子组件状态
- 复用 D81 已有 axum 服务 (127.0.0.1:9800)，不加新端口
- 约 15 行代码

**理由**: K8s/Docker 编排标准接口。D84 SSE 推送实时状态 (CPU/GPU/fps) 与健康检查职责不同 — SSE 是监控看板，/health 是探针。

**关联**: D84 (SSE 状态推送), D81 (内嵌 Web), D108 (systemd watchdog)

---

## D112: CI/CD 打包扩展

**日期**: 2026-07-17
**状态**: 已决策
**类型**: CI/CD

**决策**: D-CI-01 的第 3 阶段 `build` 扩展 tarball 打包 + GitHub Releases 上传。

**修改后的 build 阶段**:
```yaml
build:
  needs: test
  strategy:
    matrix:
      target: [x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu]
  steps:
    - cargo build --release --target ${{ matrix.target }}
    - mkdir -p dist/opt/oomspbase/{bin,etc,web,logs}
    - cp target/${{ matrix.target }}/release/omspbase-host dist/opt/oomspbase/bin/
    - cp config/host.conf dist/opt/oomspbase/etc/
    - cp config/omspbase-host.service dist/
    - cp scripts/install.sh dist/
    - tar -czf omspbase-host-${{ github.ref_name }}-${{ matrix.target }}.tar.gz -C dist .
    - gh release upload ${{ github.ref_name }} omspbase-host-*.tar.gz
```

**产物**: omspbase-host-v0.1.0-x86_64-unknown-linux-gnu.tar.gz (3 个文件: install.sh + omspbase-host.service + opt/omspbase/)

**理由**: 扩展而非新增阶段 — cargo build 已在 build 阶段，打包 tarball 是零成本附加。

**关联**: D-CI-01 (3 阶段 CI/CD), D104 (tarball), D107 (安装布局)

## D113: MVP 测试架构

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 测试架构

**决策**: MVP 4 层测试体系，覆盖车端 (CameraCapture/编码/推流) + 座舱 (拉流/解码/渲染) + DataChannel 控制链路。

**4 层测试**:

| 层 | 范围 | 工具 | 运行 |
|----|------|------|------|
| 单元测试 | 单个 trait 实现，Mock 依赖 | cargo test (手写 mock，不用 mockall) | CI |
| 集成测试 | crate 间连通：CameraCapture→Encoder→Transport | cargo test --test integration | CI |
| E2E | 完整链路：V4L2→编码→WebRTC→解码→渲染 | tokio loopback | CI |
| 实车 | Jetson + 真车 + 4G 弱网 | 手动 | 每周 1 次 |

**测试目录**:
```
tests/
├── unit/         # 单元 (cargo test 默认发现)
├── integration/  # 集成 (cargo test --test)
├── e2e/          # 端到端 (tokio::test)
└── fixtures/     # I420 测试帧 + Mock 信令 JSON
```

**SDD → TDD 流程**: SDD 规范定义场景 → test_xxx.rs (RED 先写) → impl.rs (GREEN 后写) → cargo tarpaulin (80%+ 验证)

**Mock 策略**:
- 手写最小 mock struct (不要 mockall crate)
- 每个 trait 一个 mock: MockCameraCapture (预设帧序列), MockVideoEncoder (统计帧数), MockMediaTransport (loopback), MockDataChannel (命令回显)
- Mock 条件编译: #[cfg(test)] 插件 (test-utils feature)

**覆盖率**:
- 单元: 80%+ (cargo-tarpaulin)
- 集成: 70%+ (cargo-tarpaulin)
- E2E: 关键路径覆盖 (手动断言)

**关联**: D-TEST-01 (测试基础设施), D93 (MVP 验收标准), D94 (7 SDD + 4 层测试)

## D114: Host 配置文件 host.conf Schema

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 配置架构

**决策**: MVP host.conf 为 YAML 格式，6 个必要字段 + 4 个可选字段：

```yaml
# 必要
host:
  id: "host-001"
signaling:
  ws_url: "ws://192.168.1.100:8080/ws"
media:
  camera: "/dev/video0"
  width: 1280
  height: 720
  fps: 30
  bitrate_kbps: 2000
  encoder: "nvh264enc"
# 可选
turn:
  urls: "turn:192.168.1.100:3478"
  username: "user"
  credential: "pass"
web:
  bind: "127.0.0.1:9800"
  username: "admin"
  password: "changeme"
```

**部署时行为**: install.sh 生成含默认值的 host.conf，运营商编辑 camera/ws_url/bitrate。

**关联**: D104 (tarball+install.sh), D107 (install layout), D108 (systemd)

---

## D115: MVP 管线延迟预算

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 性能架构

**决策**: 720p@30fps Jetson Nano 推流延迟预算分解：

| 管线节点 | 预算 | 测量点 |
|---------|------|--------|
| V4L2 采集 | ≤8ms | GStreamer src pad probe |
| 颜色空间转换 | ≤2ms | videoconvert element |
| NVENC 编码 | ≤15ms | nvh264enc src pad probe |
| RTP 打包 | ≤2ms | appsink → libwebrtc |
| 网络 RTT (4G) | ≤50ms | RTCP SR/RR |
| JitterBuffer | ≤20ms | libwebrtc NetEq |
| 硬件解码 | ≤5ms | 座舱端 NVDEC/VAAPI |
| 渲染 | ≤10ms | frame→screen |
| **总计** | **≤110ms** | (40ms 余量到 150ms 上限) |

**超限策略**: 任一节点超过预算 2 倍时, log warning。编码节点超过 30ms → 降帧率 (30→20→15)。

**关联**: D93 (150ms 验收标准), D-OPS-01 (Prometheus 指标)

---

## D116: 安全威胁模型 (STRIDE-Lite)

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 安全架构

**决策**: MVP 3 攻击面的 STRIDE-Lite 分析：

| 攻击面 | 威胁 | 影响 | 缓解 | 残余风险 |
|--------|------|------|------|---------|
| Host 设备 (Jetson) | host.conf 篡改/物理访问 | 中 | config file 权限 600, SSH key-only | 低 |
| 网络传输 (4G) | WebSocket 劫持/SDP 注入/MITM | 高 | WSS (TLS 1.3), DTLS-SRTP, coturn long-term credential | 低 |
| 信令通道 | 未授权 SDP offer/ICE 注入 | 高 | 预共享密钥 (PSK) 验证 WebSocket upgrade | 中 |
| DataChannel | 恶意控制命令注入 | 危 | 每帧 HMAC-SHA256 (8 字节, DTLS 密钥派生) (D117) | 低 |
| 紧急停止 | 通道被 DoS/阻断 | 危 | 独立 UDP 端口, 车控器独立 listener (D117) | 低 |

**关联**: D-SEC-01 (mTLS+TLS+audit), D-SAFETY-02 (SafetyEnvelope)

---

## D117: 紧急停止 + 控制安全

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 安全架构 + 故障恢复

**决策**: 三项 MVP 安全增强：

**1. 紧急停止独立通道**:
- 车控器 (ECU) 运行独立 UDP listener (不同于 Host 进程)
- Remote → 车控器直连 UDP (不经过 Host)
- Host 的 UDP handler 是 fallback 路径, 不是主路径
- 车控器 heartbeat 超时: 150ms 无有效帧 → 自动 SoftStop, 500ms → HardStop

**2. 会话持久化恢复**:
- Host 每 10s 保存 session_state.json 到 /opt/oomspbase/etc/
- 内容: room_id, peer_fingerprint, last_ice_candidates
- 崩溃重启后: 读取 session_state → 如 < 60s stale → 自动 Re-Join → SDP 重协商
- 超过 60s → 清除, 等待新会话

**3. DataChannel 控制命令安全**:
- 每个控制帧附带 8 字节 truncated HMAC-SHA256
- 密钥: DTLS-SRTP export_keying_material("omspbase-control")
- 发送侧: send buffer > 3 帧 → 丢弃最旧的, 只发最新的
- 指标: control_frame_dropped counter

**关联**: D-SAFETY-02 (SafetyEnvelope), D-OPS-06 (优雅关闭), D102 (单进程)

---

## D-ERR-02: 错误码编号体系

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 错误架构

**决策**: C FFI 错误码枚举，5 位数字：

| 范围 | 域 | 示例 |
|------|-----|------|
| 1xxx | Transport | 1001=ICE_FAILED, 1002=DTLS_ERROR, 1003=CONNECTION_TIMEOUT |
| 2xxx | Codec | 2001=ENCODER_INIT_FAILED, 2002=ENCODE_QUEUE_FULL, 2003=DECODER_NOT_FOUND |
| 3xxx | Camera | 3001=CAMERA_NOT_FOUND, 3002=CAMERA_DISCONNECTED, 3003=UNSUPPORTED_FORMAT |
| 4xxx | Signaling | 4001=WS_CONNECT_FAILED, 4002=AUTH_FAILED, 4003=ROOM_FULL |
| 5xxx | Pipeline | 5001=PIPELINE_STALL, 5002=GSTREAMER_ERROR |
| 9xxx | Internal | 9001=OUT_OF_MEMORY, 9002=INTERNAL_ERROR |

**C FFI**: 所有 wkit_* 函数返回 int, 0=成功, 负值=错误码 (取反后为上述 5 位码)。

**关联**: D-ERR-01 (错误分类), D109 (C FFI cbindgen)

---

## D-OPS-09: MVP 告警规则

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 运维架构

**决策**: 8 条 MVP Prometheus 告警规则：

| 告警 | 条件 | 级别 |
|------|------|------|
| push_rtt_high | rtt_ms > 500 for 30s | warning |
| push_rtt_critical | rtt_ms > 1000 for 10s | critical |
| camera_lost | fps == 0 for 5s | critical |
| encoder_lag | encode_queue_depth > 5 for 10s | warning |
| gpu_mem_high | gpu_mem_pct > 90 for 30s | warning |
| signaling_lost | ws_connected == 0 for > 30s | critical |
| host_restart_loop | process restart count > 5 in 5min | critical |
| frame_dropped_rate | control_frames_dropped / total > 0.1 | warning |

**关联**: D99 (Prometheus /metrics), D-OPS-01 (Host 指标)

---

- Phase 2: 自动化回滚 (健康检查失败 → 恢复旧二进制)
## D-OPS-10: Host 升级策略

**日期**: 2026-07-17
**状态**: 已决策
**类型**: 运维架构

**决策**: MVP Host 升级流程 (手动触发, 非自动 OTA)：

```
1. 运营者上传新 tarball 到 Server
2. Server 通过 WS 推送 cmd/host/{id}/update {url, sha256}
3. Host 下载 tarball → 验证 SHA256
4. systemctl stop omspbase-host
5. 解压替换 /opt/oomspbase/bin/omspbase-host
6. systemctl start omspbase-host
7. GET /health → 200 → 升级成功
8. 失败 → 恢复旧二进制 → 启动 → 上报失败

---

## MVP 范围变更 v2 (2026-07-17)

### D118: MVP 架构 v2 — Host → Server → Remote 三组件

**日期**: 2026-07-17
**状态**: 已决策
**类型**: MVP 范围变更

**变更**: MVP 从 Host(Jetson) P2P → Remote 改为 Host(跨平台) → Server(relay+信令+监控) → Remote(跨平台拉流+控制)。

**新架构**:
```
Host (macOS/Linux/Windows)
  │ WebRTC push
  ▼
Server (omspbase-server)
  │ relay + signaling + monitoring
  │ WebRTC forward
  ▼
Remote (macOS/Linux/Windows)
  decode + render + DataChannel control
```

**三大变更**:
1. Server 从 Phase 2 监控面板升级为 Phase 1 核心 relay 引擎（信令+媒体中继+监控合一）
2. Host 从 Jetson 嵌入式扩展为跨平台桌面（macOS/Linux/Windows），采集源扩展为屏幕捕获+摄像头
3. Remote 从验证客户端升级为一等产品组件（跨平台拉流+控制）

**受影响决策** (CRITICAL):
- D15: P2P 优先 → Server relay 优先
- D86: Server 纯监控 → Server relay+信令+监控
- D73: Jetson/Ubuntu 20.04 → macOS+Linux+Windows
- D-HW-01: Jetson Nano 基线 → 跨平台桌面基线

**受影响决策** (HIGH): D12, D17, D52, D54, D92, D93, D96, D97
**受影响决策** (MEDIUM): D56, D77-D78, D87-D90, D102, D104, D107-D108, D114-D116, D-HW-02
**作废**: D95 (LiveKit SFU — Server 自建 relay 替代)

**新增产物**: crates/omspbase-server, crates/omspbase-client, config/server.conf, config/remote.conf, docs/modules/13-server-architecture.md, docs/modules/14-remote-architecture.md

**关联**: 原 MVP 提案 .sisyphus/plans/mvp-host-remote/ 需重写为三组件架构
```

**版本兼容**: Host/Server 主版本号必须一致 (v0.1 Host ↔ v0.1 Server)。版本号嵌入二进制 (cargo vergen)。

**关联**: D104 (tarball), D107 (install layout), D108 (systemd)

---

## D-AUDIT-01 — 文档审计 2026-07-17

**状态**: ✅
**日期**: 2026-07-17

全项目文档体系审计，4 路并行团队 (ultrabrain × 4)：decision-validator, consistency-checker, reference-crosschecker, gap-optimizer。

**结果**：58 项发现 → 54 项确认修复。
- 🔴 CRITICAL 8 项：AGENTS.md 过时、架构文档缺失、Tauri/Electron 歧义等
- 🟠 HIGH 12 项：架构同步状态、产业模式遗漏、运维规范
- 🟡 MEDIUM 20 项：文档完整性、操作可靠性
- 🔵 LOW 18 项：术语统一、Phase 2/3 标注

**已应用修复**（全部 54 项）：
- AGENTS.md 重写：移除 Phase 0/零源代码，CODE MAP 更新为 3 crate 状态
- architecture.md 大修：MVP v2 架构、iceoryx2/Docker 状态、Tauri v2 统一、QUIC/Simulcast 标注
- 新建：13-server-architecture.md、14-remote-architecture.md、7 篇 SDD、CI workflow
- 模块文档修复：MQTT、NAPI、OBS 描述更新
- 基础设施补充：硬件基线 (D-HW-03)、circuit breaker 设计、配置迁移机制
- cargo build 验证：0 errors

**唯一延后**：遥操作多层级安全架构 → Phase 2

**关联**: D118, architecture.md, docs/doc-audit-2026-07-17.md

---

## D-MVP-EXEC — MVP 实施计划确认

**状态**: ✅
**日期**: 2026-07-17

确认 MVP Phase 1 实施计划：Host → Server → Remote 三组件遥操作 v2。

**计划概要**：
- 32 任务，~3300 行 Rust
- 4 个 workspace crate：omspbase-core + omspbase-host + omspbase-server + omspbase-client
- 7 篇 SDD 覆盖核心模块已有
- 测试 ≥80% 覆盖率，4 层策略

**执行路线**：
1. Phase 0: 基础设施（workspace + core crate + protocol + auth）
2. Track A/B/C 并行：Host 采集推流 / Server relay 信令 / Remote 拉流控制
3. Phase I: 三组件链路集成
4. Phase T: 单元测试 + E2E + 覆盖率

**Phase 1 不包含**：Docker 部署、CI/CD、SFU 多人、音频、录制、ONVIF、RTMP/SRT/HLS、Admin Dashboard、napi-rs、C/C++ SDK、插件系统

**架构基础设施说明**：Component 框架、Gateway、AuthProvider 属于 Phase 1 架构演进（基础设施重构），不是新功能特性。不改变上述功能排除列表。

**技术栈**：Rust 2024, webrtc 0.11 (livekit/webrtc-rs, libwebrtc backend), GStreamer 0.23, axum WS, serde_yaml, tracing, prometheus-client, PSK HMAC-SHA256

**关联**: D118, .sisyphus/plans/mvp-host-remote/proposal.md, design.md, tasks.md

---

## D-HW-03 — 桌面硬件基线 (Phase 1)

**状态**: ✅
**日期**: 2026-07-17
**Phase**: 1

补充 D118 后 macOS/Linux/Windows 桌面最低硬件要求。

| 平台 | CPU | RAM | GPU | OS 最低版本 |
|------|-----|-----|-----|------------|
| macOS | Apple Silicon M1+ or Intel i5+ | 8GB | 集成 GPU (VideoToolbox) | macOS 13+ (VideoToolbox hardware decoding supported since macOS 13) |
| Linux | x86_64, 4 核 | 8GB | VAAPI 支持 (Intel HD 620+) | Ubuntu 20.04+ |
| Windows | x86_64, 4 核 | 8GB | NVENC (GTX 1050+) 或 DXGI | Windows 10+ |

**说明**：运行完整三组件 (Server + Host + Remote) 在同一台桌面机上需要 8GB+ RAM。Server 纯软件 relay，CPU 即可。Host/Remote 需要 GPU 硬件编解码。

**关联**: D-HW-01 (Jetson 基线), D118 (跨平台扩展), D73 (最低 Ubuntu 20.04)

---

## D119 — MVP Phase 1 实施完成

**状态**: ✅
**日期**: 2026-07-17

MVP Phase 1 (Host→Server→Remote 三组件遥操作 v2) 全部 32 任务实施完成。

**4 个 workspace crate**:
- omspbase-core: config, error, metrics, protocol, auth (5 模块, 41 tests)
- omspbase-host: pipeline, transport, session, signaling, emergency, control, metrics, main (9 模块, 13 tests)
- omspbase-server: room, signaling, relay, monitor, main (8 模块, 46 tests)
- omspbase-client: transport, decode, control, signaling, main (8 模块, 11 tests)

**关键实现**:
- PSK HMAC-SHA256 认证，8 字节 tag，常量时间比较
- SignalingMessage 9 变体协议
- DashMap 房间管理 (join/leave/get_other_peer/full-room lifecycle)
- Host 平台自适应编码管线 stub (vt264enc/nvenc/vaapi/x264)
- Remote GStreamer decode 管线 stub (appsrc→decodebin→autovideosink)
- UDP 紧急停止 fallback (port 9999)
- Host/Remote WebSocket 信令客户端 (tokio-tungstenite 0.24)
- Server lib+bin 双模式
- Session 持久化 (JSON)

**测试**: 111 passed (8 suites, 0.12s)。tarpaulin.toml fail-under=50。

**待 Phase 2**: WebRTC PeerConnection 真实集成、GStreamer 平台验证、插件系统、napi-binding

**技术栈**: Rust 2024, webrtc 0.11 (livekit/webrtc-rs), GStreamer 0.23 (feature-gated), axum 0.7 WS

**关联**: D118, D-MVP-EXEC, .sisyphus/plans/mvp-host-remote/

---

## D124 — webrtc test补齐

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

WebRTC crate 补齐 18 tests。

**测试清单**:
- sdp.rs: 3 tests — display_each_type (4 SdpType 枚举值), session_description_new, serde_camel_case_roundtrip
- track.rs: 2 tests — new_sets_fields, clone_preserves_fields
- channel.rs: 4 stub tests — stub_data_channel_state_is_closed, stub_label_and_id, stub_send_is_noop, stub_data_channel_init_defaults
- peer.rs: 9 stub tests — stub_factory_default_creates, stub_create_offer/answer, stub_sdp_operations, stub_add_ice_candidate, stub_create_data_channel, stub_connection_states, stub_close, pc_config_default

**关联**: D121 (webrtc-rs 0.12 rewrite), D-CI-01 (CI 中 workspace 测试覆盖)

---

## D125 — PipelineEngine hot-plug 边界测试

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

PipelineEngine 从 6 测试扩展到 11 测试，补齐热插拔边界场景。

**新增 5 测试**:
- `engine_remove_then_readd_same_id`: 运行时移除后以相同 ID 重新添加
- `engine_remove_nonexistent_errors`: 删除不存在的链返回错误
- `engine_idempotent_start`: start() 两次调用不 panic 不双倍产出
- `engine_hot_add_with_processor`: 运行中添加带处理器的链
- `engine_remove_all_chains_survives`: 运行中移除所有链后 stop() 不 panic

**关联**: D120 (PipelineEngine 设计), D-TEST-01 (测试基础设施)

---

## D126 — 三层逻辑抽象模型 (Plugin / Component / Process)

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

（区别于 D1 三层部署拓扑架构——D126 是代码维度的逻辑分层，D1 是部署维度的拓扑分层。二者互补，具体消歧参见 C3 约定）
OMSPBase 架构采用三层抽象模型，明确 Plugin 与 Component 的边界。

**层级定义**:
- Layer 1 (管线层) Plugin: 媒体管线元素。接口为 MediaPort（帧队列）。由 PipelineEngine 管理。
  - 例: H264EncoderPlugin, ScreenCapturePlugin, RtmpPublisherPlugin
  - 组合方式: 串入 Pipeline 链
- Layer 2 (服务层) Component: 有独立生命周期的服务级单元。接口为 Port（消息 via Bus）。由 ComponentManager 管理。
  - 例: SignalingComponent, AdminComponent, RecordingComponent
  - 组合方式: 通过 ComponentBus 网状通信
- Layer 3 (部署层) Process: OS 进程，承载 Components 运行。

**关键区别**:
- Plugin 回答「怎么做」（如何编码 H.264）
- Component 回答「做什么」（管理 WebRTC 信令）
- Component 内部可持有 Plugin 实例（通过 PipelineEngine）

**部署映射**:
- 单体: 1 Process 包含 N Components，Bus=SHM
- 多进程: N Processes 各含 1 Component，Bus=Unix DS
- 集群: N Processes 跨主机，Bus=QUIC

**关联**: D1 (三层架构), D127 (Component crate), D128 (Gateway)


---

## D127 — Component trait 独立 crate (omspbase-component)

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

Component trait 定义在独立 crate `crates/omspbase-component/` 中，不放 omspbase-core 或 omspbase-server。

**方案对比**:
- 方案 A (server 内): 零依赖，最简单。但 host/remote 无法复用 Component 框架。Ref: OBS (plugin API 在 libobs)。推荐度: ⭐⭐
- 方案 B (独立 crate) ✅: Component 概念独立于 server，host/remote 可选复用。Ref: ROS2 (rcl 独立于 rclcpp)，Zenoh (核心协议独立于传输层)。推荐度: ⭐⭐⭐⭐
- 方案 C (core 中): Component 和 Plugin 同属核心但层级不同——Plugin 是管线层，Component 是服务层。揉在一起会模糊概念边界的。Ref: Janus (Plugin API 在 janus.c)。推荐度: ⭐

**影响**:
- Phase 1 新增 `crates/omspbase-component/` (~200-400 lines)
- Phase 2 加 ZenohBus 实现，Component trait 不变

**关联**: D126 (三层模型), D128 (Gateway), D129 (Bus 策略)

---

## D128 — 统一 HTTP Gateway 模式

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

Server 使用单一端口暴露，内部通过 Gateway Component 路由到各业务 Component。

**与 D52 的关系**：D128 的 Gateway 是 D52 单端口信令服务的演进形式——D52 定义的 axum 服务被 Gateway 吸收，SignalingComponent 负责 WS 信令内部处理。

**方案对比**:
- 方案 A (各自绑端口): 零额外跳转，简单。但多端口运维复杂、安全面分散、各 Component 重复 auth 中间件。Ref: Janus (每 transport 各自 port), SRS (RTMP+HTTP 各自端口)。推荐度: ⭐⭐
- 方案 B (统一 Gateway) ✅: 单一端口、集中鉴权限流、Component 不感知外部协议。多一跳成本可忽略（Zenoh SHM 134ns）。Ref: Envoy (Gateway 模式), Kong/APISIX (API Gateway)。推荐度: ⭐⭐⭐⭐

**路由映射**:
- /ws → 升级 WebSocket → SignalingComponent
- /admin/api/* → REST → AdminComponent
- /health → 健康检查 → MonitorComponent
- /admin/* → SPA 静态文件

**影响**: Gateway Component 是 Phase 1 首批实现的 Component。需要路由 DSL 映射 URL → Component query。

**关联**: D127 (Component trait), D87 (React + Ant Design Admin UI)

---

## D129 — 通信中间件策略: tokio::mpsc → Zenoh

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

ComponentBus 分两阶段实现。

**方案对比**:
- 方案 A (mpsc Phase 1 → Zenoh Phase 2) ✅: 最快推进。Component trait 边界从 Day 1 就清晰。Ref: OBS (先编译期模块后 dlopen), ROS2 (先 DDS 后 rmw_zenoh)。缺点: 局部迁移成本。推荐度: ⭐⭐⭐⭐
- 方案 B (直接 Zenoh): 一步到位，但集成成本高 (zenoh crate 3.1MB, 学习概念)。拖慢 Phase 1 交付。Ref: PX4, Toyota Woven。推荐度: ⭐⭐⭐
- 方案 C (先跑功能后重构): 绝对最快，但紧耦合拆 Component 是重写而非重构。大量项目教训不支持。推荐度: ⭐

**Zenoh 选型依据**: 统一 API (pub/sub + query + key-value), 零拷贝 SHM (134ns), Peer 模式 (no daemon), Rust 原生, 生产验证 (Toyota/PX4/Eclipse)。1-2 数量级优于 MQTT, 2x DDS 吞吐。

**Bus 后端矩阵**:
- Phase 1: InProcessBus (tokio::mpsc) — 单进程函数调用
- Phase 2: LocalBus (Unix Domain Socket) — 多进程同机
- Phase 2.5: RemoteBus (Zenoh QUIC) — 跨主机集群

**关联**: D127 (Component trait), D128 (Gateway)

---

## D130 — 鉴权架构: AuthProvider trait

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

鉴权采用 trait 抽象模式，支持本地鉴权 + 外部鉴权替换。

**RBAC 对齐**：AuthProvider 的 authorize() 方法预定义 3 角色（admin/operator/auditor）和功能权限枚举（host:read、host:control 等），对齐 D88 的 RBAC 模型。PskAuthenticator（auth.rs）标记为 Phase 1 过渡——用于 WebSocket PSK 握手，AuthProvider 处理 HTTP JWT 认证。两者共存但职责不重叠。

**方案对比**:
- 方案 A (Server 自带鉴权): SQLite + JWT，零外部依赖。适合独立部署。缺点: 不能对接企业 SSO/AUDEBase RBAC。Ref: SRS (内建 HTTP API auth)。推荐度: ⭐⭐
- 方案 B (双模式 trait) ✅: AuthProvider trait，默认 LocalAuth 实现，可替换为 LdapAuth/OidcAuth/AUDEBaseAuth。AuthMiddleware 不依赖具体 Provider。Ref: nginx auth_request 模块 (可替换认证后端), Ory Kratos (IdentityProvider 接口)。推荐度: ⭐⭐⭐⭐
- 方案 C (纯外部鉴权): 完全依赖 OIDC。Server 无用户表，轻量但独立部署困难。推荐度: ⭐⭐

**AuthProvider trait**: login(credentials) → JWT token。validate(token) → User。authorize(user, permission) → bool。

**AuthMiddleware**: 集成在 Gateway Component 的 axum router 中。验证 JWT 后注入 User 到 request extensions。Component 从 context 取 User，不关心认证来源。

**关联**: D128 (Gateway), D127 (Component trait)

---

## D131 — Component 生命周期: 简单 3 阶段

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

Component 采用 init→run→shutdown 三阶段。

**方案对比**:
- 方案 A (3 阶段) ✅: 最灵活，适应多样的 Component 类型。Ref: tokio_service, axum::Router。推荐度: ⭐⭐⭐⭐
- 方案 B (消息驱动 actor): init→start→handle_message 循环。约束大。Ref: Erlang gen_server。推荐度: ⭐⭐
- 方案 C (事件驱动): register_interests→handle_events。需 Key Expression (Phase 2)。Ref: Zenoh。推荐度: ⭐⭐

**关联**: D127 (Component trait)

---

## D132 — 双模式消息路由 (RPC + Event)

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

ComponentBus 提供 send_rpc (1:1) + publish/subscribe (1:N) 双模式。

RPC: Signaling→Relay, Signaling→Recording, Admin→Signaling
Event: peer_joined/left, metrics, shutdown notifications

Ref: ROS2 (topic + service), Zenoh (pub + query)。推荐度: ⭐⭐⭐⭐

**关联**: D127, D133

---

## D133 — Channel-per-type 类型安全 RPC

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

InProcessBus 使用 register_rpc_handler::<Query, Reply>() 建立类型安全的 mpsc 通道。

**序列化策略**：Phase 1 InProcessBus 直接传递 Rust 类型（零序列化开销）。Phase 2 升级到 ZenohBus 时，序列化策略由 Bus 实现层决定——可选用 FlatBuffers（延续 D10）或 Zenoh 原生序列化。Component trait 层不感知序列化细节。

编译期保证类型匹配，零序列化开销。Phase 2 Zenoh 升级时 register_rpc_handler 改为注册 Queryable，API 层不变。

Ref: actix (Message type registration), ROS2 service (type-pair)。推荐度: ⭐⭐⭐⭐

**关联**: D127, D132

---

## D134 — 简化监督树 (ComponentManager single-level)

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

ComponentManager 提供 simplified supervision: 监控 JoinHandle, crash-loop 防护 (max_restarts + 时间窗口), 状态机 (Created→Initializing→Running→Crashed/Stopped)。

不实现 Erlang 式的 hierarchical supervisor (one_for_all/rest_for_one)，单进程场景不需要。Phase 2 多进程时切换 External Supervisor (systemd/K8s)。

**双层监督说明**：Phase 2 多进程部署时，每个进程内部保留 ComponentManager 监督其 Components（内层监督）；进程间监督由外部 Supervisor——systemd Restart=on-failure 或 K8s restartPolicy——负责（外层监督）。内层 Manager 不跨进程管理。

Ref: systemd (Restart=on-failure), Kubernetes (restartPolicy)。推荐度: ⭐⭐⭐⭐

**关联**: D127 (Component trait), D126 (三层模型)

---

## D136 — MVP 调整：三段式推进 (Component→Admin→Plugin)

**Phase 标签映射**: D136 Phase 1a = consolidated-mvp/plan.md Phase 3, D136 Phase 1b = plan.md Phase 4, D136 Phase 1c = plan.md Phase 5 (per D142 采纳计划编号)

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1

将原计划调整为三段式推进，服务于"让用户看得见"的 MVP 叙事。

**Phase 1a — Component 框架精简版**：仅实现 Admin Dashboard 所需的 Component 基础设施（~8 tasks，从原 24 tasks 裁剪）。
- Component trait + ComponentError + ComponentContext
- InProcessBus（RPC only，延迟 pub/sub）
- GatewayComponent（硬编码路由表）+ AuthComponent（JWT）+ AdminComponent（REST API）
- ComponentManager（单层监督）
- 砍掉：SignalingComponent 包装、RelayComponent 包装、完整 pub/sub、路由 DSL

**Phase 1b — Admin Dashboard**：React 19 + Ant Design 5 SPA，依赖 Phase 1a 的 AuthProvider + AdminComponent。
- 服务器状态、会话列表、房间管理、系统指标
- rust-embed 嵌入编译产物

**Phase 1c — 插件系统**：实现 PluginManager::create_node()，dlopen 动态加载，一个真实 Plugin 验证。

**关联**: D126-D134 (Component 决策), D87 (Admin UI 技术栈), D-MVP-EXEC (原 MVP 范围)

---

## D137 — WebRTC 架构升级：DataChannel → RTP 媒体轨道

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 1 (优先级最高)

**决策**: 当前 DataChannel 传媒体模式升级为 RTP 媒体轨道（TrackLocal/TrackRemote），无新依赖（webrtc-rs 0.12 已支持）。

**原因**：
- 多路视频流需要独立的 media track，DataChannel 无法多路复用
- SFU 服务器转发需要 track 级别的感知，bridge_tracks 内部逻辑完善依赖 on_track 回调
- 音频需要标准的 Opus RTP track，DataChannel 非为此设计
- 浏览器监控（Admin Dashboard）依赖浏览器原生 WebRTC track

**改造范围**：
- omspbase-webrtc: TrackLocal/TrackRemote 包装，add_track/on_track/add_transceiver API
- Host: 从 DC 切换为 RTP track（单视频先验证，再扩展到多路）
- Remote: 从 on_data_channel 切换为 on_track
- Server: bridge_tracks 从 stub 升级为实际 track 转发

**关联**: D-MVP-EXEC, D136 (三段式推进)

**⚠️ 修正 (2026-07-19)**: 默认 WebRTC 后端为 `webrtc-sys` (libwebrtc C++ FFI, D11)，非 webrtc-rs (pure Rust)。Host/Remote 使用 webrtc-sys，Server 使用 mediasoup SFU。`omspbase-webrtc` 封装层需实现 `MediaTransport` trait 支持三后端编译期切换 (D32)。

---

## D138 — mediasoup SFU 架构确认

**状态**: ✅
**日期**: 2026-07-19
**Phase**: 2

**决策**: OMSPBase Server 使用 mediasoup 作为 SFU 引擎 (v0.22, versatica/mediasoup, 7.3K ⭐)。mediasoup-sys Rust 绑定 (133K downloads) 包装 C++ Worker，提供 Router/Transport/Producer/Consumer 完整 API。

**原因**：
- mediasoup Rust crate 官方维护且生产级 (v0.22.12, 2026-07-16 更新)
- Worker/Router/Transport/Producer/Consumer 领域模型与 OMSPBase 多场景需求匹配
- 自动 RTP 转发 (zero manual bridge_tracks 代码)
- Simulcast/SVC 内置，带宽自适应
- 信令无关——Server 仍使用自定义 WebSocket 协议

**架构分层**：
- Client 端 (Host/Remote): webrtc-sys (libwebrtc C++ FFI) 或 webrtc-rs (embed 轻量)
- Server 端: mediasoup-sys (C++ Worker) → Router per room → Transport per peer
- 客户端 SDP 与 mediasoup Transport 参数需要薄适配层 (~200 行)

**依赖**：
- `mediasoup = "0.22"` + `mediasoup-sys` (C++ Worker FFI)
- 移除 Server 端的 `omspbase-webrtc` + `webrtc = "0.12"` 依赖性

**关联**: D97 (mediasoup SFU 引擎), D11 (三后端), D32 (编译期分发), D137 (RTP track)

---

## D139 — 后端特性命名修正

**状态**: ✅
**日期**: 2026-07-19

**决策**: D11/D32 中的 `backend-libwebrtc` 更名为 `backend-webrtc-sys`，与 crate 名一致。

**新命名方案**：
- `backend-webrtc-sys`: libwebrtc C++ FFI via LiveKit webrtc-sys crate (默认)
- `backend-webrtc-rs`: 纯 Rust webrtc-rs，embed/轻量平台
- `backend-str0m`: sans-I/O, AUDESYS embed 零依赖

**关联**: D11, D32

## D140 — Phase 0: 三后端 Feature Gate 实施策略

**状态**: ✅
**日期**: 2026-07-19

**决策**: Phase 0 只实现 `backend-webrtc-sys` (libwebrtc C++ FFI) 作为默认后端。`backend-webrtc-rs` 已有 struct 骨架 (`WebrtcRsPc` 实现 PcBackend trait 的方法，部分为完整实现，部分待适配)，`backend-str0m` 推迟至 Phase 2+。

**原因**:
- 三后端 (sys/rs/str0m) 类型体系完全不同，不是简单的 `#[cfg(feature)]` 替换
- `MediaTransport` trait (D11/D32) 当前不存在，三后端抽象层是 Phase 2+ 的工作
- Phase 0 目标是尽快提供可用的 RTP Track API，不应在三后端设计上阻塞

**关联**: D11, D32, D137, D139

---

## D141 — F6 删除：DC 媒体路径不在 omspbase-webrtc crate

**状态**: ✅
**日期**: 2026-07-19

**决策**: 从 Phase 0 删除 F6 (移除 DataChannel 媒体路径)。omspbase-webrtc crate 的 `channel.rs` 是通用 DC 封装，无 media-specific 代码。真正的 DC 媒体发送/接收逻辑在 Host (`host/src/webrtc_transport.rs:202`) 和 Remote (`remote/src/webrtc_transport.rs:65`)，由 Phase 1 T1/T4 完成 DC→RTP 迁移。

**关联**: D137, 整合计划 Phase 0/1

---

## D142 — 整合 MVP 实施计划采纳

**状态**: ✅
**日期**: 2026-07-19

**决策**: 采纳整合计划 `.sisyphus/plans/consolidated-mvp/plan.md` (51 任务, 6 阶段)，合并此前 4 个独立计划 (mvp-host-remote, webrtc-rtp-track, component-framework-phase1, adjusted-mvp)。

**计划结构**:
- Phase 0: Foundation — webrtc-sys Track API (8 tasks, 1 removed)
- Phase 1: Transport — Host/Remote RTP 迁移 (7 tasks)
- Phase 2: Server — mediasoup SFU (18 tasks)
- Phase 3: Components — 精简框架 (10 tasks)
- Phase 4: Admin — Dashboard SPA (7 tasks)
- Phase 5: Plugin + 集成测试 (7 tasks)

**已知风险 (已审计通过)**:
- Phase 0: MediaTransport trait 新定义, feature gate 三后端策略
- Phase 1: Host webrtc-rs 直接依赖, Remote GStreamer pipeline 未就绪, SDP m= 行变化
- Phase 2: mediasoup-sys 构建依赖, SDP↔mediasoup 适配复杂度
- Phase 3-5: 无技术风险

**关联**: D136, D137, D138, D139

---

## D143 — Phase 1-2 已知风险接受

**状态**: ✅
**日期**: 2026-07-19

**决策**: 整合计划审查中发现的 6 个实施风险已审计通过，不阻塞 Phase 0 启动。风险缓解策略记录在计划文档中各 Phase 的 "⚠️ 已知风险" 部分。

**关联**: D142, 整合计划

---

### D144: 借鉴 webrtc-kit trait 抽象模式

**决策**: 借鉴 webrtc-kit 项目的多后端 trait 抽象模式，将其以下设计原则移植到 omspbase-webrtc 的多后端架构中: 
  a) feature-gate + compile_error! 多后端互斥守卫
  b) RtcEngine::create_factory() 编译期 cfg dispatch 入口
  c) W3C API 命名兼容 (createOffer/createAnswer/addTrack/onTrack/...) 
  d) Phase 0 用 struct 非 trait（当前仅 1 个工作后端，trait 是过早抽象）


### D145: Phase 0 PeerConnection struct 而非 trait

**理由**: 当前仅 webrtc-sys 一个工作后端，定义 trait 是过早抽象。Phase 0 用 concrete struct 实现，Phase 2 多后端 (webrtc-rs/str0m) 工作后再提取 trait。这符合 YAGNI 原则——不为未来需求提前建立抽象。

**修订 (2026-07-22)**: 本决策的 struct 非 trait 指 PcBackend 内部后端层——Phase 0 后端实现用 concrete struct 直接对接 webrtc-sys，不引入后端的 trait 抽象。对外 API 层 RTCPeerConnection 定义为 pub trait 是为了提供稳定 W3C 契约，与后端层 trait 抽象不是同一概念。详见 docs/modules/17-webrtc-crate.md。

**关联**: D11, D32, D139, D144

### D146: W3C API 命名兼容

**决策**: omspbase-webrtc 的 PeerConnection API 采用 W3C WebRTC 标准命名。完整方法列表: createOffer/createAnswer/setLocalDescription/setRemoteDescription/addIceCandidate/createDataChannel/addTrack/addTransceiver/onTrack/getSenders/getReceivers/getStats/close。

**理由**: 降低开发者学习成本，与浏览器 API 一致，未来与 napi-rs 绑定无缝对接。

**关联**: D144

### D147: AudioTrack 设计

**决策**: TrackLocal/TrackRemote 支持 MediaKind::Audio。音频编码使用 Opus (samplerate=48000, channels=2)。Host 端 GStreamer audioconvert+opusenc。Remote 端 opusdec+audioconvert。多麦场景支持多 AudioTrack per PeerConnection。

**玛关**: D138 (mediasoup SFU 支持音频 relay)

### D148: 多轨管理策略

**决策**: PeerConnection 内部维护 HashMap<String, TrackRef> 轨道注册表，track_id 由 Host 端分配。add_track 注册→ on_track 回调触发→ close 清理。每个 track 独立 ssrc。轨道数量上限: 8 per PeerConnection（多摄像头 + 多麦）。

**关联**: D137

### D149: DataChannel 保留策略

**决策**: DataChannel (channel.rs) 保留用于控制指令 (emergency stop, telemetry, signaling augmentation)。不削除。RTP media track 独立管道承载音视频流。这与 D-F6-REMOVED 一致——删除的是 media-specific DC 方法，非整个 DataChannel。

**关联**: D141, D-F6-REMOVED

### D150: omspbase-core 零 WebRTC 依赖

**决策**: omspbase-core crate 不依赖任何 WebRTC crate (webrtc-sys/webrtc-rs/mediasoup-sys)。MediaTransport trait 定义在此作为纯抽象接口，由 omspbase-webrtc 实现。这保证 core 可独立编译测试，不受 libwebrtc 构建链影响。

**Phase 0 策略** (D150-a, CR2): # ponytail: Phase 0 时 MediaTransport trait 在 omspbase-webrtc 内联定义，待 omspbase-core crate (Phase 2+) 建成后迁入。当前仅 1 个工作后端，跨 crate trait 是过早抽象。

**关联**: D11, D32, D139

### D151: RtcEngine::create_factory() cfg dispatch 入口

**决策**: omspbase-webrtc 入口点为 `RtcEngine::create_factory()`，内部通过 `#[cfg(feature = "backend-webrtc-sys")]` 等 compile-time dispatch 创建对应后端。Phase 0 仅 backend-webrtc-sys 有实现，其他分支 compile_error! 占位。这确保不依赖 dyn trait 动态分发，编译期确定后端。

**关联**: D144, D139, D145


---

### D152: nginx-rtmp-module hooks 预留

**决策**: 承认 nginx-rtmp-module 的 RTMP 生命周期钩子模式 (on_publish/on_play/on_done/on_update)。Phase 2 RTMP/SRT 协议支持时设计通用 StreamLifecycleHook trait，映射 on_publish→StreamStarted、on_play→StreamSubscribed、on_done→StreamEnded。Phase 0-1 不实现。

**关联**: docs/reference/streaming/nginx-rtmp-module.md §7, Phase 2 streaming

### D153: Pion interceptor pipeline 评估

**决策**: 承认 Pion 的链式 Interceptor 管道模式 (Interceptor.NewChain with Stats/NACK/TWCC/REMB)。Phase 0-2 RTP 包级处理由 webrtc-sys (Phase 0-1) 和 mediasoup C++ (Phase 2) 内部完成，不需要自定义 interceptor。Phase 3+ 自定义 RTP 处理（录制分析、自定义拥塞控制、转码）时采纳 interceptor 链模式。

**关联**: docs/reference/streaming/pion.md §7.1, D24, D138

---

**状态**: ✅
**日期**: 2026-07-19

**决策**: nginx-rtmp hooks 作为 RTMP 协议支持的预留设计。Pion interceptor pipeline 作为 Phase 3+ RTP 处理模式参考。

**关联**: D142, 整合计划, 审计 (M3, H4)---

## D154 — crate 重命名: host→remote-host, remote→remote-client

**状态**: ✅
**日期**: 2026-07-19

**决策**: 采纳用户命名方案，全 workspace 重命名:
  - `crates/omspbase-host/` → `crates/omspbase-host/` (field/vehicle 侧，推流端)
  - `crates/omspbase-remote/` → `crates/omspbase-client/` (cockpit/operator 侧，拉流端)
  - Phase 2+ 前瞻: `omspbase-remote-c` → `omspbase-client-c`, `omspbase-remote-sdk` → `omspbase-client-sdk`

**理由**: 与远程桌面工业惯例 (Parsec ParsecHost/ParsecClient, RustDesk Controller/Controlled host, Sunshine Host/Client) 对齐:
  - host = 被控制/被监控侧 (推流) = field/vehicle 侧
  - client = 控制/监控侧 (拉流) = cockpit/operator 侧
  - 避免领域特化术语 (vehicle/cockpit 过于狭窄)
  - 避免与服务端混淆 (server 有 HTTP 语义歧义)

**影响范围**: 15 文件, ~84 行 + 3 目录重命名
**验证**: cargo build 0 errors, cargo test 135/135 通过

**关联**: D68, D76, C3
---

## D155 — Host 单体架构确认 (GStreamer + webrtc-sys 共存)

**状态**: ✅
**日期**: 2026-07-19

**决策**: remote-host 采用单体架构，单一二进制同时包含 GStreamer pipeline + webrtc-sys WebRTC 传输 + axum HTTP/WS + 信令 + 监控。不拆分为多进程 (D102 已有结论)。

**GStreamer + webrtc-sys 共存评估**:
- **无冲突**: GStreamer (C, glib, dynamic .so) 与 libwebrtc (C++, static .a) 操作在不同栈层
- **接口耦合**: 纯 `&[u8]` 字节传递 — GStreamer appsink 产出 H.264 bytes → TrackLocal::write_frame → libwebrtc RTP 封包
- **无内存共享**: GStreamer 用 glib malloc, libwebrtc 用 C++ new, 数据通过 Rust 所有权的 buffer 传递
- **无符号冲突**: 动态链接 vs 静态链接, 命名空间天然隔离
- **Feature gate**: `gstreamer = { optional = true }` — 无 GStreamer 的嵌入式平台直接排除

**工业先例**: OBS Studio (libobs + libwebrtc), Sunshine (FFmpeg + WebRTC), 无数 FFmpeg + libwebrtc 推流项目

**真实风险** (非 GStreamer/WebRTC 冲突):
1. 构建复杂 — webrtc-sys 编译 libwebrtc + GStreamer 系统依赖
2. 二进制体积 — 静态 libwebrtc 膨胀 (已有 LTO+strip)
3. 单点崩溃 — Phase 1 systemd restart (D102), Phase 2 多进程 (D103)

**关联**: D102, D103, C5

## D156: P0 — PcBackend default method 模式 (from webrtc-kit)

**决策**: PcBackend trait 上 5 个方法提供 default 实现（no-op/空返回值），调用者无需 match 后端
**日期**: 2026-07-20
**原因**:
- webrtc-kit 的 PeerConnection trait 通过 default fn body 减少后端样板代码
- set_on_data_channel / set_on_track: default 为空回调
- gather_complete: default 为 Ok(())
- get_stats: default 为 vec![]
- add_transceiver: default 为 Err(Internal("not implemented"))
- 调用者始终假设方法存在，后端按需覆盖

## D157: P1 — RTCStats + RtpParameters 类型定义 (from webrtc-kit)

**决策**: 定义 stats.rs (RtcStatsReport/RtcInboundRtpStreamStats 等) 和 rtp_params.rs (RtpCodecParameters 等)
**日期**: 2026-07-20
**原因**:
- 类型层级: RtcStatsType 枚举 → RtcStats 枚举 → 具体 struct (RtcInboundRtpStreamStats/RtcRemoteOutboundRtpStreamStats/RtcTransportStats/RtcDataChannelStats/RtcPeerConnectionStats)
- RtpCodecParameters: payload_type, mime_type, clock_rate, channels, sdp_fmtp_line
- RtpEncodingParameters: ssrc, active, max_bitrate, scalability_mode 等
- pub mod stats / pub mod rtp_params 通过 lib.rs re-export
- PcBackend::get_stats() default returns vec![]; 实际报告从后端填充

## D158: webrtc-sys backend (LiveKit libwebrtc FFI)

**决策**: 基于 webrtc-sys (LiveKit's libwebrtc CXX FFI) 创建 backend/webrtc_sys.rs
**日期**: 2026-07-20
**原因**:
- 三层后端: stub (测试用) / webrtc-rs (pure Rust) / webrtc-sys (libwebrtc FFI)
- webrtc-sys API 是 callback-based (create_offer/answer 通过 ctx + success_fn/error_fn)
- oneshot channel 桥接: 回调→async。make_ctx()/extract_tx() 辅助函数
- 类型映射: PeerConnectionState/IceConnectionState/DataState/SdpType 枚举映射
- WebrtcSysPc/Dc/Track/Factory 分别实现各后端 trait
- track 写入是 no-op stub (ponytail: 需要 video_frame 模块)
- compile_error! guard 阻止 webrtc-rs + webrtc-sys 同时启用

## D159: macOS -ObjC linker flag

**决策**: .cargo/config.toml 为 macOS 目标添加 -ObjC 链接器标志
**日期**: 2026-07-20
**原因**:
- webrtc-kit 在 mac 平台可运行 p2p egui 示例，根因是 .cargo/config.toml 中的 -ObjC
- libwebrtc 内部使用 ObjC categories (NSString+StdString)，无 -ObjC 时被链接器 dead-strip
- 运行时报错: NSInvalidArgumentException — unrecognized selector sent to instance
- webrtc-kit 的解决: .cargo/config.toml 中 mac target 添加 -ObjC -Wl
- 应用到 OMSPBase: [target.x86_64-apple-darwin] / [target.aarch64-apple-darwin]
- cxx crate 也需要 -ObjC 用于 ObjC++ bridge

## D160: loopback 测试基础设施

**决策**: 创建 tests/common/loopback.rs 及 tests/webrtc_loopback.rs (P2P 自环测试)
**日期**: 2026-07-20
**原因**:
- FpsCounter: tick()/count()/fps() 用于实时帧率统计
- exchange_sdp(): demo 风格的 SDP 交换（PC 间 createOffer→setLocal→setRemote→createAnswer）
- create_connected_pair(): 创建两个 PC + 交换 offer/answer 完成连接
- generate_test_frame(): 生成 I420 测试模式 frame（移动颜色条）
- 9 集成测试用例: factory creation, SDP exchange, video push/receive 含 onTrack 回调, FPS counter, data channel, multiple tracks, add/remove, close
- 零外部依赖（不使用 str0m/webrtc-rs/webrtc-sys），纯 stub backend 验证 API 正确性

## D161: egui 示例 (GUI 视频预览)

**决策**: 创建 examples/webrtc_loopback_egui.rs — eframe GUI 演示 P2P 自环视频管道
**日期**: 2026-07-20
**原因**:
- Pipeline 共享状态: AtomicU64 fps/send/recv 计数器 + Mutex<Option<Frame>> recent_frame
- eframe::App with TopBottomPanel: 顶部 FPS 统计 (render/send/recv) + 中央视频预览
- I420→RGBA 转换: CPU 路径用于演示（libwebrtc backend 下可 GPU zero-copy）
- 测试模式生成器: 移动对角线，显示帧编号
- 在 webrtc-sys backend (含 -ObjC) 下编译并运行，窗口弹出成功

## D-SFU-WORKER: SFU Worker per-CPU-core Process Isolation

**决策**: mediasoup Worker per-CPU-core process model。每个 Worker 独占一个物理核心，避免单进程瓶颈。IPC 通过 Unix Domain Socket。
**日期**: 2026-07-20
**原因**:
- Recommended by mediasoup, LiveKit, Jitsi, Zoom。Single-process SFU limits throughput and fault isolation。
- Worker 崩溃不影响其他 Worker 的会话
- 每个 Worker 可独立绑定 CPU 核心，避免 NUMA 跨 socket 开销

## D-QOS-AUDIO: Audio-First QoS Degradation Priority Chain

**决策**: QoS degradation priority: audio > video keyframes > video delta frames。Resolution drops before framerate drops。
**日期**: 2026-07-20
**原因**:
- Zoom, Jitsi, Google Meet all use this strategy
- Bandwidth competition should preserve audio intelligibility above visual fidelity
- 音频丢包比视频丢包对用户体验影响更大
- 分辨率下降：面部表情仍可读；帧率下降：画面卡顿影响操控

## D-STREAM-TOPOLOGY: Origin-Edge Streaming Cluster Topology

**决策**: Origin-Edge topology for Phase 2+ streaming cluster。Phase 1-2 can use Read Replica (MediaMTX pattern), upgrade to distributed later。
**日期**: 2026-07-20
**原因**:
- MediaMTX Read Replica, SRS Origin-Edge, nginx-rtmp Pull/Push all converge on origin-edge as the simplest proven model
- 单 Origin 多 Edge：推流端推到 Origin，观看端从最近的 Edge 拉流
- Phase 1 单实例足够（MediaMTX 单进程可处理 1000+ 并发观看）
- 分布式 Edge 节点按需扩展，Origin 无状态可水平扩展

## D-GOP-CACHE: GOP Cache for Instant Viewer Join

**决策**: Ring-buffer-based GOP cache for WebRTC/WHEP subscribers。New viewers receive last cached keyframe + subsequent delta frames for instant join。
**日期**: 2026-07-20
**原因**:
- SRS, MediaMTX, nginx-rtmp all implement this
- Without GOP cache, new WebRTC viewers must wait for next keyframe (up to 2-5s)
- Ring-buffer 大小 = GOP 长度 × 2（GOP=2s@30fps → 120 frames buffer）
- 新人加入时先推缓存中的最后关键帧，再跟上实时流

## D-SIMULCAST: Scene-Aware Simulcast Layer Profiles

**决策**: 4 Simulcast profiles: remote-desktop (2 layers: 720p/1080p), conference (3 layers: 180p/360p/720p), vehicle-uplink (Dynacast + 3 layers), surveillance (1 layer: 1080p)。
**日期**: 2026-07-20
**原因**:
- mediasoup and LiveKit recommend scene-aware layer selection
- Different use cases have different bandwidth patterns (stable vs fluctuating vs extremely unstable)
- remote-desktop: 静态画面多，2 层足够；conference: 标准 3 层 SFU 转发
- vehicle-uplink: Dynacast 动态切换 + 3 层应对弱网
- surveillance: 不需要 Simulcast，恒定 1080p CBR 推流

## D-AUDIT-02: 第二次文档审计 (team-mode 4 路并行)

**决策**: 全量文档审计通过 4 路并行团队 (一致性检查/决策验证/缺口优化/参考交叉检查)。33 项发现含 7 CRITICAL + 15 HIGH + 8 MEDIUM + 4 LOW，全部采纳并修复。
**日期**: 2026-07-20
**成果**:
- AGENTS.md/README.md/status.md 更新至当前状态
- 5 新建决策: D-SFU-WORKER, D-QOS-AUDIO, D-STREAM-TOPOLOGY, D-GOP-CACHE, D-SIMULCAST
- 4 篇研究文档 + 23 篇参考文档增加 D# 追溯映射
- 5 篇模块文档补充章节 (operations/security/error/testing/upgrade)
- 6 篇文档修正陈旧/重复/命名错误
- status.md 6 缺失决策补表, D15/D73/D86/D95 修正

## D162: Multi-mode VideoFrameGenerator

**决策**: 将 VideoFrameGenerator 从单一 SquarePattern 重构为多模式生成器
**日期**: 2026-07-21
**原因**:
- PatternMode 枚举: Squares(SquaresConfig) | SmpteBars
- TimestampOverlay 独立模块: DateTime/FrameCount/Combined 格式
- SquaresConfig: count, min/max_size, motion_speed, ColorStrategy
- TextBurner + Anchor (TopLeft/BottomLeft/Position) + BitmapFont 缩放
- 56 tests pass (37 lib + 19 integration)

## D163: Bouncing Box Motion

**决策**: 方块运动从随机游走改为固定方向速度 + 边缘反弹
**日期**: 2026-07-21
**原因**:
- 参考 webrtc-kit/opencv 测试源彩色方块反弹模式
- Square 创建时随机 dx/dy 速度, 碰撞边缘反转方向
- 颜色默认 RandomPerSquare 保持稳定

## D164: Timestamp Milliseconds

**决策**: 时间戳显示毫秒精度 YYYY-MM-DD HH:MM:SS.mmm
**日期**: 2026-07-21
**原因**:
- chrono %.3f 格式, 用于截图级延迟对比

## D165: Generator Background

**决策**: 生成器背景 Y=128 中灰
**日期**: 2026-07-21
**原因**:
- 匹配 webrtc-kit 测试源灰色背景, 原 Y=16 近黑对比差

---

## D166: RTC 前缀命名规范

**状态**: ✅
**日期**: 2026-07-22

**决策**: omspbase-webrtc 所有对外 pub API 类型统一使用全大写 RTC 前缀，对齐 W3C WebRTC 标准命名：RTCPeerConnection, RTCDataChannel, RTCSessionDescription, RTCIceCandidate, RTCConfiguration, RTCOfferOptions, RTCAnswerOptions, RTCRtpSender, RTCRtpReceiver, RTCRtpParameters, RTCError, RTCEngine, RTCRtcStats。内部类型 (TrackSender, PcBackend) 不加前缀。

**理由**: 与 W3C 标准 1:1 对齐，napi-rs Node.js 绑定类型名无需映射。
**关联**: D146

## D167: snake_case 方法命名

**状态**: ✅
**日期**: 2026-07-22

**决策**: 所有方法统一 snake_case，删除 #[allow(non_snake_case)] W3C 包装 (addTrack→add_track, getSenders→get_senders, etc)。

**理由**: webrtc-rs/str0m/LiveKit Rust 全部 snake_case。napi-rs 自动转 camelCase。
**关联**: D146, D166

## D168: 统一 backend/ 目录

**状态**: ✅
**日期**: 2026-07-22

**决策**: 全 workspace 统一 backend/ (singular)。omspbase-media/src/backends/ → backend/。
**理由**: Rust 生态惯例。

## D169: core pipeline → media 迁移

**状态**: ✅
**日期**: 2026-07-22

**决策**: pipeline, broadcast, engine, plugin 从 core 迁移到 media。core 聚焦微内核 (auth/config/metrics/protocol)。依赖: core ← media ← remote-*。54 tests passed。
**决策**: pipeline, broadcast, engine, plugin 从 core 迁移到 media。core 聚焦微内核 (auth/config/metrics/protocol)。依赖: core ← media ← remote-*。54 tests passed。
**关联**: D150, D3

---

## D170: RTC 前缀重命名执行

**状态**: ✅
**日期**: 2026-07-22

**决策**: 执行 D166/D167 命名规范。34 个 pub 类型重命名 (RTCPeerConnection 等)，4 个 camelCase 方法删除。影响 81 文件 (~1200 引用)。分 3 阶段执行：webrtc crate → 依赖 crate → 文档。

**验证**: omspbase-webrtc stub 0 errors, omspbase-core 编译通过。
**关联**: D166, D167

## D171: backends/ → backend/ 目录重命名

**状态**: ✅
**日期**: 2026-07-22

**决策**: omspbase-media/src/backends/ 重命名为 backend/，统一全 workspace 单数目录命名。5 个测试/示例文件导入路径同步更新。编译通过。
**关联**: D168

## D172: webrtc-sys 预存编译错误修复

**状态**: ✅
**日期**: 2026-07-22

**决策**: 修复 webrtc-sys 后端 8 个预存编译错误：WebrtcSysTrack 重导出移除、SharedPtr 导入添加、UniquePtr 上 data_y/u/v 改用 i420_to_yuv8 间接调用、i420→yuv8→yuv→vfb 完整 FFI 链路。8→0 errors。
**关联**: D158

---

## D173: RealObserver + FrameSink + VideoSink 接收端实现

**状态**: ✅
**日期**: 2026-07-22

**决策**: 实现 webrtc-sys 后端的视频帧接收通路。
- RealObserver 替换 NoOpObserver：持有 ObserverCallbacks，on_track 回调转发到用户
- FrameSink trait：`on_frame(data, w, h)` — 回调式 I420 帧接收接口
- VideoSink bridge：FrameSinkAdapter → VideoSinkWrapper → NativeVideoSink → VideoTrack::add_sink()
- 完整链路：libwebrtc on_track → media_to_video → add_sink → VideoSink::on_frame → I420 提取 → FrameSink::on_frame

webrtc_loopback_egui.rs 更新为 FrameSink + mpsc channel 模式。编译通过。
**关联**: D148, D158

---

## D174: omspbase-codec 创建 + Phase 0-1 完成

**状态**: ✅
**日期**: 2026-07-22

**决策**: 按 D82 规划 (#5 crate) 创建 omspbase-codec。14 源文件，双后端架构 (stub + FFmpeg)。
- Phase 0: crate 骨架 (Cargo.toml + 14 源文件 + stub backend)
- Phase 1: 32 unit tests (config/lifecycle/stub)
- FFmpeg 后端: ffmpeg-the-third 真实编解码, build.rs C-probe
- Roundtrip: I420→H.264→I420, stride-aware PSNR 100.0 dB
- ffmpeg-the-third 替代 ffmpeg-next（活跃维护, safer API）

测试矩阵: stub 24 pass, ffmpeg 30 pass, 总计 54 tests。
**关联**: D43, D71, D82, C5

**关联**: D43, D71, D82, C5

---

## D175: GStreamer codec 后端

**状态**: ✅
**日期**: 2026-07-23

**决策**: 实现 GStreamer 后端 (H.264 编解码)，通过 pixi 管理运行时依赖。
- 编码管线: appsrc → videoconvert → x264enc → h264parse → appsink
- 解码管线: appsrc → h264parse → avdec_h264 → videoconvert → appsink
- EncoderPreset 映射到 x264enc speed-preset (ultrafast..placebo) via set_property_from_str
- tune=zerolatency 设定
- 27/27 tests pass (需要 pixi + DYLD_LIBRARY_PATH)
- gstreamer-video 0.23 用于 VideoFrameRef I420 平面提取

CI: pixi test-gstreamer task + GitHub Actions 独立 job (prefix-dev/setup-pixi)。

**关联**: D43, D174

---

## D176: webrtc-rs write_raw_i420 接 omspbase-codec

**状态**: ✅
**日期**: 2026-07-23

**决策**: webrtc-rs 后端通过 omspbase-codec 实现 write_raw_i420。
- I420 平面数据 → VideoFrame → push_frame → pull_packet → H.264 EncodedPacket → write_frame (RTP)
- 懒初始化: 首次 write_raw_i420 调用自动 init_encoder(width, height)
- 默认 640x480 30fps VBR 2Mbps P1UltraFast
- 每次 packet 释放/重获锁，避免跨 async 持锁

**关联**: D166, D174

---

## D177: webrtc-rs P0/P1 视频管线对齐

**状态**: ✅
**日期**: 2026-07-23

**决策**: 完成 webrtc-rs 后端视频管线，实现与 webrtc-sys 功能对等。
- P0: set_on_track 连接 inner.on_track() → TrackReceiver → 用户回调
- P0: create_video_track 创建 TrackLocalStaticSample (H.264, 90kHz) → 统一工厂分发
- P1: VideoSink 桥接 (on_track → TrackRemote.read_rtp → H.264 FU-A 重组 → 解码 → FrameSink)
- 移除 factory.rs 的 #[cfg] 门控 → 所有后端统一 dispatch

webrtc-rs 视频管线完整:
  发送端: create_video_track → write_raw_i420 → codec → write_frame → RTP
  接收端: set_on_track → TrackRemote → RTP → 解码 → FrameSink

**关联**: D166, D176

---

## D178: E2E P2P 编解码测试框架

**状态**: ✅
**日期**: 2026-07-23

**决策**: 创建 p2p_codec_e2e.rs 测试完整编解码管线。
- I420 → 编码 → WebRTC 发送 → 接收 → 解码 → I420 → 验证
- SharedSink: Arc<InnerSink> 在 on_track 回调和断言之间共享状态
- 验证 frame count + 尺寸匹配
- ICE 相关测试门控到 webrtc-rs (需要 ICE 凭证，待后续修复)

**关联**: D177

---

## D179: GStreamer 静态链接评估

**状态**: ✅ (延迟)
**日期**: 2026-07-23

**决策**: 调研 GStreamer 静态链接可行性。结论: 不可行。
- gstreamer-rs 无 gstreamer-full 支持，相关 MR 均为 Draft
- Windows CRT 静态链接冲突
- conda-forge 政策禁止静态库 (CFEP-18)
- 硬件编解码插件 (nvcodec/vaapi/videotoolbox) 永远动态

当前方案: pixi 管理开发依赖 (共享库)，分发时打包 .so/.dylib

**关联**: D181

---

## D182: E2E video frame relay test

**状态**: ✅
**日期**: 2026-07-23

**决策**: 添加端到端视频帧中继测试。
- 验证 Host → Server signaling relay → Remote 帧中继链路
- 5 帧 H.264，按序到达，首帧 keyframe
- 42/42 server tests pass (12 unit + 25 e2e + 5 integration)
- base64 作为 dev-dependency 用于测试帧数据编码

**关联**: D180, D181

---

## D183: webrtc stal 双端编译修复

**状态**: ✅
**日期**: 2026-07-23

**决策**: 修复 webrtc crate stal 问题，使 host + client 双双编译通过。
- 根因: omspbase-webrtc 默认 backend-webrtc-sys，client 代码依赖 webrtc:: 重导出
- lib.rs 添加 `pub use data_channel::*;`（唯一未 re-export 的模块）
- DataChannelEvent → RTCDataChannelEvent（RTC 前缀重命名遗漏）
- MediaError 替代 CoreError（media 独立后）
- 12 处修复跨 6 文件

**关联**: D180
**关联**: D175

---

## D180: omspbase-core → omspbase-common 重命名 + 依赖清理

**状态**: ✅
**日期**: 2026-07-23

**决策**: 将 omspbase-core 重命名为 omspbase-common，准确反映其内容（共享类型库，非微内核）。同步清理不合理依赖。
- core→common: 43 文件（目录名、Cargo.toml、import 路径、文档）
- 死依赖: omspbase-codec 移除 common 依赖（0 处引用）
- 弱依赖: omspbase-media 独立，CoreError::Unknown→MediaError::Internal
- omspbase-webrtc 从未依赖 common

依赖关系现在清晰: remote-host/client/server → common · media/codec/webrtc 独立

**关联**: D181

---

## D181: 移除 remote- 前缀 (host/client)

**状态**: ✅
**日期**: 2026-07-23

**决策**: omspbase-remote-host→omspbase-host, omspbase-remote-client→omspbase-client。
- 'host' 和 'client' 已隐含角色，无需 'remote-' 前缀
- 27 文件重命名 + 13 文档文件更新

最终 crate 命名: common · media · codec · webrtc · host · client · server
**关联**: D183

---

## D184: P2P 管线生产就绪（#1-5）

**状态**: ✅ (#1-3, #5 完成，#4 推迟)
**日期**: 2026-07-24

**决策**: 完善 P2P 管线 5 项改进。
- #1: WebSocket 重连 — 指数退避 5 次 (1s→2s→4s→8s→16s) ✅
- #2: ICE 状态监控 — webrtc-rs + webrtc-sys 两个后端 ✅
- #3: STUN 配置 — stun:stun.l.google.com:19302 ✅
- #4 背压: 需要修改 engine_adapters.rs (Receiver→UnboundedReceiver 矛盾) — 推迟
- #5 房间: 已完成 — RoomConfig struct + default_room_id ✅

**关联**: D182
**关联**: D183

---

## D185: Docker + CI for mediasoup SFU

**状态**: ✅
**日期**: 2026-07-24

**决策**: 建立 Docker 和 CI 基础设施支持 mediasoup SFU 开发。
- P1-P3 全部完成: Dockerfile + docker-compose + DevContainer + 文档
- sfu-mediasoup feature flag + test-mediasoup CI job
- Cargo.lock 必须与依赖变更一同提交 (约束规则)
- 已完成 Phase 2a-2d: SfuManager → Transport → Produce/Consume → Integration
- 新增文件: sfu.rs (370行), e2e_sfu.rs (236行), Dockerfile, docker-compose.yml, devcontainer.json
- 修改文件: protocol.rs (+252), signaling.rs (+280), main.rs (+30), room.rs (+8)

SFU 平台: macOS cargo check only · Linux full build+test · Docker 统一开发

**关联**: D184
**关联**: D184

---

## D186: 统一使用 Ubuntu 22.04 LTS

**状态**: ✅
**日期**: 2026-07-24

**决策**: 全部切换到 Ubuntu 22.04 LTS。
- mediasoup 上游使用 22.04 构建预编译二进制 (最广 glibc 兼容)
- Dockerfile: base + runtime 均使用 ubuntu:22.04
- CI: test-mediasoup job 明确使用 ubuntu-22.04 (不再依赖 ubuntu-latest)
- Rust 通过 rustup 安装 (stable channel, 匹配 rust-toolchain.toml)
- 其他 CI jobs (check/test/clippy) 保持 ubuntu-latest

**关联**: D185
**关联**: D185

---

## D187: macOS E2E 验证 + webrtc 默认后端解耦

**状态**: ✅
**日期**: 2026-07-24

**决策**: macOS P2P 管线端到端验证通过，修复 webrtc 后端冲突。
- webrtc crate 默认 feature 从 backend-webrtc-sys→[] (各消费者显式选择)
- .dockerignore 排除 target/ (33GB→10MB 构建上下文)
- docker-compose.yml 端口 9800 (匹配 server 默认配置)
- scripts/e2e-test.sh 自动测试 9 步验证 (server→build→connect→SDP→DC→relay)
- 9/9 通过: Host→Server→Client 信号中继 + DataChannel 574 bytes 转发

**关联**: D184, D186
---

## D188: Phase 3 Sprint 3A — 生产就绪安全+容错

**状态**: ✅
**日期**: 2026-07-24

**决策**: 完成 P0 级别生产就绪改进。
- 优雅关闭: 30s drain timeout + panic boundary + signal handler
- 深度健康检查: HealthChecker trait + per-component JSON
- 限流: tower-governor middleware + ws_max_message_size
- 审计日志: 6 结构化事件 (RoomCreate/Destroy, PeerJoin/Leave, AuthSuccess/Failure)
- process::exit 审计: 替换为 anyhow error propagation
- Dockerfile EXPOSE 端口修复 (8000→9800)

**关联**: D186, D187

---

## D189: Phase 3 Sprint 3B — 日志+文档+备份

**状态**: ✅
**日期**: 2026-07-24

**决策**: 完成 P1 级别生产就绪改进。
- JSON 日志: logging.rs (TraceId, LoggingConfig, trace_span)
- BackupManager<T>: 原子化 save/load/delete + tokio 持久化任务
- TLS/CORS 文档: 19-tls-cors-configuration.md (517行)
- 排障文档: troubleshooting.md (Docker/网络/STUN/缓存)
- 请求超时: tower-http TimeoutLayer (30s)

**关联**: D188

---

## D190: Phase 3 Sprint 3C — 高级特性

**状态**: ✅
**日期**: 2026-07-24

**决策**: 完成 P2 级别高级特性。
- JWT 认证: JwtClaims + JwtAuth (HS256) + PSK fallback
- rustls TLS: 原生 TLS 支持 (axum_server + rustls)
- CI 压测: 6 benchmarks (165K-1.2M it/s) + benchmark CI job
- OpenAPI 3.0: docs/openapi.yaml (4 REST endpoints)

**关联**: D189

---

## D196: Admin Dashboard Architecture

**Decision**: React+TypeScript admin SPA embedded in server binary (rust-embed + build.rs). Phase 1 targets Remote Control scenario (DeviceStream). Room model refactored to per-stream rooms with N consumers. Zustand for frontend state. Admin JWT separate from signaling JWT.
**Date**: 2026-07-24
**Reason**: 
- Operators need visual monitoring of media streams (91 commits, zero visibility)
- Remote Control is the immediate use case (vehicles pushing camera streams)
- Per-stream room model simplifies relay routing
- Unified AdminEvent enum serves both audit logging and WS push
- build.rs auto-resolves dist/ path for rust-embed
**Supersedes**: D136/D142 (consolidated MVP admin plan — larger scope, different architecture)

## D197: D87 Scope Limitation — Client GUI Only

**Decision**: D87 (React + Ant Design for Server management panel) applies only to omspbase-client GUI. OMSPBase Server admin dashboard uses CSS Modules for zero-dependency lightweight panel.
**Date**: 2026-07-24
**Reason**:
- D87's rationale (share components with AUDEBase Admin UI) is irrelevant for embedded server admin
- Admin dashboard is a monitoring tool, not a user-facing application
- CSS Modules = zero runtime, smaller bundle, no framework lock-in
- Ponytail principle: don't add Ant Design for a few cards and a table
**Limits**: D87 remains in effect for omspbase-client (Tauri desktop app) and any AUDEBase-shared UI
## D198: SFU Video Playback — Server-Offer Architecture

## D199: OpenCode Instructions 精简化

**决策**: 将 instructions 数组从 23 个文件精简为 17 个，移除中英文重复（zh/）、无关语言规则（TS/CPP）、参考文档（agent-guide/model-tiers）。
**日期**: 2026-07-28
**原因**:
- zh/ 10 个文件是 common/ 的完整中文翻译，约占 1,950 tokens — 纯冗余
- TypeScript/CPP coding-style 对 Rust 项目无关 — 约 1,200+800 tokens 浪费
- agent-guide.md 和 agent-model-tiers.md 是工具参考文档，非每轮必需 — 约 1,900 tokens
- constraints.md 和 edit-safety.md 已存在但未加载 — 比 agent-guide 更重要
- Rust 专属规则（coding-style + hooks）未在 instructions 中
- 3 个小的 memorys 文件（status/conventions/pitfalls）加入 instructions
**效果**: ~11,700 → ~8,500 tokens（节省 27%，且加载了更相关的规则）

## D200: OMO Agent 模型分配优化

**决策**: metis 和 momus 从 fast 升级到 premium；explore 温度从 0.0 调整到 0.1。
**日期**: 2026-07-28
**原因**:
- metis（预规划顾问）需要强推理能力来发现需求歧义和 AI 失败点，fast 层会橡皮图章
- momus（计划批评家）是质量门禁，对抗性审查需要深度推理
- 温度：metis 0.1（逻辑为主），momus 0.3（创造性批评），explore 0.1（代码探索需要轻微多样性）
- 参考 AUDEBase 实践：质量门禁型 agent 必须 premium 以上
**新增**: teams.dev 预定义（implementer + test-writer + reviewer），staleTimeoutMs 300000→600000

## D201: Pre-commit Hook — Rust 质量门禁

**决策**: 创建 `.git/hooks/pre-commit`，对暂存 `.rs` 文件运行 `cargo fmt --check` + `cargo clippy -- -D warnings`。
**日期**: 2026-07-28
**原因**:
- 规则会被遗忘，hook 不会。pre-commit 是最后一道防线
- `grep` 无匹配时用 `{ grep ... || true; }` 包装，避免 `set -euo pipefail` 下误中断
- 仅 .rs 文件暂存时触发，不阻塞非 Rust 提交

## D202: Global Provider Config — lite 上下文修复 + Reasoning 启用

**决策**:
1. lite 模型的 context limit 从 40,960 修正为 131,072
2. deepseek-v4-pro 和 deepseek-v4-flash 的 supportsReasoning 设为 true
**日期**: 2026-07-28
**原因**:
- lite 路由到 Qwen3-32B，实际支持 128K+ context，40,960 是配置错误（参考 lite-1 Qwen3-8B 也有 131K）
- DeepSeek V4 系列支持 reasoning API，设为 true 使 opencode 可使用 reasoning 特性
- Fallback 上下文窗口经别名映射验证全部正确：premium-max-1(256K)=Kimi K2.6, premium-2(205K)=GLM-5.1, fast-1(131K)=Qwen3.6 Flash 等
- apiKey 硬编码未修改（用户选择保持现状为内网环境）
**决策**: 浏览器视频播放使用 mediasoup 的 Server-Offer 模型（SFU 创建 transport offer，客户端创建 answer），参考 LiveKit 模式。Host 通过 SFU Produce 推流（非 P2P WebRTC）。
**日期**: 2026-07-27
**原因**:
- P2P SDP 中继存在时序问题（host offer 在 viewer 加入后丢失）
- Server-Offer 模型消除此问题 — SFU 为每个 viewer 创建新 transport
- mediasoup transport 协议已在 sfu.rs 中实现
- LiveKit 使用此模型，是 WebRTC SFU 的行业标准
- Admin dashboard 复用现有 SignalingMessage 类型（CreateWebRtcTransport、Consume）用于浏览器 consumer
