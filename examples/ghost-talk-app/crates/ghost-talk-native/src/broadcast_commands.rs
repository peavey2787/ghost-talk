use ghost_api::BroadcastStartRequest;
use ghost_broadcast::{
    validate_relay_url, validate_rtmp_configuration, validate_session_id, BroadcastFrame,
    BroadcastPipeline, FfmpegRtmpSink, FileRecordingSink, RtmpDestination, RtmpSecret,
};
use ghost_talk::{VoiceChunk, VoiceCodec};
use std::{collections::BTreeMap, fs, path::PathBuf, sync::Mutex};
use tauri::{AppHandle, State};

#[derive(Default)]
pub struct BroadcastRuntimeState {
    sessions: Mutex<BTreeMap<String, BroadcastSession>>,
}

struct BroadcastSession {
    config: BroadcastConfig,
    pipeline: Option<BroadcastPipeline>,
    recording_path: Option<PathBuf>,
    recording_content_type: Option<String>,
    input_format: Option<&'static str>,
}

struct BroadcastConfig {
    record_local: bool,
    rtmp: Option<RtmpDestination>,
}

#[tauri::command]
pub fn broadcast_start(
    state: State<'_, BroadcastRuntimeState>,
    request: BroadcastStartRequest,
) -> Result<(), String> {
    validate_session_id(&request.session_id)?;
    let config = validate_config(&request)?;
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| "broadcast state lock failed")?;
    if sessions.contains_key(&request.session_id) {
        return Err("broadcast session already exists".into());
    }
    sessions.insert(
        request.session_id,
        BroadcastSession {
            config,
            pipeline: None,
            recording_path: None,
            recording_content_type: None,
            input_format: None,
        },
    );
    Ok(())
}

fn validate_config(request: &BroadcastStartRequest) -> Result<BroadcastConfig, String> {
    validate_rtmp_configuration(
        request.rtmp_server.as_deref(),
        request.rtmp_stream_key.as_deref(),
    )?;
    let relay = validated_relay(request)?;
    let direct_rtmp = validate_rtmp_destination(request)?;
    if cfg!(any(target_os = "android", target_os = "ios"))
        && relay.is_none()
        && direct_rtmp.is_some()
    {
        return Err("Android/iOS live RTMP requires a Ghost Talk broadcast relay".into());
    }
    let rtmp = relay.is_none().then_some(direct_rtmp).flatten();
    if !request.record_local && rtmp.is_none() && relay.is_none() {
        return Err("broadcast requires local recording, direct RTMP, or a relay sink".into());
    }
    Ok(BroadcastConfig {
        record_local: request.record_local,
        rtmp,
    })
}

fn validated_relay(request: &BroadcastStartRequest) -> Result<Option<&str>, String> {
    let relay = request
        .relay_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(url) = relay {
        validate_relay_url(url)?;
    }
    Ok(relay)
}

fn validate_rtmp_destination(
    request: &BroadcastStartRequest,
) -> Result<Option<RtmpDestination>, String> {
    match request.rtmp_server.as_ref() {
        Some(server) => configured_rtmp_destination(request, server),
        None => missing_rtmp_destination(request),
    }
}

fn configured_rtmp_destination(
    request: &BroadcastStartRequest,
    server: &str,
) -> Result<Option<RtmpDestination>, String> {
    let key = request
        .rtmp_stream_key
        .clone()
        .ok_or_else(|| "RTMP stream key is required".to_string())?;
    Ok(Some(RtmpDestination::new(
        server.to_owned(),
        RtmpSecret::new(key)?,
    )?))
}

fn missing_rtmp_destination(
    request: &BroadcastStartRequest,
) -> Result<Option<RtmpDestination>, String> {
    if request
        .rtmp_stream_key
        .as_ref()
        .is_some_and(|key| !key.trim().is_empty())
    {
        return Err("RTMP server is required when a stream key is provided".into());
    }
    Ok(None)
}

#[tauri::command]
pub fn broadcast_push(
    app: AppHandle,
    state: State<'_, BroadcastRuntimeState>,
    session_id: String,
    sequence: u64,
    timestamp_ms: u64,
    encoded: Vec<u8>,
) -> Result<(), String> {
    if encoded.is_empty() || encoded.len() > 2 * 1024 * 1024 {
        return Err("broadcast frame size is invalid".into());
    }
    let chunk = VoiceChunk::decode(&encoded)
        .map_err(|error| format!("invalid Ghost voice frame: {error}"))?;
    let format = format_for_codec(chunk.codec);
    let content_type = chunk.codec.mime_type();
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|_| "broadcast state lock failed")?;
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| "broadcast session is not active".to_string())?;
    ensure_pipeline(&app, &session_id, session, format, content_type)?;
    session
        .pipeline
        .as_mut()
        .ok_or_else(|| "broadcast pipeline was not initialized".to_string())?
        .fan_out(&BroadcastFrame {
            sequence,
            timestamp_ms,
            encoded: chunk.payload,
        });
    Ok(())
}

fn ensure_pipeline(
    app: &AppHandle,
    session_id: &str,
    session: &mut BroadcastSession,
    format: &'static str,
    content_type: &str,
) -> Result<(), String> {
    if let Some(active) = session.input_format {
        return (active == format)
            .then_some(())
            .ok_or_else(|| "broadcast codec changed during an active session".into());
    }
    let mut pipeline = BroadcastPipeline::new();
    let recording_path = add_recording_sink(
        app,
        &mut pipeline,
        session_id,
        session.config.record_local,
        format,
    )?;
    if let Some(destination) = session.config.rtmp.as_ref() {
        pipeline.add_sink(Box::new(FfmpegRtmpSink::spawn(
            "rtmp",
            format,
            destination,
        )?));
    }
    session.pipeline = Some(pipeline);
    session.recording_path = recording_path;
    session.recording_content_type = session.config.record_local.then(|| content_type.to_owned());
    session.input_format = Some(format);
    Ok(())
}

fn add_recording_sink(
    app: &AppHandle,
    pipeline: &mut BroadcastPipeline,
    session_id: &str,
    enabled: bool,
    format: &str,
) -> Result<Option<PathBuf>, String> {
    if !enabled {
        return Ok(None);
    }
    let dir = crate::persistence::storage_root::data_root(app)?.join("recordings");
    fs::create_dir_all(&dir).map_err(|error| format!("create recording directory: {error}"))?;
    let path = dir.join(format!("{session_id}.{format}"));
    pipeline.add_sink(Box::new(FileRecordingSink::create(
        "recording",
        &path,
        format,
    )?));
    Ok(Some(path))
}

#[tauri::command]
pub fn broadcast_stop(
    app: AppHandle,
    state: State<'_, BroadcastRuntimeState>,
    session_id: String,
) -> Result<ghost_api::BroadcastStopResult, String> {
    let mut session = state
        .sessions
        .lock()
        .map_err(|_| "broadcast state lock failed")?
        .remove(&session_id)
        .ok_or_else(|| "broadcast session is not active".to_string())?;
    if let Some(pipeline) = session.pipeline.as_mut() {
        pipeline.finish();
    }
    let recording = finalize_recording(&app, &session)?;
    let failures = session
        .pipeline
        .as_ref()
        .map(|pipeline| pipeline.failures().to_vec())
        .unwrap_or_default();
    Ok(ghost_api::BroadcastStopResult {
        failures,
        recording,
    })
}

fn finalize_recording(
    app: &AppHandle,
    session: &BroadcastSession,
) -> Result<Option<ghost_media::MediaReference>, String> {
    let Some(path) = session.recording_path.as_ref() else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|error| format!("read completed recording: {error}"))?;
    let content_type = session
        .recording_content_type
        .as_deref()
        .unwrap_or("audio/webm;codecs=opus");
    let reference = crate::media_commands::store_local_media(app, content_type, &bytes)?;
    let _ = fs::remove_file(path);
    Ok(Some(reference))
}

fn format_for_codec(codec: VoiceCodec) -> &'static str {
    match codec {
        VoiceCodec::OpusWebM => "webm",
        VoiceCodec::OpusOgg => "ogg",
    }
}
