use base64::Engine as _;

use super::super::{
    runtime_session_queries::clear_peer_handshake_state,
    runtime_state::HydraProfileRuntime,
    session_state::{install_kktp_binding, retire_kktp_binding},
    session_types::{
        KktpRole, KktpSessionState, PeerRoute, BASE64, GHOST_KKTP_VERSION, MAX_CONTACT_CARD_BYTES,
        MAX_MAILBOX_ENVELOPE_BYTES,
    },
    transport_persistence::persist_transport_state,
};

pub(crate) fn decode_signed_contact_request(
    signed_request_hex: &str,
) -> Result<ghost_protocol::GhostContactRequest, String> {
    if !signed_request_hex.len().is_multiple_of(2)
        || signed_request_hex.len() > MAX_MAILBOX_ENVELOPE_BYTES.saturating_mul(2)
    {
        return Err("signed Ghost Talk contact request hex length is invalid".into());
    }
    let encoded = hex::decode(signed_request_hex)
        .map_err(|_| "signed Ghost Talk contact request is not valid hex".to_string())?;
    let request = ghost_protocol::GhostContactRequest::decode(&encoded)?;
    ghost_kaspa::verify_contact_request(&request)?;
    Ok(request)
}

pub(crate) fn validate_contact_request_recipient(
    request: &ghost_protocol::GhostContactRequest,
    local_kaspa_addresses: &[String],
) -> Result<(), String> {
    if local_kaspa_addresses
        .iter()
        .any(|address| address == &request.recipient_kaspa_address)
    {
        Ok(())
    } else {
        Err("contact request is not addressed to this Ghost Talk wallet".into())
    }
}

pub(crate) fn decode_contact_card(
    request: &ghost_protocol::GhostContactRequest,
) -> Result<Vec<u8>, String> {
    let card = BASE64
        .decode(&request.sender.hydra_contact_card_b64)
        .map_err(|_| "contact-request HYDRA contact card is not valid base64".to_string())?;
    if card.is_empty() || card.len() > MAX_CONTACT_CARD_BYTES {
        return Err("contact-request HYDRA contact card size is invalid".into());
    }
    Ok(card)
}

pub(crate) fn add_verified_contact(
    runtime: &mut HydraProfileRuntime,
    request: &ghost_protocol::GhostContactRequest,
    card: &[u8],
) -> Result<String, String> {
    let preview = runtime.hydra.preview_contact(card)?;
    if !request.sender.hydra_identity_id.is_empty()
        && request.sender.hydra_identity_id != preview.handle
    {
        return Err(
            "contact-request HYDRA identity does not match its authenticated contact card".into(),
        );
    }
    if preview.handle == runtime.identity_id {
        return Err("cannot accept a Ghost Talk contact request from this identity itself".into());
    }
    let contact = runtime.hydra.add_contact(card)?;
    if contact.handle != preview.handle {
        return Err("HYDRA contact changed between preview and acceptance".into());
    }
    Ok(contact.handle)
}

pub(crate) fn is_same_kktp_request(
    runtime: &HydraProfileRuntime,
    peer: &str,
    request: &ghost_protocol::GhostContactRequest,
) -> bool {
    request.version == GHOST_KKTP_VERSION
        && runtime.kktp_sessions.get(peer).is_some_and(|binding| {
            binding.sid == request.request_id
                && binding.role == KktpRole::Responder
                && binding.state != KktpSessionState::Closed
        })
}

pub(crate) fn reject_retired_contact_request(
    runtime: &HydraProfileRuntime,
    request: &ghost_protocol::GhostContactRequest,
    same_session: bool,
) -> Result<(), String> {
    let retired = runtime.retired_kktp_sids.contains(&request.request_id);
    if request.version == GHOST_KKTP_VERSION && !same_session && retired {
        return Err(
            "this KKTP chat request belongs to a retired session; the sender must start a new chat"
                .into(),
        );
    }
    Ok(())
}

pub(crate) fn reset_contact_session(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
) -> Result<(), String> {
    match runtime.hydra.session_status(peer)?.as_str() {
        "active" => runtime.hydra.close_session(peer)?,
        "pending" => runtime.hydra.abort_handshake(peer)?,
        _ => {}
    }
    clear_peer_handshake_state(runtime, peer);
    retire_kktp_binding(runtime, peer);
    persist_transport_state(runtime)?;
    Ok(())
}

pub(crate) fn prepare_contact_session(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    request: &ghost_protocol::GhostContactRequest,
) -> Result<(), String> {
    let same_session = is_same_kktp_request(runtime, peer, request);
    reject_retired_contact_request(runtime, request, same_session)?;
    if same_session {
        return Ok(());
    }
    reset_contact_session(runtime, peer)?;
    if request.version == GHOST_KKTP_VERSION {
        install_kktp_binding(
            runtime,
            peer,
            request.request_id.clone(),
            KktpRole::Responder,
            KktpSessionState::Discovered,
        )?;
    }
    Ok(())
}

pub(crate) fn remember_contact_route(
    runtime: &mut HydraProfileRuntime,
    peer: String,
    request: &ghost_protocol::GhostContactRequest,
) {
    runtime.blocked_peers.remove(&peer);
    runtime.peer_routes.insert(
        peer,
        PeerRoute {
            kaspa_address: request.sender.kaspa_address.clone(),
            display_name: request.sender.display_name.clone(),
            session_sid: Some(request.request_id.clone()),
            session_role: Some(KktpRole::Responder),
            resume_required: false,
        },
    );
}

pub(crate) fn accept_signed_contact_request(
    runtime: &mut HydraProfileRuntime,
    signed_request_hex: &str,
    local_kaspa_addresses: &[String],
) -> Result<ghost_protocol::GhostContactRequest, String> {
    let request = decode_signed_contact_request(signed_request_hex)?;
    validate_contact_request_recipient(&request, local_kaspa_addresses)?;
    let card = decode_contact_card(&request)?;
    let peer = add_verified_contact(runtime, &request, &card)?;
    prepare_contact_session(runtime, &peer, &request)?;
    remember_contact_route(runtime, peer, &request);
    Ok(request)
}
