use std::{cell::RefCell, rc::Rc, time::Duration};

use ghost_talk::{VoiceSender, VoiceSenderConfig};
use wasm_bindgen::{closure::Closure, JsValue};

use super::media_api::{
    microphone_stream, set_stream_muted, stop_media_recorder, stop_stream_tracks,
};

mod capture;
use capture::start_capture_window;

const CAPTURE_WINDOW_MS: u32 = 350;

/// Browser capture adapter for the transport-neutral Ghost Talk SDK.
///
/// Each MediaRecorder window is stopped and restarted so every emitted payload is
/// an independently-decodable Opus/WebM or Opus/Ogg unit. `VoiceSender` owns the
/// stream id, sequence and defensive chunk limits.
#[derive(Clone)]
pub struct BrowserVoiceSender {
    inner: Rc<RefCell<SenderInner>>,
}

struct SenderInner {
    sender: VoiceSender,
    stream: Option<JsValue>,
    recorder: Option<JsValue>,
    handlers: Option<RecorderHandlers>,
    running: bool,
    muted: bool,
    generation: u64,
    on_chunk: Option<Rc<dyn Fn(Vec<u8>)>>,
}

struct RecorderHandlers {
    _data: Closure<dyn FnMut(JsValue)>,
    _stop: Closure<dyn FnMut()>,
    _error: Closure<dyn FnMut(JsValue)>,
}

impl BrowserVoiceSender {
    pub fn new() -> Result<Self, String> {
        let sender = VoiceSender::new(VoiceSenderConfig {
            target_chunk_duration: Duration::from_millis(CAPTURE_WINDOW_MS as u64),
            target_max_chunk_bytes: Some(32 * 1024),
            ..VoiceSenderConfig::default()
        })
        .map_err(|error| error.to_string())?;
        Ok(Self {
            inner: Rc::new(RefCell::new(SenderInner {
                sender,
                stream: None,
                recorder: None,
                handlers: None,
                running: false,
                muted: false,
                generation: 0,
                on_chunk: None,
            })),
        })
    }

    pub async fn prepare(&self) -> Result<(), String> {
        if self.inner.borrow().stream.is_some() {
            return Ok(());
        }
        let stream = microphone_stream().await?;
        set_stream_muted(&stream, self.inner.borrow().muted);
        self.inner.borrow_mut().stream = Some(stream);
        Ok(())
    }

    pub async fn start<F>(&self, on_chunk: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>) + 'static,
    {
        self.prepare().await?;
        {
            let mut inner = self.inner.borrow_mut();
            if inner.running {
                return Err("Ghost Talk live voice capture is already running.".into());
            }
            inner.sender.start().map_err(|error| error.to_string())?;
            inner.running = true;
            inner.generation = inner.generation.wrapping_add(1);
            inner.on_chunk = Some(Rc::new(on_chunk));
        }
        start_capture_window(self.inner.clone())
    }

    pub fn set_muted(&self, muted: bool) {
        let mut inner = self.inner.borrow_mut();
        inner.muted = muted;
        if let Some(stream) = inner.stream.as_ref() {
            set_stream_muted(stream, muted);
        }
    }

    pub fn stop(&self) {
        let recorder = {
            let mut inner = self.inner.borrow_mut();
            inner.running = false;
            inner.generation = inner.generation.wrapping_add(1);
            inner.on_chunk = None;
            let _ = inner.sender.stop();
            inner.recorder.clone()
        };
        if let Some(recorder) = recorder {
            stop_media_recorder(&recorder);
        }
    }

    pub fn close(&self) {
        self.stop();
        let stream = self.inner.borrow_mut().stream.take();
        if let Some(stream) = stream {
            stop_stream_tracks(&stream);
        }
        let mut inner = self.inner.borrow_mut();
        inner.recorder = None;
        inner.handlers = None;
    }
}
