# Host 多进程架构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将单进程 mediaservo-host 重构为 OxMgr 管理的多进程架构（host-capturer/streamer/recorder/controller/emergency/audio/agent + host 薄封装 CLI），实现崩溃隔离、单 WS 信令总线、link 授权与顺序无关健壮性。

**Architecture:** 一个 crate 多 bin（共享 lib）+ FrameBus(I420) 媒体总线 + host-agent 信令网关（单 WS 聚合）+ OxMgr 进程管理 + host.toml 业务配置经翻译器生成 oxfile.toml。外部 ROS 节点经 link SDK（单文件自描述令牌）接入。Server 侧零改动（Phase D 之前），P2P 模式先行验证控制 DC。

**Tech Stack:** Rust (edition 2024), iceoryx2 (FrameBus), tokio, mediaservo-webrtc (webrtc-sys 后端), OxMgr (进程管理器), ed25519-dalek (link 令牌)

**Spec:** `docs/superpowers/specs/2026-08-18-host-multiprocess-design.md`（14 决策 D-H1~H14——本计划从 spec 论证，执行者必须同时阅读 spec 与计划）

## Global Constraints

- 所有跨进程交互顺序无关（D-H14）：本地验签 / open_or_create 双向兼容 / WS 重连 / 停滞检测
- 进程命名 host- 前缀族（D-H5）：host-capturer/host-streamer/host-recorder/host-controller/host-emergency/host-audio/host-agent/host
- 实例参数化：`host-capturer --camera cam0`（一个二进制 N 实例，D-H13）
- 配置单一来源：host.toml（业务）→ 翻译器 → oxfile.toml（进程语义）——oxfile 是产物非源码（D-H9）
- C21 依赖方向：host 族进程禁止依赖 mediaservo-server / mediasoup；SFU 集成走 WS 信令协议
- C22：host 原生运行（macOS/Linux 宿主），禁止 Docker 内运行 host 测试
- Rust 纪律：无 unwrap 生产、thiserror 库错误、每个 unsafe 有 SAFETY 注释、clippy -D warnings
- link 令牌：单文件自描述（verifying key + claims + signature），文件权限 0600（D-H10/D-H13）
- 每阶段独立可验证 + commit（TDD：先测试 RED → 实现 GREEN → 重构 → commit）

---

## 阶段划分总览

| Phase | 名称 | 交付物 | 验证 |
|---|---|---|---|
| **A** | 底座骨架 | lib + 8 bin 薄壳 + host CLI 雏形 + OxMgr 接入 | host init/start → 8 进程全起全退 |
| **B** | link 增强 | SignalClient 重连 + 单文件自描述令牌 + ros_bridge 导出 | 单元测试 + 断线重连 e2e |
| **C** | 媒体拆分 | capturer(采集→FrameBus) + streamer(订阅→推流) + recorder(订阅→落盘) | 多进程闭环 e2e（替代单进程）|
| **D** | 信令总线 | host-agent WS 网关（本地聚合 + 远端单 WS）| 一车一会话 e2e（Server 零改动）|
| **E** | monitor | 拓扑/数据流/信令状态监控 + 期望态对比 + 上报 | monitor e2e（杀进程验证告警）|
| **F** | 控制/急停 | host-controller（DC 多 label）+ host-emergency（本地兜底）| P2P DC 控制 e2e |
| **G** | 安全 | 设备凭证 + 会话 token（Server）+ 令牌签发流程 | 认证/授权 e2e |
| **H** | Server 扩展 | 音频房间 + data 域 + dispatcher 权限 | 音频会议 e2e + 调度端面板 |
| **I** | 部署包 | host/sdk 双包 + install host + 配发 | 干净机安装验证 |

**依赖链**：A → B → C → D → E → F → G → H → I（F 的 DC 控制先 P2P 验证，SFU data 域在 H 补齐）

执行策略：**每阶段在开始前由执行会话细化为本计划同格式的 bite-size 任务**（本计划给出阶段级任务清单与接口契约；A 阶段已细化到步）。每阶段结束跑全量回归（现有 e2e_sfu 4/4 + codec_prefs 6/6 + host 9/9 不回归——C 阶段开始后 host 9/9 适配为新形态）。

---

## Phase A: 底座骨架（细化到步）

**目标**: crates/mediaservo-host 重构为 lib + 8 bin 薄壳（每 bin 仅"解析配置→日志初始化→运行占位→优雅退出"），host CLI 生成 oxfile 并驱动 OxMgr 起停全部进程。不迁移任何媒体功能（现有单进程功能保持不动直到 Phase C——**lib.rs 先只放共享基建，main.rs 原功能暂存为 host-legacy bin**）。

**文件结构**:
- `crates/mediaservo-host/src/lib.rs`（新建）：配置解析（host.toml schema 初版）、日志初始化、进程公共工具
- `crates/mediaservo-host/src/bin/host.rs`（新建）：薄封装 CLI（init/status/start/stop/apply/doctor/version——Phase A 实现 init/status/start/stop，apply/doctor 在 Phase C/E 补）
- `crates/mediaservo-host/src/bin/host-agent.rs` 等 7 个 bin 薄壳（Phase A 占位：启动 → 打印角色 → 等 SIGTERM → 退出码 0）
- `crates/mediaservo-host/src/main.rs`（改名 `src/bin/host-legacy.rs`）：现有 770 行单进程功能原样保留（Phase C 迁移后删除）
- `crates/mediaservo-host/Cargo.toml`：`[[bin]]` 段 × 9（host-legacy 保留）+ lib 段

### Task A1: lib + bin 骨架

**Files:**
- Create: `crates/mediaservo-host/src/lib.rs`
- Create: `crates/mediaservo-host/src/bin/host.rs`, `host-agent.rs`, `host-capturer.rs`, `host-streamer.rs`, `host-recorder.rs`, `host-controller.rs`, `host-emergency.rs`, `host-audio.rs`
- Modify: `crates/mediaservo-host/Cargo.toml`, rename `src/main.rs` → `src/bin/host-legacy.rs`

**Interfaces:**
- Consumes: 无（新骨架）
- Produces:
  - `lib.rs`: `pub fn init_logging(role: &str)`, `pub fn run_placeholder(role: &str) -> Result<(), Box<dyn std::error::Error>>`（打印角色 + 阻塞等 SIGTERM/SIGINT → Ok）
  - `bin/host.rs`: `host init`（生成 `etc/host.toml` 模板）、`host status`（调 oxmgr list 解析输出）、`host start`（oxmgr apply oxfile.toml）、`host stop`（oxmgr stop namespace host）——CLI 子命令用 `std::env::args` 手工解析（Phase A 不引入 clap）
  - `host.toml` 初版 schema: `[host] device_id = "..."` `[[cameras]] id = "cam0" source = "stub" fps = 30` `[[streams]] id = "cam0-stream" camera = "cam0" codec = "h264"` `[record] enabled = false` `[control] enabled = false`

- [ ] **Step 1: 写失败测试（bin 存在性 + lib 函数）**

`crates/mediaservo-host/tests/multiproc_skeleton.rs`:
```rust
#[test]
fn all_bins_declared() {
    // 读取 Cargo.toml [[bin]] 段：必须含 host, host-agent, host-capturer,
    // host-streamer, host-recorder, host-controller, host-emergency, host-audio, host-legacy
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    for bin in ["host", "host-agent", "host-capturer", "host-streamer",
                "host-recorder", "host-controller", "host-emergency", "host-audio", "host-legacy"] {
        assert!(manifest.contains(&format!("name = \"{bin}\"")), "missing bin {bin}");
    }
}

#[test]
fn placeholder_blocks_and_exits_on_signal() {
    // spawn host-capturer 二进制（env CARGO_BIN_EXE_host-capturer）→ 等 200ms
    // → 断言进程存活（输出含 "capturer placeholder ready"）→ SIGTERM → 退出码 0
}
```

- [ ] **Step 2: 运行验证失败**

Run: `cargo test -p mediaservo-host --test multiproc_skeleton`
Expected: FAIL（bins 不存在 / Cargo.toml 无 [[bin]] 段）

- [ ] **Step 3: 实现骨架**

`Cargo.toml` 增加：
```toml
[lib]
name = "mediaservo_host"
path = "src/lib.rs"

[[bin]]
name = "host"
path = "src/bin/host.rs"
# ... host-agent/host-capturer/host-streamer/host-recorder/host-controller/host-emergency/host-audio 同构

[[bin]]
name = "host-legacy"
path = "src/bin/host-legacy.rs"
```
`git mv src/main.rs src/bin/host-legacy.rs`（原功能不动）。
lib.rs 与各 bin 薄壳按 Interfaces 实现（run_placeholder 用 `tokio::signal::ctrl_c()` + `signal(SIGTERM)` 等待）。

- [ ] **Step 4: 运行验证通过**

Run: `cargo test -p mediaservo-host --test multiproc_skeleton`
Expected: PASS（2 tests）
Run: `cargo build -p mediaservo-host && cargo clippy -p mediaservo-host -- -D warnings`
Expected: 构建成功 + clippy 零警告（现有 host-legacy 代码若已 clippy 干净则保持）

- [ ] **Step 5: Commit**

```bash
git add crates/mediaservo-host/
git commit -m "refactor(host): lib + 9 bin 骨架（host CLI + 7 进程占位 + host-legacy 保留）"
```

### Task A4: 外部脚本适配 + 接口契约记录（A1 审查 I1/I2）
- **Files**: `scripts/e2e-test.sh:47`（`--bin mediaservo-host` → host-legacy）、`scripts/mediaservo_cli.py:358-373`（run-host/stop-host 二进制路径 + pkill）、`scripts/install.sh:11`（BIN_NAME）、`scripts/run-e2e-sfu.sh:24`（pgrep）——4 文件全部更新为 host-legacy 或新 CLI
- **接口记录**: `run_placeholder` 为 **async** 签名（brief 原文 sync 为漂移——实现以 `pub async fn run_placeholder(role: &str)` 为准，tokio 信号所需）；`init_logging(role)` 同步
- **验证**: `./mediaservo.sh e2e`（9/9）+ run-host/stop-host 冒烟 + install.sh 安装冒烟

### Task A2: host CLI init/status/start/stop + oxfile 翻译器雏形

**Files:**
- Modify: `crates/mediaservo-host/src/bin/host.rs`
- Create: `crates/mediaservo-host/src/translate.rs`（lib 模块：host.toml → oxfile.toml 字符串）
- Test: `crates/mediaservo-host/tests/translate.rs`

**Interfaces:**
- Consumes: A1 的 lib.rs（init_logging）
- Produces:
  - `translate.rs`: `pub fn to_oxfile(cfg: &str) -> Result<String, String>`——输入 host.toml 内容，输出 oxfile.toml 文本（apps 含 8 个 host 进程 + 每 camera 实例化 capturer + 每 stream 实例化 streamer；Phase A 的 oxfile 只含占位进程与参数化实例骨架）
  - `host init <dir>`: 写 `etc/host.toml` 模板 + 生成 `etc/link/signing.pem`（Ed25519 keypair，0600）+ 空 `etc/link/` 目录
  - `host start --dir <dir>`: 读 etc/host.toml → translate → 写 run/oxfile.toml → 执行 `oxmgr apply run/oxfile.toml`（oxmgr 不在 PATH 时报清晰错误并提示 install）
  - `host stop --dir <dir>`: 执行 `oxmgr stop --namespace host` + `oxmgr delete --namespace host`（幂等）
  - `host status --dir <dir>`: 执行 `oxmgr list` 并过滤 host 命名空间，输出每进程状态表
  - 进程数: 7 类型 + 参数化实例（agent + capturer×N + streamer×N + recorder + controller + emergency + audio）——host CLI 本身非进程

- [ ] **Step 0: 确认 OxMgr CLI 动词与 oxfile 格式（C11/C18）**

按 C11 读 OxMgr 官方文档（README 已确认 `oxmgr apply`/`oxmgr list`/oxfile `version=1 + [[apps]]` ✓；**未确认**: `oxmgr stop --namespace host` / `oxmgr delete --namespace host` 语法——官方 docs/CLI.md 或 AI Skill Reference docs/SKILL.md 核对后定，冒烟测试按确认后的动词实现）

- [ ] **Step 1: 写失败测试**

`tests/translate.rs`:
```rust
#[test]
fn to_oxfile_emits_all_placeholder_apps() {
    let cfg = "[host]\ndevice_id = \"car-01\"\n[[cameras]]\nid = \"cam0\"\nsource = \"stub\"\nfps = 30\n[[streams]]\nid = \"cam0-stream\"\ncamera = \"cam0\"\ncodec = \"h264\"\n[record]\nenabled = false\n[control]\nenabled = false\n";
    let ox = translate::to_oxfile(cfg).unwrap();
    for app in ["host-agent", "host-capturer", "host-streamer", "host-recorder",
                "host-controller", "host-emergency", "host-audio"] {
        assert!(ox.contains(&format!("name = \"{app}\"")), "missing {app}");
    }
    assert!(ox.contains("host-capturer --camera cam0")); // 参数化实例
    assert!(ox.contains("host-streamer --stream cam0-stream"));
}
```

- [ ] **Step 2: 运行验证失败**

Run: `cargo test -p mediaservo-host --test translate`
Expected: FAIL（translate 模块不存在）

- [ ] **Step 3: 实现翻译器 + CLI 子命令**

translate.rs：手写 oxfile.toml 文本生成（对齐 OxMgr oxfile 格式：`version = 1` + `[[apps]]` name/command/restart_policy="always"）。
host.rs：args 匹配 "init"/"start"/"stop"/"status"/"version"（未知子命令打印用法退出码 2）。

- [ ] **Step 4: 运行验证**

Run: `cargo test -p mediaservo-host --test translate`
Expected: PASS
Run（手动冒烟，oxmgr 需已安装）:
```bash
cargo build -p mediaservo-host
rm -rf /tmp/host-smoke && ./target/debug/host init /tmp/host-smoke
./target/debug/host start --dir /tmp/host-smoke   # 预期: oxmgr apply 成功
./target/debug/host status --dir /tmp/host-smoke  # 预期: 7 进程（模板配置: 5 固定 + capturer + streamer——Momus/A2 审查修正算术）
./target/debug/host stop --dir /tmp/host-smoke    # 预期: 全部停止
```
Expected: 全流程 0 退出码；oxmgr list 显示进程起停

- [ ] **Step 5: Commit**

```bash
git add crates/mediaservo-host/
git commit -m "feat(host): host CLI init/start/stop/status + host.toml→oxfile 翻译器（A2）"
```

### Task A3: OxMgr 检测与优雅降级

**Files:**
- Modify: `crates/mediaservo-host/src/bin/host.rs`

**Interfaces:**
- Consumes: A2 CLI
- Produces: `host doctor`（Phase A 最小版：检查 oxmgr 存在/版本、host.toml 可解析、oxfile 可生成）

- [ ] **Step 1: 写失败测试**

`tests/doctor.rs`: 无 oxmgr 环境下 `host doctor` 输出含 "oxmgr: not found" 且退出码非 0（测试内 `which oxmgr` 跳过——CI 可能无 oxmgr）

- [ ] **Step 2-4: 实现 + 验证**（doctor 子命令：which oxmgr → host.toml 解析 → translate → 输出检查表；退出码 = 失败项数）

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(host): host doctor 环境诊断（oxmgr/配置/翻译三检查）"
```

### Phase A 验收

- [ ] `cargo test -p mediaservo-host` 全绿（含 host-legacy 既有测试不回归）
- [ ] 冒烟：host init → start → status（8 进程）→ stop 全流程
- [ ] 现有 host 9/9 E2E 不回归（host-legacy bin 名适配：e2e 脚本里二进制路径更新为 host-legacy）
- [ ] `host doctor` 在无 oxmgr 机器输出清晰指引

---

## Phase B: link 增强（阶段级任务清单）

**目标**: link SDK 三项增强，独立可测，不依赖 host 重构。

### Task B1: SignalClient 重连（指数退避 + jitter）
- **Files**: `crates/mediaservo-link/src/signal.rs`（connect 增加重连参数 `RetryConfig { max_retries, base_delay, max_delay }`，复用 coding-style retry_with_backoff 模式）、`tests/` mock WS server 断线测试
- **接口**: `SignalClient::connect_with_retry(cfg: RetryConfig) -> Result<SignalSession, LinkError>`；`SignalSession` 增加 `on_disconnect(cb)`（断线通知，供上层触发重连）；**重连后重新认证**（设备凭证/session token → 会话恢复——spec D-H14）
- **测试**: mock server 先拒后收 → 重连成功；连续拒 → 退避次数上限报错
- **验证**: `cargo test -p mediaservo-link` + clippy

### Task B2: 单文件自描述令牌
- **Files**: `crates/mediaservo-link/src/token.rs`（`TokenFile` 格式：verifying key + claims + signature 合并；`CapabilityToken::to_file()/from_file()`）、`tests/token_file.rs`
- **接口**: `TokenFile::encode(&CapabilityToken, &Ed25519VerifyingKey) -> Vec<u8>`；`TokenFile::decode(&[u8]) -> (Ed25519VerifyingKey, CapabilityToken)`（decode 内验签）
- **测试**: roundtrip；篡改任一字节验签失败；错误长度报错
- **验证**: `cargo test -p mediaservo-link`

### Task B3: host init 导出 ros_bridge.yaml
- **Files**: `crates/mediaservo-link/src/bridge.rs`（新：从 host.toml 相机/流清单生成 ros_bridge.yaml）+ host CLI `host init` 接线
- **接口**: `pub fn ros_bridge(cameras: &[String], streams: &[String], token_path: &str) -> String`（yaml 文本：topics + token_path）
- **测试**: 输出含全部 camera/vision topic + token_path
- **验证**: 单测 + `host init` 冒烟

---

## Phase C: 媒体拆分（阶段级任务清单）

**目标**: 现有单进程媒体功能迁移为进程对（capturer → FrameBus → streamer/recorder），deck closed_loop 模式多进程化。

### Task C1: capturer 进程（采集 → FrameBus 发布）
- **Files**: `crates/mediaservo-host/src/bin/host-capturer.rs`（真实实现替换占位）：`--camera cam0` 参数 → 读 host.toml 相机配置 → 复用 mediaservo-media VideoFrameGenerator（stub 彩条起步，USB/MIPI 源后接）→ link FrameBus publish（token 从 `--token <path>` 加载）
- **接口**: 依赖 mediaservo-link（C12/C21 允许——link 是 SDK 底座）；发布 topic `camera/cam0`（命名规范固定）
- **测试**: 单进程测试（publish N 帧 → FrameBus 订阅端收到 + meta 正确）；CLI 参数解析测试

### Task C2: streamer 进程（订阅 → 编码 → 推流）
- **Files**: `crates/mediaservo-host/src/bin/host-streamer.rs`：`--stream cam0-stream` → 订阅 `camera/cam0` → 编码（复用 host 现有编码逻辑 + webrtc TrackSender）→ 推流（P2P 模式先行：信令仍走原 WS 直连——Phase D 前不引入总线）
- **接口**: 迁移 host 现有 sfu_media.rs/webrtc_transport.rs 的推流段；P2P 信令复用 field PushSession 模式（C21 外部 server 交互）
- **测试**: e2e（capturer + streamer 双进程 → 外部 server 收流 bytes_sent>0——对齐 field push_e2e 6/6 模式）

### Task C3: recorder 进程（订阅 → 落盘）
- **Files**: `crates/mediaservo-host/src/bin/host-recorder.rs`：订阅 camera/* → deck Recorder（复用——deck 已实证 FrameBus→Recorder 闭环）
- **测试**: 闭环 e2e（capturer → FrameBus → recorder 落盘 → ffprobe 验证 h264/moov）

### Task C4: e2e 适配
- **Files**: `scripts/`（host 9/9 E2E 脚本：`host start` 起多进程对替代单进程 host）
- **验证**: 全量回归（e2e_sfu 4/4 + codec_prefs 6/6 + 新多进程闭环）

---

### Task C5: 崩溃重启故障注入 e2e（Momus MEDIUM-3——架构核心价值验证）
- **Files**: `crates/mediaservo-host/tests/crash_recovery.rs`
- **测试**: 杀 capturer 进程 → OxMgr 拉起（restart_policy=always）→ **同 topic 重发布成功**（max_publishers(1) + iceoryx2 残留 service 不阻塞——若失败需生产级 SHM 残留清理机制，不能靠人工 rm，C25）→ 订阅端（streamer/recorder）恢复收帧 + monitor 无持续告警
- **验证**: `cargo test -p mediaservo-host --test crash_recovery` + 手动冒烟

## Phase D: 信令总线（阶段级任务清单）

**目标**: host-agent WS 网关——各进程 WS 连本地 127.0.0.1:PORT，agent 聚合单 WS 上 Server（D-H6）。

### Task D1: agent 网关核心（Momus HIGH-1 修正：协议语义必须先落定）
- **Files**: `crates/mediaservo-host/src/bin/host-agent.rs`（真实实现）：本地 WS accept（tokio-tungstenite）→ 远端单 WS（SignalClient 重连——B1 产物）→ 双向转发 + 响应路由
- **协议语义（实证依据——Momus 已核对 signaling.rs）**:
  - (a) **RoomJoin 拦截**: 各本地进程的 RoomJoin 由 agent 拦截（不再逐进程上行——signaling.rs 实证 RoomJoin 只在建连阶段处理，relay 循环内再收 RoomJoin 被静默丢弃；relay 白名单仅 Sdp/RTCIceCandidate/Frame/EncoderStatus）；agent 以整车身份单次 join
  - (b) **响应路由**: SFU 消息按 msg_peer_id/transport_id 映射回本地连接（signaling.rs 已按此语义实现，PIT-65）；P2P relay 的 Sdp/RTCIceCandidate 无 transport 标识 → 按协商归属追踪（agent 维护 transport↔本地连接映射表）或显式串行化
  - (c) **删除不可行选项**: "远端加 from 前缀"（改变线上格式，破坏 Server 零改动）与"按序复用"（并发协商必错乱）均不采用
- **接口**: 本地协议 = `{src: "host-streamer-cam0", msg: SignalingMessage}`；远端 = 纯 SignalingMessage（零 wire 改动）；agent 维护 `transport_id/peer_id → 本地连接` 映射
- **测试**: mock Server 验证: ① 多本地客户端转发正确 ② RoomJoin 拦截（子进程 RoomJoin 不上行、agent 单次 join）③ **并发协商**（两路同时 create offer → 响应路由不串）④ 断线重连恢复转发

### Task D2: 各进程信令地址配置化
- **Files**: host.toml 加 `[signaling] local_gateway = "127.0.0.1:PORT"`（默认开）+ 各进程 WS 目标从 Server 地址改为本地网关；翻译器传参
- **测试**: D1 mock 全链路 e2e（capturer/streamer 进程经网关连 mock Server）

### Task D3: 一车一会话 e2e
- **验证**: 真 Server（Docker）+ 多进程 host → Server 侧仅 1 个 peer 会话 + 多 transport produce 全通；现有 e2e_sfu 4/4 回归

---

## Phase E: host-agent 监控（阶段级任务清单）

**目标**: 拓扑/数据流/信令状态监控 + 期望态对比 + 上报（D-H4）。

### Task E1: 拓扑监控
- **Files**: `crates/mediaservo-host/src/monitor/topology.rs`：期望态（host.toml 声明：N capturer + N streamer + recorder + controller...）vs 实际态（oxmgr list 进程存活 + 发布者枚举）→ 差异列表
- **发布者枚举数据源决策（Momus MEDIUM-2）**: link 跨进程 discovery 未实现（status.md "跨进程发现留 Phase 2"）——E1 二选一: ① E 阶段新增 discovery 实现任务（iceoryx2 ServiceRegistry 枚举——D-H4 完整兑现）② MVP 降级：仅 oxmgr list 进程级拓扑 + FrameBus 订阅统计（数据平面拓扑延后）。**默认选 ①**（D-H4 声明式期望+发现式实际是监控核心），任务并入 E1
- **测试**: 期望 3 capturer 实际 2 → 报告缺失；grace period 抑制启动窗口

### Task E2: 数据流监控
- **Files**: `crates/mediaservo-host/src/monitor/flow.rs`：FrameBus 订阅统计（每 topic 帧率/字节率/停滞检测——ts_mono 时间戳增量）+ streamer 推流状态（bytes_sent/frames_encoded——webrtc stats）
- **测试**: 模拟发布者变速 → 帧率曲线 + 停滞告警

### Task E3: 信令状态监控 + 上报
- **Files**: `crates/mediaservo-host/src/monitor/signal.rs`：各进程 WS 连接状态（网关持有）+ 远端 Server 连接状态 → 状态面板数据 + 上报 Server（信令扩展消息 StatusReport）
- **测试**: 断连 → 状态更新 + 上报消息内容断言

---

### Task E4: 云端配置闭环（spec §5——Momus HIGH-3 补）
- **Files**: `crates/mediaservo-host/src/bin/host-agent.rs`（扩展：ConfigPush 接收）+ `crates/mediaservo-host/src/bin/host.rs`（补 `host apply`/`host restart` 子命令）
- **接口**: 信令扩展消息 `ConfigPush { config: String, version: u64 }`（PSK/JWT 认证 + 审计日志——C15/C16 纪律）→ 校验 → 写 host.toml（备份旧版）→ 调 `host apply` → 翻译器 → oxfile.toml → OxMgr file-watch 热生效（增删路 = 增量 apply）
- **测试**: mock Server 下发 ConfigPush → host.toml 更新 + oxfile 重新生成 + 进程重启生效；非法配置拒绝 + 审计日志断言

## Phase F: 控制/急停（阶段级任务清单）

**目标**: host-controller（DC 多 label）+ host-emergency（独立 PC + 本地兜底）——P2P 模式先行（D-H3/H8）。

### Task F1: controller 进程
- **Files**: `crates/mediaservo-host/src/bin/host-controller.rs`：纯 DC PC（create_data_channel × 3 label: chassis/gimbal/light）→ 舱端 client 直连（P2P）→ DC 消息路由到执行器接口（Phase A 先 stub 执行器：日志 + 回执）
- **测试**: 双进程 e2e（controller ↔ mock 舱端 PC：DC 消息往返 + label 区分 + reliable 顺序断言；执行器 stub 在 Phase F 本阶段实现——"Phase A 先 stub"为笔误）

### Task F2: emergency 进程
- **Files**: `crates/mediaservo-host/src/bin/host-emergency.rs`：独立 PC + DC（label emergency）+ 本地兜底接口（trait `EmergencyActuator`——stub 实现：日志；CAN/GPIO 实现在 Phase I 后）
- **测试**: 急停消息 → actuator 触发 + 回执；controller 崩溃不影响 emergency（杀 controller 进程验证）

---

### Task F3: streamer 视觉 DC（D-H8 链路——Momus HIGH-2 补）
- **Files**: `crates/mediaservo-host/src/bin/host-streamer.rs`（扩展）：独立 transport B（纯 DC，无 track——mediasoup 官方 send/recv 分离）+ label "vision"；订阅视觉 topic（如 `vision/cam0`，源 = ROS 视觉节点）→ DC JSON 消息（对象数组: class/confidence/bbox/text/color + 帧关联 ts_mono/seq）转发舱端 HMI
- **测试**: ① 外部发布者（ROS 模拟进程，link SDK attach 视觉 topic）→ streamer 订阅 → DC 收到 JSON（D-H7 外部节点验证缺口一并补上）② vision DC 与视频 track 分离 transport 断言（SDP 两 m-line）③ ts/seq 帧关联字段存在

## Phase G: 安全（阶段级任务清单）

**目标**: 设备凭证 + 会话 token（Server 侧）+ 令牌签发流程（D-H10/H11 落地）。

### Task G1: link 令牌签发流程
- **Files**: host CLI `host token issue --role <R> --topic <T> --out <path>`（signing key 读 etc/link/signing.pem）+ ros_bridge.yaml 接线
- **测试**: 签发 → ROS 侧 from_file attach 成功；错误 role/topic 拒绝

### Task G2: Server 设备凭证
- **Files**: `crates/mediaservo-server/src/`（auth 扩展）：device 表（配置/DB：device_id + secret hash）+ Join 设备认证 → 短期 session token（JWT 已有）→ 连接级身份
- **测试**: 认证成功/失败/重连 token 刷新；多设备隔离（车 A token 不能以车 B 身份）

### Task G3: 舱端分级授权
- **Files**: Server 会话/权限层：viewer/operator/admin/dispatcher 角色 + 授权矩阵（consume/control/emergency/config/audio 按角色校验——矩阵见 spec D-H11）
- **测试**: 角色×能力矩阵全组合测试（表驱动）

---

## Phase H: Server 扩展（阶段级任务清单）

**目标**: 音频会议房间 + data 域（SFU 模式 DC）+ dispatcher 拉流权限。

### Task G4: host 设备身份配发（Momus MEDIUM-4——G2 测试的前置）
- **Files**: `crates/mediaservo-host/src/bin/host.rs`（`host init` 生成）+ `crates/mediaservo-host/src/bin/host-agent.rs`（Join 携带）
- **接口**: host init/install 生成 `identity.json`（device_id + device_secret，0600——D-H13 布局）；host-agent Join 认证流程携带设备凭证 → session token
- **测试**: init 生成 → agent Join 成功；凭证错误 → 认证失败明确报错

### Task H1: SFU data 域
- **Files**: `crates/mediaservo-server/src/sfu.rs`：DataProducer/DataConsumer（mediasoup 原生——官方 API 用法，C18）+ 信令消息扩展（produce_data/consume_data）
- **测试**: SFU DC 端到端（controller/vision DC 经 Server 转发——F 阶段 P2P 验证的补充）

### Task H2: 音频会议房间
- **Files**: Server 音频房间管理（join/leave/成员列表/权限——dispatcher 任意车/舱端仅授权车）+ host-audio 进程真实实现（麦克风/扬声器——ALSA/MMAPI + opus codec FFmpeg 后端）
- **测试**: 3 方音频会议 e2e（车端 + 舱端 + dispatcher 浏览器——播放静音帧验证路由）

### Task H3: admin/dispatcher 前端
- **Files**: `www/apps/admin/`（音频面板 + 多车监控视图 + dispatcher 登录角色）
- **验证**: Playwright e2e（browser-testing 技能）

---

## Phase I: 部署包（阶段级任务清单）

**目标**: host/sdk 双包 + install host + 配发脚本（D-H13）。

### Task I1: install host 命令
- **Files**: `scripts/mediaservo_cli.py`（`install host --prefix`：产出 /opt/mediaservo-host 布局——bin 8 + oxmgr 复制/检测 + etc 模板 + run 初始化）+ host init 接线
- **测试**: 干净目录安装 → host doctor 全绿 → start/stop 冒烟

### Task I2: 双包发布脚本
- **Windows 验证（spec §7）**: CARLA 机（x86-Windows）capturer 采集 + 推流最小闭环——打包验证纳入本任务
- **Files**: `scripts/`（打包 mediaservo-host-<ver>.tar.gz + mediaservo-sdk-<ver>.tar.gz——SDK 复用 install bindings 产物）
- **验证**: 解包到干净机验证两包独立可用 + 协议版本声明文件

---

## 风险登记

| 风险 | 缓解 |
|---|---|
| 媒体拆分破坏现有 e2e（9/9）| host-legacy 保留至 Phase C 完成；每阶段全量回归 |
| OxMgr 小众/API 变化 | oxfile.toml 生成集中 translate.rs；oxmgr CLI 调用薄封装（A2 的 _run_or_exit 模式）|
| iceoryx2 多进程 service 参数冲突 | 配置统一在 link API 固定（D-H14）；跨进程测试前置清理（C25）|
| 单 WS 聚合与 Server 协议不兼容 | Phase D 先 mock Server 验证协议包装；真 Server 回归兜底 |
| 音频/控制依赖 Server data 域 | F 阶段 P2P 先行验证；H1 单独可测 |
| C22（host 禁 Docker）| 所有 host 测试宿主原生；Server 仅 Docker |
| host-agent 单点（Momus MEDIUM-5）| 崩溃窗口内信令中断（远程急停通道受影响）——OxMgr 秒级拉起 + 本地兜底缓解；F2 验收含"controller 崩不影响 emergency" |
| P2P 控制 NAT 失败（Momus MEDIUM-5）| 无 TURN 时 P2P DC 可能不可用——F 验收加 ICE 失败降级说明；SFU data 域（H1）为最终兜底，风险窗口 = F→H 阶段 |
