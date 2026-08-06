# tether-rally 参考分析
> 生成日期：2026-07-16 | 分类：遥操作

## 1. 产品画像
- **名称**：tether-rally
- **开发者**：roman01la（个人开源项目）
- **首次发布**：2024 年（GitHub 仓库创建）
- **产品定位**：基于 WebRTC 的 ARRMA RC 遥控车全球远程操控系统，提供低延迟 FPV 驾驶体验。虽为玩具级项目，但其 RTCDataChannel 协议设计、分级安全机制和实时控制架构对生产级遥操作系统有直接参考价值
- **目标用户群体**：RC 遥控车爱好者、遥操作协议研究者、WebRTC RTCDataChannel 实践者
- **许可 / 商业模式**：开源（GitHub: roman01la/tether-rally），MIT 许可

## 2. 技术特性
### 整体架构
```
┌──────────────────────────────────────────────────────────────────┐
│                        Cloudflare Workers                        │
│              signaling + TURN + 静态页面托管                      │
│              Cloudflare Tunnel 穿透内网                           │
└─────────────┬──────────────────────────────────────┬─────────────┘
              │ WebRTC (Video + RTCDataChannel)         │ Cloudflare Tunnel
              │ P2P 直连 (LAN RTT ~10-15ms)          │
              ▼                                      ▼
┌──────────────────────────┐          ┌─────────────────────────────┐
│    Browser (操控端)       │          │    Raspberry Pi 4/5 (桥接层) │
│  ┌────────────────────┐  │          │  ┌─────────────────────────┐ │
│  │ Video 渲染          │  │          │  │ MediaMTX 视频服务        │ │
│  │ 720p @ 60fps       │  │          │  │  · WebRTC/WHEP 端点     │ │
│  │ 前置主摄像头 +     │  │          │  │  · H.264 转发 (无转码)  │ │
│  │ 后置 RTSP PiP      │  │          │  │  · RTSP → WebRTC 桥接   │ │
│  ├────────────────────┤  │          │  ├─────────────────────────┤ │
│  │ RTCDataChannel        │  │          │  │ HMAC-SHA256 Token 验证  │ │
│  │ ordered:false      │  │          │  │  · 短期有效              │ │
│  │ maxRetransmits:0   │  │          │  │  · 连接时验证一次       │ │
│  │ 自定义二进制协议    │  │          │  │  · 防未授权操控          │ │
│  │ 多通道:            │  │          │  ├─────────────────────────┤ │
│  │  · 控制 (50Hz)     │  │          │  │ WiFi/UDP → ESP32       │ │
│  │  · 遥测 (10Hz)     │  │          │  │  端口 54321              │ │
│  │  · 心跳 (2Hz)      │  │          │  │  无线 LAN (2.4/5GHz)    │ │
│  └────────────────────┘  │          │  └──────────┬──────────────┘ │
│  ┌────────────────────┐  │          └──────────────┼──────────────┘
│  │ Auto-reconnect     │  │                         │ WiFi/UDP
│  │ · 指数退避          │  │                         ▼
│  │ · 先重连信令再     │  │    ┌─────────────────────────────────────┐
│  │   WebRTC           │  │    │   ESP32 (安全执行 & 控制层)        │
│  └────────────────────┘  │    │   ┌──────────────────────────────┐ │
└──────────────────────────┘    │   │ FreeRTOS 双核架构            │ │
                                │   │ Core 0 (UDP 接收线程)        │ │
                                │   │  · 50Hz 控制帧接收           │ │
                                │   │  · 命令解析 (seq+cmd+payload) │ │
                                │   │  · 序列号乱序/丢包检测       │ │
                                │   ├──────────────────────────────┤ │
                                │   │ Core 1 (控制循环 200Hz)      │ │
                                │   │  · EMA 指数加权移动平均      │ │
                                │   │  · 斜率限制 (max_rate)       │ │
                                │   │  · 分级超时安全              │ │
                                │   │  · 稳定性控制链              │ │
                                │   ├──────────────────────────────┤ │
                                │   │ 传感器融合                   │ │
                                │   │  · BNO055 IMU (9-DOF)        │ │
                                │   │  · GPS 模块 (NEO-6M)         │ │
                                │   │  · 车轮 RPM 传感器 (IR)      │ │
                                │   ├──────────────────────────────┤ │
                                │   │ 执行器输出                   │ │
                                │   │  · MCP4728 12-bit DAC ×2    │ │
                                │   │  · steering 通道 (0-4095)    │ │
                                │   │  · throttle 通道 (0-4095)    │ │
                                │   │  · GPIO 大灯控制             │ │
                                │   └──────────────────────────────┘ │
                                └──────────────┬──────────────────────┘
                                               │ DAC 模拟信号
                                               ▼
                                ┌─────────────────────────────────────┐
                                │  ARRMA 遥控器 (Trainer Port)        │
                                │  → RC Car (ARRMA Granite/Fury)     │
                                └─────────────────────────────────────┘
```

### 关键技术能力
| 能力 | 详情 |
|------|------|
| 视频传输延迟 | LAN ~120ms G2G（采集→编码→网络→解码→渲染），Internet ~150ms+；720p@60fps H.264 硬件编码 |
| 控制协议 | 自定义最小二进制协议：`seq(uint16 LE) + cmd(uint8) + payload`，控制包仅 7 字节 (3 字节头 + 4 字节负载) |
| RTCDataChannel | `ordered: false, maxRetransmits: 0`（纯 UDP 语义），控制频率 50Hz（20ms 间隔），4 条独立 RTCDataChannel |
| 安全冗余 | ESP32 端三级超时：L1(80ms 保活) → L2(250ms 回中) → L3(长期无包安全停车)；WiFi 信号监测 |
| 编码 | Raspberry Pi Camera Module 3 硬件 H.264 编码，零 CPU 开销；GStreamer/MediaMTX 封装 |
| 多通道分离 | 控制(50Hz)·遥测(10Hz)·心跳(2Hz)·可靠指令(按需) — 四条独立 RTCDataChannel |
| 认证 | HMAC-SHA256 短期 Token，Pi 端验证，防未授权操控 |
| 缓冲管理 | bufferedAmount 监控 + onbufferedamountlow 事件驱动背压处理 |

### WebRTC RTCDataChannel 配置详解
```
控制通道 (dc-ctrl):
  ordered: false          ← 关闭排序，旧指令到达即丢弃
  maxRetransmits: 0       ← 不重传，过时指令比丢包更危险
  protocol: "ctrl"        ← 子协议标识
  发送频率: 50Hz (每 20ms)
  包大小: 7-11 字节

遥测通道 (dc-telem):
  ordered: false          ← 最新遥测优先
  maxRetransmits: 0
  protocol: "telem"
  发送频率: 10Hz
  包大小: 37-64 字节

心跳通道 (dc-hb):
  ordered: false
  maxRetransmits: 0
  protocol: "heartbeat"
  发送频率: 2Hz
  包大小: 5 字节 (仅 seq + cmd)

可靠通道 (dc-reliable):
  ordered: true           ← 有序保证
  reliable: true          ← 可靠传输
  protocol: "reliable"
  发送频率: 按需 (RACE 模式切换/配置下发)
```

### 技术栈
- **前端**：HTML/CSS/JavaScript（浏览器原生 WebRTC API，无框架）
- **桥接层 OS**：Raspberry Pi OS（Bookworm），Node.js v20+
- **桥接层软件**：`wrtc` / 浏览器 API（WebRTC），`node-fetch`（信令）
- **视频服务**：MediaMTX 1.x（开源 Go 编写的 WebRTC/WHEP 服务器）
- **边缘控制**：ESP32-WROOM-32E，FreeRTOS 双核，Arduino 框架 C++
- **云服务**：Cloudflare Workers (JS)、Cloudflare TURN、Cloudflare Tunnel
- **DAC**：MCP4728 12-bit I2C DAC ×2（steering + throttle，分辨率 4096 级）
- **传感器**：BNO055 9-DOF IMU（加速度计、陀螺仪、磁力计）、NEO-6M GPS、IR RPM 传感器
- **认证**：HMAC-SHA256，Python 脚本生成 Token（有效期可配置）
- **视频编码**：H.264 硬件编码（Pi Camera Module 3，Broadcom VideoCore VI ISP）
- **音频**：无音频通道（设计取舍：FPV 驾驶无需语音通信）

### 延迟数据与性能指标

| 测量项 | LAN 环境 | Internet (4G 热点) | 说明 |
|------|---------|------------------|------|
| 视频 G2G 延迟 | ~120ms | ~150ms+ | 720p@60fps H.264 硬件编码全链路 |
| RTCDataChannel P2P RTT | ~10-15ms | ~30-100ms | 依赖 TURN 中继距离和 NAT 类型 |
| 控制指令全链路延迟 | ~15-20ms | ~50-120ms | Browser → Pi → WiFi UDP → ESP32 |
| ESP32 控制循环 | 5ms (200Hz) | 5ms (200Hz) | Core 1 固定速率，不受网络影响 |
| Camera 编码延迟 | ~8-12ms | ~8-12ms | Pi Camera Module 3 硬件 H.264 编码器 |
| 浏览器解码延迟 | ~5-8ms | ~5-8ms | 硬件加速解码 (VideoToolbox/VAAPI) |
| 信令连接建立时间 | ~1-2s | ~3-5s | Cloudflare Workers 信令 + ICE 打洞 |
| HMAC 认证延迟 | ~50ms | ~50-200ms | Token 生成与验证 |

### 二进制控制协议字段详解

tether-rally 的自定义二进制协议共定义 10 种命令类型，覆盖控制、心跳、遥测、配置和管理全场景：

**CTRL (0x01) 控制命令 — 7 字节：**
```
Byte 0-1: seq (uint16 LE)    — 序列号，接收端检测丢包/乱序
Byte 2:   cmd (uint8)        — 0x01 = CTRL
Byte 3-4: steering (int16 LE) — 转向值，范围 -4095 ~ +4095，0 为居中
Byte 5-6: throttle (uint16 LE) — 油门值，范围 0 ~ 4095，0 为全制动
```

**TELEM (0x07) 遥测上报 — 37 字节：**
```
Byte 0-1:  seq (uint16 LE)         — 序列号
Byte 2:    cmd = 0x07              — 命令标识
Byte 3-4:  speed (uint16 LE)       — km/h × 10 (0-6553.5 km/h)
Byte 5-6:  rpm (uint16 LE)         — 车轮转速 (0-65535 RPM)
Byte 7-10: lat (int32 LE)          — 纬度 × 10^7
Byte 11-14: lon (int32 LE)         — 经度 × 10^7
Byte 15-16: heading (uint16 LE)    — 航向角 0.01° (0-359.99°)
Byte 17-18: accel_x (int16 LE)     — 加速度 m/s² × 100
Byte 19-20: accel_y (int16 LE)
Byte 21-22: accel_z (int16 LE)
Byte 23-24: gyro_x (int16 LE)      — 角速度 °/s × 10
Byte 25-26: gyro_y (int16 LE)
Byte 27-28: gyro_z (int16 LE)
Byte 29-30: battery (uint16 LE)    — 电池电压 mV
Byte 31-32: rssi (int16 LE)        — WiFi 信号强度 dBm
Byte 33-34: status_flags (uint16 LE) — 位掩码
Byte 35-36: crc16 (uint16 LE)      — CRC-16/XMODEM 校验
```

**CONFIG (0x05) 配置命令字段：**
| 参数 | 类型 | 范围 | 说明 |
|------|------|------|------|
| max_throttle | uint16 | 0-4095 | 油门上限（默认 30%） |
| turbo_throttle | uint16 | 0-4095 | Turbo 模式油门上限（默认 65%） |
| smoothing_alpha | uint8 | 0-100 | EMA 平滑系数 × 100 |
| rate_limit | uint16 | 0-4095 | 斜率限制（每 5ms 最大变化量） |
| traction_enabled | uint8 | 0/1 | 牵引力控制开关 |
| abs_enabled | uint8 | 0/1 | ABS 开关 |
| timeout_l1_ms | uint16 | 0-1000 | 一级超时阈值（保持，默认 80ms） |
| timeout_l2_ms | uint16 | 0-1000 | 二级超时阈值（回中，默认 250ms） |
| timeout_l3_ms | uint16 | 0-5000 | 三级超时阈值（安全停车，默认 2000ms） |

### ESP32 数据流吞吐分析

| 连接类型 | 吞吐量 | 控制频率 | 数据方向 |
|---------|--------|---------|---------|
| RTCDataChannel | ~1-2 KB/s | 50Hz TX | Browser → Pi |
| RTCDataChannel | ~300-500 B/s | 10Hz TX | Pi → Browser |
| WiFi UDP | ~350 B/s | 50Hz TX | Pi → ESP32 |
| WiFi UDP | ~370 B/s | 10Hz TX | ESP32 → Pi |
| DAC 输出 | — | 200Hz | ESP32 → Servo |

控制通道总带宽消耗极低：50Hz CTRL 指令仅 350 B/s（7B × 50Hz），10Hz TELEM 上报 370 B/s（37B × 10Hz）。若使用 JSON 替代二进制，CTRL 指令将膨胀至 80-120 字节（~5 KB/s），TELEM 膨胀至 150-200 字节（~2 KB/s），总带宽需求增加约 8-10 倍。

### 视频编码管线详解

```
Pi Camera Module 3 → Broadcom ISP → H.264 硬件编码器 → NAL 打包 → MediaMTX WHEP
  → WebRTC SDP 协商 → ICE 打洞 → DTLS-SRTP 加密 → RTP 分组 → 网络 → 浏览器解码 → 渲染

编码参数: 1280×720 (720p), 60fps, VideoCore VI H.264, VBR, Profile High, GOP ~30

延迟分解:
  Sensor 曝光 + ISP 处理: ~3-4ms
  H.264 编码: ~4-6ms
  NAL + RTP 打包: ~1-2ms
  网络传输 (LAN): ~1ms
  jitter buffer: ~33-50ms (@60fps)
  解码: ~5-8ms
  渲染合成: ~1ms
  ─────────────────────────
  总 G2G 延迟: ~120-150ms
```

## 3. 功能概览
### 核心功能模块

**视频传输模块**
- 720p @ 60fps 低延迟 FPV 视频流（主体视觉）
- Pi Camera Module 3 硬件 H.264 编码
- 支持 RTSP 后置摄像头作为 PiP（Picture-in-Picture）画中画
- MediaMTX 提供 WebRTC/WHEP 端点
- 视频路径：Pi Camera → Broadcom ISP → H.264 硬件编码器 → MediaMTX → WebRTC → Browser
- PiP 路径：后置 RTSP Camera → MediaMTX 桥接 → 与主流合成（前端 canvas）→ 显示

**控制传输模块**
- WebRTC RTCDataChannel 双向通信（4 条独立 DC）
- 自定义最小二进制控制协议（7-11 字节）
- 10 种命令类型：
  - PING(0x00): 心跳请求
  - CTRL(0x01): 方向盘+油门+模式位（7 字节）
  - PONG(0x02): 心跳响应（含 RTT 计算）
  - RACE(0x03): 模式切换（练习/竞赛）
  - STATUS(0x04): ESP32 状态上报
  - CONFIG(0x05): 运行时参数配置
  - KICK(0x06): ESP32 软重启
  - TELEM(0x07): 遥测数据上报
  - TURBO(0x08): Turbo 模式切换
  - TRACTION(0x09): 牵引力控制切换
- 控制频率 50Hz（RTCDataChannel → UDP → ESP32 全链路）
- 序列号机制：uint16 LE（0-65535 回绕），接收端检测丢包/乱序

**ESP32 边缘执行层**
- FreeRTOS 双核架构
- Core 0 (接收线程)：50Hz UDP 接收 + 命令解析 + 序列号检测
- Core 1 (控制循环)：200Hz 运行频率
  - EMA 指数平滑（smoothing factor α=0.3）
  - 斜率限制（最大变化率 rate_limit per 5ms tick）
  - 分级超时响应
  - 稳定性控制链串联执行
- 12-bit MCP4728 DAC 输出（4096 级精度 vs 8-bit PWM 256 级）
- 传感器数据融合（IMU + RPM + GPS 用于稳定性控制）

**遥测系统**
- GPS 位置追踪（经纬度 + 航向角 + 速度矢量）
- 实时速度与车轮 RPM 监控
- IMU 数据（3 轴加速度、3 轴角速度、3 轴欧拉角/四元数姿态）
- 指南针方向（磁力计融合）
- 电池电压监测（通过 ADC 分压电路）
- WiFi 信号 RSSI

**竞赛管理模块**
- 圈速计时（GPS 触发或手动触发）
- 分段计时（split times）
- 比赛状态管理（ready / racing / finished / DNF）

### 特色功能

**稳定性控制系统（独特的工程实践）**
1. **Traction Control（牵引力控制）**：IMU + 车轮 RPM 滑移检测，当检测到车轮速度 > 地面速度 × 1.15 时自动限制油门输出。滑移阈值和干预力度可通过 CONFIG 命令远程调节
2. **Stability Control（稳定性控制）**：基于偏航率的过度转向/不足转向检测。计算期望偏航率（转向角 × 速度 / 轴距）vs 实际偏航率（IMU 陀螺仪），偏差超过阈值时介入——过度转向：减少转向角度 + 增加反向油门；不足转向：增加转向角度 + 减少油门
3. **ABS（防抱死制动）**：ESC 状态机控制（IDLE → BRAKING → ABS_ACTIVE → RELEASED），车轮 RPM 检测到锁死时脉冲制动
4. **Hill Hold（坡道保持）**：IMU 俯仰倾角 > 5° 时自动施加制动保持，油门超过 10% 时释放
5. **Coast Control（滑行控制）**：滑行（油门=0）时检测到后退加速度（IMU），自动注入小油门防止后退
6. **Surface Adaptation（路面自适应）**：持续监控车轮滑移率，动态估计路面抓地力系数 μ，自适应调整 Traction Control 和 ABS 的触发阈值
7. **Steering Shaper（转向整形）**：速度越高，最大允许转向角度越小（v² 反比关系）。高速时降低转向灵敏度防止翻车。检测到过度转向时自动施加反打辅助

**驾驶辅助功能**
- **Turbo 模式**：油门限值从 30% 提升至 65%，适合直线加速段
- **大灯控制**：GPIO → MOSFET 驱动高功率 LED
- **油门限制**：在车辆端（ESP32）强制执行，不信任浏览器指令。ESP32 的油门输出上限硬编码为 config.max_throttle（默认 30%，Turbo 65%）

### 扩展性
- 模块化硬件架构（Pi + ESP32 + 传感器分离，各组件通过标准协议连接）
- 二进制协议可扩展：命令码 0x00-0x09 仅用 10 种，有 246 种命令空间可用
- 视频源可扩展：已支持主摄像头 + 后置 RTSP 摄像头 PiP，可增加更多 RTSP 源
- 驾驶辅助系统采用独立模块设计，每个模块可单独启用/禁用（通过 CONFIG 命令）
- 传感器可替换：IMU/GPS/RPM 传感器可通过 I2C/UART 标准接口替换为不同型号
- 社区贡献友好：清晰的硬件 BOM 和接线图，README 含完整部署指南
- 信令层解耦：Cloudflare Workers 信令可用其他实现替换（如自建 WebSocket 服务）

## 4. 现状与生态
- **当前版本**：活跃开发中（最近更新 2026-06-30），无正式版本号，遵循主分支持续发布
- **GitHub Stars / 活跃度**：37 Stars, 6 Forks, 0 Open Issues
- **社区规模**：小型个人开源项目，非社区驱动。Issue 和 PR 由作者直接管理
- **文档 / SDK / API 生态**：
  - README 包含完整硬件 BOM（物料清单）、接线图（Fritzing 格式）
  - 系统架构说明（Markdown + 框图）
  - 环境搭建指南（Pi 端 + ESP32 端 + 云部署）
  - 无 SDK 或 API；协议格式在源码中定义（TypeScript 类型 + C 头文件）
  - 控制协议和命令编码在源码注释中说明
- **已知缺陷或限制**：
  - 仅支持单一 ARRMA RC 车型号（Granite/Fury），硬件耦合度高
  - ESP32 WiFi 而非蜂窝网络 — 操控距离受限于本地网络覆盖（<100m WiFi 直连，<50m 穿墙）
  - Cloudflare 基础设施依赖重：无 TURN 则无法跨 NAT，无 Tunnel 则无法从公网访问 Pi
  - 无生产级安全认证（ISO 26262 / ISO 21434），不能直接用于载人车辆
  - 无多车并发支持（单 Pi 对单 ESP32 对单车）
  - RTCDataChannel JSON 遥测序列化效率低于全二进制（10Hz 遥测 JSON 包约 150 字节 vs 二进制 37 字节）
  - 无音频通道（仅视频 + 控制，无语音通信）
  - HMAC Token 无过期自动刷新 — 需手动生成新 Token 重新部署

### 项目演进历史

tether-rally 的架构经历了三个主要阶段：

| 阶段 | 时间 | 架构 | 控制延迟 | 备注 |
|------|------|------|---------|------|
| Phase 1 | 2024 Q3 | WebSocket relay (Node.js 中间件) | 100-200ms | 初期原型，浏览器→Node.js→Pi→ESP32 |
| Phase 2 | 2025 Q1 | Cloudflare Workers 信令 + 直连 | 30-60ms | 移除 Node.js 中继，改为 P2P 直连 |
| Phase 3 (当前) | 2025 Q3+ | WebRTC RTCDataChannel + 自定义二进制协议 | 10-15ms (LAN) | 统一视频和控制到 WebRTC |

### 社区生态与使用情况

- **GitHub 统计**：37 Stars, 6 Forks, 0 Open Issues, 1 Watch。仓库创建于 2024 年，共约 50+ commits
- **贡献者**：仅作者 roman01la 一人（100% commits），无外部合并 PR
- **讨论渠道**：GitHub Issues、Reddit r/rccars、r/webRTC 社区讨论
- **已知部署案例**：作者在个人后院和本地公园验证，最远成功操控距离约 800m（通过 TURN 中继）
- **技术债务**：前端为单文件 JavaScript（约 400 行，无框架、无 TypeScript），ESP32 固件无单元测试框架，稳定性控制参数（EMA α=0.3, rate_limit=200）硬编码需编译后修改
- **竞品定位 (RC FPV 遥操作领域)**：
  - vs 传统 RC 遥控器 (Spektrum DX/Futaba T): 传输距离 1-3km, 延迟 <10ms, 但无 FPV 视频反馈
  - vs DJI FPV 数字图传 (O3 Air Unit): 延迟 ~28ms, 1080p@60fps, 距离 15km, 但控制协议封闭
  - vs Rosserial/ROS2 Serial 方案：适合研究用途，但缺乏实时控制特性（分级超时、稳定性控制链）

### 已知问题与改进方向

1. **WiFi 范围限制**：ESP32 WiFi 覆盖距离 <100m（室外 LOS），<50m（室内穿墙），无法适配蜂窝网络模块
2. **Cloudflare 强依赖**：Signaling + TURN + Tunnel 均绑定 Cloudflare 服务，无自托管备选方案
3. **HMAC Token 管理缺失**：Token 由手动脚本生成，无自动过期刷新机制，泄露后无法热替换
4. **多摄像头受限**：仅支持单路主摄像头 + 单路 RTSP 后置 PiP，无多路同步支持
5. **无自适应码率**：视频编码参数固定，WebRTC 的 GCC 未被利用
6. **无音频通道**：FPV 驾驶无需语音通信的设计取舍，但 AUDEMSP 场景需要双向 Opus 音频

## 5. 市场定位
- **主要应用行业**：RC 遥控车爱好者社区、WebRTC 遥操作协议参考、教育/研究、机器人入门
- **竞品对比简表**：

| 维度 | tether-rally | TUM Teleoperated Driving | comma.ai openpilot (body) |
|------|-------------|--------------------------|---------------------------|
| 目标平台 | ARRMA RC 遥控车 | 全尺寸乘用车 + 模型车 | comma body 机器人 |
| 网络 | WiFi/LAN | LTE/5G + MQTT | WiFi/LTE (athenad) |
| 控制协议 | 7 字节二进制 (DC) | ROS2 DDS (UDP) | Cereal/Cap'n Proto → JSON |
| RTCDataChannel | 4 条独立 DC | 无 (RTSP+UDP 方案) | 双向 DC (JSON) |
| 安全等级 | 分级超时（ESP32 端） | 三级安全管道 | athenad 认证 + 错误分级 |
| 开源 | MIT | Apache 2.0 | MIT |
| 成熟度 | 个人项目 | 学术研究 | 生产级 (L2 汽车) |
| Stars | 37 | ~200 | 63,127 |
| 延迟数据 | 全链路分段公开 | 学术论文完整公开 | 未公开 |

- **定价 / 许可**：MIT 开源许可，完全免费；硬件 BOM 成本约 $200-400（Pi 4/5 $35-60 + ESP32 $8-15 + Camera Module $25 + DAC + 传感器 + RC 车）

## 6. 产品特色
1. **极致精简的二进制协议（7 字节控制包）**：展示了遥操作控制通道的理论最小开销 — seq(2B) + cmd(1B) + steering(2B) + throttle(2B) = 7B vs 标准 MTU 1500B。这证明了高速遥操作不需要大包，20Hz-200Hz 控制只需最小化包头
2. **三级超时安全模型**：ESP32 端执行分级安全响应（L1 80ms 保持 → L2 250ms 回中 → L3 长期安全停车），不信任网络传入的任何数据。安全逻辑完全在边缘端闭环，即使攻击者控制了浏览器也无法绕过
3. **稳定性控制链架构**：7 个独立模块串联修改最终油门/转向输出，每个模块可独立开关。展示了车载控制系统的模块化设计 — 控制链输出 = S(C(T(A(H(Surf(Steer(input)))))))，每个模块只修改其关注的维度
4. **双核实时控制分离**：专门解决了 WebSocket relay 阶段的控制抖动问题（stuttering）。Core 0 专用于网络 I/O，Core 1 运行 200Hz 硬实时控制循环，证明了实时控制需要专用处理线程
5. **WebSocket relay → WebRTC RTCDataChannel 的架构演进**：控制延迟从 100-200ms 降至 10-15ms（LAN），展示了直连 P2P 对遥操作延迟的决定性改善 — 中继引入的额外延迟可通过 P2P 直接避免

## 7. 对 AUDEMSP 的参考价值
### [Adopt] 可直接借鉴
- **RTCDataChannel 二进制协议设计**：`seq + cmd + flags + payload` 的紧凑包头格式。AUDEMSP 推荐格式：`timestamp(4B) + seq(2B) + cmd(1B) + flags(1B) + payload(N×2B)`，总计 8+N×2 字节
- **通道分离策略**：四条独立 RTCDataChannel — 控制(unordered)、遥测(unordered)、心跳(unordered)、可靠指令(ordered+reliable)
- **分级超时安全模型**：L1(80ms 保持) → L2(250ms 回中) → L3(500ms 紧急制动) → L4(2000ms 安全停车) — 作为 teleop SDK 安全基线
- **边缘端安全执行**：车端验证所有指令，不信任网络数据。AUDEMSP 的 Vehicle Agent 应实现 `validateCommand()` 方法，独立于操控站端的 `sanitizeCommand()` 方法
- **HMAC Token 短期授权**：简单有效的按需接入控制，适合单次遥控会话
- **硬件编码器直通 WebRTC**：H.264 硬件编码器 → MediaMTX/WHEP → WebRTC 的零拷贝流水线

### [Adapt] 需修改后采用
- **Pi 桥接层抽象**：当前 Pi 仅做 UDP forward。AUDEMSP 需要 `BridgeController` trait 支持多种边缘硬件（Pi/Jetson/ESP32/STM32）
- **ESP32 双核实时分离**：AUDEMSP Vehicle Agent 需要 dedicate 线程：接收线程 + 控制循环线程（Rust `std::thread` + `mpsc::channel`）
- **稳定性系统插件化**：`DrivingAssist` trait，每个模块实现 `process(command: &mut ControlCommand, state: &VehicleState) -> Result<()>`，控制链通过 PluginManager 串联
- **EMA 平滑 + 斜率限制**：作为 `ControlSmoother` 的默认策略，支持配置 α 和 rate_limit
- **7 字节最小包 → 16 字节标准包**：新增 timestamp(u32)、flags(u8)、emergency bit — 参考扩展协议设计

### [Avoid] 已知坑 / 不适用场景
- **JSON 遥测低效**：10Hz JSON 遥测比二进制浪费约 4x 带宽 — AUDEMSP 采用全二进制遥测格式
- **WiFi 仅限局域网**：AUDEMSP 必须支持 4G/5G 蜂窝和卫星通信
- **单视频流**：AUDEMSP 需支持多路同步视频（前向+侧向+后向）
- **无音频**：AUDEMSP 遥操作场景需要双向 Opus 音频
- **无自适应码率**：需集成 WebRTC GCC 和 SVC 可伸缩编码
- **信令绑定 Cloudflare**：需自研通用信令服务（WebSocket + 消息队列）
- **DAC 精度教训**：8-bit PWM (256 级) 不够精细 — AUDEMSP 需 12-bit+ 精度执行器
- **HMAC Token 无自动刷新**：AUDEMSP 需 Token 过期自动续期机制
- **硬件选型耦合**：AUDEMSP 需构建硬件抽象层(HAL) — `SensorHal` / `ActuatorHal` traits
- **无多车并发**：AUDEMSP 信令服务需支持 `room_id` 多会议室/多车隔离

### [Adopt] 可直接借鉴 (补充)

- **WebSocket → RTCDataChannel 的架构演进路径**：为 AUDEMSP 提供了从快速原型到生产部署的明确演进路线。初期使用轻量信令中继快速验证协议设计，稳定后再引入 P2P 直连优化
- **bufferedAmount 背压管理**：`bufferedAmountLowThreshold` + `onbufferedamountlow` 事件驱动模式是 WebRTC RTCDataChannel 发送端的标准实践。AUDEMSP 的 `DataChannelManager` 组件应内置此背压管理逻辑
- **ESP32 电源与信号监测**：WiFi RSSI 监测 + 电池 ADC 分压监测提供车端健康状态基本视图。AUDEMSP Vehicle Agent 内置 `HealthMonitor` 组件定时上报供电电压和无线信号强度
- **MCP4728 12-bit DAC 选型**：证明了 12-bit (4096 级) 精度是执行器控制的最低要求，8-bit PWM (256 级) 在转向和油门控制上分辨率严重不足
- **序列号回绕处理**：`seq.wrapping_sub(last_seq) < 32768` 的模式是 uint16 序列号检测的正确实现，AUDEMSP 所有 RTCDataChannel 数据包头部均应采用

### [Adapt] 需修改后采用 (补充)

- **MCP4728 DAC → ActuatorHal trait**：当前使用 I2C DAC 直接输出模拟信号驱动 RC 遥控器 Trainer Port。AUDEMSP 需要抽象为 `ActuatorHal` trait，支持 DAC/UART PWM/RC PWM/CAN/CAN-FD 多种物理层接口
- **Core 0/1 双核固定分工 → 动态线程池**：FreeRTOS 双核固定分工在 ESP32 上高效。AUDEMSP 的 Rust Vehicle Agent 需要 `tokio` 异步运行时处理网络 I/O + 专用优先级线程处理实时控制循环
- **7 种稳定性控制模块 → PluginRegistry 可配置控制链**：当前每个模块硬编码调用顺序。AUDEMSP 应实现 `PluginRegistry` 机制，允许通过 YAML/JSON 配置文件动态组装控制链
- **WiFi UDP 直连 → NetworkHal trait**：AUDEMSP Vehicle Agent 需要抽象为 `NetworkHal` trait，支持 WiFi/UART 串口/CAN 总线/Ethernet 多种连接方式
- **7 字节最小包 → 16 字节扩展包**：增加 timestamp(u32)、flags(u8, 含 emergency bit)、checksum(u8) 字段，形成 16 字节的标准包格式
- **单视频流 → MultiStreamManager**：AUDEMSP 需要管理 3-6 路同步视频流，支持摄像头动态切换（RTCDataChannel 信令触发）

### [Avoid] 已知坑 / 不适用场景 (补充)

- **Cloudflare Workers 生态绑定**：AUDEMSP 必须自托管信令服务，不绑定任何特定云厂商，支持多云部署和私有化部署
- **单玩家架构**：AUDEMSP 的多用户场景（操作员 + 监督员 + 观察员）需要不同权限层级设计——操作员完整控制权，监督员可覆盖指令，观察员仅可查看
- **无 RTCDataChannel 主动 QoS 监测**：未在 RTCDataChannel 上实施主动单向延迟测量和可用带宽估算。AUDEMSP 需内置 RTCDataChannel 质量探针
- **ESP32 固件无 OTA**：固件更新需 USB 串口烧录。AUDEMSP Vehicle Agent 需内置 OTA 更新机制（支持分片下载 + 校验 + 回滚）
- **单摄像头局限**：仅单一前向摄像头 + 后置 RTSP PiP。AUDEMSP 的车辆遥操作场景需 360° 覆盖——至少前向 + 后向 + 两侧共 4-6 路视频
- **硬件 BOM 过于定制化**：ARRMA 遥控器 Trainer Port 接口是非常规的 RC 遥控器接口。AUDEMSP 应直接通过 CAN/CAN-FD 或工业 PWM 驱动执行器
- **尾延迟抖动未处理**：仅关注控制延迟均值，未对 P95/P99 尾延迟做针对性优化。AUDEMSP 的 jitter buffer 需要遥操作场景专门调优

**总体评分**：★★★☆☆ (3/5)
— 作为 RTCDataChannel 协议设计和边缘安全架构的参考，价值极高。但项目规模和成熟度有限。其 7 字节二进制协议、三级超时安全模型和稳定性控制链架构是最核心的可借鉴资产。对于 AUDEMSP 的生产级 teleop SDK，tether-rally 提供了最佳的最小可行实现参考。
**相关决策**: D117 (紧急停止), D4, D149


## 附录
### A. RTCDataChannel 控制包二进制格式示例
```
CTRL 命令 (0x01) — 7 字节:
  Byte 0-1: seq (uint16 LE)  — 序列号 0-65535
  Byte 2:   cmd (uint8)      — 0x01 = CTRL
  Byte 3-4: steering (int16 LE) — 转向值 (-4095 ~ +4095)
  Byte 5-6: throttle (uint16 LE) — 油门值 (0 ~ 4095)

TELEM 命令 (0x07) — 37 字节:
  Byte 0-1:  seq (uint16 LE)
  Byte 2:    cmd = 0x07
  Byte 3-4:  speed (uint16 LE)         — km/h × 10
  Byte 5-6:  rpm (uint16 LE)
  Byte 7-10: lat (int32 LE)             — 纬度 × 10^7
  Byte 11-14: lon (int32 LE)            — 经度 × 10^7
  Byte 15-16: heading (uint16 LE)       — 0.01°
  Byte 17-18: accel_x (int16 LE)        — m/s² × 100
  Byte 19-20: accel_y (int16 LE)
  Byte 21-22: accel_z (int16 LE)
  Byte 23-24: gyro_x (int16 LE)         — °/s × 10
  Byte 25-26: gyro_y (int16 LE)
  Byte 27-28: gyro_z (int16 LE)
  Byte 29-30: battery (uint16 LE)       — mV
  Byte 31-32: rssi (int16 LE)           — dBm
  Byte 33-34: status_flags (uint16 LE)  — 位掩码
  Byte 35-36: crc16 (uint16 LE)         — CRC-16/XMODEM
```

### B. 与 AUDEMSP 控制协议映射建议
| tether-rally 命令 | AUDEMSP 映射 | 说明 |
|-------------------|---------------|------|
| CTRL (0x01) | ControlCommand (50Hz) | 方向盘 + 油门 + 模式位 |
| PING (0x00) | Heartbeat (2Hz) | 心跳请求 + RTT 测量 |
| PONG (0x02) | HeartbeatResponse | 心跳响应 |
| TELEM (0x07) | VehicleState (10Hz) | GPS + IMU + 车速 + 电池 |
| STATUS (0x04) | SystemState (1Hz) | WiFi 信号 + CPU 温度 + 设备健康 |
| CONFIG (0x05) | ConfigUpdate (按需) | 编码参数 + 安全阈值 + 操控限值 |
| RACE (0x03) | ModeSwitch | 操作模式切换 |
| KICK (0x06) | RestartDevice | 远程重启设备控制器 |
| TURBO (0x08) | DrivingAssistToggle | Turbo 模式开关 |
| TRACTION (0x09) | DrivingAssistToggle | 牵引力控制开关 |

### C. ESP32 分级超时参数建议
| 级别 | 超时 | 动作 | 适用场景 |
|------|------|------|---------|
| L0 | 0-50ms | 正常接收, 直接执行 | 所有正常操作 |
| L1 | 50-100ms | EMA 平滑插值 (保持最后指令方向) | 短暂网络波动 |
| L2 | 100-300ms | Steering 回中 + Throttle 归零 (渐进) | 网络间歇性中断 |
| L3 | 300-2000ms | 全功率制动 + 紧急停止 | 网络严重故障 |
| L4 | >2000ms | 安全停车 + 危险灯 + 远程诊断请求 | 连接完全丢失 |

### D. 稳定性控制链代码结构 (伪代码)
```rust
fn compute_final_output(input: ControlCommand, state: VehicleState) -> ActuatorOutput {
    // 控制链串联执行 — 每个模块只修改其关注的维度
    let mut cmd = input;
    cmd = steering_shaper(cmd, state.speed);        // 1. 转向整形
    cmd = traction_control(cmd, state);              // 2. 牵引力控制
    cmd = stability_control(cmd, state);             // 3. 稳定性控制
    cmd = abs_control(cmd, state);                   // 4. ABS
    cmd = hill_hold(cmd, state);                     // 5. 坡道保持
    cmd = coast_control(cmd, state);                 // 6. 滑行控制
    cmd = surface_adaptation(cmd, state);            // 7. 路面自适应
    cmd = ema_smooth(cmd, prev_cmd, alpha=0.3);     // 8. 平滑滤波
    cmd = slope_limit(cmd, prev_cmd, max_rate);     // 9. 斜率限制
    ActuatorOutput::from(cmd)
}
```