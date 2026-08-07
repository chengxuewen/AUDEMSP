# AUDEMSP 架构决策记录

> **说明**: 本文件包含活跃决策（D196+）。历史决策（D1-D190）归档在 `decisions-archived.md`（含 20 个历史跳号）。
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

**Decision**: D87 (React + Ant Design for Server management panel) applies only to audemsp-client GUI. AUDEMSP Server admin dashboard uses CSS Modules for zero-dependency lightweight panel.
**Date**: 2026-07-24
**Reason**:
- D87's rationale (share components with AUDEBase Admin UI) is irrelevant for embedded server admin
- Admin dashboard is a monitoring tool, not a user-facing application
- CSS Modules = zero runtime, smaller bundle, no framework lock-in
- Ponytail principle: don't add Ant Design for a few cards and a table
**Limits**: D87 remains in effect for audemsp-client (Tauri desktop app) and any AUDEBase-shared UI
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
**修订**: 2026-07-29（参考 AUDEMSP+AUDEBase 联合评估）
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
- 参考 AUDEMSP+AUDEBase 联合评估：oracle + prometheus 为 premium-max 双高杠杆
**影响**: oh-my-openagent.jsonc 已更新，agent-model-tiers.md 已同步

**Phase**: System

## D204: ecosystem-scan 技能体系

**决策**: 创建 ecosystem-scan 技能（双层 Quick/Full + 社区对比 + 安全门禁），同时创建 doc-audit AUDEMSP 适配版。
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
> **修订 (D208)**: cargo 镜像由清华 tuna 改为 rsproxy（tuna 只镜像 index，.crate 二进制 404 实测）；apt/rustup 清华源保留。

## D207: 预构建 dev 镜像推送 ghcr.io (B 方案)

**决策**: 构建一次含全部编译依赖的 dev 镜像，推送 `ghcr.io/{org}/audemsp-server-dev:latest`，后续 Dockerfile FROM 直接拉取。
**日期**: 2026-07-31
**原因**:
- mediasoup C++ Worker 编译 15-30min 无法在每次构建时重复（OpenVidu pre-built binary 模式）
- 层缓存（P0.1）只对 Cargo.toml 不变时生效，首次构建仍慢
- 预构建镜像一劳永逸：apt/rustup/crates 全部跳过
> **修订 (D208)**: 机制改为 compose `image:` + `pull_policy: always`（本地零构建）；命名统一 `audemsp-server-dev` / `audemsp-server-builder`；预烘焙按需启动（团队扩张时实施）。
**影响**: 待用户确认 ghcr org 名称后实施。Dockerfile base 阶段改为 FROM 预构建镜像。

---

## D208: 构建优化策略实施 (2026-08-03)

**决策**: 采纳 docs/reference/codec/build-optimization-strategy.md 方案 B（dev+builder 双镜像预烘焙 + 国内镜像修复 + lto 优化），分三阶段执行（本周修复 / 本月结构 / 下月按需）。
**日期**: 2026-08-03
**原因**:
- 首次 Docker 构建 15-30 min（mediasoup C++ 45% + Rust deps 35%），dev 镜像无预编译依赖 + gha cache 本地不可达
- 团队模式 4 分析师 + 4 审核员交叉验证：审计发现全部属实，方案经 H1-H4/M1-M6 修正
- 实测发现：pixi 无国内镜像（最慢层）、rsproxy sparse URL 失效、tuna 不镜像 cargo 二进制、ghproxy 停运
**修订**:
- **D206 部分修订**：apt/rustup 清华镜像保留；cargo 镜像 tuna → rsproxy（tuna 只镜像 index，.crate 二进制 404）
- **D207 机制修订**：FROM 预构建 base → compose `image:` + `pull_policy: always`（本地零构建，命名卷 copy-on-first-use 灌入烘焙产物）；镜像命名统一 `audemsp-server-dev` / `audemsp-server-builder`
**关键约束**（审核修正，实施时强制执行）:
- 卷 copy-on-first-use 仅空卷生效 → 落地必须显式 `docker volume rm audemsp_cargo-cache`
- 预烘焙镜像 amd64 only，Apple Silicon 走仿真，dev service 显式声明 platform
- GHCR 清理 workflow（sha tag 保留 N=10）+ path-filter（仅依赖变更时推 dev 镜像）
- ghcr 可达性未实测前不实施预烘焙（PIT-14/31 背景下可能 30min+）
- 生产 runtime 缺口是 admin dist 产物（非 feature）→ Docker 构建需 `pnpm build:admin` 先于 cargo build（PIT-23）
**影响**: 本地首次构建 15-28 min → 2-5 min（预计）；日常增量每轮省 30-60s。实施细节见 docs/reference/codec/build-optimization-strategy.md。

---

## D209: 项目重命名 OMSPBase → AUDEMSP (2026-08-03)

**决策**: 项目对外名称与全部标识符统一由 OMSPBase 更名为 AUDEMSP（AUDE 生态多媒体系统）。范围：crates/ 7 个目录与包名（audemsp-*）、Rust 代码标识符（281 处 import 路径）、环境变量（OMSPBASE_PSK→AUDEMSP_PSK）、Docker 镜像/服务/卷名、www npm 包名、docs（73 文件）+ .agents 记忆/规则/技能（20 文件）+ README/AGENTS.md + 脚本/CI（含 /opt/omspbase→/opt/audemsp 及 oomspbase 笔误修正）。
**日期**: 2026-08-03
**原因**: 项目归属 AUDESYS/AUDEBase 生态，统一 AUDEMSP 命名消除「OMSPBase 是独立项目」歧义，与生态命名体系一致。团队 4 分析师交叉验证（217 文件/2363 处）。
**例外（保留原名）**: decisions-archived.md 历史档案（174 处实测旧名引用）、git 历史/commit 消息、.omo/.sisyphus 归档快照、node_modules 生成物。
**影响**: ① 改名后 Docker 镜像层缓存全部失效（路径变化），首次构建回滚全量编译（一次性成本）；② 旧 env（OMSPBASE_PSK）与 localStorage 键失效——项目未发布，接受破坏；③ git mv 保留历史，单 commit 可 revert 回滚；④ 后续所有文档/命令使用 audemsp-* 命名。


## D210: 帧时间戳锚定单调真实时钟 (2026-08-05)

**决策**: write_raw_i420 的 VideoFrame 时间戳用 `ts_base_us(SystemTime 锚点) + Instant::elapsed()`（锚定单调），废弃假时钟（+33333us 固定步进）与裸 SystemTime::now()。

**原因**: 假时钟与 livekit TimestampAligner（delta-preserving，映射到 wall-clock 时间域）不一致 → 编码器帧率估计异常 → 停摆（PIT-63，T2.5 假设验证门证实）；裸 SystemTime 非单调（NTP 跳变 → ts 倒退）。

**影响**: 帧时间戳真实化是相机接入（V4L2 buffer timestamp）的前提；`write_raw_i420_with_ts` 参数化留口（T4）。

## D211: 帧率必须匹配 libwebrtc 编码器配置 — 帧循环绝对时间轴 (2026-08-05)

**决策**: Host 帧循环用绝对时间轴（`sleep_until(next); next += 33ms;`），禁止"固定 sleep + 耗时操作"模式；帧率目标 = libwebrtc 编码器配置（30fps）。

**原因**: SquaresPattern::draw 耗时 7-17ms 拖慢固定 sleep 循环 → 实际 ~20fps ≠ 配置 30fps → 编码器 rate control 异常（PIT-64）。OpenCTK RepeatingTask 同机制（审核评估的 tokio sleep_until 等价落地）。

**影响**: 任何视频源（生成器/相机）接入必须保证帧率匹配；C17 约束固化；E2E 连跑不稳定（PIT-65）为剩余问题。

## D212: docs/reference Diátaxis 重组 + 计划体系清理 (2026-08-06)

**决策**: ① `docs/reference/` 按 **Diátaxis 框架**重组——活参考（Reference，按产品模块镜像 webrtc/ codec/ + 根目录平铺）与调研存档（Explanation，`research/<领域>/`）分离，README 作唯一索引（C19 约束固化）。② codec 验收标准从 `docs/sdd/` 迁入 `.sisyphus/plans/audemsp-codec/`（pre-implementation 产物归计划区）。③ 计划体系收敛为单一权威源 `.sisyphus/plans/`——移除已全部完成的 `video-framepipeline-hardening`（.sisyphus+.omo 双副本）、去重 `.omo/plans/phase3-production` 副本、清理空 `.omo/plans/` 目录。

**原因**: ① 原 34 篇平铺 + 领域子目录重叠，混入 28 篇一次性竞品调研 → 活参考被污染（Diátaxis"按用途分离"原则，参考对齐 VitePress/Docusaurus 主流）；② acceptance 是 Phase 2 规划产物，Phase 1 sdd/ 目录放它格格不入（未编号+内容形态+Draft 状态不符）；③ 已完成计划/重复副本是死重，`e2e-acceptance-matrix.md` 断链暴露内容已内部化。

**影响**: 文档按用途可预测；计划唯一权威源，无重复无死链；历史调研保留在 `research/` 不碍事。保留：`phase3-production`（Phase 编号约定被 5 篇文档引用）、`host-sfu-w3c-alignment`（活跃待办，C18 待实施）。

**验证**: `ls docs/reference/` 顶层 = README + webrtc/ + codec/ + janus-gateway.md + research/；`find .sisyphus/plans/` 剩 3 个计划；无 `e2e-acceptance-matrix` 断链残留。

## D213: Agent 上下文爆炸治理 — instructions 瘦身 + 模型容量 + .agents 精简 (2026-08-06)

**决策**: 针对「当前项目配置容易上下文爆炸满」，实施三层治理：
1. **instructions 瘦身**：`.opencode/opencode.json` 的 `instructions[]` 移除 `pitfalls.md`（59KB 历史调试日志，占原 19 文件 130KB 的 46%）→ 改为按需读取（`read`/`grep` 查询），保留 18 文件（~70KB）。
2. **模型容量**：全局 `~/.config/opencode/opencode.jsonc` 将 premium-max-1（256K）与 premium-2（205K）的 `limit.context` 提升至 1024K，使 premium-max/-1/-2、premium/-1/-2 六模型全部 1024K（原 premium-2 fallback 是当前会话实际模型，205K 减去 40-50K 静态基线即紧张）。
3. **.agents 精简**：删 `rules/zh/`（11 文件，common 的中文翻译副本，C7 已声明不重复加载）；瘦身 `skills/book-to-skill/`（删 docs/.github/tests/tools/CHANGELOG 等，956KB→192KB，保留运行必需的 SKILL.md+scripts+book_to_skill 包）。**非项目语言规则保留**（用户明确改口，不删 cpp/csharp/dart/golang/.../swift）。

**原因**: ① 静态基线 ~60-100K tokens 主要来自 instructions 全量注入（pitfalls 59KB 是最大单一文件）+ 重复 codegraph MCP（oh-my-opencode 自动注册 `codegraph` + 项目 `local-codegraph` 双实例）+ 插件注入块；② premium-2 205K 上下文偏小，静态基线占比过高；③ zh/ 与 common/ 内容重复违背 C7；book-to-skill 混入完整 Python 库属异常膨胀。

**影响**: ① 每轮静态上下文估算 -59KB（pitfalls）+ 去重 codegraph，估算节省 ~40-50K tokens/turn；② pitfalls 不再常驻，调试时需主动 `read .agents/memorys/pitfalls.md` 查历史坑；③ 配置类变更需重启 opencode 生效；④ `limit.context` 是客户端声明，实际取决于 New API 网关后端是否真有 1024K 窗口。

**验证**: `python3 -c "import json; json.load(open('.opencode/opencode.json'))"` 通过；`node` 字符串感知注释剥离后 JSON.parse 通过（opencode.jsonc / oh-my-openagent.jsonc / ~/.config/opencode/opencode.jsonc 均有效）；六模型 `limit.context` 均 1024000；`.agents/` 从 1.7MB → 984KB。

## D214: audemsp-webrtc 补全 W3C API 面 + Host SFU 标准协商 (2026-08-06)

**决策**: ① 补全 audemsp-webrtc 的所有 W3C API 接口——新增 `RTCRtpTransceiver`/`RTCRtpTransceiverInit`/`RTCRtpTransceiverDirection`/`RTCRtpCapabilities`/`RTCRtpCodecCapability`/`RTCRtpHeaderExtensionCapability` 类型，`RTCRtpParameters` 补 `mid`、`RTCRtpEncodingParameters` 补 `codec`/`dtx`；PcBackend trait 扩展 19 个同步方法（get_transceivers/add_transceiver(+track 版)/sender-receiver get_parameters/capabilities/restart_ice/config/descriptions/transceiver 对象方法）；PeerConnectionApi + RTCPeerConnection 包装层同步扩展；RTCRtpSender 加 backend 句柄实现 get_parameters 等 W3C 对象方法。② Host SFU produce 走标准协商（add_transceiver_with_track → create_offer → set_local → get_sending_rtp_parameters → produce），删除 sfu_media.rs 的 build_remote_sdp/negotiated_ssrc_from_sdp/build_produce_rtp_parameters（C18 检查 src/ 无残留）。

**原因**: ① 用户要求尽量补全 W3C API（团队审核 + MDN spec 对标确认）；webrtc-sys 0.3.x FFI 已暴露 ~95% 接口无需新 C++；仅 RTCDTMFSender/identity 等缺 FFI 标注未来实现。② PIT-65 黑屏根因是 Host 绕过标准协商手工构造 SDP——对齐官方 mediasoup-client/Handler.cpp 标准流程。

**影响**: ① Host produce rtp_parameters 从 transceiver.sender.get_parameters() 推导（含 ssrc/PT），非手工硬编码；② 三后端（webrtc-sys/webrtc-rs/stub）对称实现，stub 状态化；③ 无法实现的 API（DTMF/identity/浏览器专属）在 docs/reference/webrtc/webrtc-w3c-alignment.md §5 标注未来实现；④ webrtc-sys 下 w3c_api_tests 有 4 个预存失败（ice/sdp 测试假设 stub 宽松状态机，非本次改动）；⑤ client crate 预存 feature 不匹配（用 webrtc-rs 方法却配 webrtc-sys feature），待 P4 回归处理。

**验证**: `cargo test -p audemsp-webrtc` (stub 46 passed) + `cargo test -p audemsp-webrtc --features backend-webrtc-sys` (除 4 预存失败全过) + `cargo check -p audemsp-host` 通过 + C18 检查 `grep build_remote_sdp src/` 无残留。

## D215: client P2P 迁移到通用 W3C API — 修复 feature 不匹配 (2026-08-06)

**决策**: ① audemsp-webrtc 加通用 `RTCPeerConnection::on_data_channel`（三后端，替代 webrtc-rs cfg 专属版），webrtc-sys observer 的 on_data_channel 接线到 callbacks。② client `webrtc_transport.rs` 迁移：`on_data_channel` 用通用版（`Fn(RTCDataChannel)` 异步 spawn spool），删 `from_webrtc`；`on_ice_candidate_native` → 通用 `on_ice_candidate`；`handle_ice` 用本地 serde struct 解析 camelCase ICE JSON（替代 webrtc-rs `RTCIceCandidateInit` 类型）。

**原因**: client Cargo.toml 配 `backend-webrtc-sys`（C12 webrtc-sys 为主），但 `webrtc_transport.rs` 仍用 webrtc-rs 专属方法（`on_data_channel`/`from_webrtc`/`on_ice_candidate_native`，均 `#[cfg(backend-webrtc-rs)]`）→ 编译失败。历史遗留：client 代码是 webrtc-rs 时代写的，未随 C12 迁移，且 audemsp-webrtc 之前只对 webrtc-rs 暴露这些方法。

**影响**: client 现可编译（5 crate 全通过）；通用 on_data_channel 补全了 W3C 接口面；webrtc-rs cfg 专属 on_data_channel 删除（被通用版取代）。client P2P 收帧走 webrtc-sys DataChannel（future: spool 需 webrtc-sys 实现，当前 stub）。

**验证**: `cargo check -p audemsp-client` 通过（之前 E0433/E0599 失败）+ `cargo check -p audemsp-host -p audemsp-client -p audemsp-webrtc -p audemsp-common -p audemsp-media` 全通过。
