use serde::{Deserialize, Serialize};

pub const HYDRA_PQ: &str = "hydra-pq";
pub const KKTP_V2: &str = "kktp-v2";
pub const KASPA_MAILBOX: &str = "kaspa-mailbox";
pub const PRIVATE_BOOTSTRAP: &str = "private-bootstrap";
pub const KASIA_V1: &str = "kasia-v1";
pub const GHOST_PROFILE_V2: &str = "ghost-profile-v2";
pub const GHOST_AVATAR_V1: &str = "ghost-avatar-v1";
pub const GHOST_ROOM_VOICE_V1: &str = "ghost-room-voice-v1";
pub const GHOST_MEDIA_V1: &str = "ghost-media-v1";
pub const GHOST_BROADCAST_V1: &str = "ghost-broadcast-v1";

/// User-visible protocol mode for one-to-one conversations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConversationMode {
    GhostPq,
    Kasia,
}

/// Availability advertised by a remote identity at selection time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProtocolAvailability {
    pub ghost_pq: bool,
    pub kasia: bool,
}

impl ProtocolAvailability {
    pub fn from_capabilities(values: &[String]) -> Self {
        Self {
            ghost_pq: values.iter().any(|value| value == HYDRA_PQ),
            kasia: values.iter().any(|value| value == KASIA_V1),
        }
    }

    fn supports(self, mode: ConversationMode) -> bool {
        match mode {
            ConversationMode::GhostPq => self.ghost_pq,
            ConversationMode::Kasia => self.kasia,
        }
    }
}

/// Selects a protocol without ever silently weakening an established Ghost PQ chat.
pub fn select_conversation_mode(
    established: Option<ConversationMode>,
    availability: ProtocolAvailability,
) -> Result<ConversationMode, &'static str> {
    if let Some(mode) = established {
        return keep_established(mode, availability);
    }
    select_new(availability)
}

fn keep_established(
    mode: ConversationMode,
    availability: ProtocolAvailability,
) -> Result<ConversationMode, &'static str> {
    if availability.supports(mode) {
        return Ok(mode);
    }
    Err(unavailable_message(mode))
}

fn unavailable_message(mode: ConversationMode) -> &'static str {
    match mode {
        ConversationMode::GhostPq => "established Ghost PQ mode is temporarily unavailable",
        ConversationMode::Kasia => "established Kasia mode is temporarily unavailable",
    }
}

fn select_new(availability: ProtocolAvailability) -> Result<ConversationMode, &'static str> {
    if availability.ghost_pq {
        Ok(ConversationMode::GhostPq)
    } else if availability.kasia {
        Ok(ConversationMode::Kasia)
    } else {
        Err("peer advertises no compatible messaging protocol")
    }
}

pub fn default_capabilities() -> Vec<String> {
    [
        HYDRA_PQ,
        KKTP_V2,
        KASPA_MAILBOX,
        PRIVATE_BOOTSTRAP,
        KASIA_V1,
        GHOST_PROFILE_V2,
        GHOST_AVATAR_V1,
        GHOST_ROOM_VOICE_V1,
        GHOST_MEDIA_V1,
        GHOST_BROADCAST_V1,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_chat_prefers_ghost_pq() {
        let mode = select_conversation_mode(
            None,
            ProtocolAvailability {
                ghost_pq: true,
                kasia: true,
            },
        );
        assert_eq!(mode, Ok(ConversationMode::GhostPq));
    }

    #[test]
    fn established_ghost_never_falls_back_to_kasia() {
        let mode = select_conversation_mode(
            Some(ConversationMode::GhostPq),
            ProtocolAvailability {
                ghost_pq: false,
                kasia: true,
            },
        );
        assert!(mode.is_err());
    }

    #[test]
    fn established_kasia_does_not_silently_upgrade_or_switch() {
        let mode = select_conversation_mode(
            Some(ConversationMode::Kasia),
            ProtocolAvailability {
                ghost_pq: true,
                kasia: true,
            },
        );
        assert_eq!(mode, Ok(ConversationMode::Kasia));
    }

    #[test]
    fn advertised_capabilities_drive_new_conversation_selection() {
        let availability =
            ProtocolAvailability::from_capabilities(&[KASIA_V1.to_owned(), HYDRA_PQ.to_owned()]);
        assert_eq!(
            select_conversation_mode(None, availability),
            Ok(ConversationMode::GhostPq),
        );
    }
}
