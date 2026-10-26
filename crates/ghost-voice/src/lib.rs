//! Ghost Talk live media: the 1:1 call and Room voice/video wire formats
//! carried inside sealed realtime bodies, plus the voice route policy.
//!
//! Route semantics are owned once by `ghost-realtime`; this crate names them
//! for voice consumers (`VoicePreference::Automatic` tries p2p-net, then
//! Kaspa; `VoicePreference::KaspaOnly` never opens a direct transport).
//! Video frames are p2p-net only: the application never puts them on Kaspa.

#![forbid(unsafe_code)]

mod call;
mod room;

use base64::{engine::general_purpose::STANDARD, Engine as _};

pub use call::{decode_live, encode_live, LiveVoicePacket, LIVE_VOICE_PREFIX};
pub use ghost_realtime::{
    route_order, RealtimeCarrier as VoiceRoute, ReplayIdentity, RoutePreference as VoicePreference,
    GTR1_MAGIC, VOICE_TOPIC,
};
pub use room::{
    decode_room_video, decode_room_voice, encode_room_video, encode_room_voice, RoomMediaPacket,
    ROOM_VIDEO_PREFIX, ROOM_VOICE_PREFIX,
};

/// Upper bound for one encoded audio window.
pub const MAX_VOICE_WINDOW_BYTES: usize = 256 * 1024;

/// Upper bound for one JPEG video frame.
pub const MAX_VIDEO_FRAME_BYTES: usize = 96 * 1024;

const JPEG_START: [u8; 3] = [0xFF, 0xD8, 0xFF];

/// True when a decrypted realtime body is a Ghost Talk voice or video packet.
pub fn is_media_body(body: &str) -> bool {
    [LIVE_VOICE_PREFIX, ROOM_VOICE_PREFIX, ROOM_VIDEO_PREFIX]
        .iter()
        .any(|prefix| body.starts_with(prefix))
}

fn is_wire_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn decode_bounded(data: &str, max_bytes: usize, label: &str) -> Result<Vec<u8>, String> {
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| format!("{label} is not valid base64"))?;
    if bytes.is_empty() || bytes.len() > max_bytes {
        return Err(format!("{label} length is invalid"));
    }
    Ok(bytes)
}

fn decode_audio(data: &str, label: &str) -> Result<Vec<u8>, String> {
    decode_bounded(data, MAX_VOICE_WINDOW_BYTES, label)
}

/// A video frame must be one bounded JPEG image.
fn decode_frame(data: &str, label: &str) -> Result<Vec<u8>, String> {
    let bytes = decode_bounded(data, MAX_VIDEO_FRAME_BYTES, label)?;
    if !bytes.starts_with(&JPEG_START) {
        return Err(format!("{label} is not a JPEG image"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_prefers_p2p_net_then_kaspa() {
        assert_eq!(
            route_order(VoicePreference::Automatic),
            &[VoiceRoute::P2pNet, VoiceRoute::Kaspa]
        );
    }

    #[test]
    fn kaspa_only_has_no_direct_fallback() {
        assert_eq!(
            route_order(VoicePreference::KaspaOnly),
            &[VoiceRoute::Kaspa]
        );
    }

    #[test]
    fn media_bodies_are_recognized_by_prefix() {
        assert!(is_media_body(&format!("{LIVE_VOICE_PREFIX}{{}}")));
        assert!(is_media_body(&format!("{ROOM_VOICE_PREFIX}{{}}")));
        assert!(is_media_body(&format!("{ROOM_VIDEO_PREFIX}{{}}")));
        assert!(!is_media_body("hello"));
    }

    #[test]
    fn audio_windows_are_bounded() {
        let oversized = STANDARD.encode(vec![0u8; MAX_VOICE_WINDOW_BYTES + 1]);
        assert!(decode_audio(&oversized, "window").is_err());
        assert_eq!(decode_audio(&STANDARD.encode([1u8]), "window"), Ok(vec![1]));
    }

    #[test]
    fn video_frames_must_be_bounded_jpeg_images() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(
            decode_frame(&STANDARD.encode(jpeg), "frame"),
            Ok(jpeg.to_vec())
        );
        assert!(decode_frame(&STANDARD.encode(b"<svg/>"), "frame").is_err());
        let mut oversized = vec![0u8; MAX_VIDEO_FRAME_BYTES + 1];
        oversized[..3].copy_from_slice(&JPEG_START);
        assert!(decode_frame(&STANDARD.encode(oversized), "frame").is_err());
    }
}
