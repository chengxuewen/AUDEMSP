# Host 多进程架构设计 — capturer/stitch/streamer/recorder/controller/emergency/monitor + OxMgr

**日期**: 2026-08-18
**状态**: 设计讨论确认（brainstorming 交互式完成）
**前置**: D102（host IPC tokio::mpsc Phase 1 → iceoryx2 SHM Phase 2）、D222-D243（link 四 SDK）、D235（去中心化 Registry，跨进程发现留 Phase 2）、deck closed_loop e2e（capture → FrameBus → recorder 实证）、host control.rs（P2P DataChannel 控制雏形）

## 1. 背景与动机

**场景**：车端（Jetson）8-9 个相机**全部实时推流**，舱端实时拉流遥控；车端环视拼接（AVM）视频推流；x86-Windows 跑 CARLA 仿真源；舱端 HMI 画面推流。当前 host 为单进程（mediaservo-host main.rs 770 行，采集+编码+推流+信令+配置合一）。

**核心动机**：**崩溃隔离 + 独立重启**——采集驱动崩溃不拖垮推流；一路媒体进程崩溃不影响其他路与控制；单个模块可热重启。线程模型无法提供真隔离（panic 传染/内存踩踏），进程模型是唯一选择。

**部署矩阵**（进程管理器必须跨平台）：
| 形态 | 平台 | 推流负载 |
|---|---|---|
| 车端 Jetson | linux-aarch64 | 8-9 相机 + 1 环视拼接 ≈ 10 路实时 |
| 边缘盒子 | linux-amd64 | 1-4 路 |
| CARLA 仿真机 | x86-Windows | 仿真相机 1-4 路 |
| 舱端 HMI | 待定 | 1 路 HMI 画面 |

## 2. 进程清单与链路

```
OxMgr（进程总管：重启策略/健康检查/日志轮转/CPU-RAM 指标/file-watch 配置热生效）
├── host-capturer × N      每相机一进程 → FrameBus 发布 I420（ts_mono/ts_epoch 对齐）
├── host-stitch × 1        环视拼接：订阅 N 路 → 缓冲对齐(ts) → GPU 拼接 → 发布 1 路全景
├── host-streamer × N      每路一进程：订阅 RAW → 编码 → WebRTC 推流（协商 WS → 本地总线）
├── host-recorder × 1      聚合：订阅全部路 → 各自编码落盘（磁盘故障不影响推流）
├── host-controller × 1    控制通道：一条 PC（只开 DC 不开 track）+ 多 DataChannel label
├── host-emergency × 1     急停：独立进程 + 独立 PC + 本地兜底（最高可靠性通道）
└── host-agent × 1         信令网关（单 WS 聚合）+ 拓扑/数据流/信令状态监控
                           + 云端配置镜像 + 远程上报 Server
```

**link 角色 = 共享库（非进程）**：FrameBus（iceoryx2 SHM）+ 去中心化 Registry（D235：attach 即注册，iceoryx2 service discovery 枚举）+ 静态 ACL + 能力令牌，内嵌各进程。

**媒体链路**：`host-capturer →(FrameBus I420)→ host-streamer / host-recorder / host-stitch`；`host-stitch →(FrameBus I420 全景)→ host-streamer(stitch)`
**控制链路**：`舱端/Server →(WebRTC DataChannel)→ host-controller`（P2P 直连；SFU 经 Server data 域）
**信令链路**：`各进程 →(WS→127.0.0.1:PORT)→ host-agent（信令网关）→(单 WS)→ Server`——一个 host 在 Server 侧 = 一个 peer 会话

## 3. 关键决策记录

### D-H1: 进程管理器 = OxMgr（Rust 轻量，PM2 跨平台替代）
- **来源**: github.com/Vladimir-Urik/OxMgr（Rust，226 stars，2026-08-17 活跃）；PM2 现状；Jetson 约束 C22/C23
- **优劣**（vs pm2/systemd/自研）:
  - pm2：跨平台但需 Node 运行时（车端增重 ~50MB+），非车端生态
  - systemd：Linux 最强但 **Windows CARLA 机无 systemd**（双轨运维），Jetson 裁剪可能无
  - 自研 Rust supervisor：生态一致但重造轮子（重启/日志/守护化 3-6 月），违背 C18/ponytail
  - **OxMgr**: 三平台全命中（Rust 交叉编译）+ 轻量单二进制 + 重启策略/健康检查/日志轮转/CPU-RAM 指标开箱 + oxfile.toml 配置即代码 + **file-watch 配置热生效（云端配置的关键机制）** + PM2 ecosystem 兼容
- **风险对冲**: OxMgr 只负责生命周期；进程拓扑/数据流/信令监控由自研 host-monitor 承担 → 未来换 pm2/systemd 时 monitor 接口不变
- **影响**: oxfile.toml 声明进程组（capturer/stitch/streamer/recorder/controller/emergency/monitor），restart_policy=always/on-failure

### D-H2: 帧总线 = RAW I420（非编码帧）
- **理由**: deck closed_loop 已验证同款链路（capture → FrameBus I420 → recorder 落盘）；streamer/recorder 各自编码（双编码器开销在 Jetson 硬编 NVENC/MMAPI 下可接受）；录制画质与推流码率解耦；1080p30 ≈ 93MB/s/路 SHM 零拷贝无压力（iceoryx2 实证 684MB/s 单 service，10 路独立 topic service）
- **否决**: H264 总线（录制画质=推流画质，低码率时本地录像同步受损）；双总线（YAGNI，双码率需求出现再上）

### D-H3: 控制通道 = WebRTC DataChannel（多 label）
- **理由**: 延迟（SCTP over UDP vs WS/TCP）+ 复用 WebRTC 基础设施 + 与推流协商解耦（controller 独立 PC）+ 可靠/顺序每通道可配（急停 reliable / 云台 partial-reliable）
- **现状**: mediaservo-webrtc 已有 create_data_channel/on_data_channel/RTCDataChannelRx（webrtc-sys 后端）；host control.rs 已有 DC 使用雏形（P2P E2E 验证）；**Server SFU data 域（DataProducer/DataConsumer）未实现——SFU 模式控制需 Server 后续补齐**
- **进程边界**: DC label = 通道边界（chassis/gimbal/light）；**emergency 独立进程 + 独立 PC**（PC 崩不影响急停 + 本地兜底直连执行器）

### D-H4: 监控拓扑 = 声明式期望 + 发现式实际
- **期望态**: 云端下发配置的本地镜像（config push → monitor 存储）或本地 oxfile/拓扑声明
- **实际态**: oxmgr list（进程存活）+ link Registry/iceoryx2 service discovery（进程间连接/发布者枚举）+ FrameBus 统计（数据流）+ streamer 信令状态
- **对比告警**: 期望有 N 路 capturer → 实际只有 N-1 → 告警"capturer-cam3 丢失"

## 4. host-monitor 监控维度

| 维度 | 数据源 | 产出 |
|---|---|---|
| 节点关系（拓扑）| oxmgr list + link Registry 枚举 | 期望 vs 实际对比图、缺失节点告警 |
| 数据流 | FrameBus 统计（发布/订阅、帧率、带宽/路）| 流健康度：帧率达标、停滞检测、带宽曲线 |
| 状态 | 每进程健康（oxmgr CPU/RAM/重启次数）+ 应用级心跳 | 状态面板、异常重启计数、OOM 检测 |
| 信令状态 | streamer/controller/monitor 的 WS/PC 连接状态（connected/disconnected/ICE 状态）| 信令连接矩阵、断连告警 |

**产出形态**：本地 Web/CLI（车端调试）+ 远程上报 Server（云端 dashboard 复用 admin 9800 扩展）。

## 5. 云端远程配置闭环

```
云端 Server ──(信令 WS 扩展：ConfigPush 消息 + PSK/JWT 认证 + 审计)──▶ host-monitor
host-monitor ──(写 oxfile/进程配置)──▶ OxMgr file-watch 检测 → 重启对应进程
host-monitor ──(期望态镜像)──▶ 拓扑验证/告警闭环
```

- **通道**: 信令 WS 扩展（复用连接 + 现有认证；配置下发/急停指令同一扩展面）
- **安全**: 远程配置 = 安全敏感面（远程改采集/推流参数）→ 现有 PSK/JWT 认证 + 审计日志（C15/C16 纪律）
- **生效**: 进程参数热生效（OxMgr file-watch debounce restart）；链路变更（增删路）动态启停进程

### D-H5: 进程命名规范 — host- 前缀进程族
- 所有 host 单元进程以 `host-` 前缀命名：host-capturer/host-stitch/host-streamer/host-recorder/host-controller/host-emergency/host-agent
- 多实例区分：实例后缀（host-capturer-cam0、host-streamer-cam0）；OxMgr 命名空间（namespace: host）组织
- **理由**: 进程族统一标识"车端 host 单元"；与 Server 侧进程（mediaservo-server）命名空间区分；agent 命名无占用（gateway 撞 GatewayComponent、signaling 撞模块名）

### D-H6: 单 WS 信令总线（WS 代理模式）— Server 零改动
- **形态**: 各进程 WS 连本地 127.0.0.1:PORT → host-agent 做 WS 网关（本地 accept + 远端单 WS + 双向转发 + 会话区分）→ Server 只见一个 peer = 一个车
- **理由**: 多 peer 语义缺"车"聚合层（踢下线/凭证/拉流路由/admin 视图都按设备）；Server 零改动（多路 produce = 同 peer 多 transport，mediasoup 原生支持；P2P relay 仅 SDP/ICE 交换）
- **影响**: 各进程代码零改动（信令地址一个配置项）；host-agent 兼信令网关（职责混合可接受——信令状态监控天然在手）；controller 的 PC 协商借道总线（不持独立信令）
- **演进**: 真多车时升级 Server 设备聚合（方向 2），agent 网关平滑过渡

## 6. 已知缺口与后续工作

1. **Server SFU data 域**：mediasoup DataProducer/DataConsumer（SFU 模式 DC 控制的前置）
2. **stitch 实现来源**：自研 CUDA/VPI（Jetson GPU 加速）vs 第三方黑盒——待定；输入走 FrameBus 9 路 + ts 对齐（FrameMeta ts_mono/ts_epoch 设计内）
3. **帧同步策略**：stitch 缓冲对齐窗口（多路帧到达时刻差异）
4. **采集 zero-copy**：MIPI/CSI 采集进 SHM 的零拷贝优化（immediate transfer）
5. **emergency 本地兜底**：执行器直连形态（CAN/GPIO/串口）与控制器冗余
6. **Server 侧**：10 路/车 × N 车 的会话规模与 admin 视图（云端多车监控）

## 7. 测试策略

- **多进程 e2e**：复用 deck closed_loop 模式（capture 发布 → FrameBus → recorder 落盘）扩展到真进程（spawn 子进程对）
- **故障注入**：杀任意进程 → 验证 OxMgr 拉起 + 其他进程不受影响 + monitor 告警
- **链路回归**：host 现有 9/9 E2E + e2e_sfu 4/4 + codec_prefs 6/6 不回归
- **Windows 验证**：CARLA 机（x86-Windows）capturer 采集 + 推流最小闭环

## 8. 范围边界

- **本次范围**: host 多进程形态设计 + 进程清单 + monitor 子系统 + OxMgr 接入 + 云端配置通道设计
- **不在本次**: client（舱端拉流，骨架阶段）；Server SFU data 域实现；stitch 算法实现；client 消费侧
