#![cfg(target_arch = "wasm32")]

use crate::browser_js::get;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_talk_wasm::{
    blob_to_bytes, create_opus_media_recorder, microphone_stream, stop_stream_tracks,
};
use js_sys::{Function, Reflect};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;

const VOICE_MESSAGE_PREFIX: &str = "\u{1e}GHOST-VOICE-MESSAGE-V1:";

struct RecorderState {
    recorder: JsValue,
    _data_handler: Closure<dyn FnMut(JsValue)>,
    _stop_handler: Closure<dyn FnMut()>,
}

thread_local! {
    static RECORDER: RefCell<Option<RecorderState>> = const { RefCell::new(None) };
}

pub async fn start_clip_recording<F>(on_complete: F) -> Result<(), String>
where
    F: Fn(String) + 'static,
{
    if RECORDER.with(|state| state.borrow().is_some()) {
        return Err("A voice recording is already active.".into());
    }
    let stream = microphone_stream().await?;
    let (recorder, mime) = create_opus_media_recorder(&stream, None)?;
    let callback: Rc<dyn Fn(String)> = Rc::new(on_complete);
    let data_handler = create_data_handler(callback, mime);
    Reflect::set(
        &recorder,
        &JsValue::from_str("ondataavailable"),
        data_handler.as_ref(),
    )
    .map_err(js_error)?;
    let stop_handler = create_stop_handler(stream);
    Reflect::set(
        &recorder,
        &JsValue::from_str("onstop"),
        stop_handler.as_ref(),
    )
    .map_err(js_error)?;
    let start = get(&recorder, "start")?
        .dyn_into::<Function>()
        .map_err(|_| "MediaRecorder.start is unavailable".to_string())?;
    start.call0(&recorder).map_err(js_error)?;
    RECORDER.with(|state| {
        *state.borrow_mut() = Some(RecorderState {
            recorder: recorder.clone(),
            _data_handler: data_handler,
            _stop_handler: stop_handler,
        });
    });
    schedule_recording_timeout();
    Ok(())
}

fn create_data_handler(callback: Rc<dyn Fn(String)>, mime: String) -> Closure<dyn FnMut(JsValue)> {
    Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let Ok(blob) = get(&event, "data") else {
            return;
        };
        let size = get(&blob, "size")
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0);
        if size <= 0.0 {
            return;
        }
        let callback = callback.clone();
        let mime = mime.clone();
        spawn_local(async move {
            if let Ok(bytes) = blob_to_bytes(&blob).await {
                let data = STANDARD.encode(bytes);
                let payload = serde_json::json!({"mime": mime, "data": data});
                callback(format!("{VOICE_MESSAGE_PREFIX}{}", payload));
            }
        });
    })
}

fn create_stop_handler(stream: JsValue) -> Closure<dyn FnMut()> {
    Closure::<dyn FnMut()>::new(move || {
        stop_stream_tracks(&stream);
        RECORDER.with(|state| {
            state.borrow_mut().take();
        });
    })
}

fn schedule_recording_timeout() {
    spawn_local(async {
        gloo_timers::future::TimeoutFuture::new(10_000).await;
        let _ = stop_clip_recording();
    });
}

pub fn stop_clip_recording() -> Result<(), String> {
    RECORDER.with(|state| {
        let borrowed = state.borrow();
        let Some(recording) = borrowed.as_ref() else {
            return Err("No voice recording is active.".into());
        };
        let current = get(&recording.recorder, "state")
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        if current == "inactive" {
            return Ok(());
        }
        let stop = get(&recording.recorder, "stop")?
            .dyn_into::<Function>()
            .map_err(|_| "MediaRecorder.stop is unavailable".to_string())?;
        stop.call0(&recording.recorder).map_err(js_error)?;
        Ok(())
    })
}

pub fn decode_voice_message(body: &str) -> Option<(String, String)> {
    let payload = body.strip_prefix(VOICE_MESSAGE_PREFIX)?;
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let mime = value.get("mime")?.as_str()?.to_string();
    let data = value.get("data")?.as_str()?.to_string();
    if !mime.starts_with("audio/") || STANDARD.decode(&data).is_err() {
        return None;
    }
    Some((mime, data))
}

fn js_error(value: JsValue) -> String {
    value
        .as_string()
        .unwrap_or_else(|| "Browser media error".into())
}
