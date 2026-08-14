//! FrameBus（iceoryx2 SHM 零拷贝帧总线，latest-frame 覆盖语义）。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use iceoryx2::port::publisher::Publisher;
use iceoryx2::prelude::*;

use crate::acl::NodeAcl;
use crate::error::LinkError;
use crate::frame::{FrameMeta, FrameRef, FrameStream, StreamInner};
use crate::id::{FrameTopic, NodeId};
use crate::registry::{NodeInfo, Registry};
use crate::token::{CapabilityToken, Ed25519VerifyingKey};

/// 单帧字节上限（1080p I420 ≈ 3.1MB，留余量）。
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

type TopicPublisher = Publisher<ipc_threadsafe::Service, [u8], ()>;

/// 帧总线（每节点一个实例；attach 即注册，D235）。
pub struct FrameBus {
    node: Node<ipc_threadsafe::Service>,
    acl: NodeAcl,
    node_id: NodeId,
    /// 已创建流的内部状态（close 时 shutdown）。
    streams: Mutex<Vec<Arc<StreamInner>>>,
    /// 缓存 publisher（持有以维持 max_publishers(1) 单发布者锁 + 交付可靠）。
    publishers: Mutex<HashMap<String, TopicPublisher>>,
}

impl FrameBus {
    /// attach 即注册（D235）：验签（fail-closed）→ 载 ACL → iceoryx2 节点 → `Registry::register`。
    pub fn attach(
        endpoint: &str,
        token: &CapabilityToken,
        verifying_key: &Ed25519VerifyingKey,
    ) -> Result<Self, LinkError> {
        let _ = endpoint; // Phase 1 预留（iceoryx2 用全局 config，无显式 endpoint）
        let claims = token
            .verify(verifying_key)
            .map_err(|e| LinkError::Attach(e.to_string()))?;
        let acl = claims.acl.clone();
        let node_id = NodeId::new(claims.node_id.clone());
        let node = NodeBuilder::new()
            .create::<ipc_threadsafe::Service>()
            .map_err(|e| LinkError::Attach(format!("create iceoryx2 node: {e:?}")))?;
        Registry::register(&NodeInfo::from_claims(&claims))
            .map_err(|e| LinkError::Attach(e.to_string()))?;
        Ok(Self {
            node,
            acl,
            node_id,
            streams: Mutex::new(Vec::new()),
            publishers: Mutex::new(HashMap::new()),
        })
    }

    /// 统一 topic service 构造（pub/sub 必须同配置，审核 C1）：
    /// buffer_size=1 + enable_safe_overflow(true) → latest-frame 覆盖；
    /// max_publishers(1) → 单发布者兜底（D239，跨进程由 iceoryx2 强制）。
    fn topic_service(
        &self,
        topic: &FrameTopic,
    ) -> Result<iceoryx2::service::port_factory::publish_subscribe::PortFactory<ipc_threadsafe::Service, [u8], ()>, LinkError> {
        let name = topic
            .as_str()
            .try_into()
            .map_err(|_| LinkError::Bus(format!("invalid topic name: {}", topic.as_str())))?;
        // open_or_create 可能遇 SystemInFlux 瞬态（服务正被并发创建/销毁），重试
        let mut last_err = None;
        for _ in 0..5 {
            match self
                .node
                .service_builder(&name)
                .publish_subscribe::<[u8]>()
                .subscriber_max_buffer_size(1)
                .enable_safe_overflow(true)
                .max_publishers(1)
                .open_or_create()
            {
                Ok(service) => return Ok(service),
                Err(e) => {
                    last_err = Some(format!("{e:?}"));
                    if format!("{e:?}").contains("SystemInFlux") {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        continue;
                    }
                    return Err(LinkError::Bus(format!("open topic service: {e:?}")));
                }
            }
        }
        Err(LinkError::Bus(format!(
            "open topic service failed after retries: {:?}",
            last_err
        )))
    }

    /// 发布一帧：ACL 检查 → 单发布者检查 → 缓存 publisher → loan 写入 SHM → send → 记录活跃发布者。
    pub fn publish(&self, topic: &FrameTopic, payload: &[u8], meta: &FrameMeta) -> Result<(), LinkError> {
        if !self.acl.can_publish(topic) {
            return Err(LinkError::AclDenied {
                topic: topic.as_str().into(),
            });
        }
        // D239 单发布者：该 topic 已有其他节点的活跃发布者 → 冲突（进程本地快速检查）
        if let Some(existing) = Registry::topic_publisher(topic).map_err(|e| LinkError::Bus(e.to_string()))? {
            if existing != self.node_id {
                return Err(LinkError::TopicConflict {
                    topic: topic.as_str().into(),
                });
            }
        }
        let buf_len = FrameMeta::WIRE_LEN + payload.len();
        if buf_len > MAX_FRAME_BYTES {
            return Err(LinkError::Bus(format!(
                "frame too large: {buf_len} > {MAX_FRAME_BYTES}"
            )));
        }
        // 取或建缓存 publisher（持有它维持 max_publishers(1) 锁；跨进程冲突在此失败）
        let mut publishers = self
            .publishers
            .lock()
            .map_err(|_| LinkError::Bus("publishers lock poisoned".into()))?;
        if !publishers.contains_key(topic.as_str()) {
            let service = self.topic_service(topic)?;
            let publisher = service
                .publisher_builder()
                .initial_max_slice_len(MAX_FRAME_BYTES)
                .allocation_strategy(AllocationStrategy::Static)
                .create()
                .map_err(|e| {
                    tracing::warn!(topic = %topic.as_str(), "publisher create failed: {e:?}");
                    // iceoryx2 max_publishers(1) 兜底：已有发布者（跨进程）
                    LinkError::TopicConflict {
                        topic: topic.as_str().into(),
                    }
                })?;
            publishers.insert(topic.as_str().to_string(), publisher);
        }
        let publisher = publishers.get(topic.as_str()).expect("just inserted");
        let mut buf = Vec::with_capacity(buf_len);
        buf.extend_from_slice(&meta.encode());
        buf.extend_from_slice(payload);
        let sample = publisher
            .loan_slice_uninit(buf_len)
            .map_err(|e| LinkError::Bus(format!("loan: {e:?}")))?;
        let sample = sample.write_from_slice(&buf);
        sample.send().map_err(|e| LinkError::Bus(format!("send: {e:?}")))?;
        Registry::mark_publisher(topic, &self.node_id).map_err(|e| LinkError::Bus(e.to_string()))?;
        Ok(())
    }

    /// 订阅一个 topic：ACL 检查 → subscriber（buffer_size=1）→ 后台线程投递 latest-slot。
    pub fn subscribe(&self, topic: &FrameTopic) -> Result<FrameStream, LinkError> {
        if !self.acl.can_subscribe(topic) {
            return Err(LinkError::AclDenied {
                topic: topic.as_str().into(),
            });
        }
        let service = self.topic_service(topic)?;
        let subscriber = service
            .subscriber_builder()
            .buffer_size(1)
            .create()
            .map_err(|e| LinkError::Bus(format!("subscriber create: {e:?}")))?;
        let stream = FrameStream::new();
        let weak = stream.weak_inner();
        let topic_for_thread = topic.clone();
        // 后台线程：receive() → 解析 meta+payload → deliver latest-slot（流丢弃即退出）
        // PortFactory(service) 移入线程，使 subscriber 的借用在线程内有效
        std::thread::spawn(move || {
            let _service = service;
            loop {
                let Some(inner) = weak.upgrade() else { break };
                match subscriber.receive() {
                    Ok(Some(sample)) => {
                        let data: &[u8] = &*sample;
                        if data.len() >= FrameMeta::WIRE_LEN {
                            if let Ok(meta) = FrameMeta::decode(&data[..FrameMeta::WIRE_LEN]) {
                                // Phase 1：一拷贝进 owned Vec（Phase 2 可持 Sample 实现真零拷贝）
                                let payload = data[FrameMeta::WIRE_LEN..].to_vec();
                                inner.deliver(FrameRef::new(meta, payload));
                            }
                        }
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(1)),
                    Err(e) => {
                        tracing::warn!(topic = %topic_for_thread.as_str(), "subscribe receive error: {e:?}");
                        break;
                    }
                }
            }
        });
        self.streams
            .lock()
            .map_err(|_| LinkError::Bus("streams lock poisoned".into()))?
            .push(stream.inner());
        Ok(stream)
    }

    /// 本节点 ID。
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// 关闭：shutdown 全部流（recv 返回 None）+ 注销节点（释放 publishers 与单发布者锁）。
    pub fn close(self) -> Result<(), LinkError> {
        let streams = self
            .streams
            .lock()
            .map_err(|_| LinkError::Bus("streams lock poisoned".into()))?;
        for s in streams.iter() {
            s.shutdown();
        }
        Registry::unregister(&self.node_id).map_err(|e| LinkError::Bus(e.to_string()))?;
        Ok(())
    }
}
