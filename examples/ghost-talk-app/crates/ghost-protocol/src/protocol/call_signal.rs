use serde::{Deserialize, Serialize};

use super::{
    canonical_json::canonical_json,
    events::{GhostContactDescriptor, GHOST_KKTP_VERSION},
    kktp_codec::{decode_kktp_anchor, encode_kktp_anchor, validate_hex, validate_sid},
    kktp_validation_types::KktpCallSignalSigning,
};

/// Authenticated Kaspa call-control signal. This control plane is deliberately
/// independent of the encrypted chat/session ratchet so an incoming ring can be
/// delivered even when the existing HYDRA/KKTP transport is stale, restarting,
/// or has never been established. Secure call media still starts only after the
/// peers have an authenticated encrypted transport.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GhostCallSignal {
    #[serde(rename = "type")]
    pub kind: String,
    pub version: u16,
    pub signal_id: String,
    pub call_id: String,
    pub action: String,
    pub recipient_kaspa_address: String,
    pub sender: GhostContactDescriptor,
    pub signature_hex: String,
}

impl GhostCallSignal {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        validate_call_signal(self)?;
        canonical_json(&KktpCallSignalSigning {
            kind: &self.kind,
            version: self.version,
            signal_id: &self.signal_id,
            call_id: &self.call_id,
            action: &self.action,
            recipient_kaspa_address: &self.recipient_kaspa_address,
            sender: &self.sender,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        validate_call_signal(self)?;
        encode_kktp_anchor(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let value: Self = decode_kktp_anchor(bytes)?;
        validate_call_signal(&value)?;
        if value.encode()?.as_slice() != bytes {
            return Err("Ghost Talk KKTP call-signal anchor is not canonical JSON".into());
        }
        Ok(value)
    }
}

pub(crate) fn validate_call_signal(value: &GhostCallSignal) -> Result<(), String> {
    if value.kind != "call_signal" || value.version != GHOST_KKTP_VERSION {
        return Err("unsupported Ghost Talk KKTP call signal".into());
    }
    validate_sid(&value.signal_id)?;
    validate_hex(&value.call_id, 32, "Ghost Talk call id")?;
    if !matches!(
        value.action.as_str(),
        "request" | "accept" | "decline" | "cancel"
    ) {
        return Err(
            "Ghost Talk call signal action must be request, accept, decline, or cancel".into(),
        );
    }
    if value.recipient_kaspa_address.trim().is_empty() {
        return Err("Ghost Talk call signal recipient is empty".into());
    }
    if value.sender.kaspa_address.trim().is_empty()
        || value.sender.hydra_identity_id.len() != 64
        || !value
            .sender
            .hydra_identity_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("Ghost Talk call signal sender descriptor is invalid".into());
    }
    Ok(())
}
