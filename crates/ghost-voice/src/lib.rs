//! Ghost Talk voice: the 1:1 call and Room voice wire formats carried inside
//! sealed realtime bodies, plus the voice route policy.
//!
//! Route semantics are owned once by `ghost-realtime`; this crate names them
//! for voice consumers (`VoicePreference::Automatic` tries p2p-net, then
//! Kaspa; `VoicePreference::KaspaOnly` never opens a direct transport).

#![forbid(unsafe_code)]

mod call;
mod room;

use base64::{engine::general_purpose::STANDARD, Engine as _};

pub use call::{decode_live, encode_live, LiveVoicePacket, LIVE_VOICE_PREFIX};
pub use ghost_realtime::{
    route_order, RealtimeCarrier as VoiceRoute, ReplayIdentity, RoutePreference as VoicePreference,
    GTR1_MAGIC, VOICE_TOPIC,
};
pub use room::{decode_room_voice, encode_room_voice, RoomVoicePacket, ROOM_VOICE_PREFIX};

/// Upper bound for one encoded audio window.
pub const MAX_VOICE_WINDOW_BYTES: usize = 256 * 1024;

/// True when a decrypted realtime body is a Ghost Talk voice packet.
pub fn is_voice_body(body: &str) -> bool {
    body.starts_with(LIVE_VOICE_PREFIX) || body.starts_with(ROOM_VOICE_PREFIX)
}

fn is_wire_id(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn decode_audio(data: &str, label: &str) -> Result<Vec<u8>, String> {
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| format!("{label} is not valid base64"))?;
    if bytes.is_empty() || bytes.len() > MAX_VOICE_WINDOW_BYTES {
        return Err(format!("{label} length is invalid"));
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
    fn voice_bodies_are_recognized_by_prefix() {
        assert!(is_voice_body(&format!("{LIVE_VOICE_PREFIX}{{}}")));
        assert!(is_voice_body(&format!("{ROOM_VOICE_PREFIX}{{}}")));
        assert!(!is_voice_body("hello"));
    }

    #[test]
    fn audio_windows_are_bounded() {
        let oversized = STANDARD.encode(vec![0u8; MAX_VOICE_WINDOW_BYTES + 1]);
        assert!(decode_audio(&oversized, "window").is_err());
        assert_eq!(decode_audio(&STANDARD.encode([1u8]), "window"), Ok(vec![1]));
    }
}
