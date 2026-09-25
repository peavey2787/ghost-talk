use ghost_api::{BroadcastStartRequest, BroadcastStopResult};
use ghost_broadcast::{validate_relay_url, validate_rtmp_configuration, validate_session_id};
use ghost_talk::{VoiceChunk, VoiceCodec};
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap};

struct Session {
    record_local: bool,
    codec: Option<VoiceCodec>,
    bytes: Vec<u8>,
    next_sequence: Option<u64>,
}

thread_local! {
    static SESSIONS: RefCell<HashMap<String, Session>> = RefCell::new(HashMap::new());
}

pub(super) fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    match command {
        "broadcast_start" => start(args),
        "broadcast_push" => push(args),
        "broadcast_stop" => stop(args),
        _ => Err(format!("unknown browser broadcast command: {command}")),
    }
}

fn start(args: &Value) -> Result<Value, String> {
    let request: BroadcastStartRequest = super::support::util::required(args, "request")?;
    validate_session_id(&request.session_id)?;
    validate_rtmp_configuration(request.rtmp_server.as_deref(), request.rtmp_stream_key.as_deref())?;
    let relay = request.relay_url.as_deref().map(str::trim).filter(|value| !value.is_empty());
    if let Some(url) = relay {
        validate_relay_url(url)?;
    }
    let has_rtmp = request.rtmp_server.as_deref().is_some_and(|value| !value.trim().is_empty())
        || request.rtmp_stream_key.as_deref().is_some_and(|value| !value.trim().is_empty());
    if has_rtmp && relay.is_none() {
        return Err("Standalone Web live RTMP requires a Ghost Talk broadcast relay".into());
    }
    if !request.record_local && relay.is_none() {
        return Err("browser broadcast requires local recording or a broadcast relay".into());
    }
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        if sessions.contains_key(&request.session_id) {
            return Err("broadcast session already exists".into());
        }
        sessions.insert(request.session_id, Session { record_local: request.record_local, codec: None, bytes: Vec::new(), next_sequence: None });
        Ok(Value::Null)
    })
}

fn push(args: &Value) -> Result<Value, String> {
    let session_id = super::support::util::required_str(args, "sessionId")?;
    let sequence = args.get("sequence").and_then(Value::as_u64).ok_or("broadcast sequence is missing")?;
    let encoded: Vec<u8> = super::support::util::required(args, "encoded")?;
    let chunk = decode_chunk(&encoded)?;
    SESSIONS.with(|sessions| push_chunk(&mut sessions.borrow_mut(), session_id, sequence, chunk))
}


fn decode_chunk(encoded: &[u8]) -> Result<VoiceChunk, String> {
    if encoded.is_empty() || encoded.len() > 2 * 1024 * 1024 {
        return Err("broadcast frame size is invalid".into());
    }
    VoiceChunk::decode(encoded).map_err(|error| format!("invalid Ghost voice frame: {error}"))
}

fn push_chunk(
    sessions: &mut HashMap<String, Session>,
    session_id: &str,
    sequence: u64,
    chunk: VoiceChunk,
) -> Result<Value, String> {
    let session = sessions.get_mut(session_id).ok_or("broadcast session is not active")?;
    validate_sequence(session, sequence)?;
    validate_codec(session, chunk.codec)?;
    session.next_sequence = Some(sequence.saturating_add(1));
    if session.record_local {
        session.bytes.extend_from_slice(&chunk.payload);
    }
    Ok(Value::Null)
}

fn validate_sequence(session: &Session, sequence: u64) -> Result<(), String> {
    if session.next_sequence.is_some_and(|expected| sequence != expected) {
        return Err("broadcast frame sequence is not contiguous".into());
    }
    Ok(())
}

fn validate_codec(session: &mut Session, codec: VoiceCodec) -> Result<(), String> {
    if session.codec.is_some_and(|active| active != codec) {
        return Err("broadcast codec changed during an active session".into());
    }
    if session.codec.is_none() {
        session.codec = Some(codec);
    }
    Ok(())
}

fn stop(args: &Value) -> Result<Value, String> {
    let session_id = super::support::util::required_str(args, "sessionId")?;
    let session = SESSIONS.with(|sessions| sessions.borrow_mut().remove(session_id))
        .ok_or("broadcast session is not active")?;
    let recording = if session.record_local && !session.bytes.is_empty() {
        let content_type = session.codec.unwrap_or(VoiceCodec::OpusWebM).mime_type();
        Some(super::media::store_local_bytes(content_type, &session.bytes)?)
    } else { None };
    super::support::util::to_value(BroadcastStopResult { failures: Vec::new(), recording })
}
