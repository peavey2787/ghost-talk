use super::{
    events::{
        Deserialize, GhostCallInviteContext, GhostContactDescriptor, GhostRoomInviteContext,
        Serialize, BASE64, GHOST_KKTP_VERSION, MAX_EVENT_BYTES,
    },
    kktp_codec::{validate_hex, validate_sid},
    kktp_types::{KktpFirstMessage, KktpHandshakeControl, KktpInnerMessage},
};
use base64::Engine as _;
pub(crate) fn validate_kktp_handshake(value: &KktpHandshakeControl) -> Result<(), String> {
    ensure(
        value.kind == "ghost_handshake" && value.version == GHOST_KKTP_VERSION,
        "unsupported Ghost Talk KKTP handshake record",
    )?;
    validate_sid(&value.sid)?;
    validate_hex(&value.initiator_hydra_id, 64, "KKTP initiator HYDRA id")?;
    validate_hex(&value.responder_hydra_id, 64, "KKTP responder HYDRA id")?;
    ensure(
        matches!(value.stage.as_str(), "pq_init" | "pq_resp" | "pq_finish"),
        "unknown Ghost Talk KKTP PQ handshake stage",
    )?;
    let payload = BASE64
        .decode(&value.payload_b64)
        .map_err(|_| "KKTP HYDRA handshake payload is not valid base64".to_string())?;
    let signature = BASE64
        .decode(&value.pq_sig_b64)
        .map_err(|_| "KKTP PQ context signature is not valid base64".to_string())?;
    ensure(
        !signature.is_empty() && signature.len() <= 8 * 1024,
        "KKTP PQ context signature size is invalid",
    )?;
    ensure(
        !payload.is_empty() && payload.len() <= MAX_EVENT_BYTES,
        "KKTP HYDRA handshake payload size is invalid",
    )?;
    if let Some(first) = &value.first_message {
        validate_hex(&first.mailbox_id, 64, "KKTP first-message mailbox id")?;
        validate_hex(&first.message_id, 32, "KKTP first-message id")?;
        validate_hex(
            &first.sender_hydra_id,
            64,
            "KKTP first-message sender HYDRA id",
        )?;
        BASE64
            .decode(&first.envelope_b64)
            .map_err(|_| "KKTP first-message HYDRA envelope is not valid base64".to_string())?;
        ensure(
            value.stage == "pq_finish",
            "KKTP first-message data is only valid on pq_finish",
        )?;
    }
    Ok(())
}

fn ensure(condition: bool, message: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}

pub(crate) fn validate_kktp_inner(value: &KktpInnerMessage) -> Result<(), String> {
    if value.version != GHOST_KKTP_VERSION
        || !matches!(value.kind.as_str(), "msg" | "reaction" | "session_end")
    {
        return Err("unsupported Ghost Talk KKTP inner message".into());
    }
    validate_sid(&value.sid)?;
    validate_hex(&value.mailbox_id, 64, "KKTP mailbox id")?;
    super::kktp_codec::validate_kktp_message_id(&value.message_id)?;
    validate_inner_content(value)?;
    validate_restart_seed(value)
}

fn validate_inner_content(value: &KktpInnerMessage) -> Result<(), String> {
    match value.kind.as_str() {
        "msg" => validate_text_content(value),
        "reaction" => validate_reaction_content(value),
        "session_end" => validate_session_end_content(value),
        _ => Err("unsupported Ghost Talk KKTP inner content".into()),
    }
}

fn validate_text_content(value: &KktpInnerMessage) -> Result<(), String> {
    ensure(
        !value.body.is_empty() && value.body.len() <= MAX_EVENT_BYTES,
        "KKTP message body is empty or too large",
    )?;
    ensure(
        value.reaction.is_none(),
        "KKTP text message cannot contain reaction metadata",
    )
}

fn validate_reaction_content(value: &KktpInnerMessage) -> Result<(), String> {
    ensure(value.body.is_empty(), "KKTP reaction body must be empty")?;
    let reaction = value
        .reaction
        .as_ref()
        .ok_or_else(|| "KKTP reaction metadata is missing".to_string())?;
    super::kktp_codec::validate_kktp_message_id(&reaction.target_message_id)
}

fn validate_session_end_content(value: &KktpInnerMessage) -> Result<(), String> {
    ensure(
        value.body.is_empty() && value.reaction.is_none(),
        "KKTP session_end content must be empty",
    )
}

fn validate_restart_seed(value: &KktpInnerMessage) -> Result<(), String> {
    let Some(seed_b64) = value.resume_seed_b64.as_deref() else {
        return Ok(());
    };
    if !matches!(value.kind.as_str(), "msg" | "reaction") || value.seq != 0 {
        return Err(
            "KKTP persistent-transport seed is only valid on the first session content event"
                .into(),
        );
    }
    let seed = BASE64
        .decode(seed_b64)
        .map_err(|_| "KKTP persistent-transport seed is not valid base64".to_string())?;
    ensure(
        seed.len() == 32,
        "KKTP persistent-transport seed must be exactly 32 bytes",
    )
}

#[derive(Serialize)]
pub(crate) struct KktpSessionEndPqSigning<'a> {
    #[serde(rename = "type")]
    pub(crate) kind: &'a str,
    pub(crate) version: u16,
    pub(crate) sid: &'a str,
    pub(crate) initiator_hydra_id: &'a str,
    pub(crate) responder_hydra_id: &'a str,
    pub(crate) sender_hydra_id: &'a str,
    pub(crate) sender_kaspa_address: &'a str,
    pub(crate) recipient_kaspa_address: &'a str,
    pub(crate) reason: &'a str,
}

#[derive(Serialize)]
pub(crate) struct KktpSessionEndKaspaSigning<'a> {
    #[serde(rename = "type")]
    pub(crate) kind: &'a str,
    pub(crate) version: u16,
    pub(crate) sid: &'a str,
    pub(crate) initiator_hydra_id: &'a str,
    pub(crate) responder_hydra_id: &'a str,
    pub(crate) sender_hydra_id: &'a str,
    pub(crate) sender_kaspa_address: &'a str,
    pub(crate) recipient_kaspa_address: &'a str,
    pub(crate) reason: &'a str,
    pub(crate) pq_sig_b64: &'a str,
}

#[derive(Serialize)]
pub(crate) struct KktpDiscoverySigning<'a> {
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    pub(crate) recipient_kaspa_address: &'a str,
    pub(crate) sender: &'a GhostContactDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) room_invite: &'a Option<GhostRoomInviteContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) call_invite: &'a Option<GhostCallInviteContext>,
    pub(crate) sid: &'a str,
    pub(crate) version: u16,
}

#[derive(Serialize)]
pub(crate) struct KktpDiscoveryWire<'a> {
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    pub(crate) recipient_kaspa_address: &'a str,
    pub(crate) sender: &'a GhostContactDescriptor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) room_invite: &'a Option<GhostRoomInviteContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) call_invite: &'a Option<GhostCallInviteContext>,
    pub(crate) sid: &'a str,
    pub(crate) sig: &'a str,
    pub(crate) version: u16,
}

#[derive(Deserialize)]
pub(crate) struct KktpDiscoveryOwned {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) recipient_kaspa_address: String,
    pub(crate) sender: GhostContactDescriptor,
    #[serde(default)]
    pub(crate) room_invite: Option<GhostRoomInviteContext>,
    #[serde(default)]
    pub(crate) call_invite: Option<GhostCallInviteContext>,
    pub(crate) sid: String,
    pub(crate) sig: String,
    pub(crate) version: u16,
}

#[derive(Serialize)]
pub(crate) struct KktpResponseSigning<'a> {
    pub(crate) acceptor_kaspa_address: &'a str,
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    pub(crate) recipient_kaspa_address: &'a str,
    pub(crate) responder: &'a GhostContactDescriptor,
    pub(crate) sid: &'a str,
    pub(crate) version: u16,
}

#[derive(Serialize)]
pub(crate) struct KktpResponseWire<'a> {
    pub(crate) acceptor_kaspa_address: &'a str,
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    pub(crate) recipient_kaspa_address: &'a str,
    pub(crate) responder: &'a GhostContactDescriptor,
    pub(crate) sid: &'a str,
    pub(crate) sig: &'a str,
    pub(crate) version: u16,
}

#[derive(Deserialize)]
pub(crate) struct KktpResponseOwned {
    pub(crate) acceptor_kaspa_address: String,
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) recipient_kaspa_address: String,
    pub(crate) responder: GhostContactDescriptor,
    pub(crate) sid: String,
    pub(crate) sig: String,
    pub(crate) version: u16,
}

#[derive(Serialize)]
pub(crate) struct KktpHandshakeSigning<'a> {
    #[serde(rename = "type")]
    pub(crate) kind: &'a str,
    pub(crate) version: u16,
    pub(crate) sid: &'a str,
    pub(crate) stage: &'a str,
    pub(crate) initiator_hydra_id: &'a str,
    pub(crate) responder_hydra_id: &'a str,
    pub(crate) payload_b64: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) first_message: &'a Option<KktpFirstMessage>,
}

#[derive(Serialize)]
pub(crate) struct KktpCallSignalSigning<'a> {
    #[serde(rename = "type")]
    pub(crate) kind: &'a str,
    pub(crate) version: u16,
    pub(crate) signal_id: &'a str,
    pub(crate) call_id: &'a str,
    pub(crate) action: &'a str,
    pub(crate) recipient_kaspa_address: &'a str,
    pub(crate) sender: &'a GhostContactDescriptor,
}
