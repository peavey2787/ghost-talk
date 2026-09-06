#![forbid(unsafe_code)]

use ghost_core::{ContactId, RoomId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChannelPolicy {
    Off,
    Interactive,
    PresentersOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Role {
    Moderator,
    Presenter,
    Audience,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Room {
    pub id: RoomId,
    pub label: String,
    pub text: ChannelPolicy,
    pub audio: ChannelPolicy,
    pub members: BTreeMap<ContactId, Role>,
}

impl Room {
    pub fn can_send_text(&self, id: ContactId) -> bool {
        self.can_send(id, self.text)
    }

    pub fn can_send_audio(&self, id: ContactId) -> bool {
        self.can_send(id, self.audio)
    }

    fn can_send(&self, id: ContactId, p: ChannelPolicy) -> bool {
        match p {
            ChannelPolicy::Off => false,
            ChannelPolicy::Interactive => self.members.contains_key(&id),
            ChannelPolicy::PresentersOnly => matches!(
                self.members.get(&id),
                Some(Role::Moderator | Role::Presenter)
            ),
        }
    }
}

pub fn radio(id: RoomId, label: String) -> Room {
    Room {
        id,
        label,
        text: ChannelPolicy::Off,
        audio: ChannelPolicy::PresentersOnly,
        members: BTreeMap::new(),
    }
}

pub fn presenter(id: RoomId, label: String) -> Room {
    Room {
        id,
        label,
        text: ChannelPolicy::PresentersOnly,
        audio: ChannelPolicy::PresentersOnly,
        members: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audience_cannot_send_presenter_room() {
        let id = ghost_core::Id128([1; 16]);
        let mut r = presenter(ghost_core::Id128([2; 16]), "demo".into());
        r.members.insert(id, Role::Audience);
        assert!(!r.can_send_text(id));
        assert!(!r.can_send_audio(id));
        r.members.insert(id, Role::Presenter);
        assert!(r.can_send_text(id));
    }
}
