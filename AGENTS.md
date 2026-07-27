# AGENTS.md — OMSPBase Project Knowledge Base

**Generated:** 2026-07-23

OMSPBase — AUDE 生态多媒体系统。为 AUDESYS 和 AUDEBase 提供统一的多媒体基础设施，涵盖远程桌面、视频会议、直播推拉流、监控相机接入等能力。当前状态: Phase 0-1 收尾，7 crate workspace，webrtc triple-backend (webrtc-rs 视频管线完整对齐)，omspbase-codec 三后端 (stub+FFmpeg+GStreamer)，44 commits on main。

## STRUCTURE

```
OMSPBase/
├── .opencode/          # OpenCode 配置（插件、MCP、LSP、instructions）
│   ├── opencode.json   # 主配置：模型、插件、instructions、MCP、LSP
│   ├── agent-guide.md  # AI 代理使用指南（5 层模型体系、OMO 编排）
│   ├── agent-model-tiers.md  # 模型分层体系
│   ├── oh-my-openagent.jsonc  # OMO Agent 配置
│   ├── init-lsp-wrap.mjs      # LSP 包装器初始化
│   ├── init-mcp-*.mjs         # MCP 初始化脚本（codegraph/playwright/postgres/openspace）
│   ├── package.json    # OpenCode 插件依赖
│   └── .gitignore
├── .agents/
│   ├── rules/          # 编码规则文件（16 语言 × common + 中文副本）
│   ├── skills/         # 技能（book-to-skill/doc-audit/openspec-*/test-harness）
│   └── memorys/        # 项目记忆文件 (decisions.md, status.md)
├── crates/              # Rust 工作区 (7 个 member crate)
│   ├── omspbase-host/   # Host 应用 (headless, 采集+编码+推流)
│   ├── omspbase-client/ # Remote 应用 (拉流+解码+控制)
│   ├── omspbase-server/ # Server 应用 (信令+relay+监控)
│   ├── omspbase-common/   # 共享基础: config, error, metrics, protocol, auth (41 tests)
│   ├── omspbase-media/  # 媒体管线: pipeline, broadcast, engine, transform (54 tests)
│   ├── omspbase-webrtc/ # WebRTC 抽象层 (stub/webrtc-rs/webrtc-sys, video pipeline parity)
│   └── omspbase-codec/  # 编解码: stub + FFmpeg + GStreamer 三后端
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
| Agent 使用指南 | `.opencode/agent-guide.md` | OMO 编排体系、5 层模型路由 |
| 模型分层 | `.opencode/agent-model-tiers.md` | 五层模型映射、provider 选择 |
| 语言规则 | `.agents/rules/{lang}/` | 各语言专属规则 |
| 通用规则 | `.agents/rules/common/` | 安全、编码风格、测试、Git 工作流 |
| 架构文档 | `docs/architecture.md` | 整体架构设计 |
│ 模块文档 | `docs/modules/` | 各领域详细设计 (25 篇)
| 项目记忆 | `.agents/memorys/` | 决策记录 (decisions.md)、状态跟踪 (status.md) |
│ Rust 源码 | `crates/` | 七个 crate: omspbase-host/remote-client/server/core/media/webrtc/codec

## CODE MAP

_项目已进入代码实施阶段。以下为当前状态：_

| 模块 | 状态 | 说明 |
|------|------|------|
| omspbase-host | 🟡 骨架完成 | Host 应用: 采集、编码、推流、信令、配置 |
| omspbase-client | 🟡 骨架完成 | Remote 应用: 拉流、解码、渲染、控制 |
| omspbase-server | 🟡 骨架完成 | Server 应用: 信令 relay、监控、会话管理 |
│ omspbase-common | ✅ 已实现 | 共享基础: config, error, metrics, protocol, auth (41 tests)
│ omspbase-media | ✅ 已实现 | 媒体管线: pipeline, broadcast, engine, transform (54 tests)
│ omspbase-webrtc | ✅ triple-backend | WebRTC 抽象层 (stub/webrtc-rs/webrtc-sys), 118+ tests, webrtc-rs 视频管线完整对齐
│ omspbase-codec | ✅ 三后端 | 编解码: stub + FFmpeg (static) + GStreamer (dynamic, pixi)
│ Phase 2+ crates | 🔲 计划中 | 详见 `.sisyphus/plans/consolidated-mvp/` 和 `docs/architecture.md`

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

## ANTI-PATTERNS

- **`as any` / `@ts-ignore`** — 永不使用，零例外
- **`console.log`** — 生产代码禁止
- **静默吞异常** — `catch(e) {}` 绝对不允许
- **对象突变** — 始终返回新对象，永不就地修改
- **硬编码密钥/端口/URL** — 使用环境变量、配置或密钥管理器；临时值必须标记 TODO
- **不必要的文件写入** — 文档文件仅在用户明确要求时创建
- **Rust `unwrap()` 用于生产** — 使用 `?` 配合 `thiserror`/`anyhow`

## NOTES

- **Phase 0-1 收尾** — webrtc-rs 视频管线完整对齐，codec 三后端完成，44 commits on main
- **骨架代码已创建** — `crates/omspbase-{host,remote,server}` 三个 crate 含模块骨架
- **AUDE 生态共享依赖** — AUDESYS 引用 Rust crate，AUDEBase 通过 napi 绑定
