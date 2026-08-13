# AGENTS.md — MediaServo Project Knowledge Base

**Generated:** 2026-07-23

MediaServo — 实时媒体伺服平台。独立部署的视频/媒体服务平台，涵盖监控相机接入与录制回放（NVR）、视频会议、远程桌面、遥操作、直播推拉流等能力。当前状态: Phase 3 完成，7 crate workspace，webrtc triple-backend (webrtc-rs 视频管线完整对齐)，mediaservo-codec 三后端 (stub+FFmpeg+GStreamer)，343 commits on main。

## STRUCTURE

```
MediaServo/
├── .opencode/          # OpenCode 配置（插件、MCP、LSP、instructions）
│   ├── opencode.json   # 主配置：模型、插件、instructions、MCP、LSP
│   ├── agent-model-tiers.md  # 模型分层体系
│   ├── oh-my-openagent.jsonc  # OMO Agent 配置
│   ├── init-lsp-wrap.mjs      # LSP 包装器初始化
│   ├── init-mcp-*.mjs         # MCP 初始化脚本（codegraph/playwright/postgres/openspace）
│   ├── package.json    # OpenCode 插件依赖
│   └── .gitignore
├── .agents/
│   ├── rules/          # 编码规则文件（12 语言 + common + web + zh/）
│   ├── skills/         # 技能（book-to-skill/doc-audit/lesson-review/openspec-*/review-hardcode/test-harness/think-before-act）
│   └── memorys/        # 项目记忆文件 (decisions.md, status.md)
├── crates/              # Rust 工作区 (7 个 member crate)
│   ├── mediaservo-host/   # Host 应用 (headless, 采集+编码+推流)
│   ├── mediaservo-client/ # Remote 应用 (拉流+解码+控制)
│   ├── mediaservo-server/ # Server 应用 (信令+relay+监控)
│   ├── mediaservo-common/   # 共享基础: config, error, metrics, protocol, auth (72 tests)
│   ├── mediaservo-media/  # 媒体管线: pipeline, broadcast, engine, transform (107 tests)
│   ├── mediaservo-webrtc/ # WebRTC 抽象层 (stub/webrtc-rs/webrtc-sys, video pipeline parity)
│   └── mediaservo-codec/  # 编解码: stub + FFmpeg + GStreamer 三后端
├── docs/               # 设计文档 (architecture.md + modules/ + reference/ + research/)
├── README.md           # 项目简介
├── LICENSE             # Apache 2.0
├── package.json        # 根 package.json（codegraph 开发依赖）
├── bootstrap.sh / bootstrap.bat  # 开发环境引导脚本
├── .rustfmt.toml       # Rust 格式化配置
├── clippy.toml         # Clippy lint 配置
├── deny.toml           # cargo-deny 审计配置
├── rust-toolchain.toml # Rust 工具链版本
└── .gitignore
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| 项目简介 | `README.md` | 功能范围、架构定位、技术栈 |
| Agent 配置 | `.opencode/opencode.json` | instructions、MCP、LSP |
| 模型分层 | `.opencode/agent-model-tiers.md` | 五层模型映射、provider 选择 |
| 语言规则 | `.agents/rules/{lang}/` | 各语言专属规则 |
| 通用规则 | `.agents/rules/common/` | 安全、编码风格、测试、Git 工作流 |
| 架构文档 | `docs/architecture.md` | 整体架构设计 |
| 模块文档 | `docs/modules/` | 各领域详细设计 (27 篇)
| 项目记忆 | `.agents/memorys/` | 决策记录 (decisions.md)、状态跟踪 (status.md) |
| Rust 源码 | `crates/` | 七个 crate: mediaservo-host/mediaservo-client/mediaservo-server/mediaservo-common/mediaservo-webrtc/mediaservo-media/mediaservo-codec


## SKILL DIRECTORY

当面对以下场景时，使用 `skill` 工具加载对应技能：

| 场景 | 技能 | 何时触发 |
|------|------|---------|
| **开发新功能** | `incremental-implementation` | 多文件变更、跨 crate 修改 |
| **修复 bug** | `systematic-debugging` (内置) | 测试失败、运行时错误 |
| **查外部库用法** | `source-driven-development` | 使用 mediasoup/webrtc-rs/GStreamer 等外部库 |
| **写测试** | `test-driven-development` (内置) | 任何新功能或 bug 修复前 |
| **测试浏览器 UI** | `browser-testing` | Admin Dashboard 变更、Playwright |
| **安全审计** | `security-hardening` | 密钥管理、认证代码、mediasoup 传输安全 |
| **代码审查** | `code-review` (规则) | 完成任何代码修改后 |
| **性能分析** | `performance-optimization` | WebRTC 延迟、React 渲染、cargo bench |
| **架构/API 设计** | `api-interface-design` | Rust trait 设计、WS 协议、REST 契约 |
| **上下文切换** | `context-engineering` | 在 Rust/TS/DevOps 间切换任务 |
| **CI/CD 变更** | `ci-cd-automation` | Dockerfile、pixi.toml、GitHub Actions |
| **代码简化** | `code-simplification` | 重构、降低复杂度 |
| **优化 Agent 体系** | `ecosystem-scan` | 审计 .agents/、找新技能 |
| **会话结束** | `lesson-review` | 总结经验教训 |
| **行动前** | `think-before-act` | 任何非平凡操作前（自动触发） |
| **不确定用什么技能** | `skill-router` | 意图模糊或需要组合推荐时 |
| **文档审计** | `doc-audit` | 检查文档/决策/agent 体系一致性 |
| **批量验证** | `openspec-propose` | 提出标准化变更方案 |
| **实施变更** | `openspec-apply-change` | 按方案逐步实施 |
| **归档变更** | `openspec-archive-change` | 完成后归档 |
| **探索方案** | `openspec-explore` | 调研阶段探索思路 |
| **测试工具** | `test-harness` | 生成测试骨架 |
| **硬编码扫描** | `review-hardcode` | 检查密钥/端口/URL 硬编码 |
| **文档转技能** | `book-to-skill` | 将文档/书籍转为 AI 技能 |
| **同步规格** | `openspec-sync-specs` | delta specs 同步到主 specs |

## CODE MAP


_项目已进入代码实施阶段。以下为当前状态：_

| 模块 | 状态 | 说明 |
|------|------|------|
| mediaservo-host | 🟡 骨架完成 | Host 应用: 采集、编码、推流、信令、配置 |
| mediaservo-client | 🟡 骨架完成 | Remote 应用: 拉流、解码、渲染、控制 |
| mediaservo-server | 🟡 骨架完成 | Server 应用: 信令 relay、监控、会话管理 |
│ mediaservo-common | ✅ 已实现 | 共享基础: config, error, metrics, protocol, auth (72 tests)
│ mediaservo-media | ✅ 已实现 | 媒体管线: pipeline, broadcast, engine, transform (107 tests)
│ mediaservo-webrtc | ✅ triple-backend | WebRTC 抽象层 (stub/webrtc-rs/webrtc-sys), 118+ tests, webrtc-rs 视频管线完整对齐
│ mediaservo-codec | ✅ 三后端 | 编解码: stub + FFmpeg (static) + GStreamer (dynamic, pixi)
| Phase 2+ crates | 🔲 计划中 | 详见 `docs/architecture.md`

## CONVENTIONS

### Rust
- Edition 2024，`cargo clippy -- -D warnings`
- `thiserror` 用于库，`anyhow` 用于应用
- `&str` 优先于 `String`，`&[T]` 优先于 `Vec<T>`
- 每个 `unsafe` 块必须有 `// SAFETY:` 注释
- 业务关键 enum 使用完整 match，禁止通配符 `_`

### TypeScript
- 公共 API 显式类型注解
- `interface` 优先于 `type`（对象形状）
- `unknown` > `any`
- Zod 用于边界层模式验证
- 禁止 `as any` / `@ts-ignore` / `console.log`

### C++
- RAII 无处不在 — 不用裸 `new`/`delete`，使用智能指针
- 禁止：`malloc`/`free`、C 风格数组、`strcpy`/`strcat`/`sprintf`
- 始终：`std::array`/`std::vector`、`std::string`、初始化变量

### 通用
- 不可变性优先（永不突变，总是创建新副本）
- 小文件 > 大文件（200-400 行典型，800 行最大）
- 显式错误处理，无静默吞异常
- 布尔值前缀 `is`/`has`/`should`/`can`
- 官方用法优先 — 使用依赖库/项目/工具时遵循官方文档、官方仓库源码、官方示例和社区推荐用法，禁止自创推测用法和最小接口（详见 conventions.md C18）

## ANTI-PATTERNS

- **`as any` / `@ts-ignore`** — 永不使用，零例外
- **`console.log`** — 生产代码禁止
- **静默吞异常** — `catch(e) {}` 绝对不允许
- **对象突变** — 始终返回新对象，永不就地修改
- **硬编码密钥/端口/URL** — 使用环境变量、配置或密钥管理器；临时值必须标记 TODO
- **不必要的文件写入** — 文档文件仅在用户明确要求时创建
- **Rust `unwrap()` 用于生产** — 使用 `?` 配合 `thiserror`/`anyhow`
- **自定义推测用法 / 最小接口** — 外部依赖（webrtc/mediasoup/GStreamer 等）必须用官方用法和示例，禁止为省事裁剪成最小接口或自创语义（PIT-65 教训，C18）

## NOTES

- **Phase 3 完成** — Docker/CI/DevContainer 就位，SFU connect_transport 已实现，343 commits on main
- **骨架代码已创建** — `crates/mediaservo-{host,client,server}` 三个 crate 含模块骨架
- **生态共享依赖** — 第三方平台通过 Rust crate 静态链接或 napi 绑定消费本仓库
