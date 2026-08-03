# 权限认证参考

> AUDEMSP 提供双模式认证：Local（独立部署）和 AUDEBase（平台模式）。

---

## 一、AuthProvider 模式

```rust
#[async_trait]
trait AuthProvider: Send + Sync {
    async fn login(&self, credential: &Credential) -> Result<User, AuthError>;
    async fn validate(&self, token: &str) -> Result<User, AuthError>;
    async fn authorize(&self, user_id: &str, permission: &Permission) -> Result<bool, AuthError>;
}

两种实现：
- **Local**：SQLite + JWT，独立部署使用
- **AUDEBase**：gRPC LDAP，AUDEBase Docker 模块使用

---

## 二、双模式对比

| 维度 | Local（独立模式） | AUDEBase（平台模式） |
|------|------------------|---------------------|
| **用户存储** | 内嵌 SQLite | 委托 AUDEBase RBAC |
| **认证** | 本地 JWT | AUDEBase 签发 token |
| **权限** | 本地 RBAC 表 | AUDEBase LDAP 组映射 |
| **配置** | `auth.mode: "local"` | `auth.mode: "aude"` |
| **场景** | 独立部署 | AUDEBase Docker 模块 |

---

## 三、权限模型

```typescript
interface Permission {
  // 功能开关
  capabilities: {
    streaming:    { push: boolean; pull: boolean };
    remote:       { control: boolean; controllable: boolean };
    conference:   { host: boolean; join: boolean };
    surveillance: boolean;
    teleop:       { operator: boolean; vehicle: boolean };
  };

  // 配额限制
  quotas: {
    max_streams:    number;        // 最大流数
    max_bitrate:    number;        // 最大码率 (kbps)
    max_resolution: "720p" | "1080p" | "4k";
    max_duration:   number;        // 最长会话时长 (秒)
  };

  // License
  license: {
    type:        "trial" | "standard" | "enterprise";
    expires_at:  string;          // ISO 8601
    features:    string[];        // 高级特性列表
  };
}
```

---

## 四、权限流

```
客户端 SDK 启动
       ↓
从后台拉取权限配置
       ↓
缓存在本地
       ↓
License Manager 在每个管线操作前校验
```

---

## 五、参考模型

类比群晖 DSM：AUDEMSP 作为 Docker 模块安装在 AUDEBase 上，使用 AUDEBase 的用户/权限系统（类似 Jira 安装在群晖上使用 DSM 的 LDAP 账户）。


## 六、gRPC Auth 合约 (D57)

当 AUDEMSP 运行在 AUDEBase 模块模式下，AuthProvider 通过 gRPC 调用 AUDEBase：

### Proto

```protobuf
syntax = "proto3";
package audemsp.auth;

service AuthService {
  rpc ValidateToken(ValidateTokenRequest) returns (ValidateTokenResponse);
  rpc CheckPermission(CheckPermissionRequest) returns (CheckPermissionResponse);
}

message ValidateTokenRequest {
  string token = 1;
}

message ValidateTokenResponse {
  bool valid = 1;
  string user_id = 2;
  string user_name = 3;
  repeated string roles = 4;
  repeated string permissions = 5;   // AUDEBase 预解析权限列表
  int64 expires_at = 6;              // Unix timestamp
  string error = 7;
}

message CheckPermissionRequest {
  string user_id = 1;
  string permission = 2;             // AUDEMSP 权限字符串
}

message CheckPermissionResponse {
  bool allowed = 1;
  string reason = 2;
}
```

### 调用流程

```
客户端请求 → AUDEMSP axum 收到 JWT
                           │
                           ▼
              AuthProvider.authenticate(token)
                           │
              AUDEBase 模式  ▼
              gRPC ValidateToken(token) → AUDEBase
                           │
                           ▼
              返回 user_id + permissions[]
                           │
                           ▼
              LicenseManager 检查 features[]
              Permission::StreamingStart ∈ permissions[]
```

### 缓存策略

- `validateToken`: 结果缓存在内存 5 分钟（JWT 过期时间以下）
- `checkPermission`: 优先查 `validateToken` 返回的 permissions[] 列表
- 权限变更: AUDEBase 需要通知 AUDEMSP 清除缓存 → Phase 2 Channel

### AUDEMSP Permission 枚举

AUDEMSP 自维护权限字符串，AUDEBase RBAC 做字符串→角色映射：

| 权限 | 说明 |
|------|------|
| `RemoteDesktop.Connect` | 发起远程桌面连接 |
| `RemoteDesktop.Host` | 作为被控端运行 |
| `Streaming.Start` | 启动推流 |
| `Streaming.Stop` | 停止推流 |
| `Teleop.Connect` | 遥控连接 |
| `Teleop.Control` | 发送控制指令 |
| `Conference.Create` | 创建会议室 |
| `Conference.Join` | 加入会议室 |
| `Conference.Record` | 会议录制 |
| `Surveillance.View` | 查看监控画面 |
| `Admin.Manage` | 后台管理权限 |