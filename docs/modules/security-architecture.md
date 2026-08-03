# Security Architecture — 安全架构

> 状态：Phase 4 前设计 | 整合：D116 (STRIDE-Lite) + D-SEC-01 (mTLS+TLS+audit) + D117 (E-Stop) + D130 (AuthProvider) | 创建依据：doc-audit CR5

## 威胁模型 (STRIDE-Lite, D116)

| 类别 | 威胁 | 缓解 |
|------|------|------|
| Spoofing | 伪造对等点身份 | JWT + mTLS |
| Tampering | 篡改控制指令 | RTCDataChannel HMAC |
| Repudiation | 否认操作 | 审计日志 |
| Info Disclosure | 信令窃听 | TLS 1.3 |
| DoS | 信令洪泛 | 速率限制 |
| Elevation | 权限提升 | D88 RBAC |

## 认证架构

```
Client → JWT (来自 AuthProvider login) → WebSocket upgrade
       → Token 在 HTTP header: Authorization: Bearer <jwt>
       → AuthComponent validate(token) → User + Permissions
```

## JWT Token 生命周期

| 阶段 | 说明 | Config |
|------|------|--------|
| 签发 | AuthComponent.login() → JWT | exp: 24h |
| 验证 | 每次 API/WS 请求 validate() | — |
| 刷新 | POST /admin/api/auth/refresh → 新 token | exp: 24h |
| 吊销 | DELETE /admin/api/auth/revoke → 黑名单 (SQLite) | 即时生效 |
| 轮转 | JWT_SECRET 定期更换 → 所有 token 失效 | 30 天 |

## mTLS (Phase 2)

- Phase 1: Server 端 TLS (rustls) + HTTP Basic Auth 备选
- Phase 2: 双向 mTLS，对等点通信加密
- 证书: X.509，ECDSA P-256，90 天有效期

## 证书生命周期管理

Phase 1: 自签 CA + 手动轮换。TLS 证书 90 天有效期。私钥文件权限 0600。
Phase 2: Let's Encrypt (ACME) 自动续期，CRL 吊销列表。
信任链: Root CA → Intermediate CA (可选 Phase 2) → Leaf certificate。

## 密钥管理

密钥轮转策略：

| 密钥类型 | 轮转周期 | 共存窗口 | 触发方式 |
|---------|---------|---------|---------|
| WS PSK | 30 天 | 24h | 手动/配置重载 |
| JWT 签名密钥 | 90 天 | 24h (kid header) | 手动 |
| TLS 私钥 | 90 天 | 0 (即时切换) | 手动/ACME |
| HMAC 控制命令密钥 | 30 天 | 1h (旧帧重放窗口) | 手动 |
## 审计事件 Schema

```json
{
  "event_id": "uuid",
  "timestamp": "ISO8601",
  "actor": "user_id | peer_id",
  "action": "login | room.create | peer.connect | admin.config | e-stop",
  "resource": "room_id | config_key",
  "result": "success | denied | error",
  "source_ip": "optional"
}
```

- Phase 1: 审计日志 → stdout (journald)
- Phase 2: 审计日志 → SQLite (admin queryable)

## WebSocket PSK 轮转

- Phase 1: 静态 PSK (环境变量)，服务重启时更换
- Phase 2: 动态轮转，每 24h 协商新 PSK

### PSK 轮转 Runbook (doc-audit L5)

> 现状 [已核实]：signaling.rs:173-177 连接时读取**单个** `AUDEMSP_PSK` env；**双密钥共存机制未实现**（Phase 2 设计），当前轮转 = 重启换 key。

| 步骤 | 操作 | 验证 |
|------|------|------|
| 0. 前置 | 确认 server 日志可查 AuditEvent（AuthSuccess/AuthFailure） | `grep AuthSuccess` 日志 |
| 1. 生成 | `openssl rand -hex 32` 生成新 PSK | 长度 64 hex |
| 2. 切换 | 设置新 `AUDEMSP_PSK` → 重启 server（Phase 1 语义） | 旧连接断开、新连接 AuthSuccess |
| 3. 共存窗口 | [规划] `AUDEMSP_PSK_OLD` 双 HMAC 校验（signaling.rs:174-176 改造点），24h 容忍未重连客户端 | 窗口内无 AuthFailure 增长 |
| 4. 清理 | 窗口后清除 OLD；生产 compose 移除硬编码 psk → `env_file`/secret（docker-compose.yml:10） | `grep -rn "audemsp-dev"` 仅测试文件 |
| 5. 回滚 | 失败则恢复旧 env 重启（Phase 1 无窗口内回滚，步骤 3 的 OLD 机制是关键） | 连接恢复 |

## SFU 媒体面安全 (doc-audit M7)

> 现状 [已核实]：代码定位 sfu.rs:150/155-172/215/392-431；文档定位 sfu-mediasoup-integration.md:27/120-154/170/250/266。

### 信任边界与威胁模型

STRIDE-Lite 表扩展（媒体面行）：

| 威胁 | 媒体面对应 | 缓解 |
|------|-----------|------|
| Spoofing | 伪 peer 加入 room 推流 | DTLS 指纹验证（mediasoup 内部）+ ICE credential 所有权 |
| Tampering | RTP 篡改 | SRTP 完整性（默认 AES_CM_128_HMAC_SHA1_80，支持升级） |
| DoS | 端口面滥用 | WebRtcServer 单端口收敛 + ICE consent timeout（默认 20s） |

### 传输/生产/消费授权模型

**现状 [已核实]**：路由级授权——peer 必须已认证（signaling.rs:345 用会话 peer_id）+ 在 room 中（sfu.rs:302-303/359-360）+ transport 归属校验（sfu.rs:414-422）。**无角色级授权**：RoomJoin 的 role 不存入会话，SFU 路径不校验；任何已认证 peer 均可 produce 或 consume。

**目标 [规划]**：role 注入 SfuManager（host=produce-only / remote=consume-only），produce 校验 send transport 归属、consume 校验 producer 同 room。列入 Phase 4 安全项。

### DTLS 指纹验证

- [已核实] connect_transport 将客户端 DtlsParameters（含指纹）传给 mediasoup `transport.connect()`（sfu.rs:424），**指纹比对由 mediasoup 在 DTLS 握手时内部执行**。
- [规划] 应用层策略：记录指纹哈希审计、拒绝 role 变更后的新指纹。

### SRTP 套件策略

- [已核实] 默认套件 AES_CM_128_HMAC_SHA1_80（mediasoup 默认，`WebRtcTransportOptions::new_with_server()` 未配置）。
- [规划] 显式指定（如 AEAD_AES_256_GCM）与降级策略。

### 端口暴露与防火墙（B1）

- **⚠️ [已核实] WebRtcServer 20000/udp 曾未 publish**——docker-compose.yml:6 与 dev.yml:12 已补 `20000:20000/udp` 映射（doc-audit B1 修复，2026-08-03）。
- [已核实] 40000-40100/udp 仅 PlainTransport 路径需要（WebRtcTransport 经 WebRtcServer 20000 承载）；Worker 实际用 `WorkerSettings::default()`（sfu.rs:150），与 sfu-mediasoup-integration.md:27 声称的 rtcMinPort 40000 不符——文档待对齐。
- [规划] `announced_address` 配置支持 NAT 部署（sfu.rs:161 当前 None）。

### 审计扩展

- [已核实] 现有 AuditEvent（audit.rs:32-34）覆盖信令层。
- [规划] SFU 动作（produce/consume/connect）审计事件。

## 速率限制

| 端点 | 限制 | 窗口 |
|------|------|------|
| /admin/api/auth/login | 5 req | 1min |
| /admin/api/* | 100 req | 1min |
| /api/* | 无限制 | — |

## Phase 依赖

- Phase 3 Component 框架: AuthComponent (JWT + 速率限制)
- Phase 4 Admin Dashboard: login/logout/session 过期 UX
- Phase 2 mediasoup SFU: 信令层 mTLS
