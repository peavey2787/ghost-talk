//! Canonical p2p-net topic names. Every Ghost Talk realtime consumer derives
//! topics here so the application, Kaspa Kinesis, and future hosts can never
//! drift onto different namespaces.

/// Session control (handshakes that bind transport and game/app identities).
pub const CONTROL_TOPIC: &str = "ghost-talk/session/control/v1";
/// Shared realtime session carrier (gameplay/session traffic).
pub const REALTIME_TOPIC: &str = "ghost-talk/session/realtime/v1";
/// Shared voice carrier.
pub const VOICE_TOPIC: &str = "ghost-talk/voice/v1";
/// Prefix of per-room topics; see [`room_topic`].
pub const ROOM_TOPIC_PREFIX: &str = "ghost-talk/room/v1/";
/// Prefix of per-conversation topics; see [`session_topic`].
pub const SESSION_TOPIC_PREFIX: &str = "ghost-talk/realtime/v1/";

pub fn room_topic(room_id: &[u8; 16]) -> String {
    format!("{ROOM_TOPIC_PREFIX}{}", hex::encode(room_id))
}

/// Domain-separated topic id for one authenticated conversation session. The
/// SID itself never appears on the wire as readable hexadecimal.
pub fn session_topic_id(sid: &[u8; 16]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"GhostTalk/realtime-topic/v1\0");
    hasher.update(sid);
    hasher.finalize().to_hex()[..32].to_owned()
}

pub fn session_topic(sid: &[u8; 16]) -> String {
    format!("{SESSION_TOPIC_PREFIX}{}", session_topic_id(sid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_topics_are_stable() {
        assert_eq!(
            room_topic(&[0xab; 16]),
            "ghost-talk/room/v1/abababababababababababababababab"
        );
    }

    #[test]
    fn session_topic_is_deterministic_and_hides_the_sid() {
        let sid = [7; 16];
        let topic = session_topic(&sid);
        assert_eq!(topic, session_topic(&sid));
        assert!(topic.starts_with(SESSION_TOPIC_PREFIX));
        assert!(!topic.contains(&hex::encode(sid)));
        assert_eq!(session_topic_id(&sid).len(), 32);
        assert_ne!(session_topic(&sid), session_topic(&[8; 16]));
    }

    #[test]
    fn shared_topics_are_versioned_and_distinct() {
        let topics = [CONTROL_TOPIC, REALTIME_TOPIC, VOICE_TOPIC];
        for topic in topics {
            assert!(topic.starts_with("ghost-talk/"));
            assert!(topic.ends_with("/v1"));
        }
        assert_ne!(CONTROL_TOPIC, REALTIME_TOPIC);
        assert_ne!(REALTIME_TOPIC, VOICE_TOPIC);
    }
}
