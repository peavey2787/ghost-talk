use super::super::session_types::{Deserialize, Serialize};
#[derive(Clone, Debug)]
pub(crate) struct PreparedCompletion {
    pub pending_id: String,
    pub message_id: String,
    pub sid: String,
    pub contact_id: String,
    pub destination: String,
    pub payloads_hex: Vec<String>,
}

pub(crate) fn delivery_ack_matches_prepared_completion(
    prepared: Option<&PreparedCompletion>,
    ack: &ghost_protocol::GhostDeliveryAck,
) -> bool {
    prepared.is_some_and(|prepared| {
        prepared.contact_id == ack.signer_hydra_id && prepared.message_id == ack.message_id
    })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PreparedKktpDelivery {
    pub contact_id: String,
    #[serde(default = "default_delivery_kind")]
    pub kind: String,
    pub body: String,
    pub stego_profile: String,
    pub payloads_hex: Vec<String>,
}

fn default_delivery_kind() -> String {
    "msg".into()
}
