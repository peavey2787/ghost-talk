#![cfg(target_arch = "wasm32")]

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};

const LIVE_PREFIX: &str = "\u{1e}GHOST-LIVE-VOICE-V1:";
const P2P_CONTROL_PREFIX: &str = "\u{1e}GHOST-P2P-CONTROL-V1:";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveVoicePacket {
    pub version: u8,
    pub call_id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl LiveVoicePacket {
    pub fn new(call_id: String, kind: impl Into<String>) -> Self {
        Self {
            version: 1,
            call_id,
            kind: kind.into(),
            data: None,
        }
    }

    pub fn audio(call_id: String, bytes: &[u8]) -> Self {
        Self {
            version: 1,
            call_id,
            kind: "audio".into(),
            data: Some(STANDARD.encode(bytes)),
        }
    }

    pub fn audio_bytes(&self) -> Option<Vec<u8>> {
        (self.kind == "audio")
            .then_some(self.data.as_deref())
            .flatten()
            .and_then(|value| STANDARD.decode(value).ok())
    }
}

pub fn encode_live(packet: &LiveVoicePacket) -> Result<String, String> {
    validate_live(packet)?;
    serde_json::to_string(packet)
        .map(|json| format!("{LIVE_PREFIX}{json}"))
        .map_err(|error| error.to_string())
}

pub fn decode_live(body: &str) -> Option<LiveVoicePacket> {
    let json = body.strip_prefix(LIVE_PREFIX)?;
    let packet = serde_json::from_str::<LiveVoicePacket>(json).ok()?;
    validate_live(&packet).ok()?;
    Some(packet)
}

pub fn encode_p2p_control(packet: &ghost_protocol::RealtimeBodyV1) -> Result<String, String> {
    validate_p2p_control(packet)?;
    serde_json::to_string(packet)
        .map(|json| format!("{P2P_CONTROL_PREFIX}{json}"))
        .map_err(|error| error.to_string())
}

pub fn decode_p2p_control(body: &str) -> Option<ghost_protocol::RealtimeBodyV1> {
    let json = body.strip_prefix(P2P_CONTROL_PREFIX)?;
    let packet = serde_json::from_str::<ghost_protocol::RealtimeBodyV1>(json).ok()?;
    validate_p2p_control(&packet).ok()?;
    Some(packet)
}

pub fn is_realtime_body(body: &str) -> bool {
    body.starts_with(LIVE_PREFIX)
        || body.starts_with(P2P_CONTROL_PREFIX)
        || body.starts_with(ROOM_PREFIX)
}

fn validate_live(packet: &LiveVoicePacket) -> Result<(), String> {
    if packet.version != 1 || !wire_id(&packet.call_id) {
        return Err("Invalid live voice packet identity".into());
    }
    if !matches!(
        packet.kind.as_str(),
        "request" | "accept" | "decline" | "hangup" | "audio"
    ) {
        return Err("Invalid live voice packet kind".into());
    }
    if packet.kind == "audio" {
        let data = packet
            .data
            .as_deref()
            .ok_or_else(|| "Live voice audio packet is empty".to_string())?;
        let decoded = STANDARD
            .decode(data)
            .map_err(|_| "Live voice audio packet is not valid base64".to_string())?;
        if decoded.is_empty() || decoded.len() > 256 * 1024 {
            return Err("Live voice audio packet length is invalid".into());
        }
    }
    Ok(())
}

fn validate_p2p_control(packet: &ghost_protocol::RealtimeBodyV1) -> Result<(), String> {
    match packet {
        ghost_protocol::RealtimeBodyV1::TransportAnnounce(value)
        | ghost_protocol::RealtimeBodyV1::TransportAck(value) => {
            value.validate().map_err(|error| error.to_string())?;
            if value.peer_id.trim().is_empty() {
                return Err("p2p transport announcement has an empty PeerId".into());
            }
            Ok(())
        }
        _ => Err("unsupported p2p transport control body".into()),
    }
}

fn wire_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

const ROOM_PREFIX: &str = "\u{1e}GHOST-ROOM-VOICE-V1:";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomVoicePacket {
    pub version: u8,
    pub room_id: String,
    pub speaker_hydra_id: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub data: String,
}

impl RoomVoicePacket {
    pub fn audio(
        room_id: String,
        speaker_hydra_id: String,
        sequence: u64,
        timestamp_ms: u64,
        bytes: &[u8],
    ) -> Self {
        Self {
            version: 1,
            room_id,
            speaker_hydra_id,
            sequence,
            timestamp_ms,
            data: STANDARD.encode(bytes),
        }
    }

    pub fn audio_bytes(&self) -> Option<Vec<u8>> {
        STANDARD.decode(&self.data).ok()
    }
}

pub fn encode_room_voice(packet: &RoomVoicePacket) -> Result<String, String> {
    validate_room_voice(packet)?;
    serde_json::to_string(packet)
        .map(|json| format!("{ROOM_PREFIX}{json}"))
        .map_err(|error| error.to_string())
}

pub fn decode_room_voice(body: &str) -> Option<RoomVoicePacket> {
    let json = body.strip_prefix(ROOM_PREFIX)?;
    let packet = serde_json::from_str::<RoomVoicePacket>(json).ok()?;
    validate_room_voice(&packet).ok()?;
    Some(packet)
}

fn validate_room_voice(packet: &RoomVoicePacket) -> Result<(), String> {
    if packet.version != 1 || !wire_id(&packet.room_id) || packet.speaker_hydra_id.trim().is_empty()
    {
        return Err("Invalid room voice identity".into());
    }
    let bytes = STANDARD
        .decode(&packet.data)
        .map_err(|_| "Room voice audio is not valid base64")?;
    if bytes.is_empty() || bytes.len() > 256 * 1024 {
        return Err("Room voice audio length is invalid".into());
    }
    Ok(())
}
