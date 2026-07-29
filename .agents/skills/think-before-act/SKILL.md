---
name: think-before-act
description: "元约束：先查再动手，列方案让用户审批，不蛮干。Use BEFORE any non-trivial action (debug/test/implement/refactor/fix/config/upgrade). Especially when facing silent failures, runtime errors not caught by compiler, or behavior that contradicts documentation."
---

# think-before-act — 先调研→列方案→用户审批→(团队审核)→执行

> 不蛮干。

## 触发条件

任何**非平凡**操作：调试、测试、实现、重构、修复、配置变更、升级。
失败模式异常时尤其触发：静默失败、编译期无法捕获的运行时错误、与文档描述不符的行为。

---

## 决策门（每次操作前自问）

1. **"官方推荐怎么做？"** → 不知道就去查，别猜。
2. **"项目之前遇到过吗？"** → 查项目知识库（.agents/memorys/ / git log）。
3. **"社区有人解决过吗？"** → 搜 issue / 论坛 / StackOverflow（用错误信息原文搜）。
4. **"用户同意这个方案吗？"** → 列方案等审批，别直接改。
5. **"上一次尝试为什么失败？"** → 说不清就别试下一个。

---

## 分级

| 档位 | 判据 | 流程 |
|------|------|------|
| **平凡** | 不改接口/构建/依赖，可逆，单文件，无副作用 | 直接做 + 一行说明 |
| **标准** | 改接口/构建/依赖/多文件/不可逆/框架级问题 | Phase 1→2→3 |
| **紧急** | 用户明确声明"紧急/先修后报"（生产宕机、数据丢失） | 先做 + 事后补报告 |

---

## Phase 1: 调研（标准档禁止跳过）

### 5 层知识源（按优先级）

1. **项目历史**: .agents/memorys/decisions.md / pitfalls.md / git log
2. **语言/工具链原生文档**: cargo doc / rustdoc / --help / man
3. **框架/工具官方文档**: 找推荐做法，不是随便搜
4. **社区经验**: GitHub issues / StackOverflow / 论坛（搜错误信息原文）
5. **类似项目**: 同技术栈开源实现怎么做的

### 测试场景额外清单

- [ ] 项目测试约定（框架? AAA? 命名? 覆盖率?）
- [ ] 规格/契约文档 → 提取测试场景
- [ ] 已有测试模式（同模块怎么测的? fixture?）

### 输出

调研摘要：找到了什么、官方推荐什么、项目约定是什么。

---

## Phase 2: 列方案（标准档，禁止自动执行）

### 格式

```
方案 A: [描述] — 优势 / 劣势 / 影响范围
方案 B: [描述] — 优势 / 劣势 / 影响范围
推荐: [X]，理由: [...]
```

### 代码变更必须列出

- 哪些文件会被修改
- 修改内容摘要（不是完整 diff）
- 潜在风险
- 回滚方式

### 审批

按 `question` 工具等待用户确认。
- 沉默 / 超时 / "继续" ≠ 批准
- 必须用户显式肯定响应
- 部分批准（"做 A 不做 B"）→ 只执行被批准的部分

### 复杂变更：团队审核

对于复杂变更（改构建/改架构/框架级问题/影响 >5 文件），
用户审批后、执行前，增加团队审核环节：

```
1. team_create — 创建审核团队（3 视角）
2. team_task_create — 分配审核任务
3. team_send_message — 发送计划草案
4. 收集审核反馈 → 修订计划
5. 用户确认修订 → 进入 Phase 3
```

审核发现的 CRITICAL 项必须修复。LOW 项可记录为技术债。

---

## Phase 3: 执行（用户已审批）

- 按用户选择的方案执行
- 每步验证（不积累未验证变更）
- 失败时: 回滚 + 报告，**不自动尝试方案 B**（回到 Phase 1 补充调研）

---

## 核心铁律

1. **无新信息 = 无新尝试。** 失败后换参数再试，没有新诊断信息就是在蛮干。
2. **测对层级。** 单元(逻辑) / 集成(边界) / E2E(用户流) / 验收(业务) — 不同层级捕获不同故障。如果单元测试通过但功能是坏的，你测错了层。
3. **平凡代码无需测试。** 一行、无分支、无副作用 → YAGNI 适用于测试。

---

## 测试层级速查

| 层级 | 捕获什么 | Rust 工具 |
|------|----------|-----------|
| 单元 | 逻辑错误 | `cargo test` |
| 集成 | 接口不匹配 | `cargo test --test '*'` |
| E2E | 用户路径断裂 | Playwright / Python WS scripts |
| 验收 | 需求未满足 | 手动验证 / pixi run test-sfu |

---

## 禁止清单（做 X 前先验证 Y）

| 想做 | 先验证 |
|------|--------|
| 改配置/改构建 | 查了官方推荐配置方式 |
| 失败后再试一次 | 有新的诊断信息（不是换参数碰运气） |
| 编辑生成文件/编译产物 | 确认修改不会在下次构建时被覆盖 |
| 删除不理解的东西 | 确认它不是被其他模块依赖的 |
| 直接编辑文件 | 用户已审批方案 |
| 在生产环境改依赖/pixi.toml | 本地验证通过后再改 |

---

## 与现有技能的关系

| 技能 | 关系 |
|------|------|
| `lesson-memory` (C9) | think-before-act 查过去教训；lesson-memory 记新教训 |
| `lesson-review` | think-before-act 是预防；lesson-review 是回顾 |
| `systematic-debugging` | think-before-act 管"修之前先查"；debugging 管"怎么修" |
| `verification-before-completion` | think-before-act 管"动手前"；verification 管"完成后" |

## 多生态示例

| 生态 | "先查再动手"的典型场景 |
|------|----------------------|
| Rust | 生命周期报错 → 先读 rustbook 对应章，别乱加 `.clone()` |
| Python | 依赖冲突 → 先查 pip/uv 解析规则，别反复 `pip install` |
| Go | 接口不满足 → 先读 godoc 接口契约，别瞎改签名 |
| JS/TS | 模块重复 → 先查打包器官方推荐，别乱调 alias |
| Java | 版本冲突 → 先跑 `mvn dependency:tree`，别手动排除 |
| K8s | Pod 崩溃 → 先查官方 troubleshooting + `kubectl describe`，别随机改 yaml |

---

## Karpathy 四原则映射

本项目 think-before-act 技能与 Andrej Karpathy 的工程哲学四原则天然对齐：

| Karpathy 原则 | 本技能对应 | 说明 |
|--------------|-----------|------|
| **Think Before Coding** | Phase 1: 调研 + 5 层知识源 | 先查项目历史、官方文档、社区经验，不猜就动手 |
| **Simplicity First** | 核心铁律 #3 + 分级（平凡/标准） | 简单方案优先，YAGNI，不过度设计 |
| **Surgical Changes** | Phase 2: 列方案 + 禁止清单 | 精准修改，列影响范围，不碰无关代码 |
| **Goal-Driven Execution** | Phase 3: 执行 + 决策门 | 按审批方案执行，每步验证，不跑偏 |

> **社区参考**: [multica-ai/andrej-karpathy-skills](https://github.com/multica-ai/andrej-karpathy-skills) — 将 Karpathy 工程哲学体系化为 AI 可执行技能，与本技能设计理念一致。
