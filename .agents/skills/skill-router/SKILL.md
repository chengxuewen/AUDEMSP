---
name: skill-router
description: "分析用户意图，输出推荐技能列表。当用户说'怎么做'、'用什么技能'、意图模糊时自动激活。Use when the user asks 'what skill should I use', 'how to approach this', or when intent is ambiguous."
---

# skill-router — 技能路由分析

> 用户意图模糊或多技能竞争时，分析并推荐最佳技能组合。

## 触发条件

- 用户说"怎么做"/"用什么技能"/"帮我选择"
- 用户意图模糊，需要先澄清再推荐
- 多个技能可能适用，需要对比选择

## 分析流程

### Step 1: 意图分类

读取用户消息，分类为：

| 分类 | 关键词 | 默认推荐 |
|------|--------|---------|
| **实现** | "添加"/"实现"/"创建"/"修改" | `/think-before-act` + `/test-driven-development` |
| **修复** | "修复"/"bug"/"错误"/"不工作" | `/systematic-debugging` |
| **重构** | "重构"/"优化"/"简化"/"清理" | `/think-before-act` + `/remove-ai-slops` |
| **设计** | "设计"/"架构"/"方案"/"怎么做" | `/brainstorming` + `/openspec-propose` |
| **测试** | "测试"/"E2E"/"覆盖率" | `/test-driven-development` + `/playwright` |
| **文档** | "文档"/"README"/"说明" | `/doc-audit` |
| **安全** | "安全"/"权限"/"认证" | `/security-review` |
| **探索** | "调研"/"对比"/"有什么方案" | `/ecosystem-scan` + librarian agent |
| **模糊** | 无明确关键词 | 提出 2-3 个可能方向，让用户选择 |

### Step 2: 上下文检查

在推荐前，检查当前状态：

1. **当前有哪些 in_progress 任务？** — 如果已有任务在进行，只推荐与当前任务相关的技能
2. **用户最近用了哪些技能？** — 避免重复推荐刚用过的
3. **项目当前阶段？** — 新功能 vs 修复 vs 重构，不同阶段推荐不同
4. **技术栈匹配？** — 确保推荐的技能适用于当前技术栈（Rust/TS/Docker/Web）

### Step 3: 输出推荐

**单技能场景**（意图明确）：
```
推荐: /systematic-debugging
理由: 连续 2 次 fix commit，说明需要先系统性诊断根因
```

**多技能场景**（需要组合）：
```
推荐组合:
1. /think-before-act — 先调研方案，避免盲目修改
2. /test-driven-development — 先写测试再实现
理由: 非平凡的多模块变更，需要测试保障
```

**模糊场景**（需要澄清）：
```
意图不明确，可能的方向:
1. 如果是实现新功能 → /think-before-act + /test-driven-development
2. 如果是修复 bug → /systematic-debugging
3. 如果是重构 → /think-before-act
请问具体想做什么？
```

### Step 4: 执行建议

推荐后，如果用户确认，直接调用该技能：
```
确认: 加载 /think-before-act 技能...
```

## 与 context-engineering 的关系

- **context-engineering**：被动触发（检测到场景时自动推荐，通过 AGENTS.md 目录表）
- **skill-router**：主动调用（用户问"怎么做"或意图模糊时，深度分析推荐）

两者互补：context-engineering 的路由表处理关键词匹配，skill-router 处理模糊意图和多技能组合推荐。

## 不推荐的情况

| 场景 | 不推荐 | 原因 |
|------|--------|------|
| 用户正在紧急修复 | 任何技能 | 不打断，修完再说 |
| 1 行修改 | /think-before-act | 杀鸡用牛刀 |
| 用户明确指定技能 | 其他技能 | 尊重用户选择 |
| 刚推荐过同一技能 | 同一技能 | 防骚扰 |
