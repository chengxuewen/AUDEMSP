# AUDEMSP 约定与约束

## C1: 架构决策对比格式

**约束**：任何涉及方案选择的架构讨论，必须逐项列出：
- **优缺点**：每个方案的优点和缺点
- **来源/参考**：借鉴的现有系统/开源项目/行业实践
- **影响**：选择该方案对后续开发的影响
- **推荐**：明确推荐及理由

禁止仅列举选项让用户选择而没有上述分析。

**来源**：用户显式要求（2026-07-19 架构讨论）

---

## C2: 三层抽象模型

AUDEMSP 采用三层抽象模型：

| 层级 | 概念 | 职责 |
|------|------|------|
| Layer 1 — 管线层 | Plugin | 媒体管线元素（capture/encode/decode/render） |
| Layer 2 — 服务层 | Component | 有独立生命周期的服务级单元（signaling/relay/admin） |
| Layer 3 — 部署层 | Process | OS 进程，承载 Component 运行 |

- Plugin 和 Component 是不同层次的概念，不应合并为一个 trait
- Component 内部可以持有 Plugin 实例（通过 PipelineEngine）
- Component 通过 ComponentBus 通信，Plugin 通过 MediaPort 通信

**来源**：ROS2（Node vs ComposableNode）、Janus（Plugin vs Transport）、OBS（Module vs Source）

## C3: 术语"三层"消歧

**约束**：AUDEMSP 使用"三层"描述两个不同维度的分层模型，阅读/引用时必须区分：
- **D1 三层部署拓扑架构**：部署维度——控制面（Server） / 数据面（Host+Remote） / SDK 层（napi-binding）
- **D126 三层逻辑抽象模型**：代码维度——Plugin（管线层） / Component（服务层） / Process（部署层）

D1 和 D126 是互补关系，不是替代关系。

**来源**：Doc Audit 审计 #3（2026-07-19）

---

## C4: crate 命名: host/client 对称

**约束**：远程控制场景的 crate 命名遵循 host/client 对称模式：
- **host** = 被控侧 → 推流端 → field/vehicle 侧 → `audemsp-host`
- **client** = 主控侧 → 拉流端 → cockpit/operator 侧 → `audemsp-client`

命名对应关系：
| AUDEMSP | Parsec | RustDesk | Moonlight/Sunshine |
|----------|--------|----------|-------------------|
| `remote-host` | `ParsecHost` | Controlled host (server.rs) | Sunshine (Host) |
| `remote-client` | `ParsecClient` | Controller (client.rs) | Moonlight (Client) |

**来源**：远程桌面/远程操控工业惯例分析 (2026-07-19), D154

---

## C5: GStreamer → WebRTC 数据流边界

**约束**: remote-host 中 GStreamer 和 WebRTC 的接口**仅允许 `&[u8]` 字节传递**：

```
GStreamer pipeline (C, glib)
  capture → encode → appsink
                       ↓
              H.264 byte buffer (&[u8])
                       ↓
audemsp-webrtc (Rust wrapper)
  TrackLocal::write_frame(&[u8])
                       ↓
webrtc-sys (C++, libwebrtc)
  RTP packetizer → ICE → network
```

禁止模式：
- GStreamer buffer 直接传递给 libwebrtc（内存管理边界不兼容）
- 共享内存池（glib allocator ≠ C++ new）
- 跨 FFI 边界传递原始指针

**理由**: GStreamer 和 libwebrtc 使用不同的内存分配器 (glib malloc vs C++ new)。`&[u8]` 接口强制 copy，确保 Rust 所有权语义下的内存安全。

**来源**: D155, OBS Studio 实践

---

## C6: audemsp-webrtc 命名规范

**约束**：audemsp-webrtc crate 遵循以下命名规范：
- **类型名**: 对外 pub 类型全大写 RTC 前缀 (RTCPeerConnection, RTCDataChannel...)，内部类型不加前缀
- **方法名**: 全部 snake_case (create_offer, add_track, on_track)，禁止 camelCase W3C 包装
- **目录名**: backend/ (uniform singular)
- **枚举变体**: PascalCase
- **常量**: SCREAMING_SNAKE_CASE

其他 crate (core, media, server, remote-*) 使用 bare names，无前缀。

**来源**: D166, D167, D168 (2026-07-22)



## C7: OpenCode Instructions 内容策略

**约束**：instructions 数组只放每轮必需加载的文件。参考性文档保留在磁盘，按需读取。

**纳入 instructions 的文件类型**：
- 项目记忆（status, conventions, pitfalls）
- 编码规则（security, coding-style, testing 等）
- 项目语言专属规则（Rust）
- 编辑安全约束（edit-safety, constraints）

**不纳入 instructions 的文件类型**：
- 工具自身参考文档（agent-guide, model-tiers）
- 非项目语言规则（TS/CPP/Go/Web 等对 Rust 项目无关）
- 多语言重复翻译（zh/ 是 common/ 的中文副本，不重复加载）
- 大型归档决策（decisions.md 按需读取，仅在精简后考虑加入）

**原则**：instructions 总量控制在 ~8,500 tokens（< 30K 目标）。每新增一个文件，评估是否可移除一个。

**来源**：D199 (2026-07-28)

---

## C8: 质量门禁 Agent 模型分配

**约束**：Agent 模型分配必须符合层级映射表，高杠杆任务用最强模型。

| Agent | 层级 | 原因 |
|-------|------|------|
| oracle | premium-max | 最复杂架构决策 |
| prometheus | premium-max | 高杠杆计划生成，错误代价最大 |
| momus（计划批评家） | premium | 对抗性审查需要深度推理，temp 0.3 |

执行型 agent（explore/librarian/metis/sisyphus-junior）使用 fast 层。

**来源**：D200 (2026-07-28), D203 (2026-07-29), AUDEBase 配置实践

---

## C9: 经验教训自动沉淀

**约束**：开发过程中发现的问题、教训、经验必须在当轮会话中自动更新到相应记忆文档。

**触发条件**：识别到以下情况时，主动更新：

| 情况 | → 更新文件 | 示例 |
|------|-----------|------|
| 发现 bug / 踩坑 | `pitfalls.md` | 症状 → 根因 → 解法 |
| 用户纠正 AI 行为 | `conventions.md` | 新约束 / 新规范 |
| 架构/配置决策 | `decisions.md` | 决策编号 → 原因 → 影响 |
| 项目状态变化 | `status.md` | Phase 完成、测试数变化 |
| 编码模式/反模式 | `rules/common/coding-style.md` | 禁止模式 / 推荐模式 |
| 安全相关教训 | `rules/common/security.md` | 新增安全规则 |
| 编辑工具使用教训 | `rules/common/edit-safety.md` | 新增编辑约束 |

**格式要求**：
- `pitfalls.md`：症状 + 根因 + 解法，三要素缺一不可
- `conventions.md`：C{n} 编号，「约束」/「原则」开头，注明来源
- `decisions.md`：D{n} 编号，决策 + 日期 + 原因 + 影响
- `status.md`：更新生成日期、commit 数、Phase 状态

**原则**：不等用户要求。识别到可沉淀的经验即主动更新。宁可多记，不可遗漏。

**来源**：用户显式要求（2026-07-28）

---

## C10: OMO 插件版本监控

**约束**：每次 OMO 插件版本升级前，应检查 changelog 和与当前配置的兼容性。

**检查步骤**：
1. `npm view oh-my-opencode version` — 对比本地版本
2. 检查 breaking changes（主要版本升级）vs patch（直接升级）
3. 检查 oh-my-openagent.jsonc schema 是否变更
4. 重启 opencode 后验证 agent/技能加载正常

**当前状态**：v4.19.2（npm 最新 4.19.3，待升级）

**来源**：ecosystem-scan 审计（2026-07-29）

---

## C11: 调试前必须查阅官方资料

**约束**：遇到问题、故障、不确定的技术细节时，优先调研官方仓库源码、官方文档、社区资料（GitHub issues/discussions、Stack Overflow），禁止凭直觉盲目尝试。

**优先级**：
1. **官方文档** — mediasoup.org, webrtc-rs docs, Rust std docs
2. **官方仓库源码** — GitHub 集成测试（最权威的 API 用法示例）
3. **官方示例** — mediasoup-demo, 官方 examples 目录
4. **社区资料** — GitHub issues/discussions（同问题+解法）
5. **最后**：凭经验推断（标记为假设，需验证）

**触发条件**：
- 遇到编译错误/运行时错误/行为异常
- 不确定 API 的参数格式、字段类型、序列化方式
- 库版本升级后行为变化
- ICE/DTLS/RTP 等协议层问题
- mediasoup Worker、mediasoup-client、webrtc-rs 等第三方库问题

**禁止模式**：
- 连续 2 次尝试同一修复 → 说明方向错误，必须停下来查资料
- 凭记忆构造 API 参数格式 → 必须对照官方测试或文档
- SDP 字符串手动拼接 → 必须先查 RFC 格式或官方生成示例
- 假设 serde 字段映射 → 必须查 `#[serde(rename_all)]` 注解

**反例（本次教训）**：Host→SFU 方案中 `connect_transport` 消息从未发送、`add_track` 时序错误、Router H264 参数缺失，均因未先对照 mediasoup-demo 的完整信令流程。

**来源**：用户显式要求（2026-07-31 团队评审后）

---

## C12: 仅通过 audemsp-webrtc 使用 WebRTC

**约束**：所有 client 端 crate（host/client）禁止直接依赖 webrtc-rs（`webrtc = "0.12"`），必须通过 `audemsp-webrtc` 抽象层使用 WebRTC 能力。P2P 和 SFU 路径统一经此抽象层。Server 端 SFU 路径不依赖 audemsp-webrtc（WebRTC 来自 mediasoup），webrtc feature 为 P2P relay 保留。

**后端策略**：
- 默认/当前后端 = `backend-webrtc-sys`（libwebrtc C++ via webrtc-sys FFI），不依赖 audemsp-codec
- `backend-webrtc-rs` 为备选后端（Phase 2+），需额外依赖 audemsp-codec

**Reason**：
- `audemsp-webrtc` 已封装 W3C API（RTCPeerConnection + TrackSender + DataChannel）
- 三后端抽象（stub/webrtc-rs/webrtc-sys）由 audemsp-webrtc 统一控制
- 直接依赖绕过抽象层破坏后端切换能力和可测试性

**禁止**：
```toml
# 任何 client crate 的 Cargo.toml — 禁止
webrtc = "0.12"
```

**允许**：
```toml
# Cargo.toml — 正确
audemsp-webrtc = { path = "../audemsp-webrtc", features = ["backend-webrtc-sys"] }
```

**来源**：用户显式要求（2026-07-31 Host SFU 评审后）

---

## C13: Server 统一 Docker 构建

**约束**：audemsp-server 统一通过 Docker 编译（mediasoup C++ Worker 需要 Linux x86_64 + meson/ninja）。原生 `cargo check --workspace` 排除 server crate。

**pixi 任务映射**：
| 任务 | 命令 | 说明 |
|------|------|------|
| `check` | `cargo check --workspace --exclude audemsp-server` | 原生 |
| `check-server` | `scripts/docker-cargo.sh check -p audemsp-server --features sfu-mediasoup` | Docker |
| `build-server` | `scripts/docker-cargo.sh build -p audemsp-server --features sfu-mediasoup` | Docker |
| `check-native` | `scripts/cargo-sfu.sh check --workspace` | 原生备选 |

**原因**：
- mediasoup-sys 的 meson wrap 依赖 wrapdb.mesonbuild.com（不可达时无法下载 flatbuffers patch）
- Docker 镜像预装所有依赖，构建环境一致
- macOS/Windows 开发者统一使用 Docker

**来源**：用户显式要求（2026-07-31）、OpenVidu pre-built binary 参考

---

## C14: 子代理产物必须验证（编排者铁律）

**约束**：子代理返回的完成声明不可信。编排者必须验证实际产物后才标记任务完成（PIT-34）。

**验证清单**：
- 声称创建的文件 → `cat`/`ls` 确认存在 + 内容完整
- 声称修改的配置 → `grep` 关键字段
- 声称可运行的命令 → 实际执行
- 声称通过的测试 → 重新运行

**失败处理**：验证失败 → `task(task_id="ses_...", prompt="fix: <具体问题>")` resume 修复，不自行编辑。

**验证**：`ls <声称的文件> && grep <关键字段> <修改的文件>`。

**来源**：PIT-34 (2026-07-31 docker-compose.dev.yml 声称创建实际缺失)
