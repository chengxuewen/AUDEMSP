---
name: review-hardcode
description: "Scan project source for hardcoded ports, URLs, secrets, tokens, and credentials using grep-based detection. Reports findings by severity (CRITICAL/HIGH/MEDIUM). Use before commits or as pre-merge check."
---

# 硬编码值扫描 (Hardcoded Values Scanner)

对 OMSPBase 项目源码扫描硬编码端口、URL 和密钥。

**哲学**: 硬编码值就是定时炸弹。环境变量 = 安全，配置文件 = 可接受，源码字面量 = 阻塞。每次扫描清一批，代码库往前一步。

---

## 入口

### `/review-hardcode`（无参数）
启动快速扫描，输出发现的硬编码值按严重性排序。

### `/review-hardcode full`
启动全量扫描（等同于默认单次扫描）。

---

## 扫描规则

| 模式 | 严重性 | 说明 |
|------|--------|------|
| `token="..."` / `password="..."` / `secret="..."` / `api_key="..."` | 🔴 CRITICAL | 硬编码密钥/令牌 |
| `:9800` 等硬编码端口 | 🟠 HIGH | 生产端口不应硬编码 |
| `localhost:PORT` / `127.0.0.1:PORT` / `0.0.0.0:PORT` | 🟠 HIGH | 地址+端口应配置化 |
| `http://IP` 硬编码 IP URL | 🟡 MEDIUM | 应使用配置或 DNS |

---

## 工作流

### Phase 1: 扫描
运行 `scripts/scan-hardcode.sh`，收集所有发现。

### Phase 2: 分类
- 🔴 CRITICAL → 立即修复，不可合并
- 🟠 HIGH → 应修复，建议阻塞合并
- 🟡 MEDIUM → 记录 TODO，下次清理

### Phase 3: 修复
1. 替换为环境变量或配置项
2. 验证启动不依赖硬编码值
3. 重新扫描确认清零

### Phase 4: 报告
```
扫描完成 — [日期]
发现总数: N | 已修复: M | TODO: K
严重项: X (已清零: ✅ / 剩余: Y)
```

---

## 排除规则

扫描自动排除: `target/`, `node_modules/`, `.git/`, `.pixi-cache/`。

标记 `TODO:` 的硬编码值（允许临时存在）不会被 grep 过滤——应手动审核后决定是否忽略。

---

## 快速参考

### 运行扫描
```bash
./scripts/scan-hardcode.sh          # 扫描项目根目录
./scripts/scan-hardcode.sh crates/  # 仅扫描某个子目录
```

### 严重性标准
| 严重性 | 触发条件 | 阻断合并? |
|--------|---------|:---:|
| 🔴 CRITICAL | 密钥/令牌/密码硬编码 | ✅ |
| 🟠 HIGH | 硬编码端口/地址 | ⚠️ |
| 🟡 MEDIUM | 硬编码 IP URL | ❌ |

### 建议频率
- 每次提交前: `/review-hardcode`
- 每次合并前: `/review-hardcode full`
- 新 crate 创建后: `/review-hardcode`
