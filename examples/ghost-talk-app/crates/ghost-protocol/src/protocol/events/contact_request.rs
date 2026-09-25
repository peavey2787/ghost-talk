use super::{
    canonical_json, decode_contact_request_anchor, decode_contact_request_v1, encode_kktp_anchor,
    validate_call_invite_context, validate_canonical_contact_request,
    validate_contact_request_payload_size, validate_decoded_contact_request,
    validate_room_invite_context, validate_sid, Deserialize, GhostCallInviteContext,
    GhostContactDescriptor, GhostRoomInviteContext, KktpDiscoverySigning, KktpDiscoveryWire,
    Serialize, GHOST_KKTP_VERSION, GTCR_MAGIC, KKTP_ANCHOR_PREFIX, MAX_GHOST_TX_PAYLOAD,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GhostContactRequest {
    pub version: u16,
    pub request_id: String,
    pub recipient_kaspa_address: String,
    pub sender: GhostContactDescriptor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_invite: Option<GhostRoomInviteContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_invite: Option<GhostCallInviteContext>,
    pub signature_hex: String,
}

impl GhostContactRequest {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        if self.version == GHOST_KKTP_VERSION {
            return canonical_json(&KktpDiscoverySigning {
                kind: "discovery",
                recipient_kaspa_address: &self.recipient_kaspa_address,
                sender: &self.sender,
                room_invite: &self.room_invite,
                call_invite: &self.call_invite,
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
        validate_room_invite_context(self.room_invite.as_ref())?;
        validate_call_invite_context(self.call_invite.as_ref())?;
        if self.version == GHOST_KKTP_VERSION {
            let wire = KktpDiscoveryWire {
                kind: "discovery",
                recipient_kaspa_address: &self.recipient_kaspa_address,
                sender: &self.sender,
                room_invite: &self.room_invite,
                call_invite: &self.call_invite,
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
        validate_contact_request_payload_size(v)?;
        let is_anchor = v.starts_with(KKTP_ANCHOR_PREFIX);
        let value = if is_anchor {
            decode_contact_request_anchor(v)?
        } else {
            decode_contact_request_v1(v)?
        };
        validate_decoded_contact_request(&value)?;
        validate_canonical_contact_request(v, &value, is_anchor)?;
        Ok(value)
    }
}
