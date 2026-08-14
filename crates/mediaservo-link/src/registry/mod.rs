//! 节点注册/发现（D235：attach 即注册，去中心化，无 daemon）。
//!
//! Phase 1：进程本地注册表 + 活跃发布者追踪。
//! 跨进程节点元数据共享 Phase 2 完善；单发布者由 iceoryx2 `max_publishers(1)` 兜底。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::acl::Role;
use crate::error::LinkError;
use crate::id::{FrameTopic, NodeId};
use crate::token::Claims;

/// 节点自描述信息。
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: NodeId,
    pub role: Role,
    /// 允许发布的 topic 模式（来自 ACL，如 `"camera/*"`）。
    pub publishes: Vec<String>,
    /// 订阅的 topic 模式。
    pub subscribes: Vec<String>,
}

impl NodeInfo {
    /// 从令牌 claims 构造（attach 即注册）。
    pub fn from_claims(claims: &Claims) -> Self {
        Self {
            id: NodeId::new(claims.node_id.clone()),
            role: claims.role,
            publishes: claims.acl.publish_allow.clone(),
            subscribes: claims.acl.subscribe_allow.clone(),
        }
    }
}

/// topic 发现信息。
#[derive(Debug, Clone)]
pub struct TopicInfo {
    pub topic: FrameTopic,
    pub publisher: NodeId,
}

/// 注册中心（去中心化，无 daemon，D235）。
pub struct Registry;

static NODES: OnceLock<Mutex<HashMap<NodeId, NodeInfo>>> = OnceLock::new();
/// 活跃发布者：topic -> 正在发布它的节点（单发布者检查用）。
static PUBLISHERS: OnceLock<Mutex<HashMap<FrameTopic, NodeId>>> = OnceLock::new();

fn nodes() -> &'static Mutex<HashMap<NodeId, NodeInfo>> {
    NODES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn publishers() -> &'static Mutex<HashMap<FrameTopic, NodeId>> {
    PUBLISHERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_err() -> LinkError {
    LinkError::Registry("registry lock poisoned".into())
}

impl Registry {
    /// 注册节点（attach 即注册，由 `FrameBus::attach` 调用）。
    pub fn register(info: &NodeInfo) -> Result<(), LinkError> {
        nodes().lock().map_err(|_| lock_err())?.insert(info.id.clone(), info.clone());
        Ok(())
    }

    /// 注销节点（`FrameBus::close` 调用，尽力而为）。
    pub fn unregister(id: &NodeId) -> Result<(), LinkError> {
        nodes().lock().map_err(|_| lock_err())?.remove(id);
        publishers().lock().map_err(|_| lock_err())?.retain(|_, n| n != id);
        Ok(())
    }

    /// 记录活跃发布者（`FrameBus::publish` 成功后调用）。
    pub fn mark_publisher(topic: &FrameTopic, id: &NodeId) -> Result<(), LinkError> {
        publishers().lock().map_err(|_| lock_err())?.insert(topic.clone(), id.clone());
        Ok(())
    }

    /// 发现发布指定前缀 topic 的 `(topic, publisher)`。
    ///
    /// Phase 1：进程本地注册表，topic 取节点声明的发布模式。
    pub fn discover_topics(prefix: &str) -> Result<Vec<TopicInfo>, LinkError> {
        let reg = nodes().lock().map_err(|_| lock_err())?;
        let mut out = Vec::new();
        for info in reg.values() {
            for p in &info.publishes {
                if p.starts_with(prefix) {
                    out.push(TopicInfo {
                        topic: FrameTopic::new(p.clone()),
                        publisher: info.id.clone(),
                    });
                }
            }
        }
        Ok(out)
    }

    /// 发现指定 role 的节点（Phase 1：进程本地注册表）。
    pub fn discover_nodes(role: Role) -> Result<Vec<NodeInfo>, LinkError> {
        let reg = nodes().lock().map_err(|_| lock_err())?;
        Ok(reg.values().filter(|i| i.role == role).cloned().collect())
    }

    /// 某 topic 的**活跃**发布者（单发布者检查；跨进程由 iceoryx2 `max_publishers(1)` 兜底）。
    pub fn topic_publisher(topic: &FrameTopic) -> Result<Option<NodeId>, LinkError> {
        Ok(publishers().lock().map_err(|_| lock_err())?.get(topic).cloned())
    }
}
