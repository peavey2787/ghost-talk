use super::super::{
    contact_acceptance::reset_contact_session,
    mailbox_dispatch::discard_result,
    runtime_state::HydraProfileRuntime,
    session_state::install_kktp_binding,
    session_types::{
        HydraContactAcceptedProjection, HydraMailboxResult, KktpRole, KktpSessionState, PeerRoute,
        BASE64, GHOST_KKTP_VERSION, MAX_CONTACT_CARD_BYTES,
    },
    transport_persistence::persist_transport_state,
};
pub(crate) fn historical_contact_accept(
    runtime: &HydraProfileRuntime,
    accepted: &ghost_protocol::GhostContactAccept,
) -> bool {
    runtime
        .peer_routes
        .get(&accepted.responder.hydra_identity_id)
        .is_some_and(|route| {
            route.resume_required && route.session_sid() == Some(accepted.request_id.as_str())
        })
}

pub(crate) fn accepted_sid_is_allowed(
    runtime: &HydraProfileRuntime,
    accepted: &ghost_protocol::GhostContactAccept,
) -> bool {
    if accepted.version != GHOST_KKTP_VERSION {
        return true;
    }
    let peer = &accepted.responder.hydra_identity_id;
    runtime
        .pending_contact_request_sids
        .contains(&accepted.request_id.to_ascii_lowercase())
        || runtime
            .allowed_kktp_sids
            .get(peer)
            .is_some_and(|sids| sids.contains(&accepted.request_id.to_ascii_lowercase()))
        || runtime
            .kktp_sessions
            .get(peer)
            .is_some_and(|binding| binding.sid == accepted.request_id)
}

pub(crate) fn decode_accept_contact_card(
    accepted: &ghost_protocol::GhostContactAccept,
) -> Result<Vec<u8>, String> {
    let card = BASE64
        .decode(&accepted.responder.hydra_contact_card_b64)
        .map_err(|_| "contact-accept HYDRA contact card is not valid base64".to_string())?;
    if card.is_empty() || card.len() > MAX_CONTACT_CARD_BYTES {
        return Err("contact-accept HYDRA contact card size is invalid".into());
    }
    Ok(card)
}

pub(crate) fn same_contact_accept_session(
    runtime: &HydraProfileRuntime,
    peer: &str,
    accepted: &ghost_protocol::GhostContactAccept,
) -> bool {
    accepted.version == GHOST_KKTP_VERSION
        && runtime.kktp_sessions.get(peer).is_some_and(|binding| {
            binding.sid == accepted.request_id
                && binding.role == KktpRole::Initiator
                && binding.state != KktpSessionState::Closed
        })
}

pub(crate) fn prepare_accepted_session(
    runtime: &mut HydraProfileRuntime,
    peer: &str,
    accepted: &ghost_protocol::GhostContactAccept,
) -> Result<bool, String> {
    let same_session = same_contact_accept_session(runtime, peer, accepted);
    let retired = runtime.retired_kktp_sids.contains(&accepted.request_id);
    if accepted.version == GHOST_KKTP_VERSION && !same_session && retired {
        return Ok(false);
    }
    if same_session {
        return Ok(true);
    }
    reset_contact_session(runtime, peer)?;
    if accepted.version == GHOST_KKTP_VERSION {
        install_kktp_binding(
            runtime,
            peer,
            accepted.request_id.clone(),
            KktpRole::Initiator,
            KktpSessionState::Discovered,
        )?;
    }
    Ok(true)
}

pub(crate) fn contact_accepted_result(
    accepted: ghost_protocol::GhostContactAccept,
    peer: String,
) -> HydraMailboxResult {
    HydraMailboxResult {
        contact_accepted: Some(HydraContactAcceptedProjection {
            request_id: accepted.request_id,
            peer_address: accepted.responder.kaspa_address,
            acceptor_address: accepted.acceptor_kaspa_address,
            peer_label: accepted.responder.display_name,
            peer_hydra_id: peer,
        }),
        ..discard_result()
    }
}

fn contact_accept_is_actionable(
    runtime: &HydraProfileRuntime,
    accepted: &ghost_protocol::GhostContactAccept,
    local_kaspa_addresses: &[String],
) -> Result<bool, String> {
    let sent_by_local_identity = accepted.responder.hydra_identity_id == runtime.identity_id;
    if sent_by_local_identity || discard_historical_contact_accept(runtime, accepted) {
        return Ok(false);
    }
    let addressed_here = local_kaspa_addresses
        .iter()
        .any(|address| address == &accepted.recipient_kaspa_address);
    if !addressed_here {
        return Err("contact acceptance is not addressed to this Ghost Talk wallet".into());
    }
    if !accepted_sid_is_allowed(runtime, accepted) {
        crate::debug_log::record(
            "warn",
            "mailbox",
            "stale-response-discarded",
            format!(
                "sid={} peer={}",
                accepted.request_id, accepted.responder.hydra_identity_id
            ),
        );
        return Ok(false);
    }
    Ok(true)
}

fn discard_historical_contact_accept(
    runtime: &HydraProfileRuntime,
    accepted: &ghost_protocol::GhostContactAccept,
) -> bool {
    let historical = historical_contact_accept(runtime, accepted);
    if historical {
        crate::debug_log::record(
            "info",
            "mailbox",
            "historical-contact-accept-discarded",
            format!(
                "sid={} peer={}",
                accepted.request_id, accepted.responder.hydra_identity_id
            ),
        );
    }
    historical
}

fn install_accepted_contact(
    runtime: &mut HydraProfileRuntime,
    accepted: &ghost_protocol::GhostContactAccept,
) -> Result<Option<String>, String> {
    let card = decode_accept_contact_card(accepted)?;
    let contact = runtime.hydra.add_contact(&card)?;
    if !accepted.responder.hydra_identity_id.is_empty()
        && accepted.responder.hydra_identity_id != contact.handle
    {
        return Err(
            "contact-accept HYDRA identity does not match its authenticated contact card".into(),
        );
    }
    runtime.blocked_peers.remove(&contact.handle);
    if !prepare_accepted_session(runtime, &contact.handle, accepted)? {
        return Ok(None);
    }
    runtime
        .pending_contact_request_sids
        .remove(&accepted.request_id.to_ascii_lowercase());
    runtime.peer_routes.insert(
        contact.handle.clone(),
        PeerRoute {
            kaspa_address: accepted.responder.kaspa_address.clone(),
            display_name: accepted.responder.display_name.clone(),
            session_sid: Some(accepted.request_id.clone()),
            session_role: Some(KktpRole::Initiator),
            resume_required: false,
        },
    );
    persist_transport_state(runtime)?;
    Ok(Some(contact.handle))
}

pub(crate) fn handle_contact_accept_carrier(
    runtime: &mut HydraProfileRuntime,
    profile_id: &str,
    envelope: &[u8],
    local_kaspa_addresses: &[String],
) -> Result<HydraMailboxResult, String> {
    let accepted = ghost_protocol::GhostContactAccept::decode(envelope)?;
    ghost_kaspa::verify_contact_accept(&accepted)?;
    if !contact_accept_is_actionable(runtime, &accepted, local_kaspa_addresses)? {
        return Ok(discard_result());
    }
    let Some(peer) = install_accepted_contact(runtime, &accepted)? else {
        return Ok(discard_result());
    };
    crate::debug_log::record(
        "info",
        "handshake",
        "response-received-verified",
        format!(
            "profile={} sid={} peer={} recipient={} acceptor={}",
            profile_id,
            accepted.request_id,
            peer,
            accepted.recipient_kaspa_address,
            accepted.acceptor_kaspa_address
        ),
    );
    Ok(contact_accepted_result(accepted, peer))
}
use base64::Engine as _;
