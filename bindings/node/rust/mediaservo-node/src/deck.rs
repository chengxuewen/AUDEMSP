//! deck 数据面 napi 绑定 — CameraSource（帧回调经 ThreadsafeFunction → JS 主线程，
//! 帧数据 I420 拼接 Buffer + meta JSON）。

use std::sync::Arc;

use mediaservo_deck::{CameraSource, CaptureOptions};
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;

/// 采集选项（省略字段用默认 1280x720@30）。
#[napi(object)]
pub struct JsCaptureOptions {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub framerate: Option<u32>,
}

fn closed_err() -> napi::Error {
    napi::Error::from_reason("camera not started or closed")
}

/// 相机采集源（async；帧回调经 TSFN 线程安全转发）。
#[napi]
pub struct JsCameraSource {
    inner: Arc<tokio::sync::Mutex<Option<CameraSource>>>,
    stream: Arc<tokio::sync::Mutex<Option<mediaservo_deck::FrameStream>>>,
    opts: CaptureOptions,
}

// SAFETY: 所有方法经 tokio Mutex 序列化（field-c 同款先例）。
unsafe impl Send for JsCameraSource {}
unsafe impl Sync for JsCameraSource {}

#[napi]
impl JsCameraSource {
    /// 打开相机（当前 stub 设备 "stub:test-camera"）。
    #[napi(factory)]
    pub async fn open(dev_id: String, opts: JsCaptureOptions) -> Result<Self> {
        let mut cap = CaptureOptions::default();
        if let (Some(w), Some(h)) = (opts.width, opts.height) {
            cap.resolution = Some((w, h));
        }
        if let Some(f) = opts.framerate {
            cap.framerate = Some(f);
        }
        let source = CameraSource::open(mediaservo_deck::DeviceId(dev_id), cap.clone())
            .map_err(|e| napi::Error::from_reason(format!("open: {e}")))?;
        Ok(Self {
            inner: Arc::new(tokio::sync::Mutex::new(Some(source))),
            stream: Arc::new(tokio::sync::Mutex::new(None)),
            opts: cap,
        })
    }

    /// 开始产帧（只允许一次；内部生成 FrameStream）。
    #[napi]
    pub async fn start(&self) -> Result<()> {
        let mut guard = self.inner.lock().await;
        let source = guard.as_mut().ok_or_else(closed_err)?;
        let stream = source
            .start(&self.opts)
            .map_err(|e| napi::Error::from_reason(format!("start: {e}")))?;
        *self.stream.lock().await = Some(stream);
        Ok(())
    }

    /// 订阅帧回调（(meta_json, i420_buffer) → JS；stream 关闭后泵退出）。
    #[napi]
    pub fn on_frame(&self, cb: Function<(String, Vec<u8>), ()>) -> Result<()> {
        let tsfn = cb
            .build_threadsafe_function::<(String, Vec<u8>)>()
            .build()?;
        let stream = self.stream.clone();
        super::event_runtime().spawn(async move {
            // FrameStream 非 Clone（mpsc Receiver）——take 独占；sender 全 drop 后 recv 返回 None 停泵
            let mut rx = { stream.lock().await.take() };
            let Some(mut rx) = rx else { return };
            loop {
                match rx.recv().await {
                    Some(frame) => {
                        // I420 三平面拼接（Y + U + V）
                        let mut data = Vec::with_capacity(
                            frame.planes.iter().map(|p| p.data.len()).sum(),
                        );
                        for p in &frame.planes {
                            data.extend_from_slice(&p.data);
                        }
                        let meta = serde_json::json!({
                            "width": frame.format.width,
                            "height": frame.format.height,
                            "pts_us": frame.pts,
                            "keyframe": frame.keyframe,
                        })
                        .to_string();
                        let _ = tsfn.call((meta, data), ThreadsafeFunctionCallMode::Blocking);
                    }
                    None => break, // stream 关闭
                }
            }
        });
        Ok(())
    }

    /// 停止产帧（幂等）。
    #[napi]
    pub async fn stop(&self) -> Result<()> {
        let mut guard = self.inner.lock().await;
        if let Some(s) = guard.as_mut() {
            s.stop();
        }
        Ok(())
    }

    /// 关闭（幂等；停泵经 stream drop）。
    #[napi]
    pub async fn close(&self) -> Result<()> {
        let mut guard = self.inner.lock().await;
        *guard = None;
        *self.stream.lock().await = None;
        Ok(())
    }
}
