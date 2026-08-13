---
name: context-engineering
description: "Feed MediaServo agents the right context for a 7-crate polyglot workspace. Routes Rust rules to .rs tasks, C++/FFI rules to webrtc-sys/mediasoup boundaries, protocol rules to WS contract work. Prevents wrong-language lint violations and out-of-scope analysis. Use BEFORE any cross-crate or multi-language MediaServo task."
---

# context-engineering — Right Context, Right Language

> Route language-specific rules to the right crate. Don't apply C++ lints to Rust code. Don't audit Python scripts for Rust ownership semantics.

## When to Use

| Trigger | Reason |
|---------|--------|
| Cross-crate task touches 2+ languages | Avoids wrong-language rule pollution |
| New crate or module being created | Establishes correct context boundaries |
| FFI/SDK work (napi-rs, FlatBuffers) | Multi-language contract, not single-lang |
| Web/Admin Dashboard work | HTML/CSS/JS + Rust server, different rulesets |
| Code review spanning crates | Each crate needs its own rule set applied |

## MediaServo Crate → Language Mapping

| Crate / Layer | Primary | Ruleset | Verification |
|---------------|---------|---------|-------------|
| mediaservo-common | Rust | `rules/rust/` + `rules/common/` | `cargo clippy -- -D warnings` |
| mediaservo-media | Rust | `rules/rust/` + `rules/common/` | `cargo clippy -- -D warnings` |
| mediaservo-webrtc | Rust + C++ (FFI) | `rules/rust/` + `rules/common/` + C++ primer | `cargo clippy` for Rust; C++ reviewed manually |
| mediaservo-codec | Rust + C (GStreamer) | `rules/rust/` + FFI safety | `cargo clippy`; GStreamer pipeline tested via pixi |
| mediaservo-server | Rust + (HTML/JS admin) | `rules/rust/` + `rules/common/` | `cargo test -p mediaservo-server` |
| mediaservo-host | Rust (macOS native) | `rules/rust/` + `constraints.md` (macOS) | `cargo clippy -p mediaservo-host` |
| mediaservo-client | Rust (macOS native) | `rules/rust/` + `constraints.md` (macOS) | `cargo clippy -p mediaservo-client` |
| Admin Dashboard | HTML/CSS/JS | `rules/common/` (security, coding-style) | `npx tsc --noEmit` (if TS); manual JS review |

## Context Selection Protocol

### Phase 1: Identify the Scope

Before loading any rules, map the task to crates:

```
Task → crates affected → languages involved → applicable rulesets
```

**Example:**
```
Task: "Add new WS message type ProduceMedia"
→ mediaservo-common (protocol.rs) + mediaservo-server (handle_sfu_message)
→ Rust only → rules/rust/coding-style.md + rules/common/security.md
```

### Phase 2: Apply Rules Per Crate

**NEVER** batch-apply all rules to all crates. Each crate gets its relevant subset:

```
for each crate in scope:
  - Load rules/rust/* if crate uses Rust
  - Load rules/common/* (security, coding-style, testing) always
  - Skip rules/rust/hooks.md for non-Rust files
  - Apply constraints.md (macOS, Docker, platform limits) if relevant
```

### Phase 3: Verify Per Crate

| Scope | Verification |
|-------|-------------|
| Any .rs change | `cargo clippy -p <crate> -- -D warnings` |
| WebRTC FFI change | `cargo check -p mediaservo-webrtc --features backend-webrtc-rs` |
| SFU change | `cargo check -p mediaservo-server --features sfu-mediasoup` |
| Admin UI change | Visual QA via Playwright |
| Full workspace | `pixi run check` |

## Common Context Mistakes

| Anti-Pattern | Why Wrong | Fix |
|-------------|-----------|-----|
| Loading all 12 language rulesets for a Rust-only task | Wastes ~4K tokens, confuses agent | Route by crate (see table above) |
| Applying C++ ownership rules to Rust code | Wrong paradigm | Rust borrow checker is the authority |
| Skipping `constraints.md` for webrtc-sys work | Misses macOS -ObjC linker flag (PIT-01) | Always include constraints for FFI |
| Applying `rules/rust/hooks.md` to TypeScript files | Wrong hooks fire | Only load per-language hooks |
| Running `cargo test --workspace` for a single-crate change | Slow (~30s) | Use `cargo test -p <crate>` |
| Not loading `pitfalls.md` for SFU/mediasoup work | Misses PIT-06 through PIT-15 | Always load pitfalls for SFU tasks |

## Multi-Language Boundary Rules

### Rust → C/C++ (webrtc-sys, mediasoup-sys)
- Only `&[u8]` across FFI boundary (PIT-03, C5)
- `unsafe` blocks require `// SAFETY:` comment
- `cxx::SharedPtr` types need `impl_thread_safety!` macros

### Rust → Protocol (WebSocket JSON)
- All messages use `#[serde(tag = "type", rename_all = "snake_case")]`
- Browser clients must use snake_case NOT camelCase (PIT-06)
- Protocol changes → update `crates/mediaservo-common/src/protocol.rs`
- E2E tests must validate new message types

### Rust → Admin UI (HTML/JS)
- Embedded via `rust-embed`, compiled into server binary
- Feature-gated: `admin-dashboard`
- JS code goes in `crates/mediaservo-server/admin/`
- No shared type system — JSON envelope validated server-side

## Gate Checklist

Before submitting work touching multiple crates:

```
[ ] crate scope mapped correctly
[ ] only relevant rulesets loaded per crate
[ ] cargo check passes per crate (not just workspace)
[ ] clippy passes per crate with -D warnings
[ ] constraints.md checked for platform gotchas
[ ] pitfalls.md checked for known SFU/FFI patterns
[ ] protocol changes have backward-compat review
```

## Related Skills

| Skill | Relationship |
|-------|-------------|
| `think-before-act` | Context selection IS part of "先查再动手" |
| `api-interface-design` | Context-engineering informs protocol contracts |
| `test-harness` | Cross-crate testing needs correct per-crate context |
| `lesson-memory` (C9) | New multi-language pitfalls → write to `pitfalls.md` |

## 任务 → 技能路由

识别用户意图后，主动建议加载对应技能：

| 关键词/场景 | 建议技能 | 触发条件 |
|------------|---------|---------|
| "实现/添加/创建 + 功能" | `incremental-implementation` | 跨文件变更 |
| "修复/bug/报错/不工作" | `systematic-debugging` | 运行时错误 |
| "怎么用/文档/API + 库名" | `source-driven-development` | 外部依赖 |
| "写测试/测试失败" | `test-driven-development` | 测试相关 |
| "页面/UI/前端/Admin" | `browser-testing` | 浏览器变更 |
| "安全/密钥/认证/漏洞" | `security-hardening` | 安全相关 |
| "审查/review/检查代码" | code-review 规则 | 代码修改后 |
| "性能/慢/卡顿/延迟" | `performance-optimization` | 性能问题 |
| "架构/设计/trait/协议" | `api-interface-design` | API 设计 |
| "切换/上下文/语言" | `context-engineering` | 多语言任务 |
| "CI/CD/Docker/pipeline" | `ci-cd-automation` | CI 变更 |
| "简化/重构/清理" | `code-simplification` | 降复杂度 |
| "优化 agent/审计/技能" | `ecosystem-scan` | Agent 体系 |
| "总结/记录/教训/经验" | `lesson-review` | 会话结束 |
| 任何非平凡操作前 | `think-before-act` | 自动 |
| "审计/一致性/文档矛盾" | `doc-audit` | 文档审计 |
| "方案/提案/标准化变更" | `openspec-propose` | 架构变更 |
| "实施/按方案/逐步" | `openspec-apply-change` | 方案执行 |
| "归档/完成变更" | `openspec-archive-change` | 变更归档 |
| "探索/调研/思路" | `openspec-explore` | 方案调研 |
| "生成测试/测试骨架" | `test-harness` | 测试框架 |
| "硬编码/密钥/端口扫描" | `review-hardcode` | 安全扫描 |
| "文档转技能/书籍" | `book-to-skill` | 文档转换 |
| "同步规格/delta" | `openspec-sync-specs` | 规格同步 |

## 主动建议机制

当 agent 识别到上述关键词但**未**加载对应技能时：
1. 简要提示："这个任务可能需要 `X` 技能，要我加载吗？"
2. **不强制** — 用户可跳过
3. **不重复** — 同一会话中对同一技能只建议一次
4. 建议格式：

```
💡 建议: 这个任务涉及 [场景]，`skill(name="X")` 可以提供 [价值]。需要吗？
```
