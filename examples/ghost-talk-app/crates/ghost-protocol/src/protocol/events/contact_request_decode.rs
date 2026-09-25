use super::{
    decode_kktp_anchor, validate_call_invite_context, validate_room_invite_context, validate_sid,
    GhostContactRequest, KktpDiscoveryOwned, GHOST_KKTP_VERSION, GTCR_MAGIC, MAX_GHOST_TX_PAYLOAD,
};

pub(super) fn validate_contact_request_payload_size(input: &[u8]) -> Result<(), String> {
    if input.len() > MAX_GHOST_TX_PAYLOAD {
        Err("Ghost Talk contact request exceeds the Kaspa payload limit".into())
    } else {
        Ok(())
    }
}

pub(super) fn decode_contact_request_anchor(input: &[u8]) -> Result<GhostContactRequest, String> {
    let wire: KktpDiscoveryOwned = decode_kktp_anchor(input)?;
    if wire.kind != "discovery" || wire.version != GHOST_KKTP_VERSION {
        return Err("not a supported Ghost Talk KKTP discovery anchor".into());
    }
    Ok(GhostContactRequest {
        version: wire.version,
        request_id: wire.sid,
        recipient_kaspa_address: wire.recipient_kaspa_address,
        sender: wire.sender,
        room_invite: wire.room_invite,
        call_invite: wire.call_invite,
        signature_hex: wire.sig,
    })
}

pub(super) fn decode_contact_request_v1(input: &[u8]) -> Result<GhostContactRequest, String> {
    if input.len() < 4 || input[..4] != GTCR_MAGIC {
        return Err("not a Ghost Talk contact request".into());
    }
    serde_json::from_slice(&input[4..]).map_err(|error| error.to_string())
}

pub(super) fn validate_decoded_contact_request(value: &GhostContactRequest) -> Result<(), String> {
    if value.version != 1 && value.version != GHOST_KKTP_VERSION {
        return Err("unsupported Ghost Talk contact-request version".into());
    }
    validate_sid(&value.request_id)?;
    if value.recipient_kaspa_address.trim().is_empty() {
        return Err("Ghost Talk contact request recipient is empty".into());
    }
    validate_room_invite_context(value.room_invite.as_ref())?;
    validate_call_invite_context(value.call_invite.as_ref())
}

pub(super) fn validate_canonical_contact_request(
    input: &[u8],
    value: &GhostContactRequest,
    is_anchor: bool,
) -> Result<(), String> {
    if is_anchor && value.encode()?.as_slice() != input {
        return Err("Ghost Talk KKTP discovery anchor is not canonical JSON".into());
    }
    Ok(())
}
