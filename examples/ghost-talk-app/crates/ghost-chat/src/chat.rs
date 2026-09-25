use ghost_domain::identity::{optional_binding_matches, PeerBinding};
use serde::{Deserialize, Serialize};

use crate::{IncomingRequest, Message};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub(crate) messages: Vec<Message>,
    #[serde(default)]
    pub(crate) contact_id: Option<String>,
    #[serde(default)]
    pub(crate) peer_kaspa_address: Option<String>,
    #[serde(default)]
    pub(crate) peer_kns_name: Option<String>,
    #[serde(default)]
    pub(crate) peer_dotk_name: Option<String>,
    #[serde(default)]
    pub(crate) peer_hydra_handle: Option<String>,
    #[serde(default)]
    pub(crate) verified_public: bool,
    #[serde(default)]
    pub(crate) bootstrap_complete: bool,
    #[serde(default)]
    pub(crate) archived: bool,
    #[serde(default)]
    pub(crate) left: bool,
    #[serde(default)]
    pub(crate) peer_left: bool,
    #[serde(default)]
    pub(crate) session_sid: Option<String>,
    #[serde(default)]
    pub(crate) session_role: Option<String>,
    #[serde(default)]
    pub(crate) incoming_request: Option<IncomingRequest>,
    #[serde(default)]
    pub(crate) room_transport_only: bool,
    #[serde(default)]
    pub(crate) unread_count: u32,
    #[serde(default)]
    pub(crate) transport_restore_pending: bool,
    #[serde(default)]
    pub(crate) transport_restore_message_id: Option<String>,
}

impl Chat {
    pub fn contact_id(&self) -> Option<&str> {
        self.contact_id.as_deref()
    }
    pub fn peer_kaspa_address(&self) -> Option<&str> {
        self.peer_kaspa_address.as_deref()
    }
    pub fn peer_kns_name(&self) -> Option<&str> {
        self.peer_kns_name.as_deref()
    }
    pub fn peer_dotk_name(&self) -> Option<&str> {
        self.peer_dotk_name.as_deref()
    }
    pub fn peer_hydra_handle(&self) -> Option<&str> {
        self.peer_hydra_handle.as_deref()
    }
    pub fn verified_public(&self) -> bool {
        self.verified_public
    }
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }
    pub fn bootstrap_complete(&self) -> bool {
        self.bootstrap_complete
    }
    pub fn archived(&self) -> bool {
        self.archived
    }
    pub fn left(&self) -> bool {
        self.left
    }
    pub fn peer_left(&self) -> bool {
        self.peer_left
    }
    pub fn session_sid(&self) -> Option<&str> {
        self.session_sid.as_deref()
    }
    pub fn session_role(&self) -> Option<&str> {
        self.session_role.as_deref()
    }
    pub fn incoming_request(&self) -> Option<&IncomingRequest> {
        self.incoming_request.as_ref()
    }
    pub fn room_transport_only(&self) -> bool {
        self.room_transport_only
    }
    pub fn unread_count(&self) -> u32 {
        self.unread_count
    }
    pub fn transport_restore_pending(&self) -> bool {
        self.transport_restore_pending
    }
    pub fn transport_restore_message_id(&self) -> Option<&str> {
        self.transport_restore_message_id.as_deref()
    }

    /// True when this record is a user-visible direct chat that can safely be
    /// reused for a new authenticated request/call binding.
    pub fn reusable_direct(&self) -> bool {
        !self.room_transport_only && !self.archived && !self.left && !self.peer_left
    }

    pub fn peer_binding(&self) -> Option<PeerBinding> {
        PeerBinding::from_optional(
            self.peer_kaspa_address.as_deref(),
            self.peer_hydra_handle.as_deref(),
        )
    }

    pub fn matches_peer(&self, peer: &PeerBinding) -> bool {
        optional_binding_matches(
            self.peer_kaspa_address.as_deref(),
            self.peer_hydra_handle.as_deref(),
            peer,
        )
    }
}

mod mutation;

#[cfg(test)]
mod tests {
    use super::{Chat, Message};
    use ghost_domain::identity::PeerBinding;

    #[test]
    fn peer_matching_requires_complete_binding() {
        let chat = Chat {
            peer_kaspa_address: Some("kaspa:peer".into()),
            peer_hydra_handle: Some("hydra-peer".into()),
            ..Default::default()
        };
        let exact = PeerBinding::new("KASPA:PEER", "hydra-peer").unwrap();
        let wrong = PeerBinding::new("kaspa:peer", "different").unwrap();
        assert!(chat.matches_peer(&exact));
        assert!(!chat.matches_peer(&wrong));
    }

    #[test]
    fn leaving_clears_transport_state() {
        let mut chat = Chat {
            session_sid: Some("sid".into()),
            bootstrap_complete: true,
            ..Default::default()
        };
        chat.leave_local();
        assert!(chat.left && chat.archived);
        assert!(chat.session_sid.is_none() && !chat.bootstrap_complete);
    }

    #[test]
    fn delivery_clears_pending_state() {
        let mut message = Message {
            pending: true,
            pending_id: Some("pending".into()),
            ..Default::default()
        };
        message.mark_delivered();
        assert!(!message.pending);
        assert!(message.pending_id.is_none());
    }
}
