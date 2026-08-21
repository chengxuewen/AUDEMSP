//! 设备身份（G4，D-H11/D-H13）：`identity.json` 生成/加载。
//!
//! 布局（D-H13）：实例根目录 `<dir>/identity.json`，0600 —— 与
//! `etc/link/signing.pem` 同级凭据纪律。格式（link `DeviceCredential`）：
//! ```json
//! { "device_id": "ms-<12 hex>", "device_secret": "<64 hex>" }
//! ```
//! - `device_id`：随机 6 字节 hex，前缀 `ms-`（稳定唯一即可——server 侧注册键）。
//! - `device_secret`：随机 32 字节 hex。
//! - 再生策略：`host init` 幂等——**仅缺失时生成**；覆盖会使 server 侧
//!   注册失效（G2）。存在但损坏 → 显式报错（C15），不静默覆盖。

use std::path::Path;

use mediaservo_link::DeviceCredential;

/// identity.json 文件名（实例根目录，D-H13）。
pub const IDENTITY_FILE: &str = "identity.json";

/// device_secret 随机字节数（hex = 64 字符）。
const SECRET_BYTES: usize = 32;

/// 生成新设备身份（OsRng）；device_id `<brand>-<12 hex>`（默认 "ms-"，legacy 映射见 brand.rs）。
pub fn generate_identity() -> DeviceCredential {
    use rand_core::RngCore;
    let mut id_bytes = [0u8; 6];
    rand_core::OsRng.fill_bytes(&mut id_bytes);
    let mut secret = [0u8; SECRET_BYTES];
    rand_core::OsRng.fill_bytes(&mut secret);
    DeviceCredential {
        device_id: format!("{}{}", mediaservo_common::brand::media_brand().device_prefix, hex(&id_bytes)),
        device_secret: hex(&secret),
    }
}

/// `host init`：幂等确保 identity.json 存在（存在 → 返回现有身份，不覆盖；
/// 缺失 → 生成并 0600 写入）。损坏文件 → Err（C15 显式报错）。
pub fn ensure_identity(dir: &Path) -> Result<DeviceCredential, String> {
    if let Some(existing) = load_identity(dir)? {
        return Ok(existing);
    }
    let cred = generate_identity();
    let path = dir.join(IDENTITY_FILE);
    let json = serde_json::to_string_pretty(&cred)
        .map_err(|e| format!("序列化 {} 失败: {e}", path.display()))?;
    write_secret_file(&path, json.as_bytes())?;
    Ok(cred)
}

/// 加载设备身份：文件缺失 → `Ok(None)`（PSK 回落路径）；存在但不可解析 → Err。
pub fn load_identity(dir: &Path) -> Result<Option<DeviceCredential>, String> {
    let path = dir.join(IDENTITY_FILE);
    let raw = match std::fs::read(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("读取 {} 失败: {e}", path.display())),
    };
    let cred: DeviceCredential = serde_json::from_slice(&raw)
        .map_err(|e| format!("{} 解析失败: {e}", path.display()))?;
    Ok(Some(cred))
}

/// 写凭据文件并设 0600（与 signing.pem 同纪律；幂等由调用方保证）。
fn write_secret_file(path: &Path, data: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)
        .map_err(|e| format!("创建 {} 失败: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("设置 {} 权限失败: {e}", path.display()))?;
    }
    f.write_all(data)
        .map_err(|e| format!("写入 {} 失败: {e}", path.display()))
}

/// 小端 hex（无依赖；device_id/device_secret 均为固定长度）。
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_identity_shape_and_uniqueness() {
        let a = generate_identity();
        assert!(a.device_id.starts_with("ms-"), "device_id 应带 ms- 前缀: {}", a.device_id);
        assert_eq!(a.device_id.len(), 15, "ms- + 12 hex");
        assert!(a.device_id[3..].chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(a.device_secret.len(), 64, "32 字节 hex");
        assert!(a.device_secret.chars().all(|c| c.is_ascii_hexdigit()));
        let b = generate_identity();
        assert_ne!(a.device_id, b.device_id);
        assert_ne!(a.device_secret, b.device_secret);
    }

    #[test]
    fn ensure_identity_writes_0600_and_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = ensure_identity(dir.path()).expect("ensure");
        let path = dir.path().join(IDENTITY_FILE);
        let raw = std::fs::read(&path).expect("read identity");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).expect("metadata").permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "identity.json 必须 0600");
        }
        // 可回读且与返回一致
        let loaded: DeviceCredential = serde_json::from_slice(&raw).expect("parse identity");
        assert_eq!(loaded, first);
        // 幂等：已存在 → 不覆盖（内容不变，device_id/secret 不换）
        let second = ensure_identity(dir.path()).expect("ensure again");
        assert_eq!(second, first, "已存在的身份不得再生");
        assert_eq!(std::fs::read(&path).expect("read again"), raw, "文件内容不得变化");
    }

    #[test]
    fn load_identity_missing_returns_none_and_corrupt_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(matches!(load_identity(dir.path()), Ok(None)), "缺失 → None（PSK 回落）");
        std::fs::write(dir.path().join(IDENTITY_FILE), b"not json").expect("write corrupt");
        let err = load_identity(dir.path()).expect_err("损坏文件必须报错");
        assert!(err.contains("解析失败"), "应指出解析失败: {err}");
    }
}
