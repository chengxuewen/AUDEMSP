//! 信令客户端（WS 连 server，复用 common `SignalingMessage`，PSK 认证）。
//!
//! 协议流程（与 host 一致）：
//! 1. 连接 `{url}/ws`
//! 2. 发送原始 PSK 作为首条文本消息
//! 3. 等待认证确认（`Error { code: 0 }`）
//! 4. 发送 `RoomJoin { room_id, peer_role, stream_id }`
//! 5. 等待 `RoomJoined { room_id, peer_id }`

use futures_util::{SinkExt, StreamExt};
use mediaservo_common::protocol::{PeerRole, SignalingMessage};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::error::LinkError;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// 信令事件。
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SignalEvent {
    /// 已连接并加入房间。
    Connected { room_id: String },
    /// 收到一条信令消息。
    Message(SignalingMessage),
    /// 连接断开。
    Disconnected { reason: String },
    /// 错误（解析失败等）。
    Error(String),
}

/// 信令客户端（每节点一个；connect 建立会话）。
#[derive(Debug, Clone)]
pub struct SignalClient {
    url: String,
    psk: String,
    room_id: String,
    role: PeerRole,
}

impl SignalClient {
    pub fn new(url: &str, psk: &str, room_id: &str, role: PeerRole) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            psk: psk.to_string(),
            room_id: room_id.to_string(),
            role,
        }
    }

    /// 连接 server、PSK 认证、加入房间，返回会话。
    pub async fn connect(&self) -> Result<SignalSession, LinkError> {
        let (ws_stream, _resp) = connect_async(&self.url)
            .await
            .map_err(|e| LinkError::Signal(format!("connect {}: {e}", self.url)))?;
        let (mut sender, mut receiver) = ws_stream.split();

        // Phase 1: PSK 认证
        sender
            .send(Message::Text(self.psk.clone().into()))
            .await
            .map_err(|e| LinkError::Signal(format!("send auth: {e}")))?;
        let auth_msg = receiver
            .next()
            .await
            .ok_or_else(|| LinkError::Signal("connection closed during auth".into()))?
            .map_err(|e| LinkError::Signal(format!("auth read: {e}")))?;
        let auth_msg = match auth_msg {
            Message::Text(t) => serde_json::from_str::<SignalingMessage>(&t)
                .map_err(|e| LinkError::Signal(format!("parse auth response: {e}")))?,
            Message::Close(_) => return Err(LinkError::Signal("closed during auth".into())),
            _ => return Err(LinkError::Signal("unexpected auth response".into())),
        };
        match auth_msg {
            SignalingMessage::Error { code, .. } if code == 0 => {}
            SignalingMessage::Error { code, message } => {
                return Err(LinkError::Signal(format!("auth denied [{code}]: {message}")));
            }
            _ => return Err(LinkError::Signal("unexpected auth message".into())),
        }

        // Phase 2: 加入房间
        let join = SignalingMessage::RoomJoin {
            room_id: self.room_id.clone(),
            peer_role: self.role.clone(),
            stream_id: None,
        };
        let join_json = serde_json::to_string(&join)
            .map_err(|e| LinkError::Signal(format!("serialize RoomJoin: {e}")))?;
        sender
            .send(Message::Text(join_json.into()))
            .await
            .map_err(|e| LinkError::Signal(format!("send RoomJoin: {e}")))?;
        let joined = receiver
            .next()
            .await
            .ok_or_else(|| LinkError::Signal("connection closed during room join".into()))?
            .map_err(|e| LinkError::Signal(format!("RoomJoin read: {e}")))?;
        let joined = match joined {
            Message::Text(t) => serde_json::from_str::<SignalingMessage>(&t)
                .map_err(|e| LinkError::Signal(format!("parse RoomJoined: {e}")))?,
            Message::Close(_) => return Err(LinkError::Signal("closed during room join".into())),
            _ => return Err(LinkError::Signal("unexpected RoomJoined response".into())),
        };
        match joined {
            SignalingMessage::RoomJoined { room_id, .. } => {
                let (events_tx, _) = broadcast::channel(64);
                let (send_tx, send_rx) = mpsc::unbounded_channel();
                let task = tokio::spawn(session_task(receiver, sender, send_rx, events_tx.clone()));
                let _ = events_tx.send(SignalEvent::Connected { room_id: room_id.clone() });
                Ok(SignalSession {
                    room_id,
                    send_tx,
                    events_tx,
                    task,
                })
            }
            SignalingMessage::Error { code, message } => {
                Err(LinkError::Signal(format!("room join failed [{code}]: {message}")))
            }
            _ => Err(LinkError::Signal("unexpected response to RoomJoin".into())),
        }
    }
}

/// 信令会话：send 发送消息，events 接收事件。
pub struct SignalSession {
    room_id: String,
    send_tx: mpsc::UnboundedSender<SignalingMessage>,
    events_tx: broadcast::Sender<SignalEvent>,
    task: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for SignalSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalSession")
            .field("room_id", &self.room_id)
            .finish()
    }
}

impl SignalSession {
    /// 订阅信令事件（返回新接收器）。
    pub fn events(&self) -> broadcast::Receiver<SignalEvent> {
        self.events_tx.subscribe()
    }

    /// 发送一条信令消息（JSON 序列化后经 WS 发出）。
    pub async fn send(&self, msg: SignalingMessage) -> Result<(), LinkError> {
        self.send_tx
            .send(msg)
            .map_err(|_| LinkError::Signal("session closed".into()))
    }

    /// 当前房间 ID。
    pub fn room_id(&self) -> &str {
        &self.room_id
    }

    /// 关闭会话：停止发送通道并等待后台任务退出。
    pub async fn close(self) -> Result<(), LinkError> {
        drop(self.send_tx); // 触发后台任务退出
        let _ = self.task.await;
        Ok(())
    }
}

/// 后台任务：WS 读 → events；send 通道 → WS 写。
async fn session_task(
    mut ws_rx: futures_util::stream::SplitStream<WsStream>,
    mut ws_tx: futures_util::stream::SplitSink<WsStream, Message>,
    mut send_rx: mpsc::UnboundedReceiver<SignalingMessage>,
    events_tx: broadcast::Sender<SignalEvent>,
) {
    loop {
        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<SignalingMessage>(&text) {
                            Ok(m) => { let _ = events_tx.send(SignalEvent::Message(m)); }
                            Err(e) => { let _ = events_tx.send(SignalEvent::Error(format!("parse message: {e}"))); }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        let _ = events_tx.send(SignalEvent::Disconnected { reason: "connection closed".into() });
                        break;
                    }
                    Some(Ok(_)) => {} // 忽略非文本
                    Some(Err(e)) => {
                        let _ = events_tx.send(SignalEvent::Error(e.to_string()));
                        break;
                    }
                }
            }
            msg = send_rx.recv() => {
                match msg {
                    Some(m) => {
                        let json = match serde_json::to_string(&m) {
                            Ok(j) => j,
                            Err(e) => {
                                let _ = events_tx.send(SignalEvent::Error(format!("serialize: {e}")));
                                continue;
                            }
                        };
                        if ws_tx.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break, // 会话关闭
                }
            }
        }
    }
}
