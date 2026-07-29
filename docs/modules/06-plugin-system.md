# 插件体系与 Trait 设计

> Phase 0 架构定义 — 插件 trait 设计、PluginManager、能力声明。参考 OBS 的 obs_source_info + GStreamer 的 element factory。

---

## 一、微内核架构

```
omspbase-common (微内核)
├── PluginManager     — 插件注册、发现、生命周期
├── LicenseManager    — 权限校验、配额控制
├── ProtocolBroker    — 内部协议路由 (FlatBuffers)
├── PipelineEngine    — 媒体管线调度
└── AuthProvider      — 认证接口 (trait)
```

微内核仅包含 trait 定义和调度逻辑。所有具体功能都在插件中实现。

---

## 二、Plugin trait（D28）

### 2.1 插件注册

```rust
/// 插件 = 可注册的管道节点工厂。
/// OBS 的 obs_source_info 模式：struct-of-callbacks 注册。

pub trait Plugin: Send + Sync {
    /// 插件名称
    fn name(&self) -> &str;
    /// 语义化版本
    fn version(&self) -> (u16, u16, u16);
    /// 插件类别: MediaSource | MediaProcessor | MediaSink (对齐 architecture.md §4.2)
    fn category(&self) -> PluginCategory;
    /// 能力声明: D30 注册时声明，非运行时探测
    fn capabilities(&self) -> Vec<PluginCapability>;

    /// 初始化: Phase 0 compile-time, Phase 2+ dlopen
    fn init(&self, ctx: &PluginContext) -> Result<()>;
    /// 关闭/清理
    fn shutdown(&self) -> Result<()>;
}
```

### 2.2 能力声明（D30）

```rust
/// 注册时声明，非运行时探测。OBS 的 output_flags 模式。

pub struct PluginCapability {
    pub node_type: NodeType,
    pub media_type: MediaType,
    pub codecs: Vec<CodecId>,        // 支持的 codec 列表
                                      // 空 Vec = 通配符（穿透节点）
    pub pixel_formats: Vec<RawPixelFormat>,
    pub priority: u8,                // 选择优先级 (0-255, 高值优先)
}

pub enum NodeType { Source, Processor, Sink }
pub enum MediaType { Encoded, Raw, Both }
```

### 2.3 插件分类

#### 生产类（Host — 媒体源产出）

| 插件 | 类型 | 输入 | 输出 | 插件种类 |
|------|------|------|------|---------|
| ScreenCapture | Source | — | RawFrame | 编译时 |
| CameraCapture | Source | — | RawFrame | 编译时 |
| AudioCapture | Source | — | RawFrame (audio) | 编译时 |
| RtmpSubscriber | Source | — | EncodedFragment | 编译时 |
| RTCRtpReceiver | Source | — | EncodedFragment | 编译时 |
| InputReceiver | Sink | InputEvent | — | 运行时 |

#### 处理类（Processor — 媒体变换）

| 插件 | 输入 | 输出 | 插件种类 | 优先级 |
|------|------|------|---------|--------|
| NvencEncoder | RawFrame | EncodedFragment | 编译时 | 255 (GPU first) |
| VaapiEncoder | RawFrame | EncodedFragment | 编译时 | 200 |
| VideoToolboxEncoder | RawFrame | EncodedFragment | 编译时 | 200 |
| QsvEncoder | RawFrame | EncodedFragment | 编译时 | 200 |
| SoftwareH264 | RawFrame | EncodedFragment | 编译时 | 50 (fallback) |
| HwDecoder | EncodedFragment | RawFrame | 编译时 | 200 |
| SoftwareDecoder | EncodedFragment | RawFrame | 运行时 | 50 |
| ColorConvert | RawFrame | RawFrame | 编译时 | — |
| Passthrough | EncodedFragment | EncodedFragment | 编译时 | — |

#### 消费类（Sink — 媒体消费）

| 插件 | 输入 | 输出 | 插件种类 |
|------|------|------|---------|
| ScreenRender | RawFrame | — | 编译时 |
| AudioPlayback | RawFrame (audio) | — | 编译时 |
| RtmpPublisher | EncodedFragment | — | 编译时 |
| RTCRtpSender | EncodedFragment | — | 编译时 |
| Recorder | EncodedFragment | — | 运行时 |
| InputForwarder | — | InputEvent | 编译时 |

#### 协议类

| 插件 | 说明 |
|------|------|
| RtmpPlugin | RTMP 接入/分发 (GStreamer) |
| HlsPlugin | HLS 打包 (GStreamer + Rust CMAF) |
| SrtPlugin | SRT 传输 (GStreamer srtp) |
| RtspPlugin | RTSP 接入 (GStreamer) |
| WebRtcPlugin | WebRTC P2P/SFU (str0m / libwebrtc / webrtc-rs) |
| DataChannelPlugin | WebRTC RTCDataChannel |

#### 中继类

| 插件 | 说明 |
|------|------|
| StunTurnPlugin | NAT 穿透 (coturn 集成) |
| SfuRelayPlugin | SFU 媒体转发 (LiveKit / mediasoup 插件) |

---

## 三、PluginManager（D29）

### 3.1 双模式加载

```rust
pub struct PluginManager {
    compile_time: Vec<Arc<dyn Plugin>>,    // 编译时注册
    run_time: Vec<Arc<DynamicPlugin>>,      // 运行时加载
}

impl PluginManager {
    /// 注册编译时插件（通过 inventory 在 build.rs 生成）。
    pub fn register(&mut self, plugin: Arc<dyn Plugin>);

    /// 从插件目录加载运行时插件（dlopen）。
    pub fn load_runtime(&mut self, path: &Path) -> Result<()>;

    /// 按能力查询插件。PipelineEngine 用此方法找到匹配的节点。
    pub fn find_nodes(
        &self, node_type: NodeType, media_type: MediaType, query: &FormatQuery,
    ) -> Vec<&dyn Plugin>;

    /// 从匹配的插件创建管道节点实例。
    pub fn create_node(
        &self, capability: &PluginCapability, config: &NodeConfig,
    ) -> Result<Box<dyn AnyPipelineNode>>;
}
```

### 3.2 编译时注册（inventory 模式）

```rust
// 通过 Rust macro 在编译期自动注册

inventory::submit! {
    PluginRegistry::new(
        name: "nv-encoder",
        version: (0, 1, 0),
        kind: PluginKind::CompileTime,
        capability: PluginCapability {
            node_type: NodeType::Processor,
            media_type: MediaType::Raw,
            codecs: vec!["h264".into(), "h265".into()],
            pixel_formats: vec![RawPixelFormat::Nv12],
            priority: 255,
        },
        factory: || Box::new(NvencEncoder::new()),
    )
}
```

### 3.3 运行时加载（dlopen 模式）

```rust
// 动态插件导出 C ABI 函数

#[no_mangle]
pub extern "C" fn omspbase_plugin_register(registry: &mut PluginManager) {
    registry.register(Arc::new(MyCustomDecoder));
}

// ABI 稳定性通过 sizeof 检查保证（参考 OBS 的 obs_register_source_s）
```

### 3.4 管道引擎选择流程

```
PipelineEngine::build_pipeline(sources, sinks)
  │
  ├── PluginManager::find_nodes(NodeType::Processor, MediaType::Raw, "h264")
  │     → [NvencEncoder(255), VaapiEncoder(200), SoftwareH264(50)]
  │
  ├── 按 priority 排序 → 选择 NvencEncoder
  │
  └── PluginManager::create_node(nvenc, config)
        → Box<dyn MediaProcessor<RawFrame, EncodedFragment>>
        → 插入管道
```

---

## 四、插件生命周期

```
CompileTime:  inventory::submit! → 编译期静态注册 → PluginManager::register()
  │
  ├── trait Plugin::on_load()   → 初始化全局资源（GPU 上下文、解码器池）
  ├── [管道运行]                 → create_node() 按需创建实例
  ├── trait Plugin::on_unload() → 清理全局资源
  └── Plugin dropped

RunTime:      dlopen → extern "C" register() → PluginManager::register()
  │
  ├── [同上]
  └── dlclose
```

---

## 五、能力声明 vs 运行时协商

| 维度 | OMSPBase (D30) | GStreamer | OBS |
|------|-----------------|-----------|-----|
| 声明时机 | 编译时注册 | 插件 init + PadTemplate | 模块加载时 |
| 格式匹配 | 直接比较 (codec strings + pixel formats) | Caps 交集 + fixate | output_flags 位掩码 |
| 动态协商 | Phase 2+ 可选 (类似 GST_QUERY_CAPS) | 原生的 (GST_QUERY_CAPS → caps filter → fixate) | 有限 (video_get_color_space) |
| ABI 安全 | sizeof 检查 (OBS 模式) | GObject 类型系统 | sizeof 检查 |

**Phase 1 选择理由**：声明式能力足够覆盖所有计划中的插件。GStreamer 的复杂 caps 协商对于 Phase 1 的固定格式链来说是过度设计。
