use ghost_chat::RoomInviteMeta;
pub use ghost_hydra::ReceivedProjection;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydraReady {
    pub identity_id: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydraRealtimeEnvelope {
    pub carrier_b64: String,
    pub session_sid: String,
    pub message_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydraControlProjection {
    pub destination: String,
    #[serde(default)]
    pub payloads_hex: Vec<String>,
    pub completes_pending_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydraRecoveryProjection {
    pub destination: String,
    pub offer_hex: String,
    pub sid: String,
    pub peer_hydra_id: String,
}

pub type HydraRoomInviteProjection = RoomInviteMeta;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydraIncomingRequestProjection {
    pub request_id: String,
    pub peer_address: String,
    pub local_address: String,
    pub peer_label: String,
    pub peer_hydra_id: String,
    pub signed_request_hex: String,
    #[serde(default)]
    pub room_invite: Option<RoomInviteMeta>,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub call_action: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydraCallSignalProjection {
    pub signal_id: String,
    pub call_id: String,
    pub action: String,
    pub peer_address: String,
    #[serde(default)]
    pub local_address: String,
    pub peer_label: String,
    pub peer_hydra_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydraContactAcceptedProjection {
    pub request_id: String,
    pub peer_address: String,
    #[serde(default)]
    pub acceptor_address: String,
    pub peer_label: String,
    pub peer_hydra_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydraSessionEndedProjection {
    pub peer_hydra_id: String,
    #[serde(default)]
    pub peer_address: String,
    pub sid: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HydraMailboxResult {
    pub received: Option<ReceivedProjection>,
    pub control: Option<HydraControlProjection>,
    pub recovery: Option<HydraRecoveryProjection>,
    pub incoming_request: Option<HydraIncomingRequestProjection>,
    pub call_signal: Option<HydraCallSignalProjection>,
    pub contact_accepted: Option<HydraContactAcceptedProjection>,
    pub peer_address: Option<String>,
    #[serde(default)]
    pub peer_label: Option<String>,
    pub message_id: Option<String>,
    pub delivery_ack: Option<String>,
    pub delivery_ack_peer: Option<String>,
    pub session_established_peer: Option<String>,
    pub session_ended: Option<HydraSessionEndedProjection>,
    #[serde(default)]
    pub discard: bool,
}

impl HydraMailboxResult {
    pub fn incoming_request(&self) -> Option<&HydraIncomingRequestProjection> {
        self.incoming_request.as_ref()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydraSessionBindingProjection {
    pub sid: String,
    pub role: String,
    #[serde(default)]
    pub restart_resumable: bool,
}
