---
name: security-hardening
description: "OMSPBase security audit and hardening. OWASP Top 10 checks, hardcoded secrets/ports/URLs scan (merged review-hardcode), PSK/JWT auth flow review, WebSocket security, secrets management (PIT-10 lesson), mediasoup SFU transport security. Use before release, after auth changes, or when onboarding new PSK keys. Also accessible via /review-hardcode."
---

# security-hardening — 安全加固

> OWASP Top 10 + PSK/JWT auth + WebSocket security + secrets management.
> 每一条规则都有检查命令。每个检查都必须通过。

## 触发条件

- 发布前（release candidate）
- Auth 模块修改后
- 新增 WebSocket endpoint
- 新增 PSK key / JWT secret
- 用户说 "security review" / "安全审计"
- 新增 FFI 边界（C/ObjC bridge）

## Mode A: 审计模式 (audit)

完整安全审计，手动触发。运行所有 6 个 Phase，生成审计报告。

## Mode B: Guard-while-building (PreToolUse)

> 代码变更时自动触发，阻断不安全提交。轻量级，目标 <5s。
> 参考: JoeyPatricio/security-hardening-skill — PreToolUse guard pattern

### B1: cargo-audit 快速扫描

```bash
# 仅检查新出现的漏洞（非完整审计）
cargo audit --quiet --deny unsound 2>&1 | head -20
```

| 检查项 | 命令 | 通过标准 |
|--------|------|---------|
| 已知漏洞 | `cargo audit --quiet` | 0 RUSTSEC warnings |
| 拒绝 unsound | `cargo audit --deny unsound` | exit 0 |

### B2: unsafe 块扫描

```bash
# 每个 unsafe 块必须有 // SAFETY: 注释
UNSAFE_NO_COMMENT=$(grep -rn 'unsafe' crates/ --include='*.rs' | grep -v '// SAFETY:' || true)
if [ -n "$UNSAFE_NO_COMMENT" ]; then
  echo "ERROR: unsafe block without SAFETY comment:"
  echo "$UNSAFE_NO_COMMENT"
  exit 1
fi
```

### B3: gitleaks 密钥检测

```bash
# 安装 (one-time)
# brew install gitleaks  # macOS
# or: go install github.com/gitleaks/gitleaks/v8@latest

# PreToolUse 检查：扫描暂存区变更
gitleaks detect --source . --no-git --redact --verbose 2>&1 | head -30
```

### B4: 硬编码快速扫描 (lightweight)

```bash
# 仅扫描变更文件（非全仓库）
git diff --cached --name-only -- '*.rs' | xargs -I{} grep -n 'token.*=\|password.*=\|api[_-]key' {} 2>/dev/null || true
```

### Guard 执行顺序

```text
PreToolUse (on edit .rs/.toml):
  1. unsafe 块注释检查  (<0.5s)
  2. 硬编码快速扫描     (<1s)
  3. gitleaks 检测     (<2s)
  4. cargo audit       (<3s)

任一失败 → 阻断操作
```

### gitleaks 配置 (.gitleaks.toml)

```toml
# .gitleaks.toml — project-level config
title = "OMSPBase gitleaks config"

[extend]
useDefault = true

[allowlist]
description = "Known false positives"
paths = [
  "scripts/scan-hardcode.sh",  # test patterns
  ".env.example",               # placeholder values
]

[[rules]]
id = "custom-psk-pattern"
description = "OMSPBase PSK keys"
regex = '''(?i)(psk|pre_shared_key)\s*=\s*["'][A-Za-z0-9+/]{32,}["']'''
```

### gitleaks pre-commit hook (.git/hooks/pre-commit)

> 参考 D201: 现有 pre-commit hook 运行 `cargo fmt --check` + `cargo clippy -- -D warnings`。
> 以下为 gitleaks 集成到同一 hook 的扩展：

```bash
#!/usr/bin/env bash
# .git/hooks/pre-commit — Rust 质量门禁 + gitleaks 密钥检测
set -euo pipefail

STAGED_RS=$(git diff --cached --name-only --diff-filter=ACM -- '*.rs' || true)
STAGED_TOML=$(git diff --cached --name-only --diff-filter=ACM -- '*.toml' || true)

# ---- gitleaks 密钥检测 (全仓库，仅当 .rs/.toml 变更时) ----
if [ -n "$STAGED_RS" ] || [ -n "$STAGED_TOML" ]; then
  if command -v gitleaks &>/dev/null; then
    echo "→ gitleaks: scanning staged changes..."
    { gitleaks detect --source . --no-git --redact --verbose 2>&1; } || {
      echo ""
      echo "ERROR: gitleaks detected secrets in staged files."
      echo "  Review findings above. False positive? Add to .gitleaks.toml allowlist."
      echo "  To bypass (emergency only): GITLEAKS_SKIP=1 git commit ..."
      exit 1
    }
  else
    echo "⚠ gitleaks not installed. Install: brew install gitleaks"
  fi
fi

# ---- Rust 质量门禁 (仅 .rs 变更时) ----
if [ -n "$STAGED_RS" ]; then
  echo "→ cargo fmt: checking..."
  cargo fmt --check
  echo "→ cargo clippy: checking..."
  cargo clippy -- -D warnings
fi

echo "✅ pre-commit checks passed"
```

### gitleaks 安装

| 平台 | 命令 |
|------|------|
| macOS | `brew install gitleaks` |
| Linux | `go install github.com/gitleaks/gitleaks/v8@latest` 或下载 [release binary](https://github.com/gitleaks/gitleaks/releases) |
| Docker | `docker run -v $(pwd):/path zricethezav/gitleaks detect --source /path` |

## Phase 1: 密钥扫描 (PIT-10 enforced)

```bash
# 运行硬编码扫描
./scripts/scan-hardcode.sh

# 手动补充检查
grep -rn 'api[_-]key\|api[_-]secret\|token.*=\|password.*="' crates/ --include='*.rs' | grep -v '//.*TODO' | grep -v 'env::var'
grep -rn 'sk-\|pk-\|AKID\|SecretId' crates/ --include='*.rs'
grep -rn 'apiKey\|api_key\|API_KEY' .opencode/ --include='*.json' --include='*.jsonc'
```

### PIT-10 规则

> 全局配置中的硬编码 API Key 存在泄露风险。使用环境变量插值：`"apiKey": "{env:NEW_API_KEY}"`。

| 检查项 | 命令 | 通过标准 |
|--------|------|---------|
| 无硬编码密钥 | `scripts/scan-hardcode.sh` | 0 CRITICAL |
| env var 插值 | `grep -rn '{env:' .opencode/` | 所有 apiKey 使用插值 |
| .gitignore 覆盖 | `grep '\.env' .gitignore` | 包含 .env, .env.local |
| 示例文件占位 | `grep 'EXAMPLE_KEY\|your-key-here' .env.example` | 无真实密钥 |

### 硬编码值严重性（原 review-hardcode）

本技能吸收了 `review-hardcode` 的完整扫描能力。`/review-hardcode` 命令指向此处。

| 模式 | 严重性 | 说明 |
|------|--------|------|
| `token="..."` / `password="..."` / `secret="..."` / `api_key="..."` | 🔴 CRITICAL | 硬编码密钥/令牌 |
| `:9800` 等硬编码端口 | 🟠 HIGH | 生产端口不应硬编码 |
| `localhost:PORT` / `127.0.0.1:PORT` | 🟠 HIGH | 地址+端口应配置化 |
| `http://IP` 硬编码 IP URL | 🟡 MEDIUM | 应使用配置或 DNS |

### 排除规则

扫描自动排除: `target/`, `node_modules/`, `.git/`, `.pixi-cache/`。
标记 `TODO:` 的硬编码值（允许临时存在）应手动审核后决定是否忽略。

## Phase 2: Auth 流审计

### PSK 认证 (Host↔Server)

```rust
// OMSPBase PSK flow:
// Host ──[PSK in WS header]──> Server ──[validate]──> Session token

// 检查点:
// 1. PSK 是否通过环境变量注入？(不是 config file 明文)
// 2. PSK 是否 ≥32 字节？
// 3. Server 是否限速 PSK 验证？(防暴力破解)
// 4. Session token 是否有过期时间？
// 5. PSK 错误响应是否模糊？(不泄露"用户存在"或"密钥接近")
```

```bash
# 检查命令
grep -rn 'pre_shared_key\|psk' crates/omspbase-server/src/ --include='*.rs'
grep -rn 'env::var.*PSK\|env::var.*SECRET' crates/ --include='*.rs'
grep -rn 'session.*ttl\|token.*expir\|jwt.*exp' crates/omspbase-common/src/auth/ --include='*.rs'
grep -rn 'rate.limit\|429\|too.many' crates/omspbase-server/src/ --include='*.rs'
```

### JWT 认证 (Admin UI)

```bash
# 检查命令
# 1. JWT secret 是否 ≥256 bit？
grep -rn 'jwt.*secret\|JWT_SECRET' crates/ --include='*.rs' | grep -v env::var

# 2. 确认算法是 HS256 或 RS256 (不是 none)
grep -rn 'Algorithm\|alg.*HS\|alg.*RS' crates/omspbase-common/src/auth/ --include='*.rs'

# 3. 确认有 exp 声明
grep -rn 'exp\|expir' crates/omspbase-common/src/auth/ --include='*.rs'

# 4. 确认有 refresh token 轮换
grep -rn 'refresh_token\|refresh' crates/omspbase-server/src/ --include='*.rs'
```

## Phase 3: WebSocket 安全

```bash
# 检查命令
# 1. 消息大小限制 (防 OOM)
grep -rn 'max.*message\|max.*frame\|message.*size' crates/omspbase-server/src/ --include='*.rs'

# 2. 连接速率限制
grep -rn 'connection.*limit\|max_connections\|concurrent' crates/omspbase-server/src/ --include='*.rs'

# 3. Origin 验证
grep -rn 'origin\|allowed_origin\|verify_origin' crates/omspbase-server/src/ --include='*.rs'

# 4. TLS (生产环境)
grep -rn 'wss://\|tls\|ssl_config' crates/omspbase-server/src/ --include='*.rs'
```

### 消息注入防护

```rust
// 所有 WS 消息反序列化必须使用 serde 严格模式
// 禁止: serde_json::from_str(&raw) — 未验证额外字段
// 正确: serde_json::from_str::<StrictSchema>(&raw) 或 deny_unknown_fields
```

```bash
grep -rn '#\[serde(deny_unknown_fields)\|unknown_fields\|additional_properties' crates/ --include='*.rs'
grep -rn 'serde_json::from_slice\|serde_json::from_str' crates/omspbase-server/src/ --include='*.rs' --include='*.rs'
```

## Phase 4: mediasoup SFU 安全

```bash
# 检查命令
# 1. RTP 端口范围是否受控 (非 0-65535)
grep -rn 'rtc_min_port\|rtc_max_port\|rtp_port' crates/omspbase-server/src/ --include='*.rs'

# 2. WebRTC transport 是否需要 auth
grep -rn 'web_rtc_server\|webRtcTransport\|transport.*auth' crates/omspbase-server/src/ --include='*.rs'

# 3. Room 创建是否需要权限
grep -rn 'create_room\|router.*create' crates/omspbase-server/src/sfu/ --include='*.rs'

# 4. Producer/Consumer 权限隔离
grep -rn 'peer_id\|producer.*peer\|consumer.*peer' crates/omspbase-server/src/sfu/ --include='*.rs'
```

## Phase 5: 依赖审计

```bash
# 运行 cargo-deny 检查已知漏洞
cargo deny check advisories

# 运行 cargo-audit
cargo audit

# 检查 unsafe 使用
grep -rn 'unsafe' crates/ --include='*.rs' | grep -v '// SAFETY:'
# 规则: 每个 unsafe 块必须有 // SAFETY: 注释说明为何安全
```

## Phase 6: Admin UI 安全

```bash
# 检查命令
# 1. Dashboard 是否要求认证
grep -rn 'auth\|login\|redirect.*login' crates/omspbase-server/src/admin/ --include='*.rs'

# 2. 是否有 CSRF 防护
grep -rn 'csrf\|xsrf\|same_site' crates/omspbase-server/src/ --include='*.rs'

# 3. CORS 是否严格
grep -rn 'access-control\|allow_origin\|cors' crates/omspbase-server/src/ --include='*.rs'

# 4. Content Security Policy
grep -rn 'content-security\|CSP\|frame-ancestors' crates/omspbase-server/src/admin/ --include='*.rs'
```

## 安全清单 (OWASP Top 10 aligned)

| # | 检查项 | OMSPBase 对应 | 命令 | 必须 |
|---|--------|-------------|------|:---:|
| A01 | 访问控制失效 | Auth trait 实现完整 | `grep -rn 'TODO\|FIXME' crates/*/src/auth/` | ✅ |
| A02 | 加密失败 | PSK ≥32B, TLS wss:// | `grep -rn 'wss://' crates/` (生产检查) | ✅ |
| A03 | 注入 | WS 消息 strict deser | `grep -rn 'deny_unknown' crates/` | ✅ |
| A04 | 不安全设计 | 速率限制 + 超时 | `grep -rn 'rate.limit\|timeout' crates/` | ✅ |
| A05 | 安全配置错误 | 无 debug 模式生产 | `grep -rn 'debug_assert\|cfg(debug)' crates/` | ✅ |
| A06 | 脆弱组件 | cargo-audit 通过 | `cargo audit` | ✅ |
| A07 | 认证失败 | PSK + JWT 双模式 | Phase 2 全部检查 | ✅ |
| A08 | 软件数据完整性 | FlatBuffers 验证 | (Phase 2+) | ✅ |
| A09 | 日志监控失败 | 审计日志 | `grep -rn 'audit\|security.log' crates/` | ⚠️ |
| A10 | SSRF | 无服务端 HTTP 拉取 | `grep -rn 'reqwest\|hyper::Client' crates/omspbase-server/` | ✅ |

## 报告格式

```
## 安全审计报告 — [日期]

### Phase 1: 密钥扫描
✅ 扫描通过: 0 CRITICAL, 0 HIGH, 0 MEDIUM

### Phase 2: Auth 流
✅ PSK 来自环境变量 (OMSP_PSK)
✅ PSK 长度: 64 字节
⚠️ Session TTL: 24h (建议 2h)
❌ 无速率限制 (P0)

### Phase 3: WebSocket
✅ 消息大小限制: 1MB
❌ 无 Origin 验证 (可被 CSWSH 攻击)
⚠️ TLS 仅 Docker 环境启用

### Phase 4: mediasoup
✅ RTP 端口: 40000-40100
✅ Room 需 auth token
⚠️ Producer 无带宽限制

### Phase 5: 依赖
✅ cargo-audit: 0 vulnerabilities
✅ cargo-deny: 0 unlicensed

### 总结
CRITICAL: 0 | HIGH: 1 | MEDIUM: 2
修复建议: [按严重性排序]
```

## 禁止

- 跳过 PIT-10 反模式：决不允许硬编码密钥
- 静默吞错误：auth 失败必须日志但不泄露密钥
- 忽略 cargo-audit 警告：任何 RUSTSEC 必须有决策记录
- 生产环境使用 debug 配置：debug_assert! 在 release 中不执行
- HTTP 明文 WS：生产必须 wss:// 或受控内网
