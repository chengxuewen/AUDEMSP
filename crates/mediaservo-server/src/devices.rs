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
    /// 面向客户端的可读消息（C15: 错误响应必须信息充分）。
    /// 防枚举（review #1）: 未知设备与错误 secret 必须返回**逐字一致**的消息 —
    /// 区分会泄漏注册表成员资格。内部区分（Unknown/BadSecret）仅保留在审计日志。
    /// 4010 单一错误码（signaling.rs 认证点常量）+ 此单一消息。
    pub fn message(&self) -> &'static str {
        match self {
            DeviceAuthError::Incomplete => {
                "device authentication failed: both device_id and device_secret are required"
            }
            DeviceAuthError::Unknown | DeviceAuthError::BadSecret => {
                "device authentication failed: invalid device credentials"
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
/// 注意: 不实现 Default — dummy_hash 必须启动时随机生成（Default 会给空串,
/// 长度与真实哈希不同 → 未知设备比较路径的时序与已知设备可区分, 重开侧信道）。
#[derive(Debug, Clone)]
pub struct DeviceRegistry {
    devices: HashMap<String, String>, // device_id → "sha256:<hex>"
    /// 未知设备的固定比较目标（启动时随机生成, 与真实哈希同长, 永不匹配）。
    /// review #1: 未知设备也必须走完整 sha256 + ct_eq 路径, 响应时间不可区分。
    dummy_hash: String,
}

impl DeviceRegistry {
    pub fn empty() -> Self {
        Self {
            devices: HashMap::new(),
            dummy_hash: new_dummy_hash(),
        }
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
        Ok(Self {
            devices,
            dummy_hash: new_dummy_hash(),
        })
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    fn verify(&self, device_id: &str, secret: &str) -> Result<(), DeviceAuthError> {
        let known = self.devices.contains_key(device_id);
        // review #1 防时序: 未知设备也用 dummy_hash 走完整 sha256 + ct_eq（无提前返回）。
        // 已知/未知的响应时间不可区分; 匹配与否经 same-length ct_eq 判定。
        let stored = self.devices.get(device_id).unwrap_or(&self.dummy_hash);
        let want = hash_secret(device_id, secret);
        let matched: bool = stored.as_bytes().ct_eq(want.as_bytes()).into();
        match (matched, known) {
            (true, _) => Ok(()),
            // 内部区分保留（审计用）; 对外 wire 响应两者完全一致（见 message()）。
            (false, true) => Err(DeviceAuthError::BadSecret),
            (false, false) => Err(DeviceAuthError::Unknown),
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

/// 启动时生成一次的 dummy 比较目标（review #1）: 随机 device_id 保证与任何
/// 注册设备不冲突（uuid v4），长度与真实哈希一致（71 字符）保证 ct_eq 路径恒等。
fn new_dummy_hash() -> String {
    hash_secret(&format!("ms-dummy-{}", uuid::Uuid::new_v4()), "dummy")
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
    fn error_messages_informative_and_unknown_badsecret_identical() {
        // C15: 消息可读；4010 单一错误码（signaling.rs 常量）+ 单一消息。
        assert!(DeviceAuthError::Incomplete.message().contains("both device_id"));
        assert!(DeviceAuthError::Incomplete.message().contains("device authentication failed"));
        // review #1: 未知设备与错误 secret 的 wire 消息必须逐字一致（防枚举）。
        assert_eq!(
            DeviceAuthError::Unknown.message(),
            DeviceAuthError::BadSecret.message()
        );
        assert!(
            DeviceAuthError::Unknown
                .message()
                .contains("invalid device credentials")
        );
    }

    #[test]
    fn unknown_vs_bad_secret_wire_response_identical() {
        // review #1 TDD: 两种失败路径的完整 wire 响应（code=4010 + message）必须一致。
        // code 由 signaling.rs 认证点统一为 4010；此处锁定 message 层等价。
        let reg = test_registry();
        let e_unknown = authenticate(&reg, Some("ms-nope"), Some("x"))
            .expect("creds present")
            .unwrap_err();
        let e_bad = authenticate(&reg, Some("ms-0a1b2c3d4e5f"), Some("wrong"))
            .expect("creds present")
            .unwrap_err();
        assert_eq!(e_unknown, DeviceAuthError::Unknown);
        assert_eq!(e_bad, DeviceAuthError::BadSecret);
        // 内部错误类型不同（审计可区分）但 wire 消息相同 — 防枚举。
        assert_ne!(e_unknown, e_bad);
        assert_eq!(e_unknown.message(), e_bad.message());
        // 两路径都必须走"设备认证失败"家族消息（客户端按 4010+前缀识别）。
        assert!(e_unknown.message().starts_with("device authentication failed"));
    }

    #[test]
    fn dummy_hash_is_per_instance_random_and_same_length() {
        // review #1: dummy 每次启动生成、与真实哈希同长（ct_eq 路径恒等）。
        let a = DeviceRegistry::empty();
        let b = DeviceRegistry::empty();
        assert_ne!(a.dummy_hash, b.dummy_hash, "dummy 必须每实例随机");
        assert_eq!(a.dummy_hash.len(), "sha256:".len() + 64, "dummy 与真实哈希同长");
        assert_eq!(a.dummy_hash.len(), hash_secret("ms-x", "s").len());
        // 未知设备仍走 verify 全路径（返回 Unknown 但内部已完成 sha256+ct_eq）。
        assert_eq!(a.verify("ms-anyone", "x"), Err(DeviceAuthError::Unknown));
    }
}
