---
name: ecosystem-scan
description: "审计 .agents/ 体系并扫描社区生态寻找可引入的技能/规则/MCP。双层（Quick/Full）+ 质量评分 + 安全门禁。Use when the user asks to 'optimize agents', 'scan ecosystem', 'find new skills', 'audit .agents/', or '/ecosystem-scan'."
---

# ecosystem-scan — 项目 Agent 体系审计 + 生态扫描

> 双层扫描（Quick 日常 / Full 深度），质量评分，安全审计门禁。
> 参考先例：[autoskills](https://github.com/B143KC47/autoskills)、[agent-skill-discovery](https://github.com/ericgandrade/claude-superskills)、[skill-update-team](https://github.com/franktsai2008-eng/skill-update-team)、[agent-self-audit](https://github.com/Xxt-XN/agent-self-audit)。

## 触发条件

- 用户说"优化 agents"/"审计 .agents"/"扫描生态"/"找新技能"
- `/ecosystem-scan` 命令（默认 Quick）
- `/ecosystem-scan full` — 深度扫描（首次或 >5 次 Quick 后自动升级）

## 双层模式

| 特性 | Quick | Full |
|------|:-----:|:----:|
| 本地审计 | 6 项快速检查 | 所有 13 项 |
| 外部扫描 | 已知 3-5 个高星仓库 | 全面 websearch + GitHub 搜索 |
| 评分 | 3 维快速 | 5 维完整评分 |
| 安全审计 | 无 | 6 项安全检查 |
| 用时 | < 30s | 3-5min |
| 触发 | `/ecosystem-scan` | `/ecosystem-scan full` 或 >=5 次 Quick |

---

## Phase 1: Quick Scan（默认）

### 1A: 本地快速审计（8 项）

```
1. opencode.json instructions 文件是否都存在
2. 每个 SKILL.md 有 name + description frontmatter
3. conventions/decisions/pitfalls 编号连续
4. memorys 交叉引用完整性
5. 重复内容检测（grep 关键段落）
6. 孤儿文件检测（存在但未被引用）
7. 技能目录表同步：AGENTS.md SKILL DIRECTORY 覆盖所有技能
8. 任务路由表同步：context-engineering 路由表匹配技能清单
```

### 1B: 外部快速扫描

搜索 3-5 个已知高星仓库：
- VoltAgent/awesome-agent-skills（索引）
- addyosmani/agent-skills（工程化技能）
- ECC/affaan-m everything-claude-code（全栈配置）

用 `webfetch` 获取 README，3 维快速评分：技术栈匹配 / 项目已有 / 质量。

### 1C: Quick 输出

```
🟢 无问题 — 3 项通过，无需深度扫描
🟡 发现 N 项值得关注 — 建议 /ecosystem-scan full
```

---

## Phase 2: Full Scan

### 2A: 本地深度审计（15 项）

| # | 检查项 | Quick | Full |
|---|--------|:---:|:---:|
| 1 | 配置文件健康度 | 计数 | 5 维评分 + 压缩/拆分建议 |
| 2 | 技能清单 | 计数 | 重复检测 + 社区对比 |
| 3 | 安全 | 明文密钥 | 权限审计 |
| 4 | 记忆系统 | 计数 | 过期度 + 结构 |
| 5 | 规则质量 | — | 可执行命令检查 |
| 6 | 更新可用 | — | changelog + 优先级 |
| 7 | 技能利用率 | — | 使用率 vs 安装数 |
| 8 | 孤儿恢复 | — | 恢复候选项 |
| 9 | Agent 审计质量 | — | 合规抽查 |
| 10 | 环境 | 4 原子检查 | 工具链 + 包 + 网络 |
| 11 | 交叉引用 | — | 死链检测 |
| 12 | 重复规则 | — | 语义去重 |
| 13 | 社区趋势 | — | 市场扫描 24h 缓存 |
| 14 | 技能目录表 | 计数 | AGENTS.md SKILL DIRECTORY vs 实际技能一致性 |
| 15 | 任务路由表 | — | context-engineering 路由表 vs 实际技能一致性 |

### 2B: 外部深度扫描

#### 搜索策略（4 路并行）

1. **已知仓库**：anthropics/skills、addyosmani/agent-skills、VoltAgent/awesome-agent-skills、ECC/affaan-m、superpowers
2. **高星发现**：`websearch: "github opencode skills popular stars"`
3. **技术栈特化**：`websearch: "best AI agent skills for <tech-stack> github"`
4. **MCP 搜索**：GitHub MCP server 仓库（Rust、cargo、git、docker）

#### 评分体系（5 维，满分 10）

| 维度 | 权重 | 评分标准 |
|------|:---:|------|
| **Fit**（适配度） | 0.30 | 技术栈匹配度（Rust/TS/DevOps/Web） |
| **Trust**（可信度） | 0.20 | 仓库 star 数 + 所有者信誉 + LICENSE |
| **Track-record**（实绩） | 0.20 | 实际使用验证（非自动生成） |
| **Freshness**（新鲜度） | 0.15 | 最后更新时间（>180 天未更新扣分） |
| **Specificity**（专精度） | 0.15 | 内容专精 v.s. 泛泛而谈 |

**Sanity Gate**：任何 Trust < 2 或内容不可读 → 直接丢弃。

#### 安全审计门禁（Full 模式，安装前）

| 检查项 | 严重性 | 内容 |
|--------|:---:|------|
| repo-trust | **block** | Stars、所有者信誉、LICENSE、未归档 |
| code-review | **block** | 无 `curl | sh`、无 `eval()`、无未授权文件访问 |
| permissions-scope | **block** | 无文件系统全局访问、无 sudo |
| dependency-audit | warn | 依赖审计、CVE 检查 |
| data-exfil | **block** | 无未授权数据传输 |
| freshness | warn | 最近提交 < 180 天 |

- 任何 **block** → 拒绝
- 任何 **warn** → 警告 + 需确认
- 全部通过 → SAFE

---

## Phase 3: 综合推荐

### 3A: 交叉对比

- 多个来源推荐相同内容 → +2 分
- 技术栈不匹配但模式可移植 → 标注"改写适配"

### 3B: 输出格式

```markdown
## Ecosystem Scan Report — {date}

### 🟢 Phase 1: Quick (N 项通过)
### 🔴 Phase 2: Full Local Audit (M 项发现问题)
### 🟡 Phase 2: External Scan (K 个推荐)
### ❌ Rejected (L 个)

#### P1: 强烈推荐
| # | 内容 | 来源 | 评分 | 安全 | 工作量 |
|---|------|------|:---:|:---:|:---:|
| 1 | ... | repo | 9/10 | SAFE | 低 |

#### P2: 值得考虑
| # | 内容 | 来源 | 评分 | 安全 | 工作量 |
|---|------|------|:---:|:---:|:---:|

#### 已拒绝
| 内容 | 原因 | 安全 |
|------|------|:---:|
```

### 3C: 持久化

Quick 扫描结果记录到 `.agents/memorys/`：
- 每次扫描的发现、评分、决策
- 下次扫描优先检查上次的问题是否修复
- >=5 次 Quick → 自动建议 Full

---

## 团队模式配置

```
成员 1: structure-analyst (deep) — Phase 2A 结构审计（2-6 项并行）
成员 2: content-auditor (deep)   — Phase 2A 内容审计（7-13 项并行）
成员 3: ecosystem-scanner (deep) — Phase 2B 外部 4 路并行
成员 4: security-auditor (deep)  — Phase 2B 安全审计（Full 模式）
成员 5: synthesizer (deep)       — Phase 3 综合（等待 1-4）
```

---

## 社区参考

本技能的综合设计参考了以下社区先例：

| 先例 | 借鉴的模式 |
|------|-----------|
| [autoskills](https://github.com/B143KC47/autoskills) | 5 维评分体系、Sanity Gate、持久化记忆 |
| [agent-skill-discovery](https://github.com/ericgandrade/claude-superskills) | 双层作用域（已安装 / 仓库）、平台检测 |
| [skill-update-team](https://github.com/franktsai2008-eng/skill-update-team) | 安全审计门禁（6 项检查）、评分权重分配 |
| [agent-self-audit](https://github.com/Xxt-XN/agent-self-audit) | Quick/Full 双层设计、13 项检查、自动升级 |
| [claude-ecosystem](https://github.com/melodic-software/claude-code-plugins) | 元技能架构、16 审计 agent、组件健康度 |
| [skill-optimizer](https://github.com/hqhq1025/skill-optimizer) | 技能生命周期（miner → personalizer → generalizer） |
| [large-codebase-audit](https://github.com/MJWNA/large-codebase-audit-skill) | 9-surface AI 层审计、对齐 Anthropic 最佳实践 |

---

## 适配任意项目

替换变量：
```
- 技术栈: Rust + TypeScript + Docker + React → {你的技术栈}
- Agent 平台: OpenCode → {你的平台}
- 规则路径: .agents/ → {你的规则路径}
- 包管理器: pixi → {你的包管理器}
- 记忆路径: .agents/memorys/ → {你的记忆路径}
```
