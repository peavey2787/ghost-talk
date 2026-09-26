//! Room voice packets: ordered audio windows from one speaker to a Room.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};

use crate::{decode_audio, is_wire_id};

pub const ROOM_VOICE_PREFIX: &str = "\u{1e}GHOST-ROOM-VOICE-V1:";

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
        .map(|json| format!("{ROOM_VOICE_PREFIX}{json}"))
        .map_err(|error| error.to_string())
}

pub fn decode_room_voice(body: &str) -> Option<RoomVoicePacket> {
    let json = body.strip_prefix(ROOM_VOICE_PREFIX)?;
    let packet = serde_json::from_str::<RoomVoicePacket>(json).ok()?;
    validate_room_voice(&packet).ok()?;
    Some(packet)
}

fn validate_room_voice(packet: &RoomVoicePacket) -> Result<(), String> {
    if packet.version != 1
        || !is_wire_id(&packet.room_id)
        || packet.speaker_hydra_id.trim().is_empty()
    {
        return Err("Invalid room voice identity".into());
    }
    decode_audio(&packet.data, "Room voice audio").map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(bytes: &[u8]) -> RoomVoicePacket {
        RoomVoicePacket::audio("cd".repeat(16), "speaker".into(), 7, 1_000, bytes)
    }

    #[test]
    fn room_audio_round_trips_in_order_metadata() {
        let original = packet(&[9, 8, 7]);
        let body = encode_room_voice(&original).unwrap();
        assert!(body.starts_with(ROOM_VOICE_PREFIX));
        let decoded = decode_room_voice(&body).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(decoded.audio_bytes(), Some(vec![9, 8, 7]));
    }

    #[test]
    fn invalid_room_packets_fail_closed() {
        assert!(encode_room_voice(&packet(&[])).is_err());
        let mut speaker = packet(&[1]);
        speaker.speaker_hydra_id = " ".into();
        assert!(encode_room_voice(&speaker).is_err());
        let mut room = packet(&[1]);
        room.room_id = "room".into();
        assert!(encode_room_voice(&room).is_err());
        let mut version = packet(&[1]);
        version.version = 0;
        assert!(encode_room_voice(&version).is_err());
        let mut data = packet(&[1]);
        data.data = "***".into();
        assert!(encode_room_voice(&data).is_err());
        assert_eq!(decode_room_voice("plain text"), None);
    }
}
