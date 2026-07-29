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
**决策**: 浏览器视频播放使用 mediasoup 的 Server-Offer 模型（SFU 创建 transport offer，客户端创建 answer），参考 LiveKit 模式。Host 通过 SFU Produce 推流（非 P2P WebRTC）。
**日期**: 2026-07-27
**原因**:
- P2P SDP 中继存在时序问题（host offer 在 viewer 加入后丢失）
- Server-Offer 模型消除此问题 — SFU 为每个 viewer 创建新 transport
- mediasoup transport 协议已在 sfu.rs 中实现
- LiveKit 使用此模型，是 WebRTC SFU 的行业标准
- Admin dashboard 复用现有 SignalingMessage 类型（CreateWebRtcTransport、Consume）用于浏览器 consumer

## D203: Agent 模型层级最终确认

**决策**: prometheus 从 premium 升级到 premium-max；metis 保持 fast（非 premium）。
**日期**: 2026-07-29
**原因**:
- prometheus（计划生成）是最高杠杆 agent — 计划错误 = 下游全部返工
- metis（度量分析）本质是 pattern matching，复杂度低，调用频率高 → fast 足够
- 参考 OMSPBase+AUDEBase 联合评估：oracle + prometheus 为 premium-max 双高杠杆
**影响**: oh-my-openagent.jsonc 已更新，agent-model-tiers.md 已同步

## D204: ecosystem-scan 技能体系

**决策**: 创建 ecosystem-scan 技能（双层 Quick/Full + 社区对比 + 安全门禁），同时创建 doc-audit OMSPBase 适配版。
**日期**: 2026-07-29
**原因**:
- .agents/ 体系需要定期审计和外部对标
- 社区先例：autoskills、agent-skill-discovery、skill-update-team、agent-self-audit
- doc-audit 从 AUDESYS 直搬未适配，需改写
**影响**: 21 个技能，ecosystem-scan + doc-audit + 9 个从社区移植的技能
