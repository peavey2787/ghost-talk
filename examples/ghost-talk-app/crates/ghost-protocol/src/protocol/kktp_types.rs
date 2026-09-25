use super::{
    canonical_json::canonical_json,
    events::{
        Deserialize, GhostContactDescriptor, Serialize, GHOST_KKTP_VERSION, GTACK_MAGIC,
        GTCD_MAGIC, KKTP_ANCHOR_PREFIX, MAX_GHOST_TX_PAYLOAD,
    },
    kktp_codec::{decode_kktp_anchor, encode_kktp_anchor, validate_sid},
    kktp_validation_types::{KktpResponseOwned, KktpResponseSigning, KktpResponseWire},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GhostContactAccept {
    pub version: u16,
    pub request_id: String,
    /// Original sender/stable return address that should receive this acceptance.
    pub recipient_kaspa_address: String,
    /// Exact wallet address that received the GTCR. The outer signature proves
    /// the responder controls the destination the sender actually contacted.
    pub acceptor_kaspa_address: String,
    pub responder: GhostContactDescriptor,
    pub signature_hex: String,
}

impl GhostContactAccept {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        validate_contact_accept(self)?;
        canonical_json(&KktpResponseSigning {
            acceptor_kaspa_address: &self.acceptor_kaspa_address,
            kind: "response",
            recipient_kaspa_address: &self.recipient_kaspa_address,
            responder: &self.responder,
            sid: &self.request_id,
            version: self.version,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        validate_contact_accept(self)?;
        let wire = KktpResponseWire {
            acceptor_kaspa_address: &self.acceptor_kaspa_address,
            kind: "response",
            recipient_kaspa_address: &self.recipient_kaspa_address,
            responder: &self.responder,
            sid: &self.request_id,
            sig: &self.signature_hex,
            version: self.version,
        };
        encode_kktp_anchor(&wire)
    }

    pub fn decode(v: &[u8]) -> Result<Self, String> {
        if v.len() > MAX_GHOST_TX_PAYLOAD {
            return Err("Ghost Talk contact acceptance exceeds the Kaspa payload limit".into());
        }
        if !v.starts_with(KKTP_ANCHOR_PREFIX) {
            return Err(
                "Ghost Talk contact acceptance must use the current KKTP response anchor".into(),
            );
        }
        let value = decode_kktp_contact_accept(v)?;
        validate_contact_accept(&value)?;
        if value.encode()?.as_slice() != v {
            return Err("Ghost Talk KKTP response anchor is not canonical JSON".into());
        }
        Ok(value)
    }
}

pub(crate) fn decode_kktp_contact_accept(v: &[u8]) -> Result<GhostContactAccept, String> {
    let wire: KktpResponseOwned = decode_kktp_anchor(v)?;
    if wire.kind != "response" || wire.version != GHOST_KKTP_VERSION {
        return Err("not a supported Ghost Talk KKTP response anchor".into());
    }
    Ok(GhostContactAccept {
        version: wire.version,
        request_id: wire.sid,
        recipient_kaspa_address: wire.recipient_kaspa_address,
        acceptor_kaspa_address: wire.acceptor_kaspa_address,
        responder: wire.responder,
        signature_hex: wire.sig,
    })
}

pub(crate) fn validate_contact_accept(value: &GhostContactAccept) -> Result<(), String> {
    if value.version != GHOST_KKTP_VERSION {
        return Err("unsupported Ghost Talk contact-accept version".into());
    }
    validate_sid(&value.request_id)?;
    require_nonempty(
        &value.recipient_kaspa_address,
        "Ghost Talk contact acceptance recipient is empty",
    )?;
    require_nonempty(
        &value.acceptor_kaspa_address,
        "Ghost Talk contact acceptance signer address is empty",
    )
}

pub(crate) fn require_nonempty(value: &str, message: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(message.into());
    }
    Ok(())
}

impl GhostContactDescriptor {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        let mut descriptor = self.clone();
        descriptor.signature_hex.clear();
        serde_json::to_vec(&descriptor).map_err(|e| e.to_string())
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let body = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let mut out = GTCD_MAGIC.to_vec();
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn decode(v: &[u8]) -> Result<Self, String> {
        if v.len() < 4 || v[..4] != GTCD_MAGIC {
            return Err("not GTCD".into());
        }
        serde_json::from_slice(&v[4..]).map_err(|e| e.to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GhostDeliveryAck {
    pub version: u16,
    pub signer_kaspa_address: String,
    pub signer_hydra_id: String,
    pub destination_hydra_id: String,
    pub message_id: String,
    pub signature_hex: String,
}

impl GhostDeliveryAck {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        let mut ack = self.clone();
        ack.signature_hex.clear();
        serde_json::to_vec(&ack).map_err(|e| e.to_string())
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let body = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let mut out = GTACK_MAGIC.to_vec();
        out.extend_from_slice(&body);
        if out.len() > MAX_GHOST_TX_PAYLOAD {
            return Err("delivery acknowledgement exceeds the Kaspa payload limit".into());
        }
        Ok(out)
    }

    pub fn decode(v: &[u8]) -> Result<Self, String> {
        if v.len() < 4 || v[..4] != GTACK_MAGIC {
            return Err("not a Ghost Talk delivery acknowledgement".into());
        }
        if v.len() > MAX_GHOST_TX_PAYLOAD {
            return Err("delivery acknowledgement exceeds the Kaspa payload limit".into());
        }
        serde_json::from_slice(&v[4..]).map_err(|e| e.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum KktpDirection {
    #[serde(rename = "AtoB")]
    AtoB,
    #[serde(rename = "BtoA")]
    BtoA,
}

impl KktpDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AtoB => "AtoB",
            Self::BtoA => "BtoA",
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Self::AtoB => Self::BtoA,
            Self::BtoA => Self::AtoB,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KktpHandshakeControl {
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u16,
    pub sid: String,
    pub stage: String,
    pub initiator_hydra_id: String,
    pub responder_hydra_id: String,
    pub payload_b64: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_message: Option<KktpFirstMessage>,
    pub pq_sig_b64: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KktpSessionEnd {
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u16,
    pub sid: String,
    pub initiator_hydra_id: String,
    pub responder_hydra_id: String,
    pub sender_hydra_id: String,
    pub sender_kaspa_address: String,
    pub recipient_kaspa_address: String,
    pub reason: String,
    pub pq_sig_b64: String,
    pub sig: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KktpFirstMessage {
    pub direction: KktpDirection,
    pub mailbox_id: String,
    pub message_id: String,
    pub profile: String,
    pub seq: u64,
    pub sender_hydra_id: String,
    pub envelope_b64: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KktpMailboxMessage {
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u16,
    pub sid: String,
    pub mailbox_id: String,
    pub direction: KktpDirection,
    pub seq: u64,
    pub sender_hydra_id: String,
    pub message_id: String,
    pub profile: String,
    pub ciphertext_b64: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostReactionEvent {
    pub target_message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<ghost_domain::reaction::ReactionKind>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KktpInnerMessage {
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u16,
    pub sid: String,
    pub mailbox_id: String,
    pub direction: KktpDirection,
    pub seq: u64,
    pub message_id: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<GhostReactionEvent>,
    /// Optional Ghost Talk persistent-transport seed carried only inside the
    /// first HYDRA-protected KKTP message of a new session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_seed_b64: Option<String>,
}
