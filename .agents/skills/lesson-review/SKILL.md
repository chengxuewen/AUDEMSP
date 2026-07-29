---
name: lesson-review
description: "批量会话回顾：系统性提取经验教训，写入项目记忆。Use after long debugging sessions (>1h or >3 failed attempts), when user corrects multiple errors, after build fixes, on '总结经验'/'更新记忆'/'记录教训'/'回顾会话', or /lesson-review command."
---

# lesson-review — 批量经验教训沉淀

## 触发条件

- 用户说"总结经验" / "更新记忆" / "记录教训" / "回顾会话"
- 长时间调试会话结束（>1h 或 >3 次失败尝试）
- 用户指出多个错误后
- 解决一个耗时 >30min 的问题后
- `/lesson-review` 命令

## 与规则和技能的关系

```
think-before-act  →  [行动]  →  lesson-memory  →  doc-audit
   (咨询)                       (即时捕获)          (定期审计)
                    lesson-review ← 批量补漏
```

| | `lesson-memory` 规则 (C9) | `lesson-review` 技能 |
|---|---|---|
| 时机 | 即时（每次错误后，自动触发） | 批量（会话结束/用户触发） |
| 方式 | 反射 | 交互式回顾 |
| 粒度 | 单条教训 | 全会话扫描 |
| 互补 | 第一层防线（不遗漏） | 第二层防线（不误判 + 归类） |

- `think-before-act`（咨询）：行动前检查已有教训，避免重蹈
- `lesson-memory`（C9 规则）：每次错误后**即时自动**写入，无需用户要求
- `lesson-review`（本技能）：会话末**批量回顾**，查漏补缺、归类归档
- `doc-audit`（审计）：定期检查记忆文件自洽性

## 流程

### Step 1: 扫描会话

回顾本次会话中哪些时刻触发了规则但可能遗漏了：

- 编译/构建失败
- >1 次失败尝试才定位根因
- 用户纠正做法/偏好
- 意外发现（"原来如此"、"没想到"）
- 耗时 >30min 的问题
- 编辑后语法损坏（brace 不平衡、重复行等）

### Step 2: 逐条提取（5 问清单）

对每个发现，逐条完成：

1. **什么错了？**（现象描述）
2. **为什么错？**（根因分析）
3. **正确做法？**（解决方案）
4. **如何预防？**（检查命令 / 约束）
5. **存到哪里？**（按存储目标表）

### Step 3: 写入

按目标文件的模板格式写入。

- 重要教训 → 完整模板（症状 + 根因 + 解法，三要素）
- 平凡修复（1 行/无分支/无副作用）→ 1 行捕获，不用完整模板

### Step 4: 验证

- [ ] 每条 pitfall 有 verify 字段（检查命令 — 如何确认已修复）
- [ ] 每条 convention 可被 grep/lint 验证
- [ ] 与已有条目无重复（grep 目标文件确认）
- [ ] decisions.md 编号连续（不跳号）
- [ ] 交叉引用正确（如 pitfalls 中引用 conventions）
- [ ] 高频/高成本教训标注是否需要 CI 门禁升级

## 输出格式

```markdown
## 会话经验总结 — YYYY-MM-DD

### 已记录 (N 条)
1. [标题] → pitfalls.md
2. [标题] → conventions.md
...

### 未记录（无需记录）
- [原因：一次性问题 / 已有记录 / 环境特定]

### 建议升级
- [某条教训建议添加 CI 门禁，理由：重复 3+ 次/耗费 >1h]
```

## 存储目标（按项目配置）

适用任何项目，修改路径即可：

```
- 技术陷阱 → {pitfall_log}      # OMSPBase: .agents/memorys/pitfalls.md
- 开发约束 → {conventions}       # OMSPBase: .agents/memorys/conventions.md
- 架构决策 → {decisions}         # OMSPBase: .agents/memorys/decisions.md
- 可执行检查 → {checks}          # OMSPBase: .agents/rules/common/edit-safety.md
- 测试要求 → {test_rules}        # OMSPBase: .agents/rules/common/testing.md
- 安全规则 → {security_rules}    # OMSPBase: .agents/rules/common/security.md
- Rust 编码 → {rust_style}       # OMSPBase: .agents/rules/rust/coding-style.md
- 项目状态 → {status}            # OMSPBase: .agents/memorys/status.md
```
