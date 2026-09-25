use serde::{Deserialize, Serialize};

pub use ghost_protocol::GhostRoomInviteContext as RoomInviteMeta;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingRequest {
    pub request_id: String,
    pub peer_hydra_id: String,
    pub peer_address: String,
    pub local_address: String,
    pub signed_request_hex: String,
    #[serde(default)]
    pub room_invite: Option<RoomInviteMeta>,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub call_action: Option<String>,
    #[serde(default = "pending_state")]
    pub state: String,
}

fn pending_state() -> String {
    "pending".to_string()
}

impl Default for IncomingRequest {
    fn default() -> Self {
        Self {
            request_id: String::new(),
            peer_hydra_id: String::new(),
            peer_address: String::new(),
            local_address: String::new(),
            signed_request_hex: String::new(),
            room_invite: None,
            call_id: None,
            call_action: None,
            state: pending_state(),
        }
    }
}

impl IncomingRequest {
    pub fn set_state(&mut self, state: impl Into<String>) {
        self.state = state.into();
    }
}

#[cfg(test)]
mod tests {
    use super::IncomingRequest;

    #[test]
    fn default_request_is_pending_like_deserialized_missing_state() {
        assert_eq!(IncomingRequest::default().state, "pending");
    }
}
