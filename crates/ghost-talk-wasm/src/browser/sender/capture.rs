use std::{cell::RefCell, rc::Rc, time::Duration};

use ghost_talk::EncodedAudioChunk;
use gloo_timers::future::TimeoutFuture;
use js_sys::{Function, Reflect};
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;

use super::{RecorderHandlers, SenderInner, CAPTURE_WINDOW_MS};
use crate::browser::media_api::{
    blob_parts_to_bytes, codec_for_mime, create_opus_media_recorder, get, js_error,
    request_and_stop_media_recorder,
};

pub(super) fn start_capture_window(inner: Rc<RefCell<SenderInner>>) -> Result<(), String> {
    let Some((stream, generation)) = capture_stream_generation(&inner)? else {
        return Ok(());
    };
    let (recorder, mime) = create_opus_media_recorder(&stream, Some(24_000))?;
    let chunks = Rc::new(RefCell::new(Vec::<JsValue>::new()));
    let data_handler = install_data_handler(&recorder, chunks.clone())?;
    let stop_handler = install_stop_handler(&recorder, inner.clone(), chunks, mime, generation)?;
    let error_handler = install_error_handler(&recorder, inner.clone())?;
    start_media_recorder(&recorder)?;
    remember_recorder_handlers(&inner, &recorder, data_handler, stop_handler, error_handler);
    schedule_capture_stop(inner, generation);
    Ok(())
}

fn capture_stream_generation(
    inner: &Rc<RefCell<SenderInner>>,
) -> Result<Option<(JsValue, u64)>, String> {
    let state = inner.borrow();
    if !state.running {
        return Ok(None);
    }
    let stream = state
        .stream
        .clone()
        .ok_or_else(|| "Ghost Talk microphone is not prepared.".to_string())?;
    Ok(Some((stream, state.generation)))
}

fn install_data_handler(
    recorder: &JsValue,
    chunks: Rc<RefCell<Vec<JsValue>>>,
) -> Result<Closure<dyn FnMut(JsValue)>, String> {
    let handler = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let Ok(blob) = get(&event, "data") else {
            return;
        };
        if blob_size(&blob) > 0.0 {
            chunks.borrow_mut().push(blob);
        }
    });
    Reflect::set(
        recorder,
        &JsValue::from_str("ondataavailable"),
        handler.as_ref(),
    )
    .map_err(js_error)?;
    Ok(handler)
}

fn blob_size(blob: &JsValue) -> f64 {
    get(blob, "size")
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0)
}

fn install_stop_handler(
    recorder: &JsValue,
    inner: Rc<RefCell<SenderInner>>,
    chunks: Rc<RefCell<Vec<JsValue>>>,
    mime: String,
    generation: u64,
) -> Result<Closure<dyn FnMut()>, String> {
    let handler = Closure::<dyn FnMut()>::new(move || {
        let inner = inner.clone();
        let parts = chunks.borrow().clone();
        let mime = mime.clone();
        spawn_local(async move {
            handle_capture_stop(inner, parts, mime, generation).await;
        });
    });
    Reflect::set(recorder, &JsValue::from_str("onstop"), handler.as_ref()).map_err(js_error)?;
    Ok(handler)
}

async fn handle_capture_stop(
    inner: Rc<RefCell<SenderInner>>,
    parts: Vec<JsValue>,
    mime: String,
    generation: u64,
) {
    TimeoutFuture::new(0).await;
    if capture_generation_active(&inner, generation) && !parts.is_empty() {
        emit_captured_parts(&inner, &parts, &mime).await;
    }
    clear_recorder_state(&inner);
    if capture_generation_active(&inner, generation) {
        let _ = start_capture_window(inner);
    }
}

fn capture_generation_active(inner: &Rc<RefCell<SenderInner>>, generation: u64) -> bool {
    let state = inner.borrow();
    state.running && state.generation == generation
}

async fn emit_captured_parts(inner: &Rc<RefCell<SenderInner>>, parts: &[JsValue], mime: &str) {
    let Ok(payload) = blob_parts_to_bytes(parts, mime).await else {
        return;
    };
    if payload.is_empty() {
        return;
    }
    let encoded = encode_captured_payload(inner, mime, payload);
    if let Some(encoded) = encoded {
        if let Some(callback) = inner.borrow().on_chunk.clone() {
            callback(encoded);
        }
    }
}

fn encode_captured_payload(
    inner: &Rc<RefCell<SenderInner>>,
    mime: &str,
    payload: Vec<u8>,
) -> Option<Vec<u8>> {
    let codec = codec_for_mime(mime);
    let mut state = inner.borrow_mut();
    let captured = EncodedAudioChunk::new(
        codec,
        payload,
        Duration::from_millis(CAPTURE_WINDOW_MS as u64),
    );
    state
        .sender
        .push_encoded(captured)
        .and_then(|chunk| chunk.encode())
        .ok()
}

fn clear_recorder_state(inner: &Rc<RefCell<SenderInner>>) {
    let mut state = inner.borrow_mut();
    state.recorder = None;
    state.handlers = None;
}

fn install_error_handler(
    recorder: &JsValue,
    inner: Rc<RefCell<SenderInner>>,
) -> Result<Closure<dyn FnMut(JsValue)>, String> {
    let handler = Closure::<dyn FnMut(JsValue)>::new(move |_event: JsValue| {
        let mut state = inner.borrow_mut();
        state.running = false;
        state.on_chunk = None;
        let _ = state.sender.stop();
    });
    Reflect::set(recorder, &JsValue::from_str("onerror"), handler.as_ref()).map_err(js_error)?;
    Ok(handler)
}

fn start_media_recorder(recorder: &JsValue) -> Result<(), String> {
    get(recorder, "start")?
        .dyn_into::<Function>()
        .map_err(|_| "MediaRecorder.start is unavailable".to_string())?
        .call0(recorder)
        .map_err(js_error)?;
    Ok(())
}

fn remember_recorder_handlers(
    inner: &Rc<RefCell<SenderInner>>,
    recorder: &JsValue,
    data: Closure<dyn FnMut(JsValue)>,
    stop: Closure<dyn FnMut()>,
    error: Closure<dyn FnMut(JsValue)>,
) {
    let mut state = inner.borrow_mut();
    state.recorder = Some(recorder.clone());
    state.handlers = Some(RecorderHandlers {
        _data: data,
        _stop: stop,
        _error: error,
    });
}

fn schedule_capture_stop(inner: Rc<RefCell<SenderInner>>, generation: u64) {
    spawn_local(async move {
        TimeoutFuture::new(CAPTURE_WINDOW_MS).await;
        let recorder = capture_recorder_if_active(&inner, generation);
        if let Some(recorder) = recorder {
            request_and_stop_media_recorder(&recorder);
        }
    });
}

fn capture_recorder_if_active(
    inner: &Rc<RefCell<SenderInner>>,
    generation: u64,
) -> Option<JsValue> {
    let state = inner.borrow();
    (state.running && state.generation == generation)
        .then(|| state.recorder.clone())
        .flatten()
}
