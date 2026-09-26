use ghost_api::HydraIncomingRequestProjection;
use ghost_protocol::{kktp_anchor_type, GhostContactRequest, KKTP_ANCHOR_PREFIX};

pub fn preview_contact_request(
    envelope_hex: &str,
    local_kaspa_addresses: &[String],
) -> Result<Option<HydraIncomingRequestProjection>, String> {
    let envelope = decode_envelope(envelope_hex)?;
    if !supported_request(&envelope)? {
        return Ok(None);
    }
    let request = GhostContactRequest::decode(&envelope)?;
    crate::verify_contact_request(&request)?;
    if !local_kaspa_addresses
        .iter()
        .any(|address| address == &request.recipient_kaspa_address)
    {
        return Ok(None);
    }
    validate_identity(&request.sender.hydra_identity_id)?;
    Ok(Some(project_request(request, &envelope)))
}

fn decode_envelope(envelope_hex: &str) -> Result<Vec<u8>, String> {
    if !envelope_hex.len().is_multiple_of(2)
        || envelope_hex.len() > ghost_core::MAX_MAILBOX_ENVELOPE_BYTES.saturating_mul(2)
    {
        return Err("mailbox envelope hex length is invalid".into());
    }
    hex::decode(envelope_hex).map_err(|_| "mailbox envelope is not valid hex".to_string())
}

fn supported_request(envelope: &[u8]) -> Result<bool, String> {
    let signed_request = envelope.starts_with(&ghost_protocol::GTCR_MAGIC);
    let discovery = envelope.starts_with(KKTP_ANCHOR_PREFIX)
        && kktp_anchor_type(envelope)?.as_deref() == Some("discovery");
    Ok(signed_request || discovery)
}

fn validate_identity(identity_id: &str) -> Result<(), String> {
    if identity_id.len() == 64 && identity_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("contact-request signed HYDRA identity must be 64 hexadecimal characters".into())
    }
}

fn project_request(
    request: GhostContactRequest,
    envelope: &[u8],
) -> HydraIncomingRequestProjection {
    let peer_hydra_id = request.sender.hydra_identity_id.clone();
    HydraIncomingRequestProjection {
        request_id: request.request_id,
        peer_address: request.sender.kaspa_address,
        local_address: request.recipient_kaspa_address,
        peer_label: request.sender.display_name,
        peer_hydra_id,
        signed_request_hex: hex::encode(envelope),
        room_invite: request
            .room_invite
            .map(|invite| ghost_chat::RoomInviteMeta {
                room_id: invite.room_id,
                room_name: invite.room_name,
            }),
        call_id: request
            .call_invite
            .as_ref()
            .map(|invite| invite.call_id.clone()),
        call_action: request.call_invite.map(|invite| invite.action),
    }
}
