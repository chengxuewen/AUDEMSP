# OMSPBase 架构决策记录

> **说明**: 本文件包含活跃决策（D196+）。历史决策（D1-D195）归档在 `decisions-archived.md`。
> 决策格式: `## D{N}: 标题` — 决策 + 日期 + 原因 + 影响
## D196: Admin Dashboard Architecture

**Decision**: React+TypeScript admin SPA embedded in server binary (rust-embed + build.rs). Phase 1 targets Remote Control scenario (DeviceStream). Room model refactored to per-stream rooms with N consumers. Zustand for frontend state. Admin JWT separate from signaling JWT.
**Date**: 2026-07-24
**Reason**: 
- Operators need visual monitoring of media streams (91 commits, zero visibility)
- Remote Control is the immediate use case (vehicles pushing camera streams)
- Per-stream room model simplifies relay routing
- Unified AdminEvent enum serves both audit logging and WS push
- build.rs auto-resolves dist/ path for rust-embed
**Supersedes**: D136/D142 (consolidated MVP admin plan — larger scope, different architecture)

## D197: D87 Scope Limitation — Client GUI Only

**Decision**: D87 (React + Ant Design for Server management panel) applies only to omspbase-client GUI. OMSPBase Server admin dashboard uses CSS Modules for zero-dependency lightweight panel.
**Date**: 2026-07-24
**Reason**:
- D87's rationale (share components with AUDEBase Admin UI) is irrelevant for embedded server admin
- Admin dashboard is a monitoring tool, not a user-facing application
- CSS Modules = zero runtime, smaller bundle, no framework lock-in
- Ponytail principle: don't add Ant Design for a few cards and a table
**Limits**: D87 remains in effect for omspbase-client (Tauri desktop app) and any AUDEBase-shared UI
## D198: SFU Video Playback — Server-Offer Architecture

**决策**: 浏览器视频播放使用 mediasoup 的 Server-Offer 模式（SFU 创建 transport offer，客户端创建 answer）。Host 通过 SFU Produce 推流（非 P2P WebRTC）。
**日期**: 2026-07-27
**修订**: 2026-07-29（transport connect 已实现）
**状态**: ✅ 实现完成
- `connect_transport()` 已实现 (sfu.rs:331-371)
- signaling.rs + admin.rs handler 已调用实际连接
- 浏览器 sfu-client.ts consume 消息补充 rtp_capabilities


## D199: OpenCode Instructions 精简化

**决策**: 将 instructions 数组从 23 个文件精简为 19 个（D199 后续新增 docker/platform/lesson-memory），移除中英文重复（zh/）、无关语言规则（TS/CPP）、参考文档（agent-guide/model-tiers）。
**日期**: 2026-07-28
**修订**: 2026-07-29（数量从 17→19，新增 docker.md/platform.md/lesson-memory.md）
**原因**:
**日期**: 2026-07-28
**原因**:
- zh/ 10 个文件是 common/ 的完整中文翻译，约占 1,950 tokens — 纯冗余
- TypeScript/CPP coding-style 对 Rust 项目无关 — 约 1,200+800 tokens 浪费
- agent-guide.md 和 agent-model-tiers.md 是工具参考文档，非每轮必需 — 约 1,900 tokens
- constraints.md 和 edit-safety.md 已存在但未加载 — 比 agent-guide 更重要
- Rust 专属规则（coding-style + hooks）未在 instructions 中
- 3 个小的 memorys 文件（status/conventions/pitfalls）加入 instructions
**效果**: ~11,700 → ~8,500 tokens（节省 27%，且加载了更相关的规则）

## D200: OMO Agent 模型分配优化

**决策**: metis 保持 fast；momus 从 fast 升级到 premium；prometheus 从 premium 升级到 premium-max；explore 温度从 0.0 调整到 0.1。
**日期**: 2026-07-28
**修订**: 2026-07-29（参考 OMSPBase+AUDEBase 联合评估）
**原因**:
- metis（度量/数据分析）本质是 pattern matching，复杂度低，调用频率高 → fast 足够
- momus（计划批评家）是质量门禁，对抗性审查需要深度推理 → premium
- prometheus（计划生成）是最高杠杆 agent，计划错误 = 下游全部返工 → premium-max
- 温度：metis 0.1，momus 0.3（创造性批评），explore 0.1
- oracle 保持 premium-max（最复杂架构决策）
**新增**: teams.dev 预定义（implementer + test-writer + reviewer），staleTimeoutMs 300000→600000

## D201: Pre-commit Hook — Rust 质量门禁

**决策**: 创建 `.git/hooks/pre-commit`，对暂存 `.rs` 文件运行 `cargo fmt --check` + `cargo clippy -- -D warnings`。
**日期**: 2026-07-28
**原因**:
- 规则会被遗忘，hook 不会。pre-commit 是最后一道防线
- `grep` 无匹配时用 `{ grep ... || true; }` 包装，避免 `set -euo pipefail` 下误中断
- 仅 .rs 文件暂存时触发，不阻塞非 Rust 提交

## D202: Global Provider Config — lite 上下文修复 + Reasoning 启用

**决策**:
1. lite 模型的 context limit 从 40,960 修正为 131,072
2. deepseek-v4-pro 和 deepseek-v4-flash 的 supportsReasoning 设为 true
**日期**: 2026-07-28
**原因**:
- lite 路由到 Qwen3-32B，实际支持 128K+ context，40,960 是配置错误（参考 lite-1 Qwen3-8B 也有 131K）
- DeepSeek V4 系列支持 reasoning API，设为 true 使 opencode 可使用 reasoning 特性
- Fallback 上下文窗口经别名映射验证全部正确：premium-max-1(256K)=Kimi K2.6, premium-2(205K)=GLM-5.1, fast-1(131K)=Qwen3.6 Flash 等
- apiKey 硬编码未修改（用户选择保持现状为内网环境）

**Phase**: Config

## D203: Agent 模型层级最终确认

**决策**: prometheus 从 premium 升级到 premium-max；metis 保持 fast（非 premium）。
**日期**: 2026-07-29
**原因**:
- prometheus（计划生成）是最高杠杆 agent — 计划错误 = 下游全部返工
- metis（度量分析）本质是 pattern matching，复杂度低，调用频率高 → fast 足够
- 参考 OMSPBase+AUDEBase 联合评估：oracle + prometheus 为 premium-max 双高杠杆
**影响**: oh-my-openagent.jsonc 已更新，agent-model-tiers.md 已同步

**Phase**: System

## D204: ecosystem-scan 技能体系

**决策**: 创建 ecosystem-scan 技能（双层 Quick/Full + 社区对比 + 安全门禁），同时创建 doc-audit OMSPBase 适配版。
**日期**: 2026-07-29
**原因**:
- .agents/ 体系需要定期审计和外部对标
- 社区先例：autoskills、agent-skill-discovery、skill-update-team、agent-self-audit
- doc-audit 从 AUDESYS 直搬未适配，需改写
**影响**: 21 个技能，ecosystem-scan + doc-audit + 9 个从社区移植的技能

**Phase**: System

## D205: skill-router 技能创建

**决策**: 创建 skill-router 技能，用于意图模糊时自动分析并推荐最佳技能组合。
**日期**: 2026-07-29
**原因**:
- 21 个技能导致用户不知道何时用哪个
- context-engineering 路由表处理关键词匹配，skill-router 处理模糊意图
- 与 context-engineering 互补：规则文件处理明确场景，技能处理模糊意图
**影响**: 22 个技能，AGENTS.md 目录表已更新

## D206: Docker 构建国内镜像加速 (A 方案)

**决策**: Dockerfile 使用国内镜像源加速构建（apt 清华源 + rustup 清华镜像 + cargo sparse 清华镜像）。
**日期**: 2026-07-31
**原因**:
- 国内网络下 Ubuntu apt/rustup/crates.io 直连慢或不可达（PIT-31/PIT-33 教训）
- mediasoup-sys flatbuffers wrapdb 不可达 → 统一走 Docker 构建（C13）
- 镜像加速只解决网络瓶颈（2-5min），mediasoup C++ 编译（15-30min）为硬瓶颈
**影响**: Dockerfile base 阶段 3 处加速点。后续 B 方案（预构建 dev 镜像）将进一步缩短到 <5min。

## D207: 预构建 dev 镜像推送 ghcr.io (B 方案)

**决策**: 构建一次含全部编译依赖的 dev 镜像，推送 `ghcr.io/{org}/omspbase-server-dev:latest`，后续 Dockerfile FROM 直接拉取。
**日期**: 2026-07-31
**原因**:
- mediasoup C++ Worker 编译 15-30min 无法在每次构建时重复（OpenVidu pre-built binary 模式）
- 层缓存（P0.1）只对 Cargo.toml 不变时生效，首次构建仍慢
- 预构建镜像一劳永逸：apt/rustup/crates 全部跳过
**影响**: 待用户确认 ghcr org 名称后实施。Dockerfile base 阶段改为 FROM 预构建镜像。

---

## D208: 构建优化策略实施 (2026-08-03)

**决策**: 采纳 docs/reference/build-optimization-strategy.md 方案 B（dev+builder 双镜像预烘焙 + 国内镜像修复 + lto 优化），分三阶段执行（本周修复 / 本月结构 / 下月按需）。
**日期**: 2026-08-03
**原因**:
- 首次 Docker 构建 15-30 min（mediasoup C++ 45% + Rust deps 35%），dev 镜像无预编译依赖 + gha cache 本地不可达
- 团队模式 4 分析师 + 4 审核员交叉验证：审计发现全部属实，方案经 H1-H4/M1-M6 修正
- 实测发现：pixi 无国内镜像（最慢层）、rsproxy sparse URL 失效、tuna 不镜像 cargo 二进制、ghproxy 停运
**修订**:
- **D206 部分修订**：apt/rustup 清华镜像保留；cargo 镜像 tuna → rsproxy（tuna 只镜像 index，.crate 二进制 404）
- **D207 机制修订**：FROM 预构建 base → compose `image:` + `pull_policy: always`（本地零构建，命名卷 copy-on-first-use 灌入烘焙产物）；镜像命名统一 `omspbase-server-dev` / `omspbase-server-builder`
**关键约束**（审核修正，实施时强制执行）:
- 卷 copy-on-first-use 仅空卷生效 → 落地必须显式 `docker volume rm omspbase_cargo-cache`
- 预烘焙镜像 amd64 only，Apple Silicon 走仿真，dev service 显式声明 platform
- GHCR 清理 workflow（sha tag 保留 N=10）+ path-filter（仅依赖变更时推 dev 镜像）
- ghcr 可达性未实测前不实施预烘焙（PIT-14/31 背景下可能 30min+）
- 生产 runtime 缺口是 admin dist 产物（非 feature）→ Docker 构建需 `pnpm build:admin` 先于 cargo build（PIT-23）
**影响**: 本地首次构建 15-28 min → 2-5 min（预计）；日常增量每轮省 30-60s。实施细节见 docs/reference/build-optimization-strategy.md。
