//! G2 设备注册表 — server 侧设备凭证校验（D-H11 连接级身份）。
//!
//! 注册表为文件型配置（YAML，与 server.yaml 同构），格式：
//! ```yaml
//! devices:
//!   ms-0a1b2c3d4e5f:
//!     secret_hash: "sha256:<hex>"   # sha256(device_id + ":" + device_secret)
//! ```
//! 存储决策（G2）: 客户端经 TLS 在 wire 上明文携带 secret，注册表仅存单向哈希；
//! `sha256(device_id + ":" + device_secret)` — device_id 充当每设备盐（无需额外存储）。
//! 升级路径（H 阶段）: argon2id 替换 sha256，格式前缀 `argon2:<encoded>`。
//! 配发流程（G2 文档）: `host init` 生成 identity.json → 运维把 device_id/secret
//! 拷入 server 的 devices.yaml（`ms-field hash` 之类工具 H 阶段提供；当前用
//! `sha256sum` 手工算或本模块测试向量）。

use mediaservo_common::error::CoreError;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use subtle::ConstantTimeEq;

/// 设备认证失败原因（错误码统一 4010，见 signaling.rs 认证点注释）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceAuthError {
    /// device_id/device_secret 恰好只带了一个（形状检查，G4 review Minor 1）。
    Incomplete,
    /// device_id 不在注册表中。
    Unknown,
    /// secret 哈希不匹配。
    BadSecret,
}

impl DeviceAuthError {
    /// 面向客户端的可读消息（C15: 错误响应必须信息充分；4010 单一错误码防设备枚举）。
    pub fn message(&self) -> &'static str {
        match self {
            DeviceAuthError::Incomplete => {
                "device authentication failed: both device_id and device_secret are required"
            }
            DeviceAuthError::Unknown => "device authentication failed: device not registered",
            DeviceAuthError::BadSecret => {
                "device authentication failed: invalid device secret"
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RegistryFile {
    #[serde(default)]
    devices: HashMap<String, DeviceEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeviceEntry {
    secret_hash: String,
}

/// 设备注册表（启动时加载，只读）。
#[derive(Debug, Clone, Default)]
pub struct DeviceRegistry {
    devices: HashMap<String, String>, // device_id → "sha256:<hex>"
}

impl DeviceRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    /// 从 YAML 文件加载；文件缺失视为空注册表（PSK 路径不受影响）。
    pub fn load(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            CoreError::ConfigParse(format!("devices file {}: {e}", path.as_ref().display()))
        })?;
        Self::from_yaml(&content).map_err(|e| {
            CoreError::ConfigParse(format!("devices file {}: {e}", path.as_ref().display()))
        })
    }

    /// 从 YAML 文本解析（测试与加载共用）。
    pub fn from_yaml(content: &str) -> Result<Self, String> {
        let file: RegistryFile =
            serde_yaml::from_str(content).map_err(|e| format!("YAML parse error: {e}"))?;
        let mut devices = HashMap::new();
        for (id, entry) in file.devices {
            if !entry.secret_hash.starts_with("sha256:") {
                return Err(format!("device {id}: unsupported secret_hash scheme (want sha256:)"));
            }
            if entry.secret_hash.len() != "sha256:".len() + 64 {
                return Err(format!("device {id}: malformed sha256 hex length"));
            }
            devices.insert(id, entry.secret_hash);
        }
        Ok(Self { devices })
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    fn verify(&self, device_id: &str, secret: &str) -> Result<(), DeviceAuthError> {
        let stored = self
            .devices
            .get(device_id)
            .ok_or(DeviceAuthError::Unknown)?;
        let want = hash_secret(device_id, secret);
        // 常量时间比较（subtle）: 防时序侧信道恢复哈希字节（secret 不落地比较）。
        let stored_bytes = stored.as_bytes();
        let want_bytes = want.as_bytes();
        if stored_bytes.ct_eq(&want_bytes).into() {
            Ok(())
        } else {
            Err(DeviceAuthError::BadSecret)
        }
    }
}

/// sha256(device_id + ":" + device_secret)，hex 编码，`sha256:` 前缀。
/// device_id 充当每设备盐 — 无需额外 salt 存储（G2 存储决策，文档见模块头）。
pub fn hash_secret(device_id: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(device_id.as_bytes());
    hasher.update(b":");
    hasher.update(secret.as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

/// 设备认证决策点（RoomJoin 处理调用；纯函数便于单测）。
///
/// 返回 `None` = 未携带任何设备凭证 → PSK 路径（保持原流程）。
/// `Some(Err)` = 形状不完整或凭证校验失败 → Error 4010（见 `DeviceAuthError::message`）。
/// `Some(Ok)` = 设备认证通过 → 连接级身份绑定（peer_id → device_id，D-H11）。
pub fn authenticate(
    registry: &DeviceRegistry,
    device_id: Option<&str>,
    device_secret: Option<&str>,
) -> Option<Result<(), DeviceAuthError>> {
    match (device_id, device_secret) {
        (None, None) => None,
        (Some(id), Some(secret)) => Some(registry.verify(id, secret)),
        _ => Some(Err(DeviceAuthError::Incomplete)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> DeviceRegistry {
        // secret = "s3cret"; hash = sha256("ms-0a1b2c3d4e5f:s3cret")
        let secret = "s3cret";
        let hash = hash_secret("ms-0a1b2c3d4e5f", secret);
        let yaml = format!("devices:\n  ms-0a1b2c3d4e5f:\n    secret_hash: \"{hash}\"\n");
        DeviceRegistry::from_yaml(&yaml).unwrap()
    }

    #[test]
    fn hash_secret_uses_device_id_as_salt() {
        let a = hash_secret("ms-a", "same-secret");
        let b = hash_secret("ms-b", "same-secret");
        assert_ne!(a, b, "device_id 必须参与哈希（盐）");
        assert!(a.starts_with("sha256:") && a.len() == "sha256:".len() + 64);
        // 稳定向量: sha256("ms-a:same-secret")
        let mut h = Sha256::new();
        h.update(b"ms-a:same-secret");
        let expected: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(a, format!("sha256:{expected}"));
    }

    #[test]
    fn verify_ok_with_matching_secret() {
        let reg = test_registry();
        assert_eq!(reg.verify("ms-0a1b2c3d4e5f", "s3cret"), Ok(()));
    }

    #[test]
    fn verify_unknown_device() {
        let reg = test_registry();
        assert_eq!(reg.verify("ms-nope", "s3cret"), Err(DeviceAuthError::Unknown));
    }

    #[test]
    fn verify_wrong_secret() {
        let reg = test_registry();
        assert_eq!(reg.verify("ms-0a1b2c3d4e5f", "wrong"), Err(DeviceAuthError::BadSecret));
    }

    #[test]
    fn authenticate_shape_checks() {
        let reg = test_registry();
        // 双缺 = PSK 路径（None）
        assert_eq!(authenticate(&reg, None, None), None);
        // 半带 = Incomplete
        assert_eq!(
            authenticate(&reg, Some("ms-0a1b2c3d4e5f"), None),
            Some(Err(DeviceAuthError::Incomplete))
        );
        assert_eq!(
            authenticate(&reg, None, Some("s3cret")),
            Some(Err(DeviceAuthError::Incomplete))
        );
        // 全带 = 校验
        assert_eq!(
            authenticate(&reg, Some("ms-0a1b2c3d4e5f"), Some("s3cret")),
            Some(Ok(()))
        );
        assert_eq!(
            authenticate(&reg, Some("ms-nope"), Some("s3cret")),
            Some(Err(DeviceAuthError::Unknown))
        );
    }

    #[test]
    fn from_yaml_rejects_unsupported_scheme() {
        let yaml = "devices:\n  ms-x:\n    secret_hash: \"md5:abc\"\n";
        let err = DeviceRegistry::from_yaml(yaml).unwrap_err();
        assert!(err.contains("unsupported secret_hash"), "{err}");
    }

    #[test]
    fn from_yaml_rejects_bad_hex_length() {
        let yaml = "devices:\n  ms-x:\n    secret_hash: \"sha256:abc\"\n";
        let err = DeviceRegistry::from_yaml(yaml).unwrap_err();
        assert!(err.contains("malformed"), "{err}");
    }

    #[test]
    fn empty_registry_never_authenticates() {
        let reg = DeviceRegistry::empty();
        assert!(reg.is_empty());
        assert_eq!(
            authenticate(&reg, Some("ms-x"), Some("anything")),
            Some(Err(DeviceAuthError::Unknown))
        );
    }

    #[test]
    fn error_messages_are_informative_and_share_code_family() {
        // C15: 消息可读；4010 单一错误码防设备枚举（G2 决策）。
        assert!(DeviceAuthError::Incomplete.message().contains("both device_id"));
        assert!(DeviceAuthError::Unknown.message().contains("not registered"));
        assert!(DeviceAuthError::BadSecret.message().contains("invalid device secret"));
    }
}
