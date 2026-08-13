---
name: code-simplification
description: "Reduce complexity in MediaServo Rust/TS code. Chesterton's Fence analysis, Rule of 500 enforcement, dead code elimination, borrow checker simplification patterns. Complements ponytail for Rust-specific over-engineering. Use after ponytail-audit, before PRs, or when diagnosing complexity smells."
---

# code-simplification — 复杂度消减

> Chesterton's Fence + Rule of 500 + Rust borrow checker patterns.
> 只删该删的。不删不理解的。

## 触发条件

- 用户说"太复杂"、"简化"、"refactor"、"dead code"
- ponytail-audit 发现可删除项后
- 文件超过 500 行 (`wc -l` > 500)
- `cargo clippy -- -W clippy::cognitive_complexity` 报高复杂度
- 函数超过 50 行 (`wc -L` > 50 per function)

## Chesterton's Fence 流程

```
发现复杂度 → 查原因 → 有合理原因? → YES: 留，加注释 / NO: 删
```

### 查原因工具

| 来源 | 命令 |
|------|------|
| git blame | `git blame -L <line>,<line> <file>` |
| commit 信息 | `git log --all -S "<code>" --oneline` |
| issues/PRs | `gh search issues "<keyword>" --repo $(git remote get-url origin)` |
| ADR 决策 | `.agents/memorys/decisions.md` |
| 社区参考 | `grep_app_searchGitHub` |

### 栅栏注释

保留有原因的复杂度时，加注释标记：

```rust
// chesterton: BufferPool 使用 unsafe 零拷贝，因 GStreamer appsink 输出
// 的 &[u8] 必须直接转发到 WebRTC TrackLocal 避免 memcpy 延迟。
// 尝试过 safe Vec<u8> 拷贝方案 → 1080p60 丢帧率从 0% → 1.2%。
// git: a1b2c3d "zero-copy buffer pool for 60fps stability"
unsafe {
    pool.copy_to_track(track, data);
}
```

## Rule of 500

| 检查项 | 命令 | 阈值 | 行动 |
|--------|------|------|------|
| 文件行数 | `wc -l <file>` | >500 | 拆分为多个 domain 模块 |
| 函数行数 | `grep -n '^fn ' <file>`, 估算 block | >50 | 提取辅助函数 |
| 参数个数 | `grep -c ','` per fn sig | >5 | 参数合并为 struct |
| 嵌套深度 | 肉眼检查 indent | >4 | 提前 return / `?` 传播 |
| trait impl 数 | `grep -c 'impl.*for' <file>` | >3 | 分离到 impl 子模块 |
| match 臂数 | `grep -c '=>'` per match block | >10 | `enum_dispatch` / 策略模式 |
| pub 暴露数 | `grep -c 'pub' <file>` | >20 | 缩小可见性、细粒度模块 |

### 执行

```
# 1. 扫描超标文件
find crates/ -name '*.rs' -exec wc -l {} + | sort -rn | head -20

# 2. 对每个超标文件应用 Chesterton's Fence
# 3. 拆分、提取、删除
# 4. 验证: cargo clippy -- -D warnings && cargo test -p <crate>
```

## MediaServo 特定模式

### 借用检查器简化

```rust
// BEFORE: 不必要的 Arc<Mutex<>> 嵌套
let data: Arc<Mutex<Vec<Arc<Mutex<Option<Box<dyn Trait>>>>>>> = ...;

// AFTER: 单一所有权，按需借用
let mut data: Vec<Box<dyn Trait>> = Vec::new();
// 需要共享时: let data = Arc::new(RefCell::new(data));
// ponytail: RefCell 单线程够用，真正多线程时再换 RwLock
```

### WebRTC backend 抽象层

```rust
// BEFORE: 每个 backend 重复的 glue code
#[cfg(feature = "backend-webrtc-rs")]
fn create_pc() -> RTCPeerConnection { ... }
#[cfg(feature = "backend-webrtc-sys")]
fn create_pc() -> RTCPeerConnection { ... }
#[cfg(feature = "backend-stub")]
fn create_pc() -> RTCPeerConnection { ... }

// AFTER: 提取共性到 shared 模块, backend 只实现差异
// chesterton: 三 backend cfg 重复是 D15 决策的架构代价，
// 不能消除但可以压缩到 backend/ 子模块中最小化。
```

### 配置路径简化

```rust
// BEFORE: 深层嵌套 config
let port = config.server.webrtc.ice.transport.port.unwrap_or(9800);

// AFTER: 扁平化 + 合理默认
let port = env::var("OMSP_ICE_PORT").ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(9800);
```

## 与 ponytail 的关系

| ponytail | code-simplification |
|----------|---------------------|
| 全局审计 → 排序 | 单文件/模块深入 |
| "这个能删吗？" | "这个为什么存在？" |
| 删除决策 | 拆分+重构决策 |
| 一行一发现 | 结构性重构 |

**工作流**: `/ponytail-audit` → 排序 → `/code-simplification` 逐个处理

## 验证门禁

```bash
# 1. 编译通过
cargo check --workspace --all-features

# 2. Clippy 零警告
cargo clippy --workspace --all-features -- -D warnings

# 3. 测试全过
cargo test --workspace

# 4. 文件行数下降
git diff --stat | grep -E '\+[0-9]+.*-' | tail -5

# 5. 无新增 pub API（简化不应扩大 public surface）
# ponytail: 只检查 pub fn 数量，不强制，重构可能自然新增
```

## 反模式检测

| 反模式 | 检测命令 | 自动化修复 |
|--------|---------|-----------|
| `unwrap()` 未注释 | `grep -r 'unwrap()' crates/ --include='*.rs' \| grep -v '//.*unwrap'` | 替换为 `?` 或 `.context()` |
| `clone()` 为满足 borrow | `grep -r '\.clone()' crates/ --include='*.rs'` | 分析借用链 |
| `Box<dyn Trait>` 可以泛型 | clippy `boxed_local` lint | 改为 `impl Trait` |
| Feature-gated dead code | `cargo deadlinks` / `cargo udeps` | 移除 dead feature |
| 重复 impl block | 肉眼检查 | 合并或提取 macro |
| `as` 类型转换 | `grep -r ' as ' crates/ --include='*.rs'` | 优先 `.into()` / `.try_into()` |

## 输出格式

```
## 简化报告

文件: crates/mediaservo-webrtc/src/backend/mod.rs
检测前: 487 行 → 检测后: 312 行 (-36%)

### 移除
- [行 45-78] 未使用 trait `LegacySdpParser` → 删除 (git blame: D78 引入, D112 废弃)
- [行 203-206] 死代码 `#[cfg(all(nonexistent, feature = "..."))]`

### 拆分
- [mod.rs] → `backend/webrtc_rs.rs`, `backend/webrtc_sys.rs`, `backend/shared.rs`

### 保留 (Chesterton's Fence)
- [行 180-195] `#[cfg]` 三 backend 重复 — D15 架构代价，不能消除
- [行 300] `unsafe` buffer pool — 性能约束，见注释

### 验证
✅ cargo clippy -- -D warnings
✅ cargo test -p mediaservo-webrtc
```

## 禁止

- 删除不理解的代码
- 合并不相关的模块
- 简化 `unsafe` 块而不理解内存语义
- 在无 git blame 的情况下移除"看起来没用"的代码
- 删除 feature-gated 代码而不先确认 CI variant
