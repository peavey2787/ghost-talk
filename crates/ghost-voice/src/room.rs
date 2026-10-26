//! Room media packets: ordered audio windows or video frames from one
//! participant to a Room. The body prefix names the medium.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};

use crate::{decode_audio, decode_frame, is_wire_id};

pub const ROOM_VOICE_PREFIX: &str = "\u{1e}GHOST-ROOM-VOICE-V1:";
pub const ROOM_VIDEO_PREFIX: &str = "\u{1e}GHOST-ROOM-VIDEO-V1:";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMediaPacket {
    pub version: u8,
    pub room_id: String,
    pub speaker_hydra_id: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub data: String,
}

impl RoomMediaPacket {
    pub fn new(
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

    pub fn bytes(&self) -> Option<Vec<u8>> {
        STANDARD.decode(&self.data).ok()
    }
}

pub fn encode_room_voice(packet: &RoomMediaPacket) -> Result<String, String> {
    encode(packet, ROOM_VOICE_PREFIX, validate_audio)
}

pub fn decode_room_voice(body: &str) -> Option<RoomMediaPacket> {
    decode(body, ROOM_VOICE_PREFIX, validate_audio)
}

pub fn encode_room_video(packet: &RoomMediaPacket) -> Result<String, String> {
    encode(packet, ROOM_VIDEO_PREFIX, validate_video)
}

pub fn decode_room_video(body: &str) -> Option<RoomMediaPacket> {
    decode(body, ROOM_VIDEO_PREFIX, validate_video)
}

type Validate = fn(&RoomMediaPacket) -> Result<(), String>;

fn encode(packet: &RoomMediaPacket, prefix: &str, validate: Validate) -> Result<String, String> {
    validate_identity(packet)?;
    validate(packet)?;
    serde_json::to_string(packet)
        .map(|json| format!("{prefix}{json}"))
        .map_err(|error| error.to_string())
}

fn decode(body: &str, prefix: &str, validate: Validate) -> Option<RoomMediaPacket> {
    let json = body.strip_prefix(prefix)?;
    let packet = serde_json::from_str::<RoomMediaPacket>(json).ok()?;
    validate_identity(&packet).ok()?;
    validate(&packet).ok()?;
    Some(packet)
}

fn validate_identity(packet: &RoomMediaPacket) -> Result<(), String> {
    if packet.version != 1
        || !is_wire_id(&packet.room_id)
        || packet.speaker_hydra_id.trim().is_empty()
    {
        return Err("Invalid room media identity".into());
    }
    Ok(())
}

fn validate_audio(packet: &RoomMediaPacket) -> Result<(), String> {
    decode_audio(&packet.data, "Room voice audio").map(|_| ())
}

fn validate_video(packet: &RoomMediaPacket) -> Result<(), String> {
    decode_frame(&packet.data, "Room video frame").map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const JPEG: [u8; 4] = [0xFF, 0xD8, 0xFF, 0xE0];

    fn packet(bytes: &[u8]) -> RoomMediaPacket {
        RoomMediaPacket::new("cd".repeat(16), "speaker".into(), 7, 1_000, bytes)
    }

    #[test]
    fn room_audio_round_trips_in_order_metadata() {
        let original = packet(&[9, 8, 7]);
        let body = encode_room_voice(&original).unwrap();
        assert!(body.starts_with(ROOM_VOICE_PREFIX));
        let decoded = decode_room_voice(&body).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(decoded.bytes(), Some(vec![9, 8, 7]));
        assert_eq!(decode_room_video(&body), None);
    }

    #[test]
    fn room_video_round_trips_and_is_never_audio() {
        let original = packet(&JPEG);
        let body = encode_room_video(&original).unwrap();
        assert!(body.starts_with(ROOM_VIDEO_PREFIX));
        assert_eq!(decode_room_video(&body), Some(original));
        assert_eq!(decode_room_voice(&body), None);
        assert!(encode_room_video(&packet(&[1, 2, 3])).is_err());
    }

    #[test]
    fn invalid_room_packets_fail_closed() {
        assert!(encode_room_voice(&packet(&[])).is_err());
        let mut speaker = packet(&[1]);
        speaker.speaker_hydra_id = " ".into();
        assert!(encode_room_voice(&speaker).is_err());
        let mut room = packet(&JPEG);
        room.room_id = "room".into();
        assert!(encode_room_video(&room).is_err());
        let mut version = packet(&[1]);
        version.version = 0;
        assert!(encode_room_voice(&version).is_err());
        let mut data = packet(&[1]);
        data.data = "***".into();
        assert!(encode_room_voice(&data).is_err());
        assert_eq!(decode_room_voice("plain text"), None);
    }
}
