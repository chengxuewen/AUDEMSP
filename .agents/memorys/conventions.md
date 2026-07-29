# OMSPBase 约定与约束

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

OMSPBase 采用三层抽象模型：

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

**约束**：OMSPBase 使用"三层"描述两个不同维度的分层模型，阅读/引用时必须区分：
- **D1 三层部署拓扑架构**：部署维度——控制面（Server） / 数据面（Host+Remote） / SDK 层（napi-binding）
- **D126 三层逻辑抽象模型**：代码维度——Plugin（管线层） / Component（服务层） / Process（部署层）

D1 和 D126 是互补关系，不是替代关系。

**来源**：Doc Audit 审计 #3（2026-07-19）

---

## C4: crate 命名: host/client 对称

**约束**：远程控制场景的 crate 命名遵循 host/client 对称模式：
- **host** = 被控侧 → 推流端 → field/vehicle 侧 → `omspbase-host`
- **client** = 主控侧 → 拉流端 → cockpit/operator 侧 → `omspbase-client`

命名对应关系：
| OMSPBase | Parsec | RustDesk | Moonlight/Sunshine |
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
omspbase-webrtc (Rust wrapper)
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

## C6: omspbase-webrtc 命名规范

**约束**：omspbase-webrtc crate 遵循以下命名规范：
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
