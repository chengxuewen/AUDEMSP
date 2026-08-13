# Moonlight + Sunshine 参考分析
> 生成日期：2026-07-16 | 分类：远程桌面

## 1. 产品画像
- **名称**：Moonlight（客户端）+ Sunshine（服务端）
- **开发者**：
  - Moonlight：开源社区发起（CaseySJ 创立，最初称为 Limelight），基于 NVIDIA GameStream 协议逆向工程实现
  - Sunshine：LizardByte 社区开发，基于 C++ 的开源 GameStream 兼容服务端
- **首次发布**：
  - Moonlight (Limelight)：2013 年（首个 Android 客户端）
  - Sunshine：2020 年（NVIDIA 宣布停止 GameStream 前夕，社区启动自研服务端）
- **产品定位**：终极开源游戏串流方案。Moonlight 是高性能全平台客户端，Sunshine 是全 GPU 厂商兼容的服务端。2023 年 NVIDIA 正式停止 GameStream 后，Sunshine+Moonlight 成为开源游戏串流的事实标准
- **目标用户群体**：
  - 游戏玩家（将 PC 游戏串流到手机/平板/电视/掌机等设备，局域网或远程）
  - 家庭串流用户（通过 Steam Deck/Apple TV/Nvidia Shield 等设备将书房 PC 画面投射到客厅电视）
  - DIY 云游戏玩家（配合 VPN 或 Tailscale 构建个人云游戏方案）
  - 远程桌面用户（利用其低延迟编码能力进行非游戏的远程控制）
  - 开源社区（学习游戏串流协议的完整实现）
  - 客户端的极限覆盖（Switch/PS Vita/Steam Link 等非传统平台的嵌入）
- **许可 / 商业模式**：
  - Moonlight PC：GPLv3
  - Moonlight Android/iOS/tvOS/Switch：各自独立仓库，GPLv3
  - Sunshine：GPLv3
  - 完全免费，无商业版本，无广告，无数据收集

## 2. 技术特性
### 整体架构
```
┌──────────────────────────────────────────────────────────────────┐
│              Moonlight + Sunshine 六协议分离架构                   │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │              Sunshine Host (服务端 — C++/Qt)                │  │
│  │                                                             │  │
│  │  屏幕捕获抽象层 (多后端):                                    │  │
│  │  ┌──────────┬──────────┬──────────┬──────────────────┐     │  │
│  │  │  DXGI    │ KMS/DRM  │  NvFBC   │ X11 / Wayland    │     │  │
│  │  │ (Win)    │ (Linux)  │ (NVIDIA  │ wlroots / XDG /  │     │  │
│  │  │          │          │  专有)   │ KWin Screencast  │     │  │
│  │  └──────────┴──────────┴──────────┴──────────────────┘     │  │
│  │                         ↓                                    │  │
│  │  FFmpeg 类 GPU 编码器统一抽象层                               │  │
│  │  ┌───────┬──────┬──────────┬─────────┬─────────┬──────┐     │  │
│  │  │NVENC  │ AMF  │QuickSync │  VAAPI  │   VT    │ SW   │     │  │
│  │  │NVIDIA │ AMD  │  Intel   │  Linux  │  macOS  │ CPU  │     │  │
│  │  ├───────┼──────┼──────────┼─────────┼─────────┼──────┤     │  │
│  │  │  MF   │Vulkan │          │         │         │      │     │  │
│  │  │ Win   │ Video │          │         │         │      │     │  │
│  │  └───────┴──────┴──────────┴─────────┴─────────┴──────┘     │  │
│  │                                                             │  │
│  │  编解码支持: H.264 / HEVC (H.265) / AV1                       │  │
│  │  色度支持: YUV 4:2:0 / YUV 4:4:4 / HDR (HDR10/HLG)          │  │
│  └────────────────────────────────────────────────────────────┘  │
│                             │                                    │
│     ┌───────────────────────┼───────────────────────┐            │
│     │              ┌────────┼────────┐              │            │
│     ▼              ▼        │        ▼              ▼            │
│  HTTPS(配对)    RTSP(控制)  │   RTP视频(47998)  RTP音频(48000) │
│  TCP 47984      TCP 48010   │   UDP             UDP            │
│     │              │        │        │              │            │
│     │              │   ENet │(输入)  │              │            │
│     │              │  UDP 47999    │              │            │
│     │              ▼        │       │              │            │
│     └──────────────┼────────┘       └──────────────┘            │
│                    │                                             │
│  控制平面 (TCP)    │        数据平面 (UDP)                       │
│  可靠性优先        │        低延迟优先                           │
│                    │                                             │
│  ┌─────────────────┴─────────────────────────────────────────┐  │
│  │              Moonlight 客户端 (Qt/C++ / 原生)               │  │
│  │                                                             │  │
│  │  · 硬件加速解码: VAAPI / D3D11VA / VideoToolbox /          │  │
│  │                  NVDEC / VDPAU / Vulkan Video              │  │
│  │  · GPU 渲染: 像素着色器 颜色空间转换 (YUV→RGB)             │  │
│  │  · 性能叠加层: 实时显示 编码/网络/解码/渲染 延迟分解        │  │
│  │  · 外设: 16人同时手柄 + 力反馈 + 体感 (陀螺仪/加速度计)    │  │
│  │  · 音频: 7.1 环绕声 Opus/AAC 低延迟解码                    │  │
│  └─────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### 六协议详解
```
┌─────────────────────────────────────────────────────────────────────┐
│                 GameStream 六协议：职责与设计意图                     │
├────────┬─────────┬─────────┬────────────────────────────────────────┤
│ 协议   │ 传输层  │ 端口    │ 职责                                   │
├────────┼─────────┼─────────┼────────────────────────────────────────┤
│ HTTP   │ TCP     │ 47989   │ 服务发现：获取服务器列表、查询支持的    │
│        │         │         │ 编解码器、分辨率、帧率、HDR能力         │
│        │         │         │ 无状态请求/响应，类似 REST API          │
├────────┼─────────┼─────────┼────────────────────────────────────────┤
│ HTTPS  │ TCP     │ 47984   │ 配对认证：客户端首次连接时输入 PIN 码   │
│        │         │         │ 服务器颁发客户端证书，后续免PIN连接     │
│        │         │         │ TLS 加密 + 证书双向验证                 │
├────────┼─────────┼─────────┼────────────────────────────────────────┤
│ RTSP   │ TCP     │ 48010   │ 流会话控制：SDP 协商编解码参数(编码器   │
│        │         │         │ 类型/分辨率/帧率/色度/码率)，启动/停止/ │
│        │         │         │ 暂停视频流，动态切换分辨率              │
│        │         │         │ 类似 SIP 的"控制信令"角色               │
├────────┼─────────┼─────────┼────────────────────────────────────────┤
│ RTP    │ UDP     │ 47998   │ 视频流传输：H.264/H.265/AV1 编码的     │
│ (视频) │         │         │ NAL 单元封装为 RTP 包，含序列号和时间戳 │
│        │         │         │ 单播，单向 (Server → Client)            │
│        │         │         │ 无重传（丢一帧无所谓，下一帧马上到）    │
├────────┼─────────┼─────────┼────────────────────────────────────────┤
│ RTP    │ UDP     │ 48000   │ 音频流传输：Opus/AAC 编码，7.1 环绕声   │
│ (音频) │         │         │ 独立的端口和 RTP 流，与视频完全解耦     │
├────────┼─────────┼─────────┼────────────────────────────────────────┤
│ ENet   │ UDP     │ 47999   │ 可靠控制通道：ENet 提供基于 UDP 的可    │
│        │         │         │ 靠传输（类似 TCP 的可靠性 + UDP 的低    │
│        │         │         │ 延迟），传输手柄/键鼠/触控输入、手柄    │
│        │         │         │ 力反馈指令、体感数据(IMU)               │
└────────┴─────────┴─────────┴────────────────────────────────────────┘

设计意图总结:
┌─────────────────────────────────────────────────────────────────────┐
│  控制平面 (Control Plane)：HTTP/HTTPS/RTSP → 全部走 TCP              │
│  · 需要可靠传输（不能丢失一个 PIN 码或流控制指令）                    │
│  · 数据量极小（几 KB 级别），TCP 的开销可接受                         │
│                                                                     │
│  数据平面 (Data Plane)：RTP视频/RTP音频/ENet输入 → 全部走 UDP        │
│  · 需要低延迟（TCP 的队头阻塞和重传在实时流中是灾难）                 │
│  · 数据量大（视频流 10-100 Mbps），UDP 无拥塞控制负担                 │
│  · ENet 作为例外：在 UDP 之上实现选择性可靠传输（仅输入数据需要可靠）  │
│                                                                     │
│  六个独立端口的好处:                                                  │
│  · 每条流独立 Qos（视频优先级 > 音频 > 输入）                         │
│  · 互不影响（视频卡顿不会导致输入丢包或音频中断）                     │
│  · 可被防火墙规则独立管理（但端口多也是缺点）                          │
└─────────────────────────────────────────────────────────────────────┘
```

### 七种编码 API 兼容矩阵
```
┌─────────────────────────────────────────────────────────────────────┐
│            Sunshine 编码 API × GPU 厂商 × 平台 三维矩阵              │
├────────────┬────────┬────────┬────────┬────────┬────────┬───────────┤
│ 编码 API   │ NVIDIA │ AMD    │ Intel  │ Qualc. │ Apple  │ 平台      │
├────────────┼────────┼────────┼────────┼────────┼────────┼───────────┤
│ NVENC      │   ✅   │   ➖    │   ➖    │   ➖   │   ➖   │ Win/Linux  │
│ AMF        │   ➖    │   ✅   │   ➖    │   ➖   │   ➖   │ Windows    │
│ QuickSync  │   ➖    │   ➖    │   ✅   │   ➖   │   ➖   │ Windows    │
│ VAAPI      │   ✅   │   ✅   │   ✅   │   ➖   │   ➖   │ Linux/BSD  │
│ VideoToolbx│   ➖    │   ➖    │   ➖    │   ➖   │   ✅   │ macOS      │
│ MediaFound.│   ✅   │   ✅   │   ✅   │   ✅   │   ➖   │ Windows    │
│ Vulkan Video│  ✅   │   ✅   │   ✅   │   ➖   │   ➖   │ Linux      │
│ Software   │   ✅   │   ✅   │   ✅   │   ✅   │   ✅   │ 全平台     │
├────────────┼────────┼────────┼────────┼────────┼────────┼───────────┤
│ 编解码支持 │H.264/  │H.264/  │H.264/  │H.264/  │H.264/  │            │
│            │HEVC    │HEVC    │HEVC    │HEVC    │HEVC    │            │
│ 加AV1      │✅NVENC │➖       │✅QSV   │➖       │➖       │ GPU 4000+  │
└────────────┴────────┴────────┴────────┴────────┴────────┴───────────┘
```

### 屏幕捕获 × 编码兼容矩阵 (Linux/FreeBSD)
```
┌─────────────────────┬───────┬──────────┬──────────────┬──────────────┐
│ 捕获方法            │ VAAPI │ VulkanVid│ NVENC (CUDA) │ Software     │
├─────────────────────┼───────┼──────────┼──────────────┼──────────────┤
│ KMS/DRM             │  ✅   │    ✅    │      ✅      │     ✅       │
│ NvFBC (NVIDIA专有)  │  ❌   │    ❌    │      ✅      │     ❌       │
│ Wayland (wlroots)   │  ✅   │    ❌    │      ✅      │     ✅       │
│ X11                 │  ✅   │    ❌    │      ✅      │     ✅       │
│ XDG Desktop Portal  │  ✅   │    ✅    │      ✅      │     ✅       │
│ KWin Screencast     │  ✅   │    ✅    │      ✅      │     ✅       │
│ Windows.Graphics    │  ➖   │    ➖    │      ➖      │     ➖        │
│ Capture (Win)       │       │          │              │              │
└─────────────────────┴───────┴──────────┴──────────────┴──────────────┘
```

### 关键技术能力
| 能力 | 详情 |
|------|------|
| 视频编码 | H.264 / HEVC (H.265) / AV1；NVENC / AMF / QSV / VAAPI / VideoToolbox / Media Foundation / Vulkan Video / Software |
| 色度子采样 | YUV 4:2:0（标准游戏画质）、YUV 4:4:4（文字和色彩精准，Sunshine+Moonlight 专属，商业方案中只有 Parsec Warp 支持） |
| HDR | HDR10 和 HLG 格式支持，10-bit 色深 |
| 音频 | 7.1 环绕声、Opus 编码（现代，低延迟，广泛支持）、AAC 编码（兼容老旧设备） |
| 传输协议 | 六协议分离：HTTPS (配对认证) / RTSP (流控制 SDP) / RTP (视频 47998) / RTP (音频 48000) / ENet (输入控制 47999) |
| 安全机制 | HTTPS PIN 码配对 + 客户端证书（首次配对后免PIN）；可选 VLAN 隔离；LAN 场景默认无额外加密（低延迟优先） |
| 输入控制 | ENet 可靠UDP通道：键盘/鼠标/触控 + 最多 16 人同时游戏手柄 + 力反馈（振动）+ 体感（陀螺仪/加速度计 6 轴 IMU 数据）+ 多点触控 10 点 |
| 平台支持 (Host) | Windows 11+ / Linux (Debian/Ubuntu/Fedora) / macOS 14.2+ / FreeBSD 14.4+ |
| 平台支持 (Client) | Windows / macOS / Linux / iOS / Android / tvOS / Nintendo Switch (自制系统) / PS Vita (自制系统) / Steam Link / Web / Raspberry Pi |
| 硬件要求 | 最低：AMD VCE 1.0+ / Intel Skylake QSV+ / NVIDIA NVENC-enabled GPU；4K 推荐：RTX 2000+ 或 GTX 1080+ / Ryzen 5 / Core i5+；RAM 4GB+ |

### 技术栈
- **Sunshine 服务端**：
  - 语言：C++（核心代码库）+ Qt（Web UI 配置界面）
  - 屏幕捕获多后端：DXGI (Windows) / KMS-DRM (Linux 内核级帧缓冲) / NvFBC (NVIDIA 专有帧缓冲捕获) / PipeWire (Linux 音频视频统一管道) / wlroots / XDG Desktop Portal / KWin Screencast (Wayland 三大捕获协议) / X11 / ScreenCaptureKit (macOS) / Windows.Graphics.Capture (Win10+ API)
  - 编码抽象：FFmpeg 类 API 风格（非直接依赖 FFmpeg 库，而是借鉴其编解码器工厂模式，每个编码器后端封装为独立模块）
  - Web UI 配置：嵌入式 HTTP 服务器（Qt 内置）+ 响应式 Web 界面
  - 服务管理：systemd (Linux) / launchd (macOS) / Windows Service
  - 日志：spdlog (高性能 C++ 日志库)
- **Moonlight 客户端**：
  - 桌面版：Qt 6.7 + C++（Windows/macOS/Linux 统一代码库）
  - 移动版：原生平台开发
    - Android：Kotlin + Java，Android NDK 用于硬件解码（MediaCodec）
    - iOS/tvOS：Swift + ObjC，VideoToolbox 硬件解码
  - 第三方移植：Nintendo Switch / PS Vita（由社区独立维护，使用各自平台的原生 SDK）
  - 硬件解码后端矩阵：
    - Windows: D3D11VA (Direct3D 11 Video Acceleration) / NVDEC / DXVA2
    - Linux: VAAPI (Video Acceleration API) / VDPAU (NVIDIA) / Vulkan Video
    - macOS/iOS/tvOS: VideoToolbox (Apple 统一硬件编解码框架)
    - Raspberry Pi: V4L2 M2M (Video4Linux Memory-to-Memory，通过内核接口硬件解码)
  - GPU 渲染：OpenGL / DirectX 像素着色器（颜色空间转换 YUV→RGB + 缩放 + HDR 色调映射）
  - 音频：SDL2 / PulseAudio / ALSA / CoreAudio (各平台统一到 SDL2 抽象层)

## 3. 功能概览
### 核心功能模块
- **游戏串流**：将 PC 游戏以最高 4K@120fps HDR 串流到任意 Moonlight 客户端设备。支持 LAN（<1ms 网络延迟）和远程（IPv6/Tailscale/VPN）。自适应的码率和分辨率动态调整
- **远程桌面**：通过"直接鼠标控制"模式实现非游戏的远程桌面操作。支持系统级快捷键透传（Alt+Tab、Win+D 等），多点触控手势映射
- **音频流**：7.1 环绕声低延迟音频传输，Opus 编码实现 <20ms 端到端音频延迟，AAC 编码兼容老旧客户端
- **多点触控支持**：最多 10 点同步触控，适合在移动设备（手机/平板）上通过触摸屏操作远程 Windows 触控应用
- **全手柄生态系统**：最多 16 个游戏手柄同时连接，支持 Xbox 360/Xbox One/Xbox Series / DualShock 4 / DualSense / Switch Pro Controller；力反馈振动双马达独立控制；DualShock/DualSense 体感数据透传（6 轴 IMU：3 轴陀螺仪 + 3 轴加速度计）

### 特色功能
- **六协议分离的极致解耦**：GameStream 协议将配对（HTTPS）、流控制（RTSP-SDP）、视频传输（RTP）、音频传输（RTP）、输入控制（ENet）使用五种独立的标准化协议实现。控制平面全部走 TCP（需要可靠性），数据平面全部走 UDP（需要低延迟）。这种"关注点分离"（Separation of Concerns）是协议设计的教科书级范本
- **七种编码 API 的统一抽象**：Sunshine 的核心创新是将 NVIDIA/AMD/Intel/Qualcomm/Apple 共五家 GPU 厂商的编码 API（NVENC/AMF/QSV/VAAPI/VideoToolbox/Media Foundation/Vulkan Video）统一到 FFmpeg 类工厂模式下。这是在开源项目中的唯一实践 —— 其他项目（如 RustDesk）需要逐个集成 GPU 编码器，而 Sunshine 提供了一层统一的适配层
- **客户端矩阵的极限覆盖**：Moonlight 客户端覆盖 Windows、macOS、Linux、iOS、Android、tvOS、Nintendo Switch（自制系统）、PS Vita（自制系统）、Steam Link、Web 浏览器、Raspberry Pi —— 几乎涵盖所有带屏幕的计算设备。这种覆盖范围是任何商业产品无法企及的（需要每个平台投入原生开发资源），只有开源社区的协作力量才能实现
- **HDR 和 YUV 4:4:4 的支持**：Sunshine+Moonlight 是极少数同时支持 HDR（HDR10 + HLG）和 YUV 4:4:4 完整色度子采样的远程桌面/游戏串流方案。4:4:4 对文字清晰度和色彩准确性至关重要，商业方案中只有 Parsec Warp 支持且需要付费订阅
- **延迟分解可视化 —— 性能叠加层**：客户端实时显示端到端延迟的四大组成部分：Host 编码延迟 / 网络传输延迟 / Client 解码延迟 / Client 渲染延迟。这种透明的性能分解让用户可以精确诊断瓶颈——是 GPU 编码器不够快？是 WiFi 干扰导致网络延迟？还是解码器性能不足？
- **从危机中诞生的架构韧性**：2023 年 NVIDIA 宣布停止 GameStream 支持，Moonlight 社区面临生存危机。在 3 年内，Sunshine 从零开始发展为一个比 NVIDIA GameStream 更强大的替代品（支持更多 GPU 厂商、更多编解码器、更多平台）。这段历史是最佳的"不要绑定单一厂商"案例研究

### 扩展性 / 插件机制
- **社区分支生态**：Moonlight 有多个社区维护的分支 —— Moonlight iOS（App Store 官方上架）、Moonlight tvOS、Moonlight Switch（自制系统）、Moonlight PS Vita（自制系统）、Moonlight Steam Link —— 证明协议开放性带来了生态扩展
- **自定义编码器扩展**：Sunshine 的 FFmpeg 类工厂模式允许通过添加模块扩展新的编码后端。新增 GPU 编码器只需实现统一的编码器接口
- **Web UI 远程配置**：Sunshine 通过嵌入式 Web 服务器提供配置界面，可在局域网内任何设备的浏览器中管理服务端设置
- **命令行全配置**：所有配置项均可通过 CLI 参数或配置文件设置，支持无头服务器部署和自动化脚本
- **Docker 容器化**：Linux 版支持 Docker 部署，但需要映射 GPU 设备（`--device /dev/dri`）和网络端口
- **预/后处理脚本钩子**：支持在流启动前/结束后执行自定义脚本（如自动切换显示器分辨率/刷新率以匹配流参数）

- **自动分辨率切换脚本钩子**：流启动前执行do脚本（如切换到最佳分辨率/刷新率匹配串流），流结束后执行undo脚本（恢复原始桌面分辨率）。支持PowerShell (Windows)和Bash (Linux)
- **QoS数据包标记**：Sunshine支持DSCP (Differentiated Services Code Point)标记，对不同协议流设置IP优先级位。视频流标记为EF (低延迟)，音频流AF41 (高可靠性)，输入流AF21 (低丢包)。企业交换机可基于DSCP标记实现QoS策略
- **NVIDIA NVML集成**：Sunshine通过NVML API读取GPU编码器使用率、温度、功耗。在Web UI中实时显示编码器负载，帮助诊断性能瓶颈

### Sunshine版本演进关键节点
| 版本 | 日期 | 关键更新 |
|------|------|---------|
| v0.1 | 2020-01 | 首个概念验证：仅NVIDIA NVENC H.264编码+DXGI捕获，Linux不可用 |
| v0.10 | 2021 | 引入AMD AMF和Intel QSV编码支持。首个Linux实验版本（VAAPI+KMS/DRM) |
| v0.15 | 2022 | HEVC (H.265)编码支持，HDR10初步支持。引入PipeWire音频捕获 |
| v0.20 | 2023-03 | NVIDIA宣布停止GameStream→Sunshine成为Moonlight主要服务端。AV1编码实验支持。Web UI重写 |
| v0.21 | 2023-07 | YUV 4:4:4色度子采样支持。macOS 14.2 ScreenCaptureKit支持。Vulkan Video编码框架 |
| v0.22-0.23 | 2024 | FreeBSD支持。XDG Desktop Portal+KWin Screencast（Wayland三大捕获协议完整覆盖） |
| v2025.x | 2025 | 版本号改为年份格式。AV1编码稳定。DSCP QoS标记。10-bit色深HDR |
| v2026.516 | 2026-05 | 第46个Release。优化Wayland DMA-BUF零拷贝路径。PipeWire音频低延迟模式(<10ms) |

### Moonlight客户端版本矩阵
| 客户端 | 最新版本 | Stars | 平台 | 备注 |
|--------|---------|-------|------|------|
| Moonlight PC (Qt) | v6.1.0 | 17,900+ | Win/Mac/Linux | 桌面统一客户端，48个Release |
| Moonlight Android | v11.0 | 8,000+ | Android | Google Play上架，MediaCodec硬件解码 |
| Moonlight iOS | v2.x | 独立仓库 | iOS/tvOS | App Store上架，VideoToolbox解码 |
| Moonlight Switch | 社区维护 | - | Nintendo Switch | 自制系统（Atmosphere） |
| Moonlight PS Vita | 社区维护 | - | PS Vita | 自制系统（HENkaku） |
| Moonlight Embedded | 社区维护 | - | Raspberry Pi | V4L2 M2M硬件解码 |
| Moonlight Web | 实验性 | - | 浏览器 | WebAssembly+WebCodecs API |

### NVIDIA GameStream停服时间线（2023年危机回顾）
| 日期 | 事件 |
|------|------|
| 2022-12 | NVIDIA宣布将GameStream从NVIDIA Games App中移除 |
| 2023-02 | GameStream从GeForce Experience中移除 |
| 2023-03 | Moonlight社区正式公告：所有用户应迁移到Sunshine |
| 2023-03-12 | Sunshine v0.20发布，标志Sunshine成为官方推荐服务端 |
| 2023-04 | Sunshine用户激增500%，Discord成员从2k跃升至15k |
| 2023-07 | Sunshine v0.21发布，YUV 4:4:4支持，功能超越原NVIDIA GameStream |
| 2024-2026 | Sunshine持续独立发展，全面超越原NVIDIA GameStream的所有功能维度 |

### 性能数据分析（社区测试，非官方认证）
| 场景 | 分辨率 | 帧率 | 编码器 | 编码延迟 | LAN延迟 | 解码延迟 | 端到端 |
|------|--------|------|--------|---------|--------|---------|-------|
| 办公桌面 | 1920x1080 | 60 | NVENC H.264 | 3ms | <1ms | 2ms | ~6ms |
| 3A游戏(4K) | 3840x2160 | 60 | NVENC HEVC | 8ms | <1ms | 5ms | ~14ms |
| FPS游戏(高帧率) | 1920x1080 | 120 | NVENC H.264 | 2ms | <1ms | 2ms | ~5ms |
| 3D渲染 | 2560x1440 | 60 | AMF HEVC | 12ms | <1ms | 4ms | ~17ms |
| 视频编辑(HDR) | 3840x2160 | 60 | QSV HEVC HDR | 10ms | <1ms | 6ms | ~17ms |
| AV1游戏(新GPU) | 2560x1440 | 120 | NVENC AV1 | 5ms | <1ms | 4ms | ~10ms |
## 4. 现状与生态
- **当前版本**：
  - Sunshine：v2026.516.143833（2026 年 5 月 16 日发布，46 个 Release）
  - Moonlight PC (Qt)：v6.1.0（2024 年 9 月 17 日发布，48 个 Release）
  - Moonlight Android：v11.0（2025 年发布，Google Play 上架）
  - Moonlight iOS/tvOS：v2.x（App Store 上架）
- **GitHub Stars / 活跃度**：
  - Sunshine：39,300+ Stars，持续活跃提交，社区响应及时
  - Moonlight PC：17,900+ Stars，重大版本发布活跃
  - Moonlight Android：8,000+ Stars
- **社区规模**：
  - Moonlight 和 Sunshine 各有独立 Discord 社区（数千在线成员）
  - Reddit r/MoonlightStreaming（活跃子版）+ r/cloudygamer（综合游戏串流社区）
  - LizardByte 组织下的多个子项目社区联动
- **文档 / SDK / API 生态**：
  - 官方文档覆盖安装配置、故障排除、编解码器对比、硬件推荐
  - 无正式 SDK，但 GameStream 协议基于公开的逆向工程文档
  - 社区贡献了大量多语言教程、YouTube 配置指南、Docker 部署模板
- **已知缺陷或限制**：
  1. **依赖专有协议的历史教训**：Moonlight 最初依赖 NVIDIA GameStream 专有协议，NVIDIA 一纸公告（2023 年停止支持）几乎葬送社区。Sunshine 的自研是对这一教训的补救，但用了 3 年时间才达到比原 GameStream 更好的水平
  2. **RTSP 在远程桌面的适用性有限**：RTSP/SDP 是为流媒体设计的（像 RTMP 推流），其会话管理能力远不如 WebRTC SDP 协商灵活。固定消息格式限制了动态参数协商能力，例如无法在流中无缝切换编码器
  3. **端口过多带来的网络挑战**：六个端口（47984, 47989, 47998-48010）在企业防火墙和公共 WiFi 环境中几乎全部被封锁。远程连接只能依靠 VPN 或 Tailscale
  4. **WAN 场景下的连接困难**：设计目标为 LAN 游戏串流（低延迟有线/5GHz WiFi），远程连接需要额外的 VPN 方案（Tailscale/WireGuard/ZeroTier），增加了配置复杂度
  5. **非游戏场景的输入延迟**：在 4K 桌面场景下，直接鼠标控制模式存在输入队列处理延迟（与游戏场景的光标捕获模式使用不同的输入路径）
  6. **逆向工程的法律灰色地带**：原始 GameStream 协议基于逆向工程 NVIDIA 专有协议，始终面临法律风险。Sunshine 通过完全重新实现来解决这一问题，但历史包袱依然存在

  7. **macOS Host的ScreenCaptureKit限制**：macOS 14.2+引入的ScreenCaptureKit是用户态API，无法像Windows DXGI那样实现GPU直接帧捕获。所有帧需经过CPU中转，增加8-12ms延迟开销。这是macOS平台的根本限制
  8. **HDR在非HDR客户端上的色调映射**：如果Host输出HDR但Client显示器不支持HDR，Moonlight需要在客户端执行HDR至SDR色调映射。通过GPU像素着色器实现，增加2-5ms渲染延迟。不同HDR标准的映射算法各异，质量一致性难保证
  9. **音频延迟的跨平台差异**：Windows WASAPI低延迟模式(<10ms)在Linux PulseAudio(>30ms)和PipeWire(<15ms)之间性能差异巨大
  10. **多手柄蓝牙干扰**：4+手柄通过蓝牙同时连接客户端时，蓝牙信道的时分复用导致输入延迟抖动（5-30ms随机波动）。USB有线连接手柄是16人同时游戏的推荐方案
## 5. 市场定位
- **主要应用行业**：
  - 个人游戏玩家（将台式机游戏串流到客厅/卧室/移动设备）
  - 家庭影音串流（Steam Deck + Steam Link + Apple TV 生态）
  - 技术爱好者（DIY 云游戏、家庭服务器、树莓派实验）
  - 部分远程办公场景（对延迟有极端要求的开发者/设计师）
  - 教育和研究（学习游戏串流协议的实现，作为计算机图形学和网络课程案例）
- **竞品对比简表**：

| 竞品 | 优势 | 劣势 |
|------|------|------|
| Parsec | 7ms 更低延迟（GPU 零拷贝）、更好的 WAN 场景（NAT 穿透 97%）、BUD 协议优化 | 仅 Windows Host、仅硬件编码、闭源、4:4:4 和双屏需付费 Warp |
| Steam Remote Play | Steam 生态无缝、手柄适配最佳（Valve 原生）、自动发现局域网设备 | Steam DRM 绑定、非 Steam 游戏配置复杂、仅客户端开源 |
| RustDesk | 全功能远程桌面（文件传输/终端/端口转发）、自托管、开源 | 延迟不如游戏串流方案、TCP 优先策略 |
| RDP (远程桌面) | 操作系统级集成、GPU 加速渲染、企业安全（NLA/Certificate） | 延迟高（50-100ms LAN）、非游戏场景设计、独占/WDDM 会话冲突 |

- **定价 / 许可**：
  - Moonlight：GPLv3 — 完全免费，无商业版本，无内购
  - Sunshine：GPLv3 — 完全免费，无商业版本，无广告
  - 两者均无订阅、无付费功能、无数据收集

## 6. 产品特色
1. **六协议分离的架构范式**：GameStream 的配对（HTTPS）、流控制（RTSP-SDP）、视频（RTP）、音频（RTP）、输入（ENet）五个独立协议各司其职，实现控制平面（TCP）和数据平面（UDP）的彻底分离。这种设计避免了单一协议的复杂性膨胀，每个子协议使用业界标准（RTSP/RTP/ENet 都是公开发表的 RFC 或协议规范），独立演进互不影响。对 MediaServo 的协议模块化设计（信令/媒体/控制通道分离）有直接参考价值。
2. **全 GPU 厂商的无锁兼容**：Sunshine 是唯一能同时通过 FFmpeg 类抽象层桥接 NVENC/AMF/QSV/VAAPI/VideoToolbox/Media Foundation/Vulkan Video 七个编码后端的开源方案。不绑定任何 GPU 厂商，不依赖任何专有驱动接口（NvFBC 是可选的增强通道而非必需路径）。这种"不锁定"的设计哲学来自于 Moonlight 被 NVIDIA GameStream 锁定的惨痛教训。
3. **从危机中诞生的架构韧性**：2023 年 NVIDIA 停止 GameStream，Moonlight 社区面临"协议黑洞"。Sunshine 用 3 年时间从一个堆叠补丁的项目成长为比原 NVIDIA GameStream 更强大的替代品（支持更多 GPU 厂商、更多编解码器、更多操作系统）。这段历史是"不要绑定单一厂商"的最佳案例——也是 MediaServo 在设计之初就坚持全 GPU 兼容和全协议开放性（RFC 标准）的直接理由。
4. **客户端覆盖矩阵无出其右**：从桌面到移动、从游戏主机（Switch/Vita）到机顶盒（Apple TV/Steam Link）、从 Web 到嵌入式（Raspberry Pi），Moonlight 的覆盖范围证明了一个理念：**当协议是开放标准时，社区能实现任何商业产品都无法企及的跨平台覆盖**。这对 MediaServo 的 WebRTC 标准协议选择提供了强有力的证据支撑。
5. **AV1 编码的前瞻布局**：Sunshine 在 AV1 编解码器上的支持（AV1 是新一代免版税编解码器，压缩效率比 HEVC 高 30%）体现了其技术前瞻性。在 NVIDIA RTX 4000 系列+ 和 Intel Arc GPU 上支持 AV1 硬件编码，在整个 H.264/H.265 专利困局中提供了一条清晰的未来路径。

## 7. 对 MediaServo 的参考价值
### [Adopt] 可直接借鉴
- **控制平面与数据平面的协议分离**：信令走 TCP（可靠性），媒体和输入走 UDP（低延迟），每条数据流独立通道。MediaServo 的 ProtocolBroker 应继承这一设计原则
- **多 GPU 厂商编码 API 的统一抽象层设计**：Sunshine 的 FFmpeg 类工厂模式（每个编码后端实现统一接口）可直接移植到 MediaServo 的 HardwareEncoder 插件的 trait 设计
- **编解码能力协商机制**（Capabilities Negotiation）：客户端声明解码矩阵（支持的编解码器/分辨率/色度/HDR），服务端选择最优编码组合。这是 WebRTC SDP 协商的核心思想
- **延迟分解可视化**：在开发阶段作为性能诊断工具，分别展示编码/网络/解码/渲染各环节的延迟数据。帮助精确识别和定位性能瓶颈
- **使用业界标准协议（RTP/RTSP/ENet）而非自研协议**：每个子协议都是公开发表的 RFC 或协议规范，第三方可独立实现

### [Adapt] 需修改后采用
- **GameStream 固定六端口 → MediaServo 多路复用为 1-2 个端口**：
  - 信令 + 配对 + 流控制合并为一个 WebSocket 连接（SDP/ICE 协商复用）
  - 视频 RTP + 音频 RTP 改为 WebRTC 的媒体轨道（同端口多路复用）
  - 输入控制 ENet 改为 WebRTC RTCDataChannel（无序模式 + maxRetransmits=0 等效于低延迟可靠通道）
- **RTSP + SDP 固定消息格式 → WebRTC SDP 协商**：更灵活、可扩展性更强、支持更多动态参数（simulcast/SVC 等）
- **LAN-only 设计（无 NAT 穿透）→ ICE/STUN/TURN 标准协议族**：
  - MediaServo 需要支持 WAN 场景（远程办公、车端→云端）
  - WebRTC ICE 框架天然支持 STUN/TURN/中继 fallback
- **HTTPS 配对 + 证书认证 → SRP/PAKE + OIDC**：扩展认证方式，支持企业 SSO 和自托管场景
- **端口号硬编码 → IANA 注册或动态端口**：避免与防火墙规则冲突

### [Avoid] 已知坑 / 不适用场景
- **依赖单一厂商专有协议**：Moonlight 的历史教训是血淋淋的。MediaServo 的核心协议从设计之初就必须标准化、文档化、可被第三方独立实现。如果一个厂商停止支持，整个社区不会随之消亡
- **RTSP 在远程桌面的局限性**：RTSP 是"推流"模式，而远程桌面需要频繁的动态协商（分辨率切换、编码器切换、多用户接入/退出）。WebRTC SDP 协商更适合这一场景
- **逆向工程的法律和可持续性风险**：MediaServo 的协议必须从 Phase 0 就开始公开文档化，不存在任何"先对标再公开"的灰色地带
- **LAN-only 设计的连接限制**：MediaServo 从 Phase 0 就需要设计 WAN 连接方案（ICE 穿透 + TURN 中继 + 信令服务），不能像 Sunshine 那样"先做 LAN，WAN 靠 VPN 凑合"
- **多端口在企业防火墙中的死亡**：6 个端口在严格防火墙环境中几乎全部被封锁。MediaServo 应使用单一端口（或 1-2 个端口）多路复用所有业务流量
- **GPLv3 许可证边界**：不能直接复用 Sunshine/Moonlight 代码到 MediaServo 的 Apache 2.0 项目。架构设计思想可借鉴，代码实现需独立

**总体评分**：★★★★☆ (4/5)
**评语**：六协议分离架构和全 GPU 厂商统一抽象的最佳参考。对 MediaServo 的协议模块化结构设计和编码器插件体系有重要启发。但依赖专有协议的历史教训（NVIDIA GameStream 停止支持）和 LAN-only 设计是 MediaServo 必须避免的陷阱。协议开放性、全平台兼容性和 WAN 场景支持是 MediaServo 从 Phase 0 就应该坚持的设计原则。

### GameStream六协议至WebRTC的迁移映射
如果MediaServo采用WebRTC标准协议栈替代GameStream的六个独立协议，迁移映射如下：
| GameStream协议 | WebRTC等效 | 差异 |
|----------------|------------|------|
| HTTP (服务发现) | ICE Candidate交换 | WebRTC通过信令服务交换ICE candidates，无独立HTTP发现 |
| HTTPS (配对认证) | DTLS握手（内嵌于WebRTC） | WebRTC DTLS提供端到端加密和证书认证 |
| RTSP (流控制) | SDP Offer/Answer | WebRTC SDP比RTSP更灵活，支持Simulcast/SVC |
| RTP视频 | RTP视频（同一端口） | WebRTC内置RTP/RTCP+带宽估计+拥塞控制 |
| RTP音频 | RTP音频（同一端口） | WebRTC同一端口多路复用音视频 |
| ENet (输入控制) | RTCDataChannel（无序模式） | WebRTC RTCDataChannel通过SCTP over DTLS，maxRetransmits=0等效ENet低延迟 |

WebRTC的优势：
- 端口降维：6个端口至1-2个端口，在企业防火墙中存活率极高
- 标准化：所有组件都是RFC标准，不再依赖逆向工程协议
- 生态丰富：libwebrtc/webrtc-rs/pion(Go)等成熟WebRTC实现
- NAT穿透：ICE/STUN/TURN是WebRTC原生能力

代价：
- SDP协商复杂度：比RTSP固定消息格式更复杂，需要完整SDP解析/生成
- 带宽估计不确定性：WebRTC GCC为视频会议设计，在远程桌面高带宽低延迟场景可能非最优
- 依赖树：libwebrtc依赖树>200+（Parsec/AnyDesk明确拒绝的原因），但webrtc-rs更轻量（~30依赖）
**相关决策**: D47 (渲染), D-SIMULCAST, C5 (&[u8] boundary)

## 附录：Sunshine编码器工厂模式分析
Sunshine的FFmpeg类编码器工厂是MediaServo HardwareEncoder插件设计的直接参考：
```cpp
// Sunshine编码器工厂模式（概念抽象，非精确源代码）
class EncoderFactory {
public:
  virtual std::unique_ptr<VideoEncoder> create(
    const EncoderConfig& config) = 0;
  virtual bool supports_codec(Codec codec) = 0;
  virtual std::vector<Codec> supported_codecs() = 0;
};

// 具体实现示例
class NVENCEncoderFactory : public EncoderFactory {
public:
  std::unique_ptr<VideoEncoder> create(const EncoderConfig& c) override {
    return std::make_unique<NVENCEncoder>(c.width, c.height, c.fps);
  }
  bool supports_codec(Codec c) override {
    return c == Codec::H264 || c == Codec::HEVC || c == Codec::AV1;
  }
};
```

在Rust中的等效设计：
```rust
#[async_trait]
pub trait HardwareEncoder: Send + Sync {
    async fn encode(&mut self, frame: &VideoFrame) -> Result<EncodedPacket>;
    fn supported_codecs(&self) -> Vec<Codec>;
    fn capabilities(&self) -> EncoderCapabilities;
    async fn reconfigure(&mut self, config: EncoderConfig) -> Result<()>;
}

struct EncoderCapabilities {
    max_resolution: (u32, u32),
    max_fps: u32,
    supports_hdr: bool,
    supports_444: bool,  // YUV 4:4:4
    codecs: Vec<Codec>,
}
```
关键设计点：
- trait而非抽象类：Rust的trait系统天然支持多态和动态分发
- async编码接口：编码可能涉及GPU同步，应为异步操作
- reconfigure()方法：支持流中切换编码参数（对应WebRTC的renegotiation）
- capabilities()查询：让信令协商知道每个编码器的能力矩阵

## 附录：从GameStream危机看协议开放性的必要性
NVIDIA停止GameStream的历史是MediaServo协议设计哲学的最强案例支撑：
| 如果Moonlight... | 结果 | MediaServo的教训 |
|-----------------|------|----------------|
| 继续依赖NVIDIA GameStream | 2023年社区消亡 | 不要依赖任何单一厂商的专有协议 |
| 只有Android客户端开源 | iOS/tvOS/Switch/PSVita用户被抛弃 | 协议的开放性超越客户端实现 |
| 协议基于逆向工程（不易独立实现） | Sunshine需要3年追赶 | 从Phase 0公开文档化核心协议 |
| 没有Sunshine社区 | 整个GameStream生态消亡 | 社区是协议生命力的最终保障 |

MediaServo应从中吸取的最核心教训：**协议的开放性（RFC标准+公开文档+可被第三方独立实现）不是nice-to-have，而是生存的必需条件。** 如果MediaServo依赖任何专有技术，它可能在三五年后成为下一个GameStream——被厂商抛弃、社区消亡、遗产代码。WebRTC标准协议栈的选择不是技术偏好，而是架构生存的战略决策。

## 附录：Moonlight客户端覆盖启示
Moonlight的11个平台覆盖来自于：
1. GameStream协议基于标准化子协议——每个子协议都有开源实现
2. 开源社区的分布式贡献——每个平台由不同开发者独立维护

对MediaServo：WebRTC的标准化程度更高（所有子组件都是RFC标准），理论上客户端覆盖可超越Moonlight。关键挑战是屏幕捕获和输入注入的各平台适配。

## 附录：Sunshine架构韧性分析
Sunshine在3年内发展为功能超越NVIDIA GameStream的服务端，其架构韧性来自于：

1. 编码器工厂模式：新增GPU编码器只需实现统一接口
2. 屏幕捕获多后端：新平台支持通过添加捕获模块实现
3. GitHub社区驱动：多子项目结构使贡献者可独立负责各自模块
4. 版本号灵活演进：从v0.1到v2026.516，适应发布节奏变化

这些直接映射到MediaServo：
- HardwareEncoder Plugin trait = 编码器工厂模式
- ScreenCapture Plugin trait = 屏幕捕获多后端
- crates工作空间 = 多子项目模块化

## 附录：AV1编码在远程桌面的前景
Sunshine的AV1支持是编解码器演进的前瞻方向：

AV1优势：免版税、比HEVC压缩率高30%、支持4:4:4和HDR、新GPU硬件编码
AV1限制：仅新GPU支持硬件编码、软件编码比VP9慢2-3x、客户端解码要求高于H.264/HEVC

MediaServo AV1策略：Phase 1用VP8/VP9+H.264/H.265；Phase 2+当RTX 4000+/Arc GPU普及后启用AV1硬件编码；通过Encoder Capabilities协商自动选择最佳编码器。

### Moonlight+Sunshine技术债务清单
1. 依赖单一厂商专有协议（已由Sunshine修复，但教训永久）
2. RTSP在远程桌面的会话管理能力有限
3. 六个端口在企业防火墙中几乎全部被封锁
4. LAN-only设计：WAN需要VPN/Tailscale
5. 逆向工程的法律灰色地带（已由Sunshine重新实现解决）
6. macOS Host的ScreenCaptureKit限制（CPU中转增加8-12ms延迟）
7. HDR与非HDR客户端的色调映射复杂性
8. 多手柄蓝牙干扰（建议USB有线连接）

### 总结：Moonlight+Sunshine对MediaServo的核心价值
Moonlight+Sunshine贡献了三个对MediaServo至关重要的设计遗产：
1. 六协议分离架构——证明控制平面与数据平面的严格分离是高性能远程传输的正确范式
2. 全GPU厂商兼容——证明不绑定单一厂商不仅是技术正确性，更是生态生存的必需条件
3. 从危机中重生——证明协议的开放性（RFC标准+公开文档）是生态长期生命力的唯一保障

MediaServo从Moonlight+Sunshine学到的最重要一课：NVIDIA停止GameStream不是意外，而是厂商控制协议的必然结果。MediaServo必须从Phase 0就以RFC标准为基础、以公开文档为规范、以社区生态为目标——这不是技术偏好，而是架构生存的战略决策。

## 附录：Moonlight+Sunshine对MediaServo功能优先级的影响
基于Moonlight+Sunshine的协议分离和编码器抽象经验，MediaServo架构建议：

Phase 0：WebRTC协议栈选型（替代GameStream六协议）、编码器Plugin trait定义（参考编码器工厂模式）
Phase 1：控制/数据平面分离实现、VP8/VP9+H.264/H.265多编码器支持、编解码能力协商机制
Phase 2：AV1硬件编码支持、HDR/4:4:4完整色度支持、性能叠加层延迟分解可视化
Phase 3：全GPU厂商编码API统一抽象层、编码器运行时热切换、DSCP QoS标记
Phase 4：Web客户端完善、更多非传统平台覆盖（嵌入式/游戏主机）
