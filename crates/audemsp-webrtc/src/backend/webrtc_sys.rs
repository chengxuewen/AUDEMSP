//! webrtc-sys backend — wraps libwebrtc via LiveKit's webrtc-sys FFI crate.
//!
//! Enabled via `backend-webrtc-sys` feature.
//! Uses tokio::sync::oneshot channels to convert callback-based FFI to async.
//!
//! Backend types:
//! - WebrtcSysPc: wraps webrtc_sys::peer_connection::ffi::PeerConnection
//! - WebrtcSysDc: wraps webrtc_sys::data_channel::ffi::DataChannel
//! - WebrtcSysTrack: real video track via VideoTrackSource (webrtc-sys)
//! - WebrtcSysFactory: wraps webrtc_sys::peer_connection_factory::ffi::PeerConnectionFactory

use std::sync::{Arc, Mutex};
use cxx::SharedPtr;

use super::DcBackend;
use super::PcBackend;
use super::TrackWriteBackend;
use crate::data_channel::{RTCDataChannelRx, RTCDataChannelState};
use crate::peer_connection::{
    RTCAnswerOptions, RTCIceCandidate, RTCIceConnectionState, RTCIceGatheringState, RTCIceTransportPolicy,
    RTCOfferOptions, RTCConfiguration, RTCPeerConnectionState, RTCSignalingState,
};
use crate::sdp::{RTCSdpType, RTCSessionDescription};
use crate::track::{RTCAudioTrackConfig, TrackKind, TrackReceiver};
use crate::RTCError;

// ── WebrtcSysPc ──

#[derive(Clone)]
pub(crate) struct WebrtcSysPc {
    pc: cxx::SharedPtr<webrtc_sys::peer_connection::ffi::PeerConnection>,
    callbacks: Arc<ObserverCallbacks>,
    local_sdp: Arc<std::sync::Mutex<Option<String>>>,
}

impl std::fmt::Debug for WebrtcSysPc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebrtcSysPc")
            .field("connection_state", &self.connection_state())
            .finish()
    }
}

/// Helper: wrap a oneshot sender in a PeerContext for FFI callbacks.
fn make_ctx<T: Send + 'static>(
    tx: tokio::sync::oneshot::Sender<T>,
) -> Box<webrtc_sys::peer_connection::PeerContext> {
    Box::new(webrtc_sys::peer_connection::PeerContext(Box::new(tx)))
}

/// Helper: extract a oneshot sender from a PeerContext via downcast.
fn extract_tx<T: Send + 'static>(
    ctx: Box<webrtc_sys::peer_connection::PeerContext>,
) -> tokio::sync::oneshot::Sender<T> {
    *ctx.0
        .downcast::<tokio::sync::oneshot::Sender<T>>()
        .unwrap_or_else(|_| panic!("PeerContext downcast failed"))
}


// ponytail: map webrtc-sys RTCSdpType → crate RTCSdpType inline
fn map_sdp_type(st: webrtc_sys::jsep::ffi::SdpType) -> RTCSdpType {
    match st {
        webrtc_sys::jsep::ffi::SdpType::Offer => RTCSdpType::Offer,
        webrtc_sys::jsep::ffi::SdpType::Answer => RTCSdpType::Answer,
        webrtc_sys::jsep::ffi::SdpType::PrAnswer => RTCSdpType::PrAnswer,
        webrtc_sys::jsep::ffi::SdpType::Rollback => RTCSdpType::Rollback,
        _ => RTCSdpType::Offer, // ponytail: defensive fallback
    }
}

impl PcBackend for WebrtcSysPc {
    async fn create_offer(&self, options: &RTCOfferOptions) -> Result<RTCSessionDescription, RTCError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let ctx = make_ctx(tx);

        let mut opts = webrtc_sys::peer_connection::ffi::RtcOfferAnswerOptions::default();
        // ponytail: ICE restart toggle; full options mapping deferred
        if options.ice_restart {
            opts.ice_restart = true;
        }

        self.pc.create_offer(
            opts,
            ctx,
            |ctx, sdp| {
                let tx: tokio::sync::oneshot::Sender<Result<RTCSessionDescription, RTCError>> =
                    extract_tx(ctx);
                let sdp_type = map_sdp_type(sdp.sdp_type());
                let sdp_str = sdp.stringify();
                let _ = tx.send(Ok(RTCSessionDescription {
                    sdp_type,
                    sdp: sdp_str,
                }));
            },
            |ctx, error| {
                let tx: tokio::sync::oneshot::Sender<Result<RTCSessionDescription, RTCError>> =
                    extract_tx(ctx);
                let _ = tx.send(Err(RTCError::Internal(error.message)));
            },
        );

        rx.await
            .map_err(|_| RTCError::Internal("oneshot cancelled".into()))?
    }

    async fn create_answer(
        &self,
        _options: &RTCAnswerOptions,
    ) -> Result<RTCSessionDescription, RTCError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let ctx = make_ctx(tx);

        let opts = webrtc_sys::peer_connection::ffi::RtcOfferAnswerOptions::default();
        // ponytail: RTCAnswerOptions has no fields currently, pass defaults

        self.pc.create_answer(
            opts,
            ctx,
            |ctx, sdp| {
                let tx: tokio::sync::oneshot::Sender<Result<RTCSessionDescription, RTCError>> =
                    extract_tx(ctx);
                let sdp_type = map_sdp_type(sdp.sdp_type());
                let sdp_str = sdp.stringify();
                let _ = tx.send(Ok(RTCSessionDescription {
                    sdp_type,
                    sdp: sdp_str,
                }));
            },
            |ctx, error| {
                let tx: tokio::sync::oneshot::Sender<Result<RTCSessionDescription, RTCError>> =
                    extract_tx(ctx);
                let _ = tx.send(Err(RTCError::Internal(error.message)));
            },
        );

        rx.await
            .map_err(|_| RTCError::Internal("oneshot cancelled".into()))?
    }

    async fn set_local_description(&self, desc: &RTCSessionDescription) -> Result<(), RTCError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let ctx = make_ctx(tx);

        let sdp_type = match desc.sdp_type {
            RTCSdpType::Offer => webrtc_sys::jsep::ffi::SdpType::Offer,
            RTCSdpType::Answer => webrtc_sys::jsep::ffi::SdpType::Answer,
            RTCSdpType::PrAnswer => webrtc_sys::jsep::ffi::SdpType::PrAnswer,
            RTCSdpType::Rollback => webrtc_sys::jsep::ffi::SdpType::Rollback,
        };

        let sd = webrtc_sys::jsep::ffi::create_session_description(sdp_type, desc.sdp.clone())
            .map_err(|e| RTCError::Sdp(e.what().to_owned()))?;

        // ponytail: set_local_description has a single on_complete callback (ctx, error)
        self.pc.set_local_description(sd, ctx, |ctx, error| {
            let tx: tokio::sync::oneshot::Sender<Result<(), RTCError>> = extract_tx(ctx);
            if error.ok() {
                let _ = tx.send(Ok(()));
            } else {
                let _ = tx.send(Err(RTCError::Sdp(error.message)));
            }
        });

        let _result = rx.await
            .map_err(|_| RTCError::Internal("oneshot cancelled".into()))?;

        *self.local_sdp.lock().unwrap() = Some(desc.sdp.clone());

        _result
    }

    async fn set_remote_description(&self, desc: &RTCSessionDescription) -> Result<(), RTCError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let ctx = make_ctx(tx);

        let sdp_type = match desc.sdp_type {
            RTCSdpType::Offer => webrtc_sys::jsep::ffi::SdpType::Offer,
            RTCSdpType::Answer => webrtc_sys::jsep::ffi::SdpType::Answer,
            RTCSdpType::PrAnswer => webrtc_sys::jsep::ffi::SdpType::PrAnswer,
            RTCSdpType::Rollback => webrtc_sys::jsep::ffi::SdpType::Rollback,
        };

        let sd = webrtc_sys::jsep::ffi::create_session_description(sdp_type, desc.sdp.clone())
            .map_err(|e| RTCError::Sdp(e.what().to_owned()))?;

        self.pc.set_remote_description(sd, ctx, |ctx, error| {
            let tx: tokio::sync::oneshot::Sender<Result<(), RTCError>> = extract_tx(ctx);
            if error.ok() {
                let _ = tx.send(Ok(()));
            } else {
                let _ = tx.send(Err(RTCError::Sdp(error.message)));
            }
        });

        rx.await
            .map_err(|_| RTCError::Internal("oneshot cancelled".into()))?
    }

    async fn add_ice_candidate(&self, candidate: &RTCIceCandidate) -> Result<(), RTCError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let ctx = make_ctx(tx);

        let ic = webrtc_sys::jsep::ffi::create_ice_candidate(
            candidate.sdp_mid.clone().unwrap_or_default(),
            candidate.sdp_mline_index.map(|v| v as i32).unwrap_or(0),
            candidate.candidate.clone(),
        )
        .map_err(|e| RTCError::RTCPeerConnection(e.what().to_owned()))?;

        self.pc.add_ice_candidate(ic, ctx, |ctx, error| {
            let tx: tokio::sync::oneshot::Sender<Result<(), RTCError>> = extract_tx(ctx);
            if error.ok() {
                let _ = tx.send(Ok(()));
            } else {
                let _ = tx.send(Err(RTCError::RTCPeerConnection(error.message)));
            }
        });

        rx.await
            .map_err(|_| RTCError::Internal("oneshot cancelled".into()))?
    }

    fn connection_state(&self) -> RTCPeerConnectionState {
        match self.pc.connection_state() {
            webrtc_sys::peer_connection::ffi::PeerConnectionState::New => RTCPeerConnectionState::New,
            webrtc_sys::peer_connection::ffi::PeerConnectionState::Connecting => {
                RTCPeerConnectionState::Connecting
            }
            webrtc_sys::peer_connection::ffi::PeerConnectionState::Connected => {
                RTCPeerConnectionState::Connected
            }
            webrtc_sys::peer_connection::ffi::PeerConnectionState::Disconnected => {
                RTCPeerConnectionState::Disconnected
            }
            webrtc_sys::peer_connection::ffi::PeerConnectionState::Failed => {
                RTCPeerConnectionState::Failed
            }
            webrtc_sys::peer_connection::ffi::PeerConnectionState::Closed => {
                RTCPeerConnectionState::Closed
            }
            _ => RTCPeerConnectionState::New, // ponytail: defensive fallback
        }
    }

    fn ice_connection_state(&self) -> RTCIceConnectionState {
        match self.pc.ice_connection_state() {
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionNew => {
                RTCIceConnectionState::New
            }
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionChecking => {
                RTCIceConnectionState::Checking
            }
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionConnected => {
                RTCIceConnectionState::Connected
            }
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionCompleted => {
                RTCIceConnectionState::Completed
            }
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionFailed => {
                RTCIceConnectionState::Failed
            }
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionDisconnected => {
                RTCIceConnectionState::Disconnected
            }
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionClosed => {
                RTCIceConnectionState::Closed
            }
            _ => RTCIceConnectionState::New, // ponytail: defensive fallback
        }
    }

    fn ice_gathering_state(&self) -> RTCIceGatheringState {
        match self.pc.ice_gathering_state() {
            webrtc_sys::peer_connection::ffi::IceGatheringState::IceGatheringNew => {
                RTCIceGatheringState::New
            }
            webrtc_sys::peer_connection::ffi::IceGatheringState::IceGatheringGathering => {
                RTCIceGatheringState::Gathering
            }
            webrtc_sys::peer_connection::ffi::IceGatheringState::IceGatheringComplete => {
                RTCIceGatheringState::Complete
            }
            _ => RTCIceGatheringState::New, // ponytail: defensive fallback
        }
    }

    fn signaling_state(&self) -> RTCSignalingState {
        match self.pc.signaling_state() {
            webrtc_sys::peer_connection::ffi::SignalingState::Stable => RTCSignalingState::Stable,
            webrtc_sys::peer_connection::ffi::SignalingState::HaveLocalOffer => {
                RTCSignalingState::HaveLocalOffer
            }
            webrtc_sys::peer_connection::ffi::SignalingState::HaveLocalPrAnswer => {
                RTCSignalingState::HaveLocalPrAnswer
            }
            webrtc_sys::peer_connection::ffi::SignalingState::HaveRemoteOffer => {
                RTCSignalingState::HaveRemoteOffer
            }
            webrtc_sys::peer_connection::ffi::SignalingState::HaveRemotePrAnswer => {
                RTCSignalingState::HaveRemotePrAnswer
            }
            webrtc_sys::peer_connection::ffi::SignalingState::Closed => RTCSignalingState::Closed,
            _ => RTCSignalingState::Stable, // ponytail: defensive fallback
        }
    }

    async fn close(&self) {
        self.pc.close();
    }
    /// Override: store the on_track callback so RealObserver can invoke it.
    fn set_on_track(&self, cb: Box<dyn Fn(TrackReceiver) + Send + Sync + 'static>) {
        *self.callbacks.on_track.lock().unwrap() = Some(cb);
    }


    fn set_on_ice_connection_state_change(
        &self,
        cb: Box<dyn Fn(RTCIceConnectionState) + Send + Sync + 'static>,
    ) {
        *self.callbacks.on_ice_connection_state_change.lock().unwrap() = Some(cb);
    }

    fn set_on_peer_connection_state_change(
        &self,
        cb: Box<dyn Fn(RTCPeerConnectionState) + Send + Sync + 'static>,
    ) {
        *self.callbacks.on_peer_connection_state_change.lock().unwrap() = Some(cb);
    }

    fn local_description_sdp(&self) -> Option<String> {
        self.local_sdp.lock().unwrap().clone()
    }

    /// Override: consume a staged media track and add it to libwebrtc.
    fn register_track(
        &self, _track_id: &str, _kind: TrackKind,
    ) -> Result<(), RTCError> {
        let mut guard = self.callbacks.staged_media_tracks.lock().unwrap();
        while let Some(media_track) = guard.pop() {
            self.pc.add_track(media_track, &vec![]);
        }
        Ok(())
    }

}

// ── create_data_channel (method on WebrtcSysPc, called directly by peer.rs) ──

impl WebrtcSysPc {
    /// Stage a media track to be consumed by the next register_track call.
    pub(crate) fn stage_media_track(
        &self,
        track: cxx::SharedPtr<webrtc_sys::media_stream_track::ffi::MediaStreamTrack>,
    ) {
        self.callbacks.staged_media_tracks.lock().unwrap().push(track);
    }

    pub(crate) async fn create_data_channel(
        &self,
        label: &str,
        init: crate::data_channel::RTCDataChannelInit,
    ) -> Result<crate::data_channel::RTCDataChannel, RTCError> {
        use crate::data_channel::RTCDataChannel;

        let sys_init = webrtc_sys::data_channel::ffi::DataChannelInit {
            ordered: init.ordered,
            has_max_retransmit_time: init.max_retransmit_time.is_some(),
            max_retransmit_time: init.max_retransmit_time.unwrap_or(-1),
            has_max_retransmits: init.max_retransmits.is_some(),
            max_retransmits: init.max_retransmits.unwrap_or(-1),
            protocol: init.protocol,
            negotiated: init.negotiated,
            id: init.id,
            has_priority: false,
            priority: webrtc_sys::data_channel::ffi::Priority::Low,
        };

        let dc = self
            .pc
            .create_data_channel(label.to_string(), sys_init)
            .map_err(|e| RTCError::RTCPeerConnection(e.what().to_owned()))?;

        Ok(RTCDataChannel {
            label: label.to_string(),
            id: dc.id(),
            backend: WebrtcSysDc { dc },
        })
    }
}

// ── WebrtcSysDc ──

#[derive(Clone)]
pub(crate) struct WebrtcSysDc {
    dc: cxx::SharedPtr<webrtc_sys::data_channel::ffi::DataChannel>,
}

impl std::fmt::Debug for WebrtcSysDc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebrtcSysDc").finish()
    }
}

impl DcBackend for WebrtcSysDc {
    fn state(&self) -> RTCDataChannelState {
        match self.dc.state() {
            webrtc_sys::data_channel::ffi::DataState::Connecting => RTCDataChannelState::Connecting,
            webrtc_sys::data_channel::ffi::DataState::Open => RTCDataChannelState::Open,
            webrtc_sys::data_channel::ffi::DataState::Closing => RTCDataChannelState::Closing,
            webrtc_sys::data_channel::ffi::DataState::Closed => RTCDataChannelState::Closed,
            _ => RTCDataChannelState::Closed, // ponytail: defensive fallback
        }
    }

    async fn send(&self, data: &[u8]) -> Result<(), RTCError> {
        let buf = webrtc_sys::data_channel::ffi::DataBuffer {
            ptr: data.as_ptr(),
            len: data.len(),
            binary: true,
        };
        // ponytail: webrtc-sys send may fail if channel not open; log and ignore
        self.dc.send(&buf);
        Ok(())
    }

    async fn send_text(&self, text: &str) -> Result<(), RTCError> {
        let buf = webrtc_sys::data_channel::ffi::DataBuffer {
            ptr: text.as_ptr(),
            len: text.len(),
            binary: false,
        };
        self.dc.send(&buf);
        Ok(())
    }

    async fn spool(&self) -> RTCDataChannelRx {
        // ponytail: observer registration for spool needs a DataChannelObserver — deferred
        RTCDataChannelRx::stub()
    }

    async fn close(&mut self) {
        self.dc.close();
    }
}

// ── WebrtcSysTrack ──


/// webrtc-sys video track backend.
/// Holds a libwebrtc VideoTrackSource for pushing raw I420 frames.
/// libwebrtc handles encoding internally (VP8/H.264).
pub(crate) struct WebrtcSysTrack {
    video_source: Mutex<Option<SharedPtr<webrtc_sys::video_track::ffi::VideoTrackSource>>>,
}

impl Default for WebrtcSysTrack {
    fn default() -> Self {
        Self { video_source: Mutex::new(None) }
    }
}

impl std::fmt::Debug for WebrtcSysTrack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebrtcSysTrack")
            .field("video_source", &self.video_source.lock().unwrap().is_some())
            .finish()
    }
}

impl Clone for WebrtcSysTrack {
    fn clone(&self) -> Self {
        let source = self.video_source.lock().unwrap().clone();
        Self { video_source: Mutex::new(source) }
    }
}

impl WebrtcSysTrack {
    pub(crate) fn with_video_source(
        source: SharedPtr<webrtc_sys::video_track::ffi::VideoTrackSource>,
    ) -> Self {
        Self { video_source: Mutex::new(Some(source)) }
    }
}

impl TrackWriteBackend for WebrtcSysTrack {
    async fn write_frame(
        &self,
        data: &[u8],
        _kind: TrackKind,
        _audio_config: Option<&RTCAudioTrackConfig>,
    ) -> Result<(), RTCError> {
        tracing::debug!(
            "TrackSender::write_frame (webrtc-sys): {} bytes (encoded pass-through)",
            data.len()
        );
        // ponytail: encoded frame passthrough — stub for now, real encoding deferred
        Ok(())
    }

    async fn write_raw_i420(
        &self, data: &[u8], width: u32, height: u32,
    ) -> Result<(), RTCError> {
        use webrtc_sys::video_frame::ffi as vf;
        use webrtc_sys::video_frame_buffer::ffi as vfb;
        use webrtc_sys::video_track::ffi as vt;

        let source = self.video_source.lock().unwrap()
            .clone()
            .ok_or_else(|| RTCError::Track("video source not initialized".into()))?;

        let w: i32 = width as i32;
        let h: i32 = height as i32;
        // I420 layout: Y plane (W×H) + U plane (W/2×H/2) + V plane (W/2×H/2)
        let y_size = (w * h) as usize;
        let uv_size = ((w / 2) * (h / 2)) as usize;
        if data.len() < y_size + 2 * uv_size {
            return Err(RTCError::Track("I420 data too short".into()));
        }

        let i420 = vfb::new_i420_buffer(w, h, w, w / 2, w / 2);

        // SAFETY: I420Buffer owns the memory; slices live within the call scope.
        // The frame builder consumes the buffer via set_video_frame_buffer before build().
        unsafe {
            let yuv = vfb::i420_to_yuv8(&*i420);
            let y_slice = std::slice::from_raw_parts_mut(
                (*yuv).data_y() as *mut u8, y_size,
            );
            let u_slice = std::slice::from_raw_parts_mut(
                (*yuv).data_u() as *mut u8, uv_size,
            );
            let v_slice = std::slice::from_raw_parts_mut(
                (*yuv).data_v() as *mut u8, uv_size,
            );
            y_slice.copy_from_slice(&data[..y_size]);
            u_slice.copy_from_slice(&data[y_size..y_size + uv_size]);
            v_slice.copy_from_slice(&data[y_size + uv_size..y_size + 2 * uv_size]);
        }

        // Build VideoFrame and push to source
        let mut builder = vf::new_video_frame_builder();
        builder.pin_mut().set_timestamp_us(0);
        builder.pin_mut().set_video_frame_buffer(
            // SAFETY: i420 → yuv8 → yuv → vfb upcast chain
            unsafe { &*vfb::yuv_to_vfb(
                vfb::yuv8_to_yuv(vfb::i420_to_yuv8(&*i420))
            ) },
        );
        let frame = builder.pin_mut().build();

        let metadata = vt::FrameMetadata {
            has_packet_trailer: false,
            user_timestamp: 0,
            frame_id: 0,
            user_data: vec![],
        };

        source.on_captured_frame(&frame, &metadata);
        Ok(())
    }
}

// ponytail: WebrtcSysTrack has interior Mutex for VideoTrackSource;
// C++ side handles actual thread safety for on_captured_frame.

// ── RealObserver ──

/// Holds user-registered callbacks and active video sinks.
pub(crate) struct ObserverCallbacks {
    pub on_track: Mutex<Option<Box<dyn Fn(TrackReceiver) + Send + Sync + 'static>>>,
    /// Retain NativeVideoSink references to prevent GC
    pub video_sinks: Mutex<Vec<cxx::SharedPtr<webrtc_sys::video_track::ffi::NativeVideoSink>>>,
    pub on_ice_connection_state_change: Mutex<Option<Box<dyn Fn(RTCIceConnectionState) + Send + Sync + 'static>>>,
    pub on_peer_connection_state_change: Mutex<Option<Box<dyn Fn(RTCPeerConnectionState) + Send + Sync + 'static>>>,
    /// Staged media tracks awaiting register_track consumption (webrtc-sys only).
    pub staged_media_tracks: Mutex<Vec<cxx::SharedPtr<webrtc_sys::media_stream_track::ffi::MediaStreamTrack>>>,
}

/// Real observer that forwards libwebrtc events to Rust callbacks.
struct RealObserver {
    callbacks: Arc<ObserverCallbacks>,
}

impl webrtc_sys::peer_connection_factory::PeerConnectionObserver for RealObserver {
    fn on_signaling_change(&self, _: webrtc_sys::peer_connection::ffi::SignalingState) {}
    fn on_add_stream(&self, _: cxx::SharedPtr<webrtc_sys::media_stream::ffi::MediaStream>) {}
    fn on_remove_stream(&self, _: cxx::SharedPtr<webrtc_sys::media_stream::ffi::MediaStream>) {}
    fn on_data_channel(&self, _: cxx::SharedPtr<webrtc_sys::data_channel::ffi::DataChannel>) {}
    fn on_renegotiation_needed(&self) {}
    fn on_negotiation_needed_event(&self, _: u32) {}
    fn on_ice_connection_change(&self, state: webrtc_sys::peer_connection::ffi::IceConnectionState) {
        let mapped = match state {
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionNew => RTCIceConnectionState::New,
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionChecking => RTCIceConnectionState::Checking,
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionConnected => RTCIceConnectionState::Connected,
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionCompleted => RTCIceConnectionState::Completed,
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionFailed => RTCIceConnectionState::Failed,
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionDisconnected => RTCIceConnectionState::Disconnected,
            webrtc_sys::peer_connection::ffi::IceConnectionState::IceConnectionClosed => RTCIceConnectionState::Closed,
            _ => RTCIceConnectionState::New,
        };
        tracing::info!("ICE connection state changed: {:?}", mapped);
        if let Some(ref cb) = *self.callbacks.on_ice_connection_state_change.lock().unwrap() {
            cb(mapped);
        }
    }
    fn on_standardized_ice_connection_change(&self, _: webrtc_sys::peer_connection::ffi::IceConnectionState) {}
    fn on_connection_change(&self, state: webrtc_sys::peer_connection::ffi::PeerConnectionState) {
        let mapped = match state {
            webrtc_sys::peer_connection::ffi::PeerConnectionState::New => RTCPeerConnectionState::New,
            webrtc_sys::peer_connection::ffi::PeerConnectionState::Connecting => RTCPeerConnectionState::Connecting,
            webrtc_sys::peer_connection::ffi::PeerConnectionState::Connected => RTCPeerConnectionState::Connected,
            webrtc_sys::peer_connection::ffi::PeerConnectionState::Disconnected => RTCPeerConnectionState::Disconnected,
            webrtc_sys::peer_connection::ffi::PeerConnectionState::Failed => RTCPeerConnectionState::Failed,
            webrtc_sys::peer_connection::ffi::PeerConnectionState::Closed => RTCPeerConnectionState::Closed,
            _ => RTCPeerConnectionState::New,
        };
        tracing::info!("PC connection state changed: {:?}", mapped);
        if let Some(ref cb) = *self.callbacks.on_peer_connection_state_change.lock().unwrap() {
            cb(mapped);
        }
    }
    fn on_ice_gathering_change(&self, _: webrtc_sys::peer_connection::ffi::IceGatheringState) {}
    fn on_ice_candidate(&self, _: cxx::SharedPtr<webrtc_sys::jsep::ffi::IceCandidate>) {}
    fn on_ice_candidate_error(&self, _: String, _: i32, _: String, _: i32, _: String) {}
    fn on_ice_candidates_removed(&self, _: Vec<cxx::SharedPtr<webrtc_sys::candidate::ffi::Candidate>>) {}
    fn on_ice_connection_receiving_change(&self, _: bool) {}
    fn on_ice_selected_candidate_pair_changed(&self, _: webrtc_sys::peer_connection_factory::ffi::CandidatePairChangeEvent) {}
    fn on_remove_track(&self, _: cxx::SharedPtr<webrtc_sys::rtp_receiver::ffi::RtpReceiver>) {}
    fn on_interesting_usage(&self, _: i32) {}

    fn on_add_track(
        &self,
        _receiver: cxx::SharedPtr<webrtc_sys::rtp_receiver::ffi::RtpReceiver>,
        _streams: Vec<cxx::SharedPtr<webrtc_sys::media_stream::ffi::MediaStream>>,
    ) {
        // ponytail: on_add_track falls through to on_track for unified handling
    }

    fn on_track(
        &self,
        transceiver: cxx::SharedPtr<webrtc_sys::rtp_transceiver::ffi::RtpTransceiver>,
    ) {
        use webrtc_sys::video_frame::ffi as vff;
        use webrtc_sys::video_frame_buffer::ffi as vfb;

        let receiver = transceiver.receiver();
        let track = receiver.track();
        let kind = match receiver.media_type() {
            webrtc_sys::webrtc::ffi::MediaType::Video => TrackKind::Video,
            _ => TrackKind::Audio,
        };
        let tr = TrackReceiver::new(track.id(), kind);

        // Invoke user callback
        if let Some(ref cb) = *self.callbacks.on_track.lock().unwrap() {
            cb(tr.clone());
        }

        // If the user registered a FrameSink, create native VideoSink bridge
        if kind == TrackKind::Video {
            let sink_arc = tr.sink.clone();
            if let Some(_) = *sink_arc.lock().unwrap() {
                let callbacks = self.callbacks.clone();
                #[allow(dead_code)]
                struct VideoSinkAdapter {
                    sink: std::sync::Arc<std::sync::Mutex<Option<Box<dyn crate::track::FrameSink>>>>,
                }
                impl webrtc_sys::video_track::VideoSink for VideoSinkAdapter {
                    fn on_frame(&self, frame: cxx::UniquePtr<vff::VideoFrame>) {
                        if let Some(ref sink) = *self.sink.lock().unwrap() {
                            let w = frame.width();
                            let h = frame.height();
                            let buf = unsafe { frame.video_frame_buffer() };
                            let i420 = unsafe { (*buf).to_i420() };
                            let yuv = unsafe { vfb::i420_to_yuv8(&*i420) };
                            let y_size = (w * h) as usize;
                            let uv_size = ((w / 2) * (h / 2)) as usize;
                            let mut data = vec![0u8; y_size + 2 * uv_size];
                            unsafe {
                                std::ptr::copy_nonoverlapping(
                                    (*yuv).data_y(), data.as_mut_ptr(), y_size,
                                );
                                std::ptr::copy_nonoverlapping(
                                    (*yuv).data_u(), data.as_mut_ptr().add(y_size), uv_size,
                                );
                                std::ptr::copy_nonoverlapping(
                                    (*yuv).data_v(), data.as_mut_ptr().add(y_size + uv_size), uv_size,
                                );
                            }
                            sink.on_frame(&data, w, h);
                        }
                    }
                    fn on_discarded_frame(&self) {}
                    fn on_constraints_changed(&self, _: webrtc_sys::video_track::ffi::VideoTrackSourceConstraints) {}
                }

                let adapter = VideoSinkAdapter { sink: sink_arc.clone() };
                let wrapper = webrtc_sys::video_track::VideoSinkWrapper::new(std::sync::Arc::new(adapter));

                // Register sink with the video track
                unsafe {
                    let video_track = webrtc_sys::video_track::ffi::media_to_video(track);
                    let native_sink = webrtc_sys::video_track::ffi::new_native_video_sink(Box::new(wrapper));
                    video_track.add_sink(&native_sink);
                    callbacks.video_sinks.lock().unwrap().push(native_sink);
                }
            }
        }
    }
}

// ── WebrtcSysFactory ──

pub(crate) struct WebrtcSysFactory {
    factory: cxx::SharedPtr<webrtc_sys::peer_connection_factory::ffi::PeerConnectionFactory>,
}

impl Default for WebrtcSysFactory {
    fn default() -> Self {
        let factory =
            webrtc_sys::peer_connection_factory::ffi::create_peer_connection_factory();
        Self { factory }
}
}

impl Clone for WebrtcSysFactory {
    fn clone(&self) -> Self {
        Self { factory: self.factory.clone() }
    }
}

impl std::fmt::Debug for WebrtcSysFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebrtcSysFactory").finish()
    }
}

impl WebrtcSysFactory {
    pub(crate) async fn create_peer_connection(
        &self,
        config: RTCConfiguration,
    ) -> Result<WebrtcSysPc, RTCError> {
        tracing::info!("Creating RTCPeerConnection (webrtc-sys)");

        let ice_servers: Vec<webrtc_sys::peer_connection::ffi::IceServer> = config
            .ice_servers
            .iter()
            .map(|srv| webrtc_sys::peer_connection::ffi::IceServer {
                urls: srv.urls.clone(),
                username: srv.username.clone(),
                password: srv.password.clone(),
            })
            .collect();

        let ice_transport_type = match config.ice_transport_type {
            RTCIceTransportPolicy::Relay => {
                webrtc_sys::peer_connection::ffi::IceTransportsType::Relay
            }
            RTCIceTransportPolicy::NoHost => {
                webrtc_sys::peer_connection::ffi::IceTransportsType::NoHost
            }
            RTCIceTransportPolicy::All => webrtc_sys::peer_connection::ffi::IceTransportsType::All,
        };

        let rtc_config = webrtc_sys::peer_connection::ffi::RtcConfiguration {
            ice_servers,
            continual_gathering_policy:
                webrtc_sys::peer_connection::ffi::ContinualGatheringPolicy::GatherOnce,
            ice_transport_type,
        };

        // Create RealObserver with shared callback state
        let callbacks = Arc::new(ObserverCallbacks {
            on_track: Mutex::new(None),
            video_sinks: Mutex::new(Vec::new()),
            on_ice_connection_state_change: Mutex::new(None),
            on_peer_connection_state_change: Mutex::new(None),
            staged_media_tracks: Mutex::new(Vec::new()),
        });
        let observer = webrtc_sys::peer_connection_factory::PeerConnectionObserverWrapper::new(
            Arc::new(RealObserver { callbacks: callbacks.clone() }),
        );

        let pc = self
            .factory
            .create_peer_connection(rtc_config, Box::new(observer))
            .map_err(|e| RTCError::RTCPeerConnection(e.what().to_owned()))?;

        Ok(WebrtcSysPc { pc, callbacks, local_sdp: Arc::new(std::sync::Mutex::new(None)) })
    }

    /// Create a video track with a new VideoTrackSource.
    /// Returns (WebrtcSysTrack, SharedPtr<MediaStreamTrack>) —
    /// the media track can be added to the RTCPeerConnection via add_track.
    pub(crate) fn create_video_track(
        &self,
    ) -> (
        WebrtcSysTrack,
        cxx::SharedPtr<webrtc_sys::media_stream_track::ffi::MediaStreamTrack>,
    ) {
        use webrtc_sys::video_track::ffi as vt;

        let resolution = vt::VideoResolution { width: 640, height: 480 };
        let source = vt::new_video_track_source(&resolution, false);
        let backend = WebrtcSysTrack::with_video_source(source.clone());

        // Create VideoTrack from factory, then convert to MediaStreamTrack
        let video_track = self.factory.create_video_track("video".into(), source);
        let media_track = vt::video_to_media(video_track);

        (backend, media_track)
    }
}
