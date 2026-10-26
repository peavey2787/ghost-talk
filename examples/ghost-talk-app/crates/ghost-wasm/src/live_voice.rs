#![cfg(target_arch = "wasm32")]

//! Realtime body helpers. Voice and video packet formats are owned by
//! `ghost-voice`; this module adds only the authenticated p2p
//! transport-control envelope.

pub use ghost_voice::{
    decode_live, decode_room_video, decode_room_voice, encode_live, encode_room_video,
    encode_room_voice, LiveVoicePacket, RoomMediaPacket,
};

const P2P_CONTROL_PREFIX: &str = "\u{1e}GHOST-P2P-CONTROL-V1:";

pub fn encode_p2p_control(packet: &ghost_protocol::RealtimeBodyV1) -> Result<String, String> {
    packet
        .announcement()
        .validate()
        .map_err(|error| error.to_string())?;
    serde_json::to_string(packet)
        .map(|json| format!("{P2P_CONTROL_PREFIX}{json}"))
        .map_err(|error| error.to_string())
}

pub fn decode_p2p_control(body: &str) -> Option<ghost_protocol::RealtimeBodyV1> {
    let json = body.strip_prefix(P2P_CONTROL_PREFIX)?;
    let packet = serde_json::from_str::<ghost_protocol::RealtimeBodyV1>(json).ok()?;
    packet.announcement().validate().ok()?;
    Some(packet)
}

/// Short label of a realtime body for protocol-debug records (no content).
pub fn body_kind(body: &str) -> &'static str {
    if body.starts_with(ghost_voice::LIVE_VOICE_PREFIX) {
        if body.contains("\"kind\":\"video\"") {
            "call-video"
        } else {
            "call-voice"
        }
    } else if body.starts_with(ghost_voice::ROOM_VOICE_PREFIX) {
        "room-voice"
    } else if body.starts_with(ghost_voice::ROOM_VIDEO_PREFIX) {
        "room-video"
    } else if body.starts_with(ghost_protocol::DIRECT_TEXT_PREFIX) {
        "direct-text"
    } else {
        "transport-control"
    }
}

/// True when a decrypted body belongs to the realtime (non-transcript) plane.
pub fn is_realtime_body(body: &str) -> bool {
    ghost_voice::is_media_body(body)
        || body.starts_with(P2P_CONTROL_PREFIX)
        || body.starts_with(ghost_protocol::DIRECT_TEXT_PREFIX)
}
