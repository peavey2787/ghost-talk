//! 1:1 live call packets (signaling and audio windows) carried inside sealed
//! realtime bodies.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};

use crate::{decode_audio, is_wire_id};

pub const LIVE_VOICE_PREFIX: &str = "\u{1e}GHOST-LIVE-VOICE-V1:";
const CALL_KINDS: [&str; 5] = ["request", "accept", "decline", "hangup", "audio"];

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
    /// A signaling packet (`request`, `accept`, `decline`, `hangup`).
    pub fn new(call_id: String, kind: impl Into<String>) -> Self {
        Self {
            version: 1,
            call_id,
            kind: kind.into(),
            data: None,
        }
    }

    /// One complete, independently decodable audio window.
    pub fn audio(call_id: String, bytes: &[u8]) -> Self {
        Self {
            version: 1,
            call_id,
            kind: "audio".into(),
            data: Some(STANDARD.encode(bytes)),
        }
    }

    pub fn audio_bytes(&self) -> Option<Vec<u8>> {
        if self.kind != "audio" {
            return None;
        }
        self.data
            .as_deref()
            .and_then(|value| STANDARD.decode(value).ok())
    }
}

pub fn encode_live(packet: &LiveVoicePacket) -> Result<String, String> {
    validate_live(packet)?;
    serde_json::to_string(packet)
        .map(|json| format!("{LIVE_VOICE_PREFIX}{json}"))
        .map_err(|error| error.to_string())
}

pub fn decode_live(body: &str) -> Option<LiveVoicePacket> {
    let json = body.strip_prefix(LIVE_VOICE_PREFIX)?;
    let packet = serde_json::from_str::<LiveVoicePacket>(json).ok()?;
    validate_live(&packet).ok()?;
    Some(packet)
}

fn validate_live(packet: &LiveVoicePacket) -> Result<(), String> {
    if packet.version != 1 || !is_wire_id(&packet.call_id) {
        return Err("Invalid live voice packet identity".into());
    }
    if !CALL_KINDS.contains(&packet.kind.as_str()) {
        return Err("Invalid live voice packet kind".into());
    }
    if packet.kind == "audio" {
        let data = packet
            .data
            .as_deref()
            .ok_or_else(|| "Live voice audio packet is empty".to_string())?;
        decode_audio(data, "Live voice audio packet")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call_id() -> String {
        "ab".repeat(16)
    }

    #[test]
    fn signaling_and_audio_round_trip() {
        for kind in ["request", "accept", "decline", "hangup"] {
            let packet = LiveVoicePacket::new(call_id(), kind);
            let body = encode_live(&packet).unwrap();
            assert!(body.starts_with(LIVE_VOICE_PREFIX));
            assert_eq!(decode_live(&body), Some(packet.clone()));
            assert_eq!(packet.audio_bytes(), None);
        }
        let audio = LiveVoicePacket::audio(call_id(), &[1, 2, 3]);
        let decoded = decode_live(&encode_live(&audio).unwrap()).unwrap();
        assert_eq!(decoded.audio_bytes(), Some(vec![1, 2, 3]));
    }

    #[test]
    fn invalid_packets_fail_closed() {
        assert!(encode_live(&LiveVoicePacket::new("short".into(), "request")).is_err());
        assert!(encode_live(&LiveVoicePacket::new(call_id(), "ring")).is_err());
        let mut versioned = LiveVoicePacket::new(call_id(), "request");
        versioned.version = 2;
        assert!(encode_live(&versioned).is_err());
        let mut empty = LiveVoicePacket::audio(call_id(), &[1]);
        empty.data = None;
        assert!(encode_live(&empty).is_err());
        let mut garbage = LiveVoicePacket::audio(call_id(), &[1]);
        garbage.data = Some("***".into());
        assert!(encode_live(&garbage).is_err());
        assert!(encode_live(&LiveVoicePacket::audio(call_id(), &[])).is_err());
        assert_eq!(decode_live("no prefix"), None);
        assert_eq!(decode_live(&format!("{LIVE_VOICE_PREFIX}{{")), None);
    }
}
