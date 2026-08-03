# AUDEMSP 模型分层体系

> 五层模型映射: premium-max / premium / fast / vision / lite
> 双供应商架构: DeepSeek 官方直连 + New API 网关聚合

## 架构

AUDEMSP 通过两条独立路径接入大模型:

| 供应商 | 接入方式 | 特点 |
|--------|----------|------|
| **DeepSeek 官方** | 直连 API (`api.deepseek.com`) | 延迟低，稳定性高，模型版本官方可控 |
| **New API 网关** | 别名代理 (`192.168.100.47:3000`) | 多供应商聚合，自动降级，模型动态切换 |

New API 网关内部聚合 DeepSeek / Qwen / Kimi / Doubao / GLM / MiniMax 等供应商，
实现单 provider 多模型降级。网关侧修改别名映射即可全局切换模型，项目配置零修改。

DeepSeek 官方直连作为 New API 网关不可用时的独立降级路径，形成完整的高可用链路。

## 五层模型映射

### 双供应商对照表

| 层级 | 别名 | 用途 | DeepSeek 官方 | New API 网关 | New API Fallback 1 | New API Fallback 2 |
|------|------|------|---------------|-------------|-------------------|-------------------|
| premium-max | 极致推理 | 最复杂任务 | `deepseek-reasoner` | `deepseek-v4-pro-max` | `kimi-k2.6` | `minimax-m3` |
| premium | 主力 | 编排/构建/规划 | `deepseek-chat` | `deepseek-v4-pro` | `qwen3.7-max` | `glm-5.1` |
| fast | 快速 | 执行/搜索/审查 | `deepseek-coder` | `deepseek-v4-flash` | `qwen3.6-flash` | `doubao-seed-2.0-lite` |
| vision | 视觉 | 多模态分析 | `deepseek-vl2` | `doubao-seed-2.0-pro` | `qwen3.6-plus` | — |
| lite | 轻量 | 极简/琐碎任务 | `deepseek-chat` | `qwen3-32b` | `qwen3-8b` | — |

### New API 别名映射说明

New API 网关通过语义别名（`premium` / `fast` 等）引用模型，OpenCode 配置中
引用别名而非具体模型名。切换模型只需在网关侧修改别名映射，无需修改项目配置文件。

当前网关别名映射关系：

```
premium-max       → deepseek-v4-pro-max        # 旗舰推理
premium-max-1     → kimi-k2.6                  # Fallback 1
premium-max-2     → minimax-m3                 # Fallback 2

premium           → deepseek-v4-pro            # 主力推理
premium-1         → qwen3.7-max                # Fallback 1
premium-2         → glm-5.1                    # Fallback 2

fast              → deepseek-v4-flash          # 快速推理
fast-1            → qwen3.6-flash              # Fallback 1
fast-2            → doubao-seed-2.0-lite       # Fallback 2

vision            → doubao-seed-2.0-pro        # 视觉主力
vision-1          → qwen3.6-plus               # Fallback 1
vision-2          → gemini-3.5-flash           # Fallback 2

lite              → qwen3-32b                  # 轻量任务
lite-1            → qwen3-8b                   # Fallback
```

## 各层级详细说明

### premium-max: 极致推理

- **用途**: 最复杂任务，需要深度推理和多步思考
- **适用场景**: 架构设计、系统规划、复杂重构、技术评审、安全审计
- **DeepSeek 官方模型**: `deepseek-reasoner` — 专注于复杂推理链，长上下文
- **New API 主力**: `deepseek-v4-pro-max` / `kimi-k2.6` / `minimax-m3` — 旗舰模型链
- **温度参数**: 0.2 (高确定性)
- **Reasoning Effort**: high
- **注意事项**: 延迟最高，成本最高，仅用于真正需要深度推理的场景

### premium: 主力推理

- **用途**: 日常复杂任务，需要较强理解与生成能力
- **适用场景**: Agent 编排、构建脚本编写、规划生成、代码审查、架构咨询
- **DeepSeek 官方模型**: `deepseek-chat` — 深度求索最新对话模型，综合能力强
- **New API 主力**: `deepseek-v4-pro` / `qwen3.7-max` / `glm-5.1` — 主力推理链
- **温度参数**: 0.2 (高确定性)
- **注意事项**: 项目中最常用的层级，平衡能力与成本

### fast: 极速执行

- **用途**: 简单执行任务，要求低延迟、高吞吐
- **适用场景**: 代码搜索、文件探索、简单代码修改、快速审查、库搜索
- **DeepSeek 官方模型**: `deepseek-coder` — 编码优化模型，快速响应
- **New API 主力**: `deepseek-v4-flash` / `qwen3.6-flash` / `doubao-seed-2.0-lite`
- **温度参数**: 0.0 (完全确定性)
- **注意事项**: 延迟最敏感层级，不支持复杂推理，用于高频率小任务

### vision: 视觉专家

- **用途**: 多模态理解，图片/PDF 内容分析
- **适用场景**: UI 截图分析、文档扫描、绘图理解、视觉审查
- **DeepSeek 官方模型**: `deepseek-vl2` — 视觉语言模型
- **New API 主力**: `doubao-seed-2.0-pro` / `qwen3.6-plus`
- **温度参数**: 0.1 (高度确定性)
- **注意事项**: 仅在需要多模态能力时使用；纯文本任务应使用其他层级

### lite: 轻量级

- **用途**: 极简任务，追求极致性价比
- **适用场景**: 简单问答、日志摘要、内容格式化、元数据生成
- **DeepSeek 官方模型**: `deepseek-chat` (轻量调用)
- **New API 主力**: `qwen3-32b` / `qwen3-8b`
- **温度参数**: 0.2
- **注意事项**: 能力有限，仅用于确定性高、无需创造力的任务

## Agent 层级分配

### Agents

| Agent | 层级 | 说明 |
|-------|------|------|
| oracle | premium-max | 架构咨询、技术决策 |
| sisyphus | premium | 主协调 Agent |
| hephaestus | premium | 构建管理 |
| prometheus | premium-max | 计划生成 |
| atlas | premium | 实施执行 |
| librarian | fast | 库/文档搜索 |
| explore | fast | 代码探索 |
| metis | fast | 度量和数据分析 |
| momus | premium | 审查和批评 |
| sisyphus-junior | fast | 简单执行 |
| multimodal-looker | vision | 视觉分析 |

### Categories (task() 分发)

| 分类 | 层级 | 说明 |
|------|------|------|
| visual-engineering | premium | 可视化工程 |
| ultrabrain | premium-max | 超深度思考任务 |
| deep | premium | 深度分析 |
| unspecified-high | premium | 高复杂度未分类 |
| artistry | fast | 创作/文档书写 |
| quick | fast | 简单任务 |
| unspecified-low | fast | 低复杂度未分类 |
| writing | fast | 文档写作 |

## 选择指南

### 什么时候用什么层级

| 任务类型 | 推荐层级 | 备选 |
|----------|----------|------|
| 架构设计 / 系统评审 | premium-max | premium |
| Agent 编排 / 规划 | premium | fast（简单步骤） |
| 代码编写 / 重构 | premium | fast（小改动） |
| 文件搜索 / grep | fast | — |
| Bug 调试 | premium | premium-max（难复现 bug） |
| UI 截屏分析 | vision | premium（无视觉需求） |
| 日志摘要 / 格式化 | lite | fast |
| 代码审查 | premium | fast（简单格式审查） |
| 安全审计 | premium-max | premium |
| 简单问答 | lite | fast |

### 快速判断

1. **需要深度思考?** → premium-max
2. **需要综合理解和编码?** → premium
3. **简单执行或搜索?** → fast
4. **需要看图片/PDF?** → vision
5. **几句话就能搞定?** → lite

## Provider 选择指南

### 什么时候用 DeepSeek 官方

- New API 网关不可达时（网络隔离 / VPN 断开）
- 需要最低延迟（直连比网关少一次转发）
- DeepSeek 官方发布了新版模型，网关尚未同步
- 需要 DeepSeek 官方专属功能（如 `deepseek-reasoner` 的详尽推理过程）

### 什么时候用 New API 网关

- 日常开发（默认路径）
- 需要多供应商自动降级（一个模型超时自动切换下一个）
- 需要非 DeepSeek 模型的能力（如 Doubao 的视觉、Kimi 的长上下文）
- 网关侧已配置最优模型映射，无需关心具体供应商

### 优先顺序

```
New API 网关 → 自动降级 → DeepSeek 官方直连
```

OpenCode 的 `runtime_fallback` 机制自动处理上述逻辑:
1. 先尝试 New API 网关别名（如 `new-api/premium`）
2. 失败时尝试 New API Fallback 模型（`new-api/premium-1`, `new-api/premium-2`）
3. 全部失败后，可手工切换配置到 DeepSeek 官方模型 ID

## Fallback 链

### Agent 级 Fallback

每个 agent 在 `oh-my-openagent.jsonc` 中定义了 3 级 fallback:

```jsonc
"oracle": {
  "model": "new-api/premium-max",       // 主力
  "fallback_models": [
    "new-api/premium-max-1",            // Fallback 1
    "new-api/premium-max-2"             // Fallback 2
  ]
}
```

### Runtime Fallback

`runtime_fallback` 全局配置处理网络级错误:

| 配置项 | 值 | 说明 |
|--------|-----|------|
| retry_on_errors | 402, 429, 500, 502, 503, 504 | 触发 fallback 的 HTTP 状态码 |
| max_fallback_attempts | 2 | 最多尝试 2 次降级 |
| cooldown_seconds | 60 | 失败后冷却 60 秒 |
| timeout_seconds | 60 | 单次请求超时 |

### Fallback 数据流

```
用户请求
  │
  ▼
Agent 主力模型 (new-api/premium)
  │  ├─ 成功 → 返回结果
  │  └─ 失败 → 重试 (最多 2 次)
  │       │
  │       ▼
  │   Fallback 1 (new-api/premium-1)
  │     ├─ 成功 → 返回结果
  │     └─ 失败 → 重试
  │          │
  │          ▼
  │      Fallback 2 (new-api/premium-2)
  │        ├─ 成功 → 返回结果
  │        └─ 全部失败 → 网关不可用，切换 DeepSeek 官方
  │             │
  │             ▼
  │         DeepSeek 官方 (deepseek-chat / deepseek-reasoner)
  │           └─ 成功/失败 → 返回结果或报错
```

## 添加新模型

1. 在 DeepSeek 官方确认模型可用性，获取 API 端点
2. 在 New API 网关注册新模型渠道并创建别名映射
3. 更新本文档的映射表
4. OMO 配置无需修改（使用别名引用）

## 注意事项

- DeepSeek 官方模型 ID 随官方更新可能变化，以 `api.deepseek.com` 文档为准
- New API 别名映射由网关管理员维护，项目成员无需关心具体映射
- 切换 DeepSeek 官方直连时需确保 API Key 在环境变量中配置
- 视觉任务优先使用 New API（Doubao 视觉能力优于 DeepSeek 官方）
- 低成本任务使用 lite 层级可显著减少 token 消耗
