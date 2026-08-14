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
}
