use serde::{Deserialize, Serialize};

use crate::RoomAccess;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChannelPolicy {
    #[default]
    Off,
    Interactive,
    PresentersOnly,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum Role {
    Moderator,
    Presenter,
    #[default]
    Audience,
}

/// Canonical Room configurations. These are presets over the authoritative
/// Room model, not separate room implementations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RoomMode {
    PrivateGroup,
    CommunityVoice,
    Stage,
    Radio,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoomPolicies {
    pub access: RoomAccess,
    pub text: ChannelPolicy,
    pub audio: ChannelPolicy,
    pub broadcast: bool,
}

impl RoomPolicies {
    pub const fn for_mode(mode: RoomMode) -> Self {
        match mode {
            RoomMode::PrivateGroup => Self {
                access: RoomAccess::Private,
                text: ChannelPolicy::Interactive,
                audio: ChannelPolicy::Interactive,
                broadcast: false,
            },
            RoomMode::CommunityVoice => Self {
                access: RoomAccess::Public,
                text: ChannelPolicy::Interactive,
                audio: ChannelPolicy::Interactive,
                broadcast: false,
            },
            RoomMode::Stage => Self {
                access: RoomAccess::Public,
                text: ChannelPolicy::Interactive,
                audio: ChannelPolicy::PresentersOnly,
                broadcast: false,
            },
            RoomMode::Radio => Self {
                access: RoomAccess::Public,
                text: ChannelPolicy::Off,
                audio: ChannelPolicy::PresentersOnly,
                broadcast: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ChannelPolicy, RoomMode, RoomPolicies};

    #[test]
    fn stage_and_radio_are_room_policy_presets() {
        let stage = RoomPolicies::for_mode(RoomMode::Stage);
        assert_eq!(stage.audio, ChannelPolicy::PresentersOnly);
        assert!(!stage.broadcast);
        let radio = RoomPolicies::for_mode(RoomMode::Radio);
        assert_eq!(radio.audio, ChannelPolicy::PresentersOnly);
        assert!(radio.broadcast);
    }
}
