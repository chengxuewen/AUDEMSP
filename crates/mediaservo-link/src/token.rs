//! 能力令牌（D238：ACL 签进 JWT，Ed25519 非对称——设备私钥签、各节点公钥验）。

use crate::acl::{NodeAcl, Role};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};

/// Ed25519 签名密钥（私钥 PEM，设备权威持有）。
#[derive(Debug, Clone)]
pub struct Ed25519SigningKey(Vec<u8>);

impl Ed25519SigningKey {
    pub fn from_pem(pem: &[u8]) -> Self {
        Self(pem.to_vec())
    }
    pub fn pem(&self) -> &[u8] {
        &self.0
    }
}

/// Ed25519 验证密钥（公钥 PEM，各校验点持有；来源 `MEDIASERVO_DEVICE_PUBKEY` env / 设备配置）。
#[derive(Debug, Clone)]
pub struct Ed25519VerifyingKey(Vec<u8>);

impl Ed25519VerifyingKey {
    pub fn from_pem(pem: &[u8]) -> Self {
        Self(pem.to_vec())
    }
    pub fn pem(&self) -> &[u8] {
        &self.0
    }
}

/// 令牌 claims：node_id + role + acl + exp。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Claims {
    pub node_id: String,
    pub role: Role,
    /// ACL 签进令牌（D238）。
    pub acl: NodeAcl,
    pub exp: u64,
}

/// 能力令牌（JWT 字符串）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityToken(String);

impl CapabilityToken {
    /// Ed25519 签名（jsonwebtoken `Algorithm::EdDSA`）。**禁止 HS256**（对称，持钥可伪造，D238）。
    pub fn sign(acl: &NodeAcl, ttl_secs: u64, signing_key: &Ed25519SigningKey) -> Result<Self, crate::LinkError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| crate::LinkError::Token(e.to_string()))?
            .as_secs();
        let claims = Claims {
            node_id: acl.node_id.as_str().to_string(),
            role: acl.role,
            acl: acl.clone(),
            exp: now + ttl_secs,
        };
        let key =
            EncodingKey::from_ed_pem(signing_key.pem()).map_err(|e| crate::LinkError::Token(e.to_string()))?;
        let token = encode(&Header::new(Algorithm::EdDSA), &claims, &key)
            .map_err(|e| crate::LinkError::Token(e.to_string()))?;
        Ok(Self(token))
    }

    /// 公钥验签 + 校验 exp。
    pub fn verify(&self, verifying_key: &Ed25519VerifyingKey) -> Result<Claims, crate::LinkError> {
        let key =
            DecodingKey::from_ed_pem(verifying_key.pem()).map_err(|e| crate::LinkError::Token(e.to_string()))?;
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.validate_exp = true;
        validation.leeway = 0; // exp 精确生效（能力令牌无宽限）
        let data =
            decode::<Claims>(&self.0, &key, &validation).map_err(|e| crate::LinkError::Token(e.to_string()))?;
        Ok(data.claims)
    }

    /// 原始 JWT 字符串。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 从原始 JWT 字符串构造（诊断/测试用）。
    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    /// 序列化为自描述单文件字节（D-H10：部署分发一个文件，SDK 侧 `from_file` 加载即验签）。
    pub fn to_file(&self, verifying_key: &Ed25519VerifyingKey) -> Vec<u8> {
        TokenFile::encode(self, verifying_key)
    }

    /// 从自描述单文件字节加载并验签（篡改 / 错误 key / 过期均失败）。
    pub fn from_file(bytes: &[u8]) -> Result<(Ed25519VerifyingKey, CapabilityToken), crate::LinkError> {
        TokenFile::decode(bytes)
    }
}

/// 单文件自描述令牌（D-H10/D-H13）：verifying key + claims + signature 合并为一份文件，
/// 部署分发一个文件，SDK 经 [`CapabilityToken::from_file`] 加载即完成验签，无需外部 key。
///
/// 字节布局（全部 little-endian）：
/// ```text
/// magic    4B   "MSTK"
/// version  1B   0x01
/// key_len  2B   verifying key PEM 长度
/// key      key_len B
/// tok_len  2B   JWT 长度
/// token    tok_len B（完整 JWT: header.payload.signature，其 Ed25519 签名即文件级完整性）
/// ```
/// 无独立文件级签名：`encode` 仅持 verifying key（无私钥，无法另签），而内嵌 JWT 的 Ed25519
/// 签名覆盖全文件——篡改 key 区域使内嵌验签失败、篡改 claims/signature 使 JWT 验签失败。
#[derive(Debug, Clone, Copy)]
pub struct TokenFile;

impl TokenFile {
    /// 文件 magic（`b"MSTK"`）。
    pub const MAGIC: &[u8; 4] = b"MSTK";
    /// 文件格式版本。
    pub const VERSION: u8 = 1;

    /// 编码：verifying key PEM + 完整 JWT 合并为单文件字节。
    pub fn encode(token: &CapabilityToken, key: &Ed25519VerifyingKey) -> Vec<u8> {
        let pem = key.pem();
        let raw = token.as_str().as_bytes();
        let mut out = Vec::with_capacity(4 + 1 + 2 + pem.len() + 2 + raw.len());
        out.extend_from_slice(Self::MAGIC);
        out.push(Self::VERSION);
        out.extend_from_slice(&(pem.len() as u16).to_le_bytes());
        out.extend_from_slice(pem);
        out.extend_from_slice(&(raw.len() as u16).to_le_bytes());
        out.extend_from_slice(raw);
        out
    }

    /// 解码并验签：用文件内嵌 key 校验 JWT 签名 + exp（与 `CapabilityToken::verify` 同语义）。
    /// 篡改 key/claims/signature 任一区域、截断/多余字节、错误 key、过期均返回错误。
    pub fn decode(bytes: &[u8]) -> Result<(Ed25519VerifyingKey, CapabilityToken), crate::LinkError> {
        let err = |m: String| crate::LinkError::Token(format!("token file: {m}"));
        if bytes.len() < 9 || &bytes[..4] != Self::MAGIC {
            return Err(err("bad magic or too short".into()));
        }
        if bytes[4] != Self::VERSION {
            return Err(err(format!("unsupported version {}", bytes[4])));
        }
        let key_len = u16::from_le_bytes([bytes[5], bytes[6]]) as usize;
        let rest = &bytes[7..];
        if rest.len() < key_len + 2 {
            return Err(err("truncated key".into()));
        }
        let (key_pem, rest) = rest.split_at(key_len);
        let tok_len = u16::from_le_bytes([rest[0], rest[1]]) as usize;
        let rest = &rest[2..];
        if rest.len() != tok_len {
            return Err(err("truncated token".into()));
        }
        let token_str = String::from_utf8(rest.to_vec()).map_err(|e| err(e.to_string()))?;
        let key = Ed25519VerifyingKey::from_pem(key_pem);
        let token = CapabilityToken::from_raw(token_str);
        token.verify(&key)?; // 内嵌验签覆盖全文件（key/claims/sig 任一篡改均失败）+ exp
        Ok((key, token))
    }
}
