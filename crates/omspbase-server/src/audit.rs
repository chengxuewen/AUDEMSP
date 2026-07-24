//! Structured audit logging for security-relevant server events.
//!
//! All audit events are emitted as structured JSON via `tracing` so they
//! flow into the same observability pipeline as operational logs.
//!
//! Event types:
//! - `RoomCreate` / `RoomDestroy` — room lifecycle
//! - `PeerJoin` / `PeerLeave` — peer lifecycle within a room
//! - `AuthSuccess` / `AuthFailure` — authentication outcomes
//! - `DeviceOnline` / `DeviceOffline` — device lifecycle
//! - `StreamCreate` / `StreamDestroy` — stream lifecycle
//! - `ConsumerJoin` / `ConsumerLeave` — stream consumer lifecycle

/// Audit event variants covering all security-relevant server operations.
#[derive(Debug, Clone)]
pub enum AuditEvent {
    /// A new room was created.
    RoomCreate { room_id: String },
    /// A room was destroyed (last peer left).
    RoomDestroy { room_id: String },
    /// A peer joined a room.
    PeerJoin {
        peer_id: String,
        room_id: String,
        role: String,
    },
    /// A peer left a room.
    PeerLeave {
        peer_id: String,
        room_id: String,
    },
    /// PSK authentication succeeded.
    AuthSuccess { peer_id: String },
    /// PSK authentication failed.
    AuthFailure { peer_id: String, reason: String },
    /// A device came online.
    DeviceOnline { device_id: String },
    /// A device went offline.
    DeviceOffline { device_id: String },
    /// A media stream was created.
    StreamCreate { stream_id: String, device_id: String },
    /// A media stream was destroyed.
    StreamDestroy { stream_id: String },
    /// A peer started consuming a stream.
    ConsumerJoin { stream_id: String, peer_id: String },
    /// A peer stopped consuming a stream.
    ConsumerLeave { stream_id: String, peer_id: String },
}

/// Emit an audit event as a structured `tracing` info-level log.
///
/// The `audit.event` field is used as the JSON key for downstream filtering
/// (e.g., log aggregation, SIEM ingestion).
pub fn log_event(event: AuditEvent) {
    match event {
        AuditEvent::RoomCreate { room_id } => {
            tracing::info!(
                audit.event = "room_create",
                room_id = %room_id,
                "Room created"
            );
        }
        AuditEvent::RoomDestroy { room_id } => {
            tracing::info!(
                audit.event = "room_destroy",
                room_id = %room_id,
                "Room destroyed"
            );
        }
        AuditEvent::PeerJoin {
            peer_id,
            room_id,
            role,
        } => {
            tracing::info!(
                audit.event = "peer_join",
                peer_id = %peer_id,
                room_id = %room_id,
                role = %role,
                "Peer joined room"
            );
        }
        AuditEvent::PeerLeave {
            peer_id,
            room_id,
        } => {
            tracing::info!(
                audit.event = "peer_leave",
                peer_id = %peer_id,
                room_id = %room_id,
                "Peer left room"
            );
        }
        AuditEvent::AuthSuccess { peer_id } => {
            tracing::info!(
                audit.event = "auth_success",
                peer_id = %peer_id,
                "Authentication succeeded"
            );
        }
        AuditEvent::AuthFailure { peer_id, reason } => {
            tracing::warn!(
                audit.event = "auth_failure",
                peer_id = %peer_id,
                reason = %reason,
                "Authentication failed"
            );
        }
        AuditEvent::DeviceOnline { device_id } => {
            tracing::info!(
                audit.event = "device_online",
                device_id = %device_id,
                "Device came online"
            );
        }
        AuditEvent::DeviceOffline { device_id } => {
            tracing::info!(
                audit.event = "device_offline",
                device_id = %device_id,
                "Device went offline"
            );
        }
        AuditEvent::StreamCreate {
            stream_id,
            device_id,
        } => {
            tracing::info!(
                audit.event = "stream_create",
                stream_id = %stream_id,
                device_id = %device_id,
                "Stream created"
            );
        }
        AuditEvent::StreamDestroy { stream_id } => {
            tracing::info!(
                audit.event = "stream_destroy",
                stream_id = %stream_id,
                "Stream destroyed"
            );
        }
        AuditEvent::ConsumerJoin {
            stream_id,
            peer_id,
        } => {
            tracing::info!(
                audit.event = "consumer_join",
                stream_id = %stream_id,
                peer_id = %peer_id,
                "Consumer joined stream"
            );
        }
        AuditEvent::ConsumerLeave {
            stream_id,
            peer_id,
        } => {
            tracing::info!(
                audit.event = "consumer_leave",
                stream_id = %stream_id,
                peer_id = %peer_id,
                "Consumer left stream"
            );
        }
    }
}
