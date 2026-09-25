use super::canonical_json::canonical_json;
use super::{
    events::{
        Deserialize, GhostCallInviteContext, GhostRoomInviteContext, Serialize, Sha3_256, BASE64,
        GHOST_KKTP_VERSION, KKTP_ANCHOR_PREFIX, KKTP_MESSAGE_PREFIX, MAX_EVENT_BYTES,
    },
    kktp_types::{
        GhostReactionEvent, KktpDirection, KktpHandshakeControl, KktpInnerMessage,
        KktpMailboxMessage, KktpSessionEnd,
    },
    kktp_validation_types::{
        validate_kktp_handshake, validate_kktp_inner, KktpHandshakeSigning,
        KktpSessionEndKaspaSigning, KktpSessionEndPqSigning,
    },
};
use base64::Engine as _;
use sha3::Digest as _;
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
            reaction: None,
            resume_seed_b64: None,
        }
    }

    pub fn reaction(
        sid: String,
        mailbox_id: String,
        direction: KktpDirection,
        seq: u64,
        message_id: String,
        reaction: GhostReactionEvent,
    ) -> Self {
        Self {
            kind: "reaction".into(),
            version: GHOST_KKTP_VERSION,
            sid,
            mailbox_id,
            direction,
            seq,
            message_id,
            body: String::new(),
            reaction: Some(reaction),
            resume_seed_b64: None,
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
            reaction: None,
            resume_seed_b64: None,
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
        let mut out =
            Vec::with_capacity(KKTP_MESSAGE_PREFIX.len() + self.mailbox_id.len() + 1 + body.len());
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
        let prefix_mailbox = std::str::from_utf8(&rest[..colon])
            .map_err(|_| "KKTP mailbox id is not UTF-8".to_string())?;
        let value: Self = serde_json::from_slice(&rest[colon + 1..]).map_err(|e| e.to_string())?;
        validate_sid(&value.sid)?;
        validate_hex(&value.mailbox_id, 64, "KKTP mailbox id")?;
        validate_hex(&value.sender_hydra_id, 64, "KKTP sender HYDRA id")?;
        validate_hex(&value.message_id, 32, "KKTP message id")?;
        if value.kind != "msg"
            || value.version != GHOST_KKTP_VERSION
            || prefix_mailbox != value.mailbox_id
        {
            return Err("KKTP mailbox message header does not match its body".into());
        }
        BASE64
            .decode(&value.ciphertext_b64)
            .map_err(|_| "KKTP HYDRA ciphertext is not valid base64".to_string())?;
        if value.encode()?.as_slice() != bytes {
            return Err("Ghost Talk KKTP mailbox message is not canonical JSON".into());
        }
        Ok(value)
    }
}

pub fn validate_kktp_message_id(value: &str) -> Result<(), String> {
    validate_hex(value, 32, "KKTP message id")
}

pub fn kktp_mailbox_id(
    sid: &str,
    initiator_hydra_id: &str,
    responder_hydra_id: &str,
) -> Result<String, String> {
    validate_sid(sid)?;
    validate_hex(initiator_hydra_id, 64, "KKTP initiator HYDRA id")?;
    validate_hex(responder_hydra_id, 64, "KKTP responder HYDRA id")?;
    let mut hasher = Sha3_256::new();
    hasher.update(b"GhostTalk/KKTP/v2/mailbox\0");
    hasher.update(
        hex::decode(initiator_hydra_id).map_err(|_| "invalid initiator HYDRA id".to_string())?,
    );
    hasher.update(
        hex::decode(responder_hydra_id).map_err(|_| "invalid responder HYDRA id".to_string())?,
    );
    hasher.update(hex::decode(sid).map_err(|_| "invalid KKTP sid".to_string())?);
    Ok(hex::encode(hasher.finalize()))
}

pub(crate) fn encode_kktp_anchor<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
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
    let value: serde_json::Value =
        serde_json::from_slice(&bytes[KKTP_ANCHOR_PREFIX.len()..]).map_err(|e| e.to_string())?;
    Ok(value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned))
}

pub(crate) fn decode_kktp_anchor<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, String> {
    if !bytes.starts_with(KKTP_ANCHOR_PREFIX) {
        return Err("not a Ghost Talk KKTP anchor".into());
    }
    serde_json::from_slice(&bytes[KKTP_ANCHOR_PREFIX.len()..]).map_err(|e| e.to_string())
}

mod validation;
pub(crate) use validation::{
    validate_call_invite_context, validate_hex, validate_kktp_session_end,
    validate_kktp_session_end_core, validate_room_invite_context, validate_sid,
};
