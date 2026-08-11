# Code Edit Safety

> **Target audience**: AI agents editing AUDEMSP source code.
> **Violation of these rules causes token waste from repeated fix cycles.**

## Tool Selection

| Change Size | Tool | Reason |
|-------------|------|--------|
| Rewrite entire function/file | `write` | Guarantees brace balance, no stale lines |
| ≤20 line single-location edit | `edit` | Minimal diff, safe for small changes |
| Structural pattern replacement | `ast_grep_replace` | Syntax-aware, preserves matching |
| Complex multi-file refactor | Delegate to subagent | Isolated context, verify independently |

## Forbidden Patterns

| Anti-Pattern | Why |
|--------------|-----|
| `sed` for code modification | Quote escaping errors, regex silent failures |
| Multiple sequential `edit` calls without re-reading | Line numbers drift, stale hash IDs |
| Deleting a line by replacing with empty `lines: []` and assuming brace count is still correct | May leave unbalanced braces |
| Appending `}` to "fix" an unclosed delimiter without counting braces first | Masks root cause, may create double-close |

## Verify Immediately

After EVERY code change (edit, write, or ast_grep_replace):

```
Rust:   cargo check -p <crate>       (5-15s)
TS/TSX: npx tsc --noEmit             (3-5s)
YAML:   docker compose config --quiet (compose 文件)
Shell:  bash -n <script>             (脚本)
```

### 批量 edits 数组必须逐操作验证 (PIT-41)

多个 replace 操作引用相邻区域时，边界行号/内容易错位（一个操作可能覆盖另一个操作的保留区域）。每次 edit 调用后立即跑对应格式验证；发现破坏 → 重读文件恢复，不叠加修复。

If verification fails, STOP. Do NOT apply another edit on top. Instead:
1. `git diff` to see what changed
2. If the change is wrong, `git checkout -- <file>` to revert
3. Re-apply the fix correctly

## Brace Safety Checklist

Before marking any multi-line edit complete, verify:
- [ ] Every `{` has a matching `}` at the same indent level
- [ ] Every `(` has a matching `)`
- [ ] Every `[` has a matching `]`
- [ ] No duplicate function definitions or closing braces
- [ ] `cargo check` / `tsc --noEmit` passes

## When to Delegate

Delegate to a `deep` category subagent when:
- The change touches 3+ files
- The change requires understanding cross-module dependencies
- You've failed the same edit 2+ times

The subagent gets a clean context, reads the files fresh, and applies all changes atomically.

## Architectural Decision Gate (NON-NEGOTIABLE)

Before implementing ANY architectural change (protocol, data flow, transport mode, API contract):
- **ALWAYS ask the user first** using the `question` tool with explicit options
- **NEVER fall back to an alternative architecture** without user approval
- **NEVER silently switch** from the agreed architecture (e.g., SFU → P2P) even if it seems "easier"
- **NEVER implement a workaround** that changes the system's design without explicit user consent

If the agreed approach fails, report the failure and ask: "方案 X 失败，原因是 Y。建议改用 Z，是否同意？"

## Test Execution Constraint (NON-NEGOTIABLE)

After claiming tests are written or features are working:
- **ALWAYS run the tests** against the live system. Writing test files without executing them is a violation.
- **ALWAYS report actual test output** — pass/fail counts, error messages. Never claim "tests pass" without evidence.
- **E2E tests MUST run against the actual running service**, not mocked endpoints.
- If tests fail, fix them in the same turn. Do not defer to "later".

## Verification Honesty (NON-NEGOTIABLE)

- **NEVER claim a feature works based on a partial test.** A Python WS test passing does NOT mean the browser flow works.
- **ALWAYS verify at the actual user-facing layer.** If the feature is browser-based, test in the browser. If it's API-based, test with curl.
- **ALWAYS report exactly what was tested and what was NOT tested.** Example: "Python WS test passed. Browser flow NOT yet verified."
- **NEVER present a component test as end-to-end proof.** Each layer must be verified independently.
- **If you cannot verify at the user-facing layer, say so explicitly.** Do not imply success.

## Feature Flag Discipline

- **ALL required features MUST be in `default` features** in Cargo.toml — never require manual `--features` for core functionality
- **Build commands in docs MUST include all features** — never document `cargo build` without required features
- **Before running server, ALWAYS verify**: `cargo build -p audemsp-server` (with defaults) produces working binary
- **If a feature is optional, it must be explicitly opt-out** (disable with `--no-default-features`)

## Self-Verification Requirement (NON-NEGOTIABLE)

- **ALWAYS verify browser-based features yourself using Playwright MCP tools** (`local-playwright_browser_navigate`, `local-playwright_browser_evaluate`, etc.)
- **NEVER ask the user to test what you can test yourself.** If Playwright is available, use it.
- **After fixing a browser bug, ALWAYS re-test in the browser** before reporting the fix.
- **Report the actual browser console output** as evidence of verification.

## User Confirmation Before Edit (NON-NEGOTIABLE)

- **NEVER start editing files without explicit user approval.** Describing a plan ≠ approval to execute.
- **When user asks 'what can be done' or 'is it possible to...', they are asking a question, not giving an instruction to edit.** Answer the question. Do NOT edit files.
- **Before editing, present the plan AND use the `question` tool to confirm.** Wait for affirmative response before touching files.
- **Silence / '继续' / timeout ≠ approval.** Only explicit 'yes' / 'do it' / '执行' counts.

## Process Management (shell)

- **NEVER use `pgrep -f` / `pkill -f` with a pattern that matches your own shell command line** (e.g. `pgrep -f "audemsp-host"` from a bash tool whose command string contains that literal) — it kills the shell itself, hanging the tool. Use `pgrep -x <exact-process-name>` (matches process name only, e.g. `audemsp-host`), or exclude own PID.
- **Killing + relaunching in one shell command** can kill the just-started process (SIGTERM/SIGHUP to the process group on tool timeout). Launch with `setsid nohup ... < /dev/null & disown` and verify with `pgrep -x` in a separate call.
- **Port-in-use on relaunch** (e.g. `Failed to bind 0.0.0.0:9801`) almost always means the old process survived the kill — verify with `ss -tlnp | grep <port>` and kill by PID.
- **Container-recreated services lose apt-installed tools** (gdb etc.) — install debug tools in the Dockerfile dev target, not per-container.

**来源**：PIT-54 调试轮 (2026-08-04: pgrep -f 自杀、容器重建丢 gdb)

## Network Tooling (bash)

- **curl 本机服务必须 --noproxy**：bash 环境有 `http_proxy` 时，`curl http://127.0.0.1:5173` 会走代理 → 超时假死（表现为"Vite 无响应"）。用 `curl --noproxy "*" http://127.0.0.1:PORT/`。
- **容器 tcpdump 过滤注意 NAT**：宿主发往容器网段（172.18.0.2）的包源 IP 被改写为网关（172.18.0.1）——`not host 172.18.0.1` 会把本机浏览器/应用的流量一并过滤掉。按**源端口**区分（Host 的固定端口 vs 浏览器随机端口），不要按源 IP。

**来源**：PIT-56/58 调试轮 (2026-08-04)

## Git 恢复操作

- **批量 `git restore <paths>` 恢复已 staged 删除时，可能部分目录工作区未实际写回**——`git ls-files`（index）有文件但磁盘（worktree）为空，grep 该目录无结果。根因：`restore` 对 staged 删除的路径恢复不完整。**优先用 `git checkout HEAD -- <paths>`**（强制从 HEAD 写回工作区）。
- **验证必须是全量对比，不能抽样**：恢复/删除 N 个目录后，逐个 `for d in ...; do echo "[$d] index=$(git ls-files $d/ | wc -l) worktree=$(ls $d/ 2>/dev/null | wc -l)"; done` 核对，index 与 worktree 计数必须全部相等。只 `ls` 部分目录 = 遗漏（PIT-68：恢复 10 个目录仅 7 个实际写回，3 个磁盘为空未被发现）。

**来源**：PIT-68 (2026-08-06 .agents 精简恢复轮)

### 8. edit 工具多行替换后必须验证行唯一性 (PIT-78a)

**规则**: 对 .py/.rs 文件用 edit 做多行替换后，若替换内容含重复模式（相同行），必须 grep 验证唯一性：

```bash
grep -c "重复模式" <file>    # 期望 1；>1 = edit 重复插入
```

**先例**: 2026-08-10 会话内 edit 工具三次异常——① 替换丢失前几行（main.rs 配置路径缩进损坏但语法合法，编译通过但逻辑旧）；② 重复插入分派行（audemsp_cli.py 287/288 相同行 → restart/run-host 执行两轮容器重建）。**修复**: ① 改用 python 精确字符串替换（读文件→replace→写回）；② 删除重复行后 grep -c 验证。

**阻塞条件**: 多行 edit 后未验证唯一性/行数即提交。

### 9. 同区域连续 edit 前必须 grep 现状 (PIT-81 轮)

**规则**: 对同一文件同一函数/区域做连续 edit 时，每次 edit 前先 `grep -c "<锚点行内容>" <file>` 确认唯一性；对"已有内容 + 插入"模式（在旧代码前加日志/改签名），优先用 python 精确字符串替换（读→replace→写回），不用 edit 的 lines 数组重复命中。

**先例**: 2026-08-11 PIT-81 调试轮 — edit 工具三次重复插入（stop() 函数签名 ×2、main 声明 ×2、日志行残留），每次 build 才暴露，浪费 3 轮。修复统一走 python replace（assert count==1）。

**阻塞条件**: 同一函数连续第 2 次 edit 前未 grep 验证；已出现重复插入但未删除重复行即提交。

### 10. python 批量替换脚本必须逐块写盘或前置验证 (PIT-84)

**规则**: 多块替换的 python 脚本（assert → replace → write 模式），**每块 replace 后立即写盘**，或**所有 assert 前置验证后再统一替换**；禁止"全部替换后末尾一次写盘"（任一 assert 失败 → 全盘丢失，PIT-84 踩 2 次）。

**验证**: 脚本执行后 `grep -c "<关键替换内容>" <file>` 确认每块生效；失败重跑前检查哪些块已写。

**阻塞条件**: 多块脚本末尾一次性写盘；assert 失败后未确认中间状态直接重跑。
