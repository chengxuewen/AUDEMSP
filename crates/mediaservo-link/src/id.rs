//! 品牌化 ID（防串参）。

/// 节点 ID。
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NodeId(String);

impl NodeId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 帧 topic（如 `"camera/front/raw"`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FrameTopic(String);

impl FrameTopic {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 通配匹配：pattern `"camera/*"` 匹配 `"camera/front/raw"`；精确匹配也成立。
    pub fn matches(&self, pattern: &str) -> bool {
        if let Some(pfx) = pattern.strip_suffix("/*") {
            self.0.starts_with(&format!("{pfx}/"))
        } else {
            self.0 == pattern
        }
    }
}
