#![forbid(unsafe_code)]

use ghost_core::{
    ConversationId, EventId, Id128, PacketId, RoomId, MAX_EVENT_BYTES, MAX_FRAGMENTS,
    MAX_GHOST_TX_PAYLOAD, MAX_KSPT_V1_PAYLOAD_BYTES,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};

pub const EVENT_VERSION: u16 = 1;
pub const GHST_MAGIC: [u8; 4] = *b"GHST";
pub const GHST_VERSION: u8 = 1;
pub const GHST_HEADER: usize = 30;
/// Maximum logical bytes carried by one KSPT-v1 transaction after the
/// Ghost Talk frame header. Ordinary HYDRA carriers now fit in one transaction.
pub const GHST_DATA_MAX: usize = MAX_KSPT_V1_PAYLOAD_BYTES - GHST_HEADER;
pub const GTCD_MAGIC: [u8; 4] = *b"GTCD";
pub const GTACK_MAGIC: [u8; 4] = *b"GTAK";
pub const GTCR_MAGIC: [u8; 4] = *b"GTCR";
pub const GTCA_MAGIC: [u8; 4] = *b"GTCA";
pub const GTVA_MAGIC: [u8; 4] = *b"GTVA";
pub const GHOST_KKTP_VERSION: u16 = 2;
pub const KKTP_ANCHOR_PREFIX: &[u8] = b"KKTP:ANCHOR:";
pub const KKTP_MESSAGE_PREFIX: &[u8] = b"KKTP:";


#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChatEvent {
    pub version: u16,
    pub id: EventId,
    pub conversation: ConversationId,
    pub kind: EventKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum EventKind {
    Text {
        body: String,
        reply_to: Option<EventId>,
    },
    Reaction {
        target: EventId,
        emoji: String,
    },
    Receipt {
        target: EventId,
        read: bool,
    },
    FriendInvite {
        close_friend: bool,
    },
    FriendAccept {
        close_friend: bool,
    },
    FriendDecline,
    RoomInvite {
        room: RoomId,
        label: String,
    },
    RoomRole {
        room: RoomId,
        role: String,
    },
    VoiceSignal {
        room: Option<RoomId>,
        kind: VoiceSignalKind,
        data: String,
    },
    Game {
        game: String,
        action: String,
        data: String,
    },
    ProfileUpdate {
        display_name: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum VoiceSignalKind {
    Offer,
    Answer,
    Ice,
    Capabilities,
    RouteSelected,
    RequestToSpeak,
    Stop,
}

impl ChatEvent {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let v = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if v.len() > MAX_EVENT_BYTES {
            return Err("event too large".into());
        }
        Ok(v)
    }

    pub fn decode(v: &[u8]) -> Result<Self, String> {
        if v.len() > MAX_EVENT_BYTES {
            return Err("event too large".into());
        }
        let event: Self = serde_json::from_slice(v).map_err(|e| e.to_string())?;
        if event.version != EVENT_VERSION {
            return Err("unsupported event version".into());
        }
        Ok(event)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CarrierFrame {
    pub packet: PacketId,
    pub index: u16,
    pub count: u16,
    pub payload: Vec<u8>,
}

impl CarrierFrame {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.count == 0 || self.index >= self.count || self.count as usize > MAX_FRAGMENTS {
            return Err("invalid fragment metadata".into());
        }
        if self.payload.len() > GHST_DATA_MAX {
            return Err("fragment payload too large".into());
        }
        let mut out = Vec::with_capacity(GHST_HEADER + self.payload.len());
        out.extend_from_slice(&GHST_MAGIC);
        out.push(GHST_VERSION);
        out.push(0);
        out.extend_from_slice(&self.packet.0);
        out.extend_from_slice(&self.index.to_le_bytes());
        out.extend_from_slice(&self.count.to_le_bytes());
        out.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    pub fn decode(v: &[u8]) -> Result<Self, String> {
        if v.len() < GHST_HEADER || v[..4] != GHST_MAGIC {
            return Err("not GHST".into());
        }
        if v[4] != GHST_VERSION {
            return Err("unsupported GHST version".into());
        }
        let packet = Id128(v[6..22].try_into().unwrap());
        let index = u16::from_le_bytes(v[22..24].try_into().unwrap());
        let count = u16::from_le_bytes(v[24..26].try_into().unwrap());
        let len = u32::from_le_bytes(v[26..30].try_into().unwrap()) as usize;
        if count == 0
            || index >= count
            || count as usize > MAX_FRAGMENTS
            || len > GHST_DATA_MAX
            || v.len() != GHST_HEADER + len
        {
            return Err("invalid GHST bounds".into());
        }
        Ok(Self {
            packet,
            index,
            count,
            payload: v[30..].to_vec(),
        })
    }
}

pub fn fragment(packet: PacketId, data: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let count = data.len().max(1).div_ceil(GHST_DATA_MAX);
    if count > MAX_FRAGMENTS {
        return Err("message requires too many Kaspa transactions".into());
    }
    let mut out = Vec::with_capacity(count);
    if data.is_empty() {
        out.push(
            CarrierFrame {
                packet,
                index: 0,
                count: 1,
                payload: vec![],
            }
            .encode()?,
        );
        return Ok(out);
    }
    for (i, chunk) in data.chunks(GHST_DATA_MAX).enumerate() {
        out.push(
            CarrierFrame {
                packet,
                index: i as u16,
                count: count as u16,
                payload: chunk.to_vec(),
            }
            .encode()?,
        );
    }
    Ok(out)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GhostContactDescriptor {
    pub version: u16,
    pub kaspa_address: String,
    pub hydra_contact_card_b64: String,
    pub display_name: String,
    #[serde(default)]
    pub hydra_identity_id: String,
    #[serde(default)]
    pub discoverable: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub interests: Vec<String>,
    pub capabilities: Vec<String>,
    pub expires_daa: Option<u64>,
    pub signature_hex: String,
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GhostContactRequest {
    pub version: u16,
    pub request_id: String,
    pub recipient_kaspa_address: String,
    pub sender: GhostContactDescriptor,
    pub signature_hex: String,
}

impl GhostContactRequest {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        if self.version == GHOST_KKTP_VERSION {
            return canonical_json(&KktpDiscoverySigning {
                kind: "discovery",
                recipient_kaspa_address: &self.recipient_kaspa_address,
                sender: &self.sender,
                sid: &self.request_id,
                version: self.version,
            });
        }
        let mut request = self.clone();
        request.signature_hex.clear();
        serde_json::to_vec(&request).map_err(|e| e.to_string())
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        validate_sid(&self.request_id)?;
        if self.recipient_kaspa_address.trim().is_empty() {
            return Err("Ghost Talk contact request recipient is empty".into());
        }
        if self.version == GHOST_KKTP_VERSION {
            let wire = KktpDiscoveryWire {
                kind: "discovery",
                recipient_kaspa_address: &self.recipient_kaspa_address,
                sender: &self.sender,
                sid: &self.request_id,
                sig: &self.signature_hex,
                version: self.version,
            };
            return encode_kktp_anchor(&wire);
        }
        if self.version != 1 {
            return Err("unsupported Ghost Talk contact-request version".into());
        }
        let body = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let mut out = GTCR_MAGIC.to_vec();
        out.extend_from_slice(&body);
        if out.len() > MAX_GHOST_TX_PAYLOAD {
            return Err("Ghost Talk contact request exceeds the Kaspa payload limit".into());
        }
        Ok(out)
    }

    pub fn decode(v: &[u8]) -> Result<Self, String> {
        if v.len() > MAX_GHOST_TX_PAYLOAD {
            return Err("Ghost Talk contact request exceeds the Kaspa payload limit".into());
        }
        let value = if v.starts_with(KKTP_ANCHOR_PREFIX) {
            let wire: KktpDiscoveryOwned = decode_kktp_anchor(v)?;
            if wire.kind != "discovery" || wire.version != GHOST_KKTP_VERSION {
                return Err("not a supported Ghost Talk KKTP discovery anchor".into());
            }
            Self {
                version: wire.version,
                request_id: wire.sid,
                recipient_kaspa_address: wire.recipient_kaspa_address,
                sender: wire.sender,
                signature_hex: wire.sig,
            }
        } else {
            if v.len() < 4 || v[..4] != GTCR_MAGIC {
                return Err("not a Ghost Talk contact request".into());
            }
            serde_json::from_slice(&v[4..]).map_err(|e| e.to_string())?
        };
        if value.version != 1 && value.version != GHOST_KKTP_VERSION {
            return Err("unsupported Ghost Talk contact-request version".into());
        }
        validate_sid(&value.request_id)?;
        if value.recipient_kaspa_address.trim().is_empty() {
            return Err("Ghost Talk contact request recipient is empty".into());
        }
        if v.starts_with(KKTP_ANCHOR_PREFIX) && value.encode()?.as_slice() != v {
            return Err("Ghost Talk KKTP discovery anchor is not canonical JSON".into());
        }
        Ok(value)
    }
}

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
        if self.version == GHOST_KKTP_VERSION {
            return canonical_json(&KktpResponseSigning {
                acceptor_kaspa_address: &self.acceptor_kaspa_address,
                kind: "response",
                recipient_kaspa_address: &self.recipient_kaspa_address,
                responder: &self.responder,
                sid: &self.request_id,
                version: self.version,
            });
        }
        let mut accepted = self.clone();
        accepted.signature_hex.clear();
        serde_json::to_vec(&accepted).map_err(|e| e.to_string())
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        validate_sid(&self.request_id)?;
        if self.recipient_kaspa_address.trim().is_empty() {
            return Err("Ghost Talk contact acceptance recipient is empty".into());
        }
        if self.acceptor_kaspa_address.trim().is_empty() {
            return Err("Ghost Talk contact acceptance signer address is empty".into());
        }
        if self.version == GHOST_KKTP_VERSION {
            let wire = KktpResponseWire {
                acceptor_kaspa_address: &self.acceptor_kaspa_address,
                kind: "response",
                recipient_kaspa_address: &self.recipient_kaspa_address,
                responder: &self.responder,
                sid: &self.request_id,
                sig: &self.signature_hex,
                version: self.version,
            };
            return encode_kktp_anchor(&wire);
        }
        if self.version != 1 {
            return Err("unsupported Ghost Talk contact-accept version".into());
        }
        let body = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let mut out = GTCA_MAGIC.to_vec();
        out.extend_from_slice(&body);
        if out.len() > MAX_GHOST_TX_PAYLOAD {
            return Err("Ghost Talk contact acceptance exceeds the Kaspa payload limit".into());
        }
        Ok(out)
    }

    pub fn decode(v: &[u8]) -> Result<Self, String> {
        if v.len() > MAX_GHOST_TX_PAYLOAD {
            return Err("Ghost Talk contact acceptance exceeds the Kaspa payload limit".into());
        }
        let value = if v.starts_with(KKTP_ANCHOR_PREFIX) {
            let wire: KktpResponseOwned = decode_kktp_anchor(v)?;
            if wire.kind != "response" || wire.version != GHOST_KKTP_VERSION {
                return Err("not a supported Ghost Talk KKTP response anchor".into());
            }
            Self {
                version: wire.version,
                request_id: wire.sid,
                recipient_kaspa_address: wire.recipient_kaspa_address,
                acceptor_kaspa_address: wire.acceptor_kaspa_address,
                responder: wire.responder,
                signature_hex: wire.sig,
            }
        } else {
            if v.len() < 4 || v[..4] != GTCA_MAGIC {
                return Err("not a Ghost Talk contact acceptance".into());
            }
            serde_json::from_slice(&v[4..]).map_err(|e| e.to_string())?
        };
        if value.version != 1 && value.version != GHOST_KKTP_VERSION {
            return Err("unsupported Ghost Talk contact-accept version".into());
        }
        validate_sid(&value.request_id)?;
        if value.recipient_kaspa_address.trim().is_empty() {
            return Err("Ghost Talk contact acceptance recipient is empty".into());
        }
        if value.acceptor_kaspa_address.trim().is_empty() {
            return Err("Ghost Talk contact acceptance signer address is empty".into());
        }
        if v.starts_with(KKTP_ANCHOR_PREFIX) && value.encode()?.as_slice() != v {
            return Err("Ghost Talk KKTP response anchor is not canonical JSON".into());
        }
        Ok(value)
    }
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
}

impl KktpInnerMessage {
    pub fn text(
        sid: String,
        mailbox_id: String,
        direction: KktpDirection,
        seq: u64,
        message_id: String,
        body: String,
    ) -> Self {
        Self {
            kind: "msg".into(),
            version: GHOST_KKTP_VERSION,
            sid,
            mailbox_id,
            direction,
            seq,
            message_id,
            body,
        }
    }

    pub fn session_end(
        sid: String,
        mailbox_id: String,
        direction: KktpDirection,
        seq: u64,
        message_id: String,
    ) -> Self {
        Self {
            kind: "session_end".into(),
            version: GHOST_KKTP_VERSION,
            sid,
            mailbox_id,
            direction,
            seq,
            message_id,
            body: String::new(),
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        validate_kktp_inner(self)?;
        canonical_json(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_EVENT_BYTES {
            return Err("KKTP inner message exceeds the Ghost Talk event limit".into());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        validate_kktp_inner(&value)?;
        Ok(value)
    }
}

impl KktpSessionEnd {
    pub fn pq_signing_bytes(&self) -> Result<Vec<u8>, String> {
        validate_kktp_session_end_core(self)?;
        canonical_json(&KktpSessionEndPqSigning {
            kind: &self.kind,
            version: self.version,
            sid: &self.sid,
            initiator_hydra_id: &self.initiator_hydra_id,
            responder_hydra_id: &self.responder_hydra_id,
            sender_hydra_id: &self.sender_hydra_id,
            sender_kaspa_address: &self.sender_kaspa_address,
            recipient_kaspa_address: &self.recipient_kaspa_address,
            reason: &self.reason,
        })
    }

    pub fn kaspa_signing_bytes(&self) -> Result<Vec<u8>, String> {
        validate_kktp_session_end_core(self)?;
        let pq = BASE64
            .decode(&self.pq_sig_b64)
            .map_err(|_| "KKTP session_end PQ signature is not valid base64".to_string())?;
        if pq.is_empty() || pq.len() > 8 * 1024 {
            return Err("KKTP session_end PQ signature size is invalid".into());
        }
        canonical_json(&KktpSessionEndKaspaSigning {
            kind: &self.kind,
            version: self.version,
            sid: &self.sid,
            initiator_hydra_id: &self.initiator_hydra_id,
            responder_hydra_id: &self.responder_hydra_id,
            sender_hydra_id: &self.sender_hydra_id,
            sender_kaspa_address: &self.sender_kaspa_address,
            recipient_kaspa_address: &self.recipient_kaspa_address,
            reason: &self.reason,
            pq_sig_b64: &self.pq_sig_b64,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        validate_kktp_session_end(self)?;
        encode_kktp_anchor(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let value: Self = decode_kktp_anchor(bytes)?;
        validate_kktp_session_end(&value)?;
        if value.encode()?.as_slice() != bytes {
            return Err("Ghost Talk KKTP session_end anchor is not canonical JSON".into());
        }
        Ok(value)
    }
}

impl KktpHandshakeControl {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        canonical_json(&KktpHandshakeSigning {
            kind: &self.kind,
            version: self.version,
            sid: &self.sid,
            stage: &self.stage,
            initiator_hydra_id: &self.initiator_hydra_id,
            responder_hydra_id: &self.responder_hydra_id,
            payload_b64: &self.payload_b64,
            first_message: &self.first_message,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        validate_kktp_handshake(self)?;
        encode_kktp_anchor(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let value: Self = decode_kktp_anchor(bytes)?;
        validate_kktp_handshake(&value)?;
        if value.encode()?.as_slice() != bytes {
            return Err("Ghost Talk KKTP handshake anchor is not canonical JSON".into());
        }
        Ok(value)
    }
}

impl KktpMailboxMessage {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        validate_sid(&self.sid)?;
        validate_hex(&self.mailbox_id, 64, "KKTP mailbox id")?;
        validate_hex(&self.sender_hydra_id, 64, "KKTP sender HYDRA id")?;
        validate_hex(&self.message_id, 32, "KKTP message id")?;
        if self.kind != "msg" || self.version != GHOST_KKTP_VERSION {
            return Err("unsupported Ghost Talk KKTP message".into());
        }
        let body = canonical_json(self)?;
        let mut out = Vec::with_capacity(KKTP_MESSAGE_PREFIX.len() + self.mailbox_id.len() + 1 + body.len());
        out.extend_from_slice(KKTP_MESSAGE_PREFIX);
        out.extend_from_slice(self.mailbox_id.as_bytes());
        out.push(b':');
        out.extend_from_slice(&body);
        if out.len() > MAX_EVENT_BYTES {
            return Err("KKTP mailbox message exceeds the Ghost Talk carrier limit".into());
        }
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if !bytes.starts_with(KKTP_MESSAGE_PREFIX) || bytes.starts_with(KKTP_ANCHOR_PREFIX) {
            return Err("not a Ghost Talk KKTP mailbox message".into());
        }
        let rest = &bytes[KKTP_MESSAGE_PREFIX.len()..];
        let Some(colon) = rest.iter().position(|byte| *byte == b':') else {
            return Err("KKTP mailbox prefix is missing its separator".into());
        };
        let prefix_mailbox = std::str::from_utf8(&rest[..colon]).map_err(|_| "KKTP mailbox id is not UTF-8".to_string())?;
        let value: Self = serde_json::from_slice(&rest[colon + 1..]).map_err(|e| e.to_string())?;
        validate_sid(&value.sid)?;
        validate_hex(&value.mailbox_id, 64, "KKTP mailbox id")?;
        validate_hex(&value.sender_hydra_id, 64, "KKTP sender HYDRA id")?;
        validate_hex(&value.message_id, 32, "KKTP message id")?;
        if value.kind != "msg" || value.version != GHOST_KKTP_VERSION || prefix_mailbox != value.mailbox_id {
            return Err("KKTP mailbox message header does not match its body".into());
        }
        BASE64.decode(&value.ciphertext_b64).map_err(|_| "KKTP HYDRA ciphertext is not valid base64".to_string())?;
        if value.encode()?.as_slice() != bytes {
            return Err("Ghost Talk KKTP mailbox message is not canonical JSON".into());
        }
        Ok(value)
    }
}

pub fn kktp_mailbox_id(sid: &str, initiator_hydra_id: &str, responder_hydra_id: &str) -> Result<String, String> {
    validate_sid(sid)?;
    validate_hex(initiator_hydra_id, 64, "KKTP initiator HYDRA id")?;
    validate_hex(responder_hydra_id, 64, "KKTP responder HYDRA id")?;
    let mut hasher = Sha3_256::new();
    hasher.update(b"GhostTalk/KKTP/v2/mailbox\0");
    hasher.update(hex::decode(initiator_hydra_id).map_err(|_| "invalid initiator HYDRA id".to_string())?);
    hasher.update(hex::decode(responder_hydra_id).map_err(|_| "invalid responder HYDRA id".to_string())?);
    hasher.update(hex::decode(sid).map_err(|_| "invalid KKTP sid".to_string())?);
    Ok(hex::encode(hasher.finalize()))
}

pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    let mut out = String::new();
    write_canonical_json(&value, &mut out)?;
    Ok(out.into_bytes())
}

fn write_canonical_json(value: &serde_json::Value, out: &mut String) -> Result<(), String> {
    match value {
        serde_json::Value::Null => out.push_str("null"),
        serde_json::Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        serde_json::Value::Number(value) => out.push_str(&value.to_string()),
        serde_json::Value::String(value) => out.push_str(&serde_json::to_string(value).map_err(|e| e.to_string())?),
        serde_json::Value::Array(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 { out.push(','); }
                write_canonical_json(value, out)?;
            }
            out.push(']');
        }
        serde_json::Value::Object(values) => {
            out.push('{');
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort_unstable();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 { out.push(','); }
                out.push_str(&serde_json::to_string(key).map_err(|e| e.to_string())?);
                out.push(':');
                write_canonical_json(&values[key], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn encode_kktp_anchor<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let body = canonical_json(value)?;
    let mut out = Vec::with_capacity(KKTP_ANCHOR_PREFIX.len() + body.len());
    out.extend_from_slice(KKTP_ANCHOR_PREFIX);
    out.extend_from_slice(&body);
    if out.len() > MAX_EVENT_BYTES {
        return Err("Ghost Talk KKTP anchor exceeds the carrier limit".into());
    }
    Ok(out)
}


pub fn kktp_anchor_type(bytes: &[u8]) -> Result<Option<String>, String> {
    if !bytes.starts_with(KKTP_ANCHOR_PREFIX) {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes[KKTP_ANCHOR_PREFIX.len()..])
        .map_err(|e| e.to_string())?;
    Ok(value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned))
}

fn decode_kktp_anchor<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    if !bytes.starts_with(KKTP_ANCHOR_PREFIX) {
        return Err("not a Ghost Talk KKTP anchor".into());
    }
    serde_json::from_slice(&bytes[KKTP_ANCHOR_PREFIX.len()..]).map_err(|e| e.to_string())
}

fn validate_sid(value: &str) -> Result<(), String> {
    validate_hex(value, 32, "Ghost Talk KKTP sid")
}

fn validate_hex(value: &str, chars: usize, label: &str) -> Result<(), String> {
    if value.len() != chars || !value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()) {
        return Err(format!("{label} must be exactly {chars} lowercase hexadecimal characters"));
    }
    Ok(())
}

fn validate_kktp_session_end_core(value: &KktpSessionEnd) -> Result<(), String> {
    if value.kind != "session_end" || value.version != GHOST_KKTP_VERSION {
        return Err("unsupported Ghost Talk KKTP session_end record".into());
    }
    validate_sid(&value.sid)?;
    validate_hex(&value.initiator_hydra_id, 64, "KKTP initiator HYDRA id")?;
    validate_hex(&value.responder_hydra_id, 64, "KKTP responder HYDRA id")?;
    validate_hex(&value.sender_hydra_id, 64, "KKTP session_end sender HYDRA id")?;
    if value.initiator_hydra_id == value.responder_hydra_id {
        return Err("KKTP session_end participants must be distinct".into());
    }
    if value.sender_hydra_id != value.initiator_hydra_id
        && value.sender_hydra_id != value.responder_hydra_id
    {
        return Err("KKTP session_end sender is not a session participant".into());
    }
    if value.sender_kaspa_address.trim().is_empty() || value.recipient_kaspa_address.trim().is_empty() {
        return Err("KKTP session_end routing address is empty".into());
    }
    if value.reason.is_empty() || value.reason.len() > 64 || value.reason.chars().any(char::is_control) {
        return Err("KKTP session_end reason is invalid".into());
    }
    Ok(())
}

fn validate_kktp_session_end(value: &KktpSessionEnd) -> Result<(), String> {
    validate_kktp_session_end_core(value)?;
    let pq = BASE64
        .decode(&value.pq_sig_b64)
        .map_err(|_| "KKTP session_end PQ signature is not valid base64".to_string())?;
    if pq.is_empty() || pq.len() > 8 * 1024 {
        return Err("KKTP session_end PQ signature size is invalid".into());
    }
    validate_hex(&value.sig, 128, "KKTP session_end Kaspa signature")
}

fn validate_kktp_handshake(value: &KktpHandshakeControl) -> Result<(), String> {
    if value.kind != "ghost_handshake" || value.version != GHOST_KKTP_VERSION {
        return Err("unsupported Ghost Talk KKTP handshake record".into());
    }
    validate_sid(&value.sid)?;
    validate_hex(&value.initiator_hydra_id, 64, "KKTP initiator HYDRA id")?;
    validate_hex(&value.responder_hydra_id, 64, "KKTP responder HYDRA id")?;
    if !matches!(value.stage.as_str(), "pq_init" | "pq_resp" | "pq_finish") {
        return Err("unknown Ghost Talk KKTP PQ handshake stage".into());
    }
    let payload = BASE64.decode(&value.payload_b64).map_err(|_| "KKTP HYDRA handshake payload is not valid base64".to_string())?;
    let signature = BASE64.decode(&value.pq_sig_b64)
        .map_err(|_| "KKTP PQ context signature is not valid base64".to_string())?;
    if signature.is_empty() || signature.len() > 8 * 1024 {
        return Err("KKTP PQ context signature size is invalid".into());
    }
    if payload.is_empty() || payload.len() > MAX_EVENT_BYTES {
        return Err("KKTP HYDRA handshake payload size is invalid".into());
    }
    if let Some(first) = &value.first_message {
        validate_hex(&first.mailbox_id, 64, "KKTP first-message mailbox id")?;
        validate_hex(&first.message_id, 32, "KKTP first-message id")?;
        validate_hex(&first.sender_hydra_id, 64, "KKTP first-message sender HYDRA id")?;
        BASE64.decode(&first.envelope_b64)
            .map_err(|_| "KKTP first-message HYDRA envelope is not valid base64".to_string())?;
        if value.stage != "pq_finish" {
            return Err("KKTP first-message data is only valid on pq_finish".into());
        }
    }
    Ok(())
}

fn validate_kktp_inner(value: &KktpInnerMessage) -> Result<(), String> {
    if value.version != GHOST_KKTP_VERSION || !matches!(value.kind.as_str(), "msg" | "session_end") {
        return Err("unsupported Ghost Talk KKTP inner message".into());
    }
    validate_sid(&value.sid)?;
    validate_hex(&value.mailbox_id, 64, "KKTP mailbox id")?;
    validate_hex(&value.message_id, 32, "KKTP message id")?;
    if value.kind == "msg" && (value.body.is_empty() || value.body.len() > MAX_EVENT_BYTES) {
        return Err("KKTP message body is empty or too large".into());
    }
    if value.kind == "session_end" && !value.body.is_empty() {
        return Err("KKTP session_end body must be empty".into());
    }
    Ok(())
}

#[derive(Serialize)]
struct KktpSessionEndPqSigning<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    version: u16,
    sid: &'a str,
    initiator_hydra_id: &'a str,
    responder_hydra_id: &'a str,
    sender_hydra_id: &'a str,
    sender_kaspa_address: &'a str,
    recipient_kaspa_address: &'a str,
    reason: &'a str,
}

#[derive(Serialize)]
struct KktpSessionEndKaspaSigning<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    version: u16,
    sid: &'a str,
    initiator_hydra_id: &'a str,
    responder_hydra_id: &'a str,
    sender_hydra_id: &'a str,
    sender_kaspa_address: &'a str,
    recipient_kaspa_address: &'a str,
    reason: &'a str,
    pq_sig_b64: &'a str,
}

#[derive(Serialize)]
struct KktpDiscoverySigning<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    recipient_kaspa_address: &'a str,
    sender: &'a GhostContactDescriptor,
    sid: &'a str,
    version: u16,
}

#[derive(Serialize)]
struct KktpDiscoveryWire<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    recipient_kaspa_address: &'a str,
    sender: &'a GhostContactDescriptor,
    sid: &'a str,
    sig: &'a str,
    version: u16,
}

#[derive(Deserialize)]
struct KktpDiscoveryOwned {
    #[serde(rename = "type")]
    kind: String,
    recipient_kaspa_address: String,
    sender: GhostContactDescriptor,
    sid: String,
    sig: String,
    version: u16,
}

#[derive(Serialize)]
struct KktpResponseSigning<'a> {
    acceptor_kaspa_address: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    recipient_kaspa_address: &'a str,
    responder: &'a GhostContactDescriptor,
    sid: &'a str,
    version: u16,
}

#[derive(Serialize)]
struct KktpResponseWire<'a> {
    acceptor_kaspa_address: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    recipient_kaspa_address: &'a str,
    responder: &'a GhostContactDescriptor,
    sid: &'a str,
    sig: &'a str,
    version: u16,
}

#[derive(Deserialize)]
struct KktpResponseOwned {
    acceptor_kaspa_address: String,
    #[serde(rename = "type")]
    kind: String,
    recipient_kaspa_address: String,
    responder: GhostContactDescriptor,
    sid: String,
    sig: String,
    version: u16,
}

#[derive(Serialize)]
struct KktpHandshakeSigning<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    version: u16,
    sid: &'a str,
    stage: &'a str,
    initiator_hydra_id: &'a str,
    responder_hydra_id: &'a str,
    payload_b64: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_message: &'a Option<KktpFirstMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghost_core::Id128;

    #[test]
    fn fragments_roundtrip_across_kspt_v1_boundary() {
        let data = vec![0x5a; GHST_DATA_MAX + 17];
        let packet = Id128([7; 16]);
        let frames = fragment(packet, &data).unwrap();
        assert_eq!(frames.len(), 2);
        assert!(frames
            .iter()
            .all(|raw| raw.len() <= MAX_KSPT_V1_PAYLOAD_BYTES));
        let mut rebuilt = Vec::new();
        for raw in frames {
            rebuilt.extend(CarrierFrame::decode(&raw).unwrap().payload);
        }
        assert_eq!(rebuilt, data);
    }

    #[test]
    fn eighty_kib_logical_carrier_only_fragments_at_real_kspt_v1_boundary() {
        let data = vec![0xa5; MAX_GHOST_TX_PAYLOAD];
        let packet = Id128([9; 16]);
        let frames = fragment(packet, &data).unwrap();
        assert!(frames.len() > 1);
        assert!(frames.len() <= MAX_FRAGMENTS);
        assert!(frames
            .iter()
            .all(|raw| raw.len() <= MAX_KSPT_V1_PAYLOAD_BYTES));
        let rebuilt = frames
            .iter()
            .map(|raw| CarrierFrame::decode(raw).unwrap().payload)
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(rebuilt, data);
    }

    #[test]
    fn rejects_bad_fragment_count() {
        let frame = CarrierFrame {
            packet: Id128([1; 16]),
            index: 1,
            count: 1,
            payload: vec![],
        };
        assert!(frame.encode().is_err());
    }

    fn test_descriptor(address: &str, hydra_id: &str) -> GhostContactDescriptor {
        GhostContactDescriptor {
            version: 1,
            kaspa_address: address.to_owned(),
            hydra_contact_card_b64: "AQIDBA==".to_owned(),
            display_name: "Alice".to_owned(),
            hydra_identity_id: hydra_id.to_owned(),
            discoverable: false,
            username: String::new(),
            description: String::new(),
            interests: Vec::new(),
            capabilities: vec!["hydra-v1".to_owned()],
            expires_daa: None,
            signature_hex: "00".repeat(64),
        }
    }

    #[test]
    fn private_contact_request_roundtrips_and_binds_destination_field() {
        let request = GhostContactRequest {
            version: 1,
            request_id: "11".repeat(16),
            recipient_kaspa_address: "kaspatest:recipient".to_owned(),
            sender: test_descriptor("kaspatest:sender", &"22".repeat(32)),
            signature_hex: "33".repeat(64),
        };
        let encoded = request.encode().expect("encode GTCR");
        assert!(encoded.starts_with(&GTCR_MAGIC));
        let decoded = GhostContactRequest::decode(&encoded).expect("decode GTCR");
        assert_eq!(decoded.request_id, request.request_id);
        assert_eq!(decoded.recipient_kaspa_address, request.recipient_kaspa_address);
        assert_eq!(decoded.sender.kaspa_address, request.sender.kaspa_address);
        assert_eq!(decoded.sender.hydra_identity_id, request.sender.hydra_identity_id);
    }

    #[test]
    fn private_contact_accept_roundtrips_with_exact_acceptor_address() {
        let accepted = GhostContactAccept {
            version: 1,
            request_id: "44".repeat(16),
            recipient_kaspa_address: "kaspatest:sender".to_owned(),
            acceptor_kaspa_address: "kaspatest:exact-receive-address".to_owned(),
            responder: test_descriptor("kaspatest:stable-responder", &"55".repeat(32)),
            signature_hex: "66".repeat(64),
        };
        let encoded = accepted.encode().expect("encode GTCA");
        assert!(encoded.starts_with(&GTCA_MAGIC));
        let decoded = GhostContactAccept::decode(&encoded).expect("decode GTCA");
        assert_eq!(decoded.request_id, accepted.request_id);
        assert_eq!(decoded.recipient_kaspa_address, accepted.recipient_kaspa_address);
        assert_eq!(decoded.acceptor_kaspa_address, accepted.acceptor_kaspa_address);
        assert_eq!(decoded.responder.kaspa_address, accepted.responder.kaspa_address);
    }

    #[test]
    fn kktp_v2_discovery_and_response_are_canonical_and_share_sid() {
        let sid = "11".repeat(16);
        let request = GhostContactRequest {
            version: GHOST_KKTP_VERSION,
            request_id: sid.clone(),
            recipient_kaspa_address: "kaspatest:recipient".to_owned(),
            sender: test_descriptor("kaspatest:sender", &"22".repeat(32)),
            signature_hex: "33".repeat(64),
        };
        let encoded = request.encode().expect("encode KKTP discovery");
        assert!(encoded.starts_with(KKTP_ANCHOR_PREFIX));
        assert_eq!(kktp_anchor_type(&encoded).unwrap().as_deref(), Some("discovery"));
        let decoded = GhostContactRequest::decode(&encoded).expect("decode KKTP discovery");
        assert_eq!(decoded.request_id, sid);
        assert_eq!(decoded.encode().unwrap(), encoded);

        let accepted = GhostContactAccept {
            version: GHOST_KKTP_VERSION,
            request_id: decoded.request_id.clone(),
            recipient_kaspa_address: decoded.sender.kaspa_address.clone(),
            acceptor_kaspa_address: decoded.recipient_kaspa_address.clone(),
            responder: test_descriptor("kaspatest:responder", &"44".repeat(32)),
            signature_hex: "55".repeat(64),
        };
        let response = accepted.encode().expect("encode KKTP response");
        assert_eq!(kktp_anchor_type(&response).unwrap().as_deref(), Some("response"));
        assert_eq!(GhostContactAccept::decode(&response).unwrap().request_id, decoded.request_id);
    }

    #[test]
    fn kktp_session_end_is_canonical_and_dual_signature_bound() {
        let end = KktpSessionEnd {
            kind: "session_end".into(),
            version: GHOST_KKTP_VERSION,
            sid: "11".repeat(16),
            initiator_hydra_id: "22".repeat(32),
            responder_hydra_id: "33".repeat(32),
            sender_hydra_id: "22".repeat(32),
            sender_kaspa_address: "kaspatest:sender".into(),
            recipient_kaspa_address: "kaspatest:recipient".into(),
            reason: "left".into(),
            pq_sig_b64: "AQID".into(),
            sig: "44".repeat(64),
        };
        let encoded = end.encode().expect("encode KKTP session_end");
        assert_eq!(kktp_anchor_type(&encoded).unwrap().as_deref(), Some("session_end"));
        let decoded = KktpSessionEnd::decode(&encoded).expect("decode KKTP session_end");
        assert_eq!(decoded.sid, end.sid);
        assert_eq!(decoded.reason, "left");
        assert_eq!(decoded.encode().unwrap(), encoded);

        let mut wrong_sender = decoded.clone();
        wrong_sender.sender_hydra_id = "55".repeat(32);
        assert!(wrong_sender.encode().is_err());
    }

    #[test]
    fn kktp_mailbox_id_is_role_ordered_and_sid_bound() {
        let sid = "10".repeat(16);
        let a = "20".repeat(32);
        let b = "30".repeat(32);
        let ab = kktp_mailbox_id(&sid, &a, &b).unwrap();
        assert_eq!(ab.len(), 64);
        assert_ne!(ab, kktp_mailbox_id(&sid, &b, &a).unwrap());
        assert_ne!(ab, kktp_mailbox_id(&"11".repeat(16), &a, &b).unwrap());
    }

    #[test]
    fn kktp_mailbox_message_rejects_noncanonical_wire_json() {
        let wire = KktpMailboxMessage {
            kind: "msg".into(),
            version: GHOST_KKTP_VERSION,
            sid: "11".repeat(16),
            mailbox_id: "22".repeat(32),
            direction: KktpDirection::AtoB,
            seq: 0,
            sender_hydra_id: "33".repeat(32),
            message_id: "44".repeat(16),
            profile: "Off".into(),
            ciphertext_b64: "AQID".into(),
        };
        let canonical = wire.encode().unwrap();
        assert!(KktpMailboxMessage::decode(&canonical).is_ok());
        let text = std::str::from_utf8(&canonical).unwrap();
        let altered = text.replacen("{", "{ ", 1).into_bytes();
        assert!(KktpMailboxMessage::decode(&altered).is_err());
    }

}
