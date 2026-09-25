use ghost_talk::VoiceCodec;
use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

pub async fn microphone_stream() -> Result<JsValue, String> {
    let window = web_sys::window().ok_or_else(|| "window is unavailable".to_string())?;
    let navigator: JsValue = window.navigator().into();
    let media_devices = get(&navigator, "mediaDevices")?;
    let get_user_media = get(&media_devices, "getUserMedia")?
        .dyn_into::<Function>()
        .map_err(|_| "navigator.mediaDevices.getUserMedia is unavailable".to_string())?;
    let audio = Object::new();
    for name in ["echoCancellation", "noiseSuppression", "autoGainControl"] {
        Reflect::set(&audio, &JsValue::from_str(name), &JsValue::TRUE).map_err(js_error)?;
    }
    let constraints = Object::new();
    Reflect::set(&constraints, &JsValue::from_str("audio"), &audio).map_err(js_error)?;
    Reflect::set(&constraints, &JsValue::from_str("video"), &JsValue::FALSE).map_err(js_error)?;
    let value = get_user_media
        .call1(&media_devices, &constraints)
        .map_err(js_error)?;
    let promise = value
        .dyn_into::<Promise>()
        .map_err(|_| "getUserMedia did not return a Promise".to_string())?;
    JsFuture::from(promise).await.map_err(js_error)
}

pub(super) async fn blob_parts_to_bytes(parts: &[JsValue], mime: &str) -> Result<Vec<u8>, String> {
    let window = web_sys::window().ok_or_else(|| "window is unavailable".to_string())?;
    let ctor = get(window.as_ref(), "Blob")?
        .dyn_into::<Function>()
        .map_err(|_| "Blob is unavailable".to_string())?;
    let array = Array::new();
    for part in parts {
        array.push(part);
    }
    let options = Object::new();
    Reflect::set(
        &options,
        &JsValue::from_str("type"),
        &JsValue::from_str(mime),
    )
    .map_err(js_error)?;
    let args = Array::new();
    args.push(&array);
    args.push(&options);
    let blob = Reflect::construct(&ctor, &args).map_err(js_error)?;
    blob_to_bytes(&blob).await
}

/// Creates a browser `MediaRecorder` configured for the preferred Opus container.
pub fn create_opus_media_recorder(
    stream: &JsValue,
    audio_bits_per_second: Option<u32>,
) -> Result<(JsValue, String), String> {
    let window = web_sys::window().ok_or_else(|| "window is unavailable".to_string())?;
    let ctor = get(window.as_ref(), "MediaRecorder")?
        .dyn_into::<Function>()
        .map_err(|_| "MediaRecorder is unavailable".to_string())?;
    let requested_mime =
        preferred_opus_mime(&ctor).unwrap_or_else(|| "audio/webm;codecs=opus".to_string());
    let options = Object::new();
    Reflect::set(
        &options,
        &JsValue::from_str("mimeType"),
        &JsValue::from_str(&requested_mime),
    )
    .map_err(js_error)?;
    if let Some(bits_per_second) = audio_bits_per_second {
        Reflect::set(
            &options,
            &JsValue::from_str("audioBitsPerSecond"),
            &JsValue::from_f64(f64::from(bits_per_second)),
        )
        .map_err(js_error)?;
    }
    let args = Array::new();
    args.push(stream);
    args.push(&options);
    let recorder = Reflect::construct(&ctor, &args).map_err(js_error)?;
    let actual_mime = get(&recorder, "mimeType")
        .ok()
        .and_then(|value| value.as_string())
        .filter(|value| !value.is_empty())
        .unwrap_or(requested_mime);
    Ok((recorder, actual_mime))
}

/// Reads a browser `Blob` into owned bytes.
pub async fn blob_to_bytes(blob: &JsValue) -> Result<Vec<u8>, String> {
    let array_buffer = get(blob, "arrayBuffer")?
        .dyn_into::<Function>()
        .map_err(|_| "Blob.arrayBuffer is unavailable".to_string())?
        .call0(blob)
        .map_err(js_error)?;
    let promise = array_buffer
        .dyn_into::<Promise>()
        .map_err(|_| "Blob.arrayBuffer did not return a Promise".to_string())?;
    let buffer = JsFuture::from(promise).await.map_err(js_error)?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

pub fn preferred_opus_mime(ctor: &Function) -> Option<String> {
    let supported = get(ctor.as_ref(), "isTypeSupported")
        .ok()?
        .dyn_into::<Function>()
        .ok()?;
    ["audio/webm;codecs=opus", "audio/ogg;codecs=opus"]
        .into_iter()
        .find(|mime| {
            supported
                .call1(ctor, &JsValue::from_str(mime))
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .map(str::to_string)
}

pub(super) fn codec_for_mime(mime: &str) -> VoiceCodec {
    if mime.to_ascii_lowercase().contains("ogg") {
        VoiceCodec::OpusOgg
    } else {
        VoiceCodec::OpusWebM
    }
}

pub(super) fn request_and_stop_media_recorder(recorder: &JsValue) {
    if let Ok(request) = get(recorder, "requestData").and_then(|value| {
        value
            .dyn_into::<Function>()
            .map_err(|_| "requestData unavailable".to_string())
    }) {
        let _ = request.call0(recorder);
    }
    stop_media_recorder(recorder);
}

pub(super) fn stop_media_recorder(recorder: &JsValue) {
    let state = get(recorder, "state")
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default();
    if state == "inactive" {
        return;
    }
    if let Ok(stop) = get(recorder, "stop").and_then(|value| {
        value
            .dyn_into::<Function>()
            .map_err(|_| "MediaRecorder.stop is unavailable".to_string())
    }) {
        let _ = stop.call0(recorder);
    }
}

pub(super) fn set_stream_muted(stream: &JsValue, muted: bool) {
    let Ok(get_tracks) = get(stream, "getAudioTracks").and_then(|value| {
        value
            .dyn_into::<Function>()
            .map_err(|_| "getAudioTracks unavailable".to_string())
    }) else {
        return;
    };
    let Ok(value) = get_tracks.call0(stream) else {
        return;
    };
    let tracks = Array::from(&value);
    for track in tracks.iter() {
        let _ = Reflect::set(
            &track,
            &JsValue::from_str("enabled"),
            &JsValue::from_bool(!muted),
        );
    }
}

pub fn stop_stream_tracks(stream: &JsValue) {
    let Ok(get_tracks) = get(stream, "getTracks").and_then(|value| {
        value
            .dyn_into::<Function>()
            .map_err(|_| "getTracks unavailable".to_string())
    }) else {
        return;
    };
    let Ok(value) = get_tracks.call0(stream) else {
        return;
    };
    let tracks = Array::from(&value);
    for track in tracks.iter() {
        if let Ok(stop) = get(&track, "stop").and_then(|value| {
            value
                .dyn_into::<Function>()
                .map_err(|_| "track.stop unavailable".to_string())
        }) {
            let _ = stop.call0(&track);
        }
    }
}

pub(super) fn get(target: &JsValue, name: &str) -> Result<JsValue, String> {
    crate::browser_property(target, name).map_err(js_error)
}

pub(super) fn js_error(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| {
        js_sys::JSON::stringify(&value)
            .ok()
            .and_then(|text| text.as_string())
            .unwrap_or_else(|| "Browser media error".to_string())
    })
}
