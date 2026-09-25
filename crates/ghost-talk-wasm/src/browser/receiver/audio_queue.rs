use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use js_sys::{Array, Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};

use super::INITIAL_BUFFER_SECONDS;
use crate::browser::media_api::{get, js_error};

pub(super) struct AudioQueue {
    context: Option<JsValue>,
    next_start: Rc<RefCell<Option<f64>>>,
    pending: Rc<RefCell<VecDeque<Vec<u8>>>>,
    processing: Rc<RefCell<bool>>,
    generation: Rc<RefCell<u64>>,
}

impl AudioQueue {
    pub(super) fn new() -> Self {
        Self {
            context: None,
            next_start: Rc::new(RefCell::new(None)),
            pending: Rc::new(RefCell::new(VecDeque::new())),
            processing: Rc::new(RefCell::new(false)),
            generation: Rc::new(RefCell::new(0)),
        }
    }

    pub(super) fn enqueue(&mut self, payload: Vec<u8>) {
        self.pending.borrow_mut().push_back(payload);
        if *self.processing.borrow() {
            return;
        }
        *self.processing.borrow_mut() = true;
        let context = match self.context() {
            Ok(value) => value,
            Err(_) => {
                *self.processing.borrow_mut() = false;
                return;
            }
        };
        let pending = self.pending.clone();
        let processing = self.processing.clone();
        let generation = self.generation.clone();
        let current_generation = *generation.borrow();
        let next_start_task = self.next_start.clone();
        spawn_local(async move {
            process_pending_audio(
                &context,
                &pending,
                &generation,
                current_generation,
                &next_start_task,
            )
            .await;
            *processing.borrow_mut() = false;
        });
    }

    fn context(&mut self) -> Result<JsValue, String> {
        if let Some(context) = self.context.as_ref() {
            let state = get(context, "state")
                .ok()
                .and_then(|value| value.as_string())
                .unwrap_or_default();
            if state != "closed" {
                return Ok(context.clone());
            }
        }
        let window = web_sys::window().ok_or_else(|| "window is unavailable".to_string())?;
        let ctor = get(window.as_ref(), "AudioContext")?
            .dyn_into::<Function>()
            .map_err(|_| "AudioContext is unavailable".to_string())?;
        let context = Reflect::construct(&ctor, &Array::new()).map_err(js_error)?;
        self.context = Some(context.clone());
        Ok(context)
    }

    pub(super) fn reset(&mut self) {
        let next_generation = self.generation.borrow().wrapping_add(1);
        *self.generation.borrow_mut() = next_generation;
        self.pending.borrow_mut().clear();
        *self.processing.borrow_mut() = false;
        *self.next_start.borrow_mut() = None;
        if let Some(context) = self.context.take() {
            if let Ok(close) = get(&context, "close").and_then(|value| {
                value
                    .dyn_into::<Function>()
                    .map_err(|_| "AudioContext.close is unavailable".to_string())
            }) {
                let _ = close.call0(&context);
            }
        }
    }
}

async fn process_pending_audio(
    context: &JsValue,
    pending: &Rc<RefCell<VecDeque<Vec<u8>>>>,
    generation: &Rc<RefCell<u64>>,
    current_generation: u64,
    next_start: &Rc<RefCell<Option<f64>>>,
) {
    while *generation.borrow() == current_generation {
        let Some(payload) = pending.borrow_mut().pop_front() else {
            break;
        };
        let Ok((buffer, duration)) = decode_audio(context, &payload).await else {
            continue;
        };
        if *generation.borrow() != current_generation || duration <= 0.0 {
            break;
        }
        schedule_decoded_audio(context, &buffer, duration, next_start);
    }
}

fn schedule_decoded_audio(
    context: &JsValue,
    buffer: &JsValue,
    duration: f64,
    next_start: &Rc<RefCell<Option<f64>>>,
) {
    let now = get(context, "currentTime")
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    let start = next_start
        .borrow()
        .map(|value| value.max(now))
        .unwrap_or(now + INITIAL_BUFFER_SECONDS);
    if schedule_audio_buffer(context, buffer, start).is_ok() {
        *next_start.borrow_mut() = Some(start + duration);
    }
}

async fn decode_audio(context: &JsValue, payload: &[u8]) -> Result<(JsValue, f64), String> {
    let bytes = Uint8Array::from(payload);
    let array_buffer = bytes.buffer();
    let decode = get(context, "decodeAudioData")?
        .dyn_into::<Function>()
        .map_err(|_| "AudioContext.decodeAudioData is unavailable".to_string())?;
    let value = decode.call1(context, &array_buffer).map_err(js_error)?;
    let promise = value
        .dyn_into::<Promise>()
        .map_err(|_| "decodeAudioData did not return a Promise".to_string())?;
    let buffer = JsFuture::from(promise).await.map_err(js_error)?;
    let duration = get(&buffer, "duration")?
        .as_f64()
        .ok_or_else(|| "decoded audio duration is unavailable".to_string())?;
    Ok((buffer, duration))
}

fn schedule_audio_buffer(context: &JsValue, buffer: &JsValue, start_at: f64) -> Result<(), String> {
    let source = get(context, "createBufferSource")?
        .dyn_into::<Function>()
        .map_err(|_| "AudioContext.createBufferSource is unavailable".to_string())?
        .call0(context)
        .map_err(js_error)?;
    Reflect::set(&source, &JsValue::from_str("buffer"), buffer).map_err(js_error)?;
    let destination = get(context, "destination")?;
    get(&source, "connect")?
        .dyn_into::<Function>()
        .map_err(|_| "AudioBufferSourceNode.connect is unavailable".to_string())?
        .call1(&source, &destination)
        .map_err(js_error)?;
    get(&source, "start")?
        .dyn_into::<Function>()
        .map_err(|_| "AudioBufferSourceNode.start is unavailable".to_string())?
        .call1(&source, &JsValue::from_f64(start_at))
        .map_err(js_error)?;
    Ok(())
}
