use serde::{Deserialize, Serialize};

use crate::{ChannelPolicy, Role};
use ghost_domain::reaction::{actor_reaction, set_actor_reaction, MessageReaction, ReactionKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoomAccess {
    #[default]
    Private,
    InviteOnly,
    Unlisted,
    Public,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMember {
    #[serde(default)]
    pub contact_id: Option<String>,
    pub label: String,
    pub kaspa_address: String,
    #[serde(default)]
    pub hydra_handle: Option<String>,
    #[serde(default)]
    pub role: Role,
}

impl RoomMember {
    pub(crate) fn bind_peer(
        &mut self,
        contact_id: Option<String>,
        kaspa_address: String,
        hydra_handle: String,
    ) {
        self.contact_id = contact_id;
        self.kaspa_address = kaspa_address;
        self.hydra_handle = Some(hydra_handle);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomMessage {
    pub id: String,
    pub sender_hydra_id: String,
    pub sender_label: String,
    pub body: String,
    pub created_at: f64,
    #[serde(default)]
    pub reactions: Vec<MessageReaction>,
}

impl RoomMessage {
    pub fn set_reaction(&mut self, actor_id: &str, kind: Option<ReactionKind>) -> bool {
        set_actor_reaction(&mut self.reactions, actor_id, kind)
    }

    pub fn reaction_for(&self, actor_id: &str) -> Option<ReactionKind> {
        actor_reaction(&self.reactions, actor_id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomBan {
    pub label: String,
    pub kaspa_address: String,
    #[serde(default)]
    pub hydra_handle: Option<String>,
    /// Milliseconds since Unix epoch. None means the room-owner ban does not expire.
    #[serde(default)]
    pub expires_at: Option<f64>,
}

impl RoomBan {
    pub fn is_active(&self, now: f64) -> bool {
        self.expires_at.is_none_or(|expires_at| expires_at > now)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub id: String,
    pub name: String,
    pub owner_hydra_id: String,
    pub owner_kaspa_address: String,
    pub owner_label: String,
    #[serde(default)]
    pub(crate) access: RoomAccess,
    #[serde(default)]
    pub(crate) text_policy: ChannelPolicy,
    #[serde(default)]
    pub(crate) audio_policy: ChannelPolicy,
    #[serde(default)]
    pub(crate) broadcast_enabled: bool,
    #[serde(default)]
    pub(crate) pending_acceptance: bool,
    #[serde(default)]
    pub(crate) revision: u64,
    #[serde(default)]
    pub(crate) members: Vec<RoomMember>,
    #[serde(default)]
    pub(crate) bans: Vec<RoomBan>,
    #[serde(default)]
    pub(crate) messages: Vec<RoomMessage>,
}

impl Room {
    pub fn access(&self) -> RoomAccess {
        self.access
    }
    pub fn pending_acceptance(&self) -> bool {
        self.pending_acceptance
    }
    pub fn text_policy(&self) -> ChannelPolicy {
        self.text_policy
    }
    pub fn audio_policy(&self) -> ChannelPolicy {
        self.audio_policy
    }
    pub fn broadcast_enabled(&self) -> bool {
        self.broadcast_enabled
    }
    pub fn can_speak(&self, hydra_id: &str) -> bool {
        self.allowed_by_policy(hydra_id, self.audio_policy)
    }
    pub fn can_broadcast(&self, hydra_id: &str) -> bool {
        self.broadcast_enabled && self.can_speak(hydra_id)
    }
    pub fn can_send_text(&self, hydra_id: &str) -> bool {
        self.allowed_by_policy(hydra_id, self.text_policy)
    }
    pub fn can_react(&self, hydra_id: &str) -> bool {
        hydra_id == self.owner_hydra_id
            || self
                .members
                .iter()
                .any(|member| member.hydra_handle.as_deref() == Some(hydra_id))
    }
    fn allowed_by_policy(&self, hydra_id: &str, policy: ChannelPolicy) -> bool {
        let owner = hydra_id == self.owner_hydra_id;
        let role = self
            .members
            .iter()
            .find(|member| member.hydra_handle.as_deref() == Some(hydra_id))
            .map(|member| member.role);
        match policy {
            ChannelPolicy::Off => false,
            ChannelPolicy::Interactive => owner || role.is_some(),
            ChannelPolicy::PresentersOnly => {
                owner || matches!(role, Some(Role::Moderator | Role::Presenter))
            }
        }
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn members(&self) -> &[RoomMember] {
        &self.members
    }
    pub fn bans(&self) -> &[RoomBan] {
        &self.bans
    }
    pub fn messages(&self) -> &[RoomMessage] {
        &self.messages
    }

    pub(crate) fn accept(&mut self) {
        self.pending_acceptance = false;
    }
    pub(crate) fn bump_revision(&mut self) -> u64 {
        self.revision = self.revision.saturating_add(1);
        self.revision
    }
    pub(crate) fn apply_state(&mut self, state: crate::RoomState) {
        self.name = state.name;
        self.owner_kaspa_address = state.owner_kaspa_address;
        self.owner_label = state.owner_label;
        self.access = state.access;
        self.text_policy = state.text_policy;
        self.audio_policy = state.audio_policy;
        self.broadcast_enabled = state.broadcast_enabled;
        self.revision = state.revision;
        self.members = state.members;
        self.bans = state.bans;
        if state.accepted {
            self.pending_acceptance = false;
        }
    }

    pub(crate) fn retain_active_bans(&mut self, now: f64) {
        self.bans.retain(|ban| ban.is_active(now));
    }
    pub(crate) fn add_member(&mut self, member: RoomMember) {
        self.members.push(member);
    }
    pub(crate) fn remove_member_by_hydra(&mut self, hydra_id: &str) {
        self.members
            .retain(|member| member.hydra_handle.as_deref() != Some(hydra_id));
    }
    pub(crate) fn remove_member_by_address(&mut self, address: &str) {
        self.members
            .retain(|member| !member.kaspa_address.eq_ignore_ascii_case(address));
    }
    pub(crate) fn add_message(&mut self, message: RoomMessage) {
        self.messages.push(message);
    }
    pub(crate) fn ban_member(&mut self, ban: RoomBan) {
        self.remove_member_by_address(&ban.kaspa_address);
        self.bans.retain(|existing| {
            !existing
                .kaspa_address
                .eq_ignore_ascii_case(&ban.kaspa_address)
        });
        self.bans.push(ban);
    }
    pub(crate) fn unban_address(&mut self, address: &str) {
        self.bans
            .retain(|ban| !ban.kaspa_address.eq_ignore_ascii_case(address));
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RoomWire {
    State {
        room_id: String,
        name: String,
        owner_hydra_id: String,
        owner_kaspa_address: String,
        owner_label: String,
        #[serde(default)]
        access: RoomAccess,
        #[serde(default)]
        text_policy: ChannelPolicy,
        #[serde(default)]
        audio_policy: ChannelPolicy,
        #[serde(default)]
        broadcast_enabled: bool,
        revision: u64,
        members: Vec<RoomMember>,
        bans: Vec<RoomBan>,
    },
    Message {
        room_id: String,
        message: RoomMessage,
    },
    Reaction {
        room_id: String,
        target_message_id: String,
        actor_hydra_id: String,
        reaction: Option<ReactionKind>,
    },
    Leave {
        room_id: String,
        member_hydra_id: String,
        member_kaspa_address: String,
    },
    Kick {
        room_id: String,
        revision: u64,
        target_hydra_id: Option<String>,
        target_kaspa_address: String,
    },
    Ban {
        room_id: String,
        revision: u64,
        target_hydra_id: Option<String>,
        target_kaspa_address: String,
        expires_at: Option<f64>,
    },
    Disband {
        room_id: String,
        revision: u64,
    },
}
