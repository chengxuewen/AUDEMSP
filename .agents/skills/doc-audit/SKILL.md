---
name: doc-audit
description: "MediaServo 项目文档与架构审计。并行检查架构文档、设计文档、决策记录之间的自洽性、完整性、缺口和优化机会。以交互式问答与用户逐项确认每项发现，列出详情、方案与优劣、来源、影响、推荐。支持团队模式(大型审计)和背景代理模式(轻量检查)。"
---

# 文档架构审计 (Document & Architecture Audit)

对 MediaServo 项目文档体系进行全面审计，覆盖文档一致性、决策验证、agent 基础设施、参考对标。

**哲学**: 审计不是找茬，是清债务。文档债务和代码债务一样危险。

---

## 入口：审计类型

### `/doc-audit`（无参数）
弹出审计类型菜单：

```
[1] 完整审计 — 全部 5 维度（默认）
[2] 决策验证 — decisions.md 落实情况
[3] 文档一致性 — arch ↔ modules ↔ 子文档交叉检查
[4] Agent 体系 — .agents/ 规则/技能/记忆自洽性
[5] 缺口优化 — 安全/可靠性/运维扫描
[6] 阶段审计 — Phase 进度 vs 现状
```

### `/doc-audit full`
直接启动全量审计，跳过菜单。等效于 `[1] 完整审计`。

### `/doc-audit quick-fix`
仅检查 LOW/MEDIUM 级别问题并自动修复，不进入交互审核。

---

## 审计维度

### 1. 决策验证
核查 `decisions.md` D1-Dx 在架构文档、实现代码、状态记录中的落实。

**核心问题**：
- 决策结论是否正确反映在 architecture.md 和模块文档中？
- 是否存在「决策说了A，现状是B」的矛盾？
- 决策中的陈旧引用是否需要更新？
- **决策新鲜度检查**：该决策是否被后续决策替代？技术栈版本是否过期？

### 2. 文档一致性
核查 architecture.md ↔ modules/ ↔ README ↔ AGENTS.md 交叉一致性。

**核心问题**：
- 同一概念描述是否一致？（crate 数量、Phase 状态、技术栈版本）
- 子文档是否与主文档重复？
- Phase 术语是否歧义？
- Crate 命名是否与 C4 约定一致？

### 3. Agent 体系审计（新增）
核查 .agents/ 下规则、技能、记忆之间的自洽性。

**核心问题**：
- conventions.md 编号连续？C{n} 交叉引用有效？
- decisions.md 编号连续？D{n} 顺序正确？
- pitfalls.md 编号连续？PIT-{n} 交叉引用完整？
- 技能目录表（AGENTS.md）与实际技能清单一致？
- 任务路由表（context-engineering）与实际技能清单一致？
- opencode.json instructions 引用全部存在？
- 各技能 SKILL.md frontmatter 完整？

### 4. 缺口优化
扫描缺失的关键文档/设计章节。

**核心问题**：
- 安全架构：PSK 轮换、JWT 过期策略、mediasoup 传输安全文档化？
- 运维/可观测性：健康检查、指标导出、日志聚合？
- 错误模型：SFU 连接失败、编码降级、传输断连处理？
- 硬件基线：最低 CPU/RAM、Docker 资源配置？
- 升级策略：热更新、crate 版本迁移、配置迁移？

### 5. 阶段审计
核查 Phase 进度 vs 文档声明 vs 代码实现。

**核心问题**：
- status.md 的 Phase 状态与 git log / test count 一致？
- decisions.md 的决策 Phase 标签准确？
- 代码实现是否匹配文档声明的完成状态？

---

## 审计模式

### A. 团队模式（推荐 — 大型审计）
3+ 份大型文档 → `team_create` 4-6 个成员并行。

```
team_create(inline_spec={
  name: "doc-audit",
  members: [
    { name: "decision-validator", category: "deep", prompt: "<决策验证核心问题>" },
    { name: "consistency-checker", category: "deep", prompt: "<文档一致性核心问题>" },
    { name: "agent-auditor", category: "deep", prompt: "<Agent 体系核心问题>" },
    { name: "gap-optimizer", category: "deep", prompt: "<缺口优化核心问题>" }
  ]
})
```

**Conductor 规范**（调度者行为）：
- 启动后立即向用户报告：「启动 N 路并行审计，预计 3-5 分钟」
- 等待全部完成前只做「非重叠工作」
- 全部完成后：**去重合并**（同问题被 2+ 维度发现 → 合并为 1 项）
- 按严重性排序：CRITICAL → HIGH → MEDIUM → LOW
- 超时处理：任一路超过 10 分钟未产出 → 标注为「超时」
- 冲突处理：维度 A 说 X、维度 B 说 Y → 标记为人类审核

### B. 背景代理模式（轻量审计）
少量文档 → `task(category="deep", run_in_background=true)` × N 并行。

### C. 单线程模式
极小范围 → 直接用 Read/Grep 检查，不启动子代理。

---

## 交互审核：发现项格式

**逐项审核**，每项使用 `question()` 工具展示。

```markdown
## 🔴/🟠/🟡/🔵 [编号]: [标题]

### 详情
| 来源1 | 位置 | 内容 |
|--------|------|------|
| 文档A | 行X | ... |
| 文档B | 行Y | ... |

### 可选方案
| 方案 | 优势 | 劣势 |
|------|------|------|
| A. [方案名] | ... | ... |
| B. [方案名] | ... | ... |

### 推荐
[方案X]。[理由]
```

选项：采纳推荐方案 / 选择其他方案 / 不处理 / 自定义
进度：`[第N/共M项]`

---

## 工作流

### Phase 1: 启动
1. 确认审计范围和类型
2. 选择模式（团队/背景/单线程）
3. 报告：「启动 N 路并行审计」

### Phase 2: 合并
1. 去重：同问题多来源 → 合并标注
2. 排序：CRITICAL → HIGH → MEDIUM → LOW
3. 交叉印证：2+ 维度同意的提升优先级

### Phase 3: 交互审核
逐项审核，question() 交互确认。

### Phase 4: 修复
1. 创建 todo list
2. 按依赖顺序：先改决策 → 再改文档 → 最后改状态
3. 每次编辑后验证

### Phase 5: 报告
```
审计完成 — [日期]
审计类型: [全量/决策/一致性/Agent体系/缺口/阶段]
发现总数: N | 已修复: M | 不处理: K
下次建议: [问题密集区域]
```

---

## 严重性标准

| 严重性 | 触发条件 | 阻断？ |
|--------|---------|:---:|
| 🔴 CRITICAL | 文档矛盾导致实现错误 / 决策被推翻 / 核心 API 缺失 | ✅ |
| 🟠 HIGH | 陈旧引用 / Phase 歧义 / 重复文档 / 编号空洞 | ⚠️ |
| 🟡 MEDIUM | 表述差异 / 示例冲突 / 缺失不阻断当前阶段 | ❌ |
| 🔵 LOW | 格式不一致 / 引用缺失 / 待确认标记 | ❌ |

## 审计建议频率
- 每次 D# 决策变更后：`/doc-audit decisions`
- Phase 转换前：`/doc-audit full`
- 每周开发期间：`/doc-audit full`
- 每次文档大改后：`/doc-audit consistency`

---

## 社区参考

| 先例 | 借鉴的模式 |
|------|-----------|
| [large-codebase-audit](https://github.com/MJWNA/large-codebase-audit-skill) | 9-surface AI 层审计、对齐 Anthropic 最佳实践 |
| [claude-ecosystem](https://github.com/melodic-software/claude-code-plugins) | 元技能架构、16 审计 agent |
| [agent-self-audit](https://github.com/Xxt-XN/agent-self-audit) | 双层设计、13 项检查、自动升级 |

---

## 与 ecosystem-scan 的分工

| 维度 | ecosystem-scan | doc-audit |
|------|:---:|:---:|
| Agent 基础设施（技能/规则/MCP） | ✅ 专长 | ✅ 第 3 维度 |
| 外部社区对比 | ✅ 核心能力 | ❌ |
| 文档内部一致性 | ❌ | ✅ 专长 |
| 决策验证 | ❌ | ✅ 专长 |
| 安全审计门禁 | ✅ Full 模式 | ❌ |
