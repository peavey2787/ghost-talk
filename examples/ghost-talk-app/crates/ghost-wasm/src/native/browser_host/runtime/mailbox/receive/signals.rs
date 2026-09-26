use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::{HydraCallSignalProjection, HydraMailboxResult, HydraSessionEndedProjection};
use ghost_protocol::{GhostDeliveryAck, KktpSessionEnd};

use crate::native::browser_host::runtime::{
    handshake,
    mailbox::common::discard,
    secure_transport::with_hydra_runtime,
    session::{self, SessionState},
};

pub(super) fn receive_ack(
    profile: &str,
    identity: &str,
    envelope: &[u8],
) -> Result<HydraMailboxResult, String> {
    let ack = GhostDeliveryAck::decode(envelope)?;
    ghost_kaspa::verify_delivery_ack(&ack)?;
    if ack.signer_hydra_id == identity || ack.destination_hydra_id != identity {
        return Ok(discard());
    }
    let route = session::with(profile, |runtime| {
        Ok(runtime.routes.get(&ack.signer_hydra_id).cloned())
    })?;
    let Some(route) = route.filter(|route| route.kaspa_address == ack.signer_kaspa_address) else {
        return Ok(discard());
    };
    session::with_mut(profile, |runtime| {
        runtime.pending_outbound.remove(&ack.signer_hydra_id);
        runtime.prepared_completion.remove(&ack.signer_hydra_id);
        if let Some(binding) = runtime.sessions.get_mut(&ack.signer_hydra_id) {
            binding.state = SessionState::Active;
        }
        Ok(())
    })?;
    Ok(HydraMailboxResult {
        peer_address: Some(route.kaspa_address),
        peer_label: Some(route.display_name),
        delivery_ack: Some(ack.message_id),
        delivery_ack_peer: Some(ack.signer_hydra_id.clone()),
        session_established_peer: Some(ack.signer_hydra_id),
        ..discard()
    })
}

pub(super) fn receive_call_signal(
    profile: &str,
    identity: &str,
    local: &[String],
    envelope: &[u8],
) -> Result<HydraMailboxResult, String> {
    let signal = ghost_protocol::GhostCallSignal::decode(envelope)?;
    if signal.sender.hydra_identity_id == identity
        || !local
            .iter()
            .any(|address| address == &signal.recipient_kaspa_address)
    {
        return Ok(discard());
    }
    ghost_kaspa::verify_call_signal(&signal)?;
    let card = BASE64
        .decode(&signal.sender.hydra_contact_card_b64)
        .map_err(|_| "call-signal HYDRA contact card is not valid base64".to_string())?;
    let peer = with_hydra_runtime(profile, |hydra| {
        hydra.preview_contact(&card).map(|contact| contact.handle)
    })?;
    if peer != signal.sender.hydra_identity_id {
        return Err(
            "call-signal HYDRA identity does not match its authenticated contact card".into(),
        );
    }
    Ok(HydraMailboxResult {
        call_signal: Some(HydraCallSignalProjection {
            signal_id: signal.signal_id,
            call_id: signal.call_id,
            action: signal.action,
            peer_address: signal.sender.kaspa_address,
            local_address: signal.recipient_kaspa_address,
            peer_label: signal.sender.display_name,
            peer_hydra_id: peer,
        }),
        ..discard()
    })
}

pub(super) fn receive_session_end(
    profile: &str,
    identity: &str,
    local: &[String],
    envelope: &[u8],
) -> Result<HydraMailboxResult, String> {
    let end = KktpSessionEnd::decode(envelope)?;
    ghost_kaspa::verify_kktp_session_end(&end)?;
    if !session_end_targets_local(&end, identity, local)? {
        return Ok(discard());
    }
    verify_session_end_signature(profile, &end)?;
    let route = session::with(profile, |runtime| {
        Ok(runtime.routes.get(&end.sender_hydra_id).cloned())
    })?;
    verify_session_end_route(&end, route.as_ref())?;
    handshake::reset_peer_crypto(profile, &end.sender_hydra_id)?;
    session::with_mut(profile, |runtime| {
        runtime.sessions.remove(&end.sender_hydra_id);
        runtime.blocked.insert(end.sender_hydra_id.clone());
        Ok(())
    })?;
    Ok(HydraMailboxResult {
        peer_address: Some(end.sender_kaspa_address.clone()),
        peer_label: route.map(|route| route.display_name),
        session_ended: Some(HydraSessionEndedProjection {
            peer_hydra_id: end.sender_hydra_id,
            peer_address: end.sender_kaspa_address,
            sid: end.sid,
            reason: end.reason,
        }),
        ..discard()
    })
}

fn session_end_targets_local(
    end: &KktpSessionEnd,
    identity: &str,
    local: &[String],
) -> Result<bool, String> {
    if !local
        .iter()
        .any(|address| address == &end.recipient_kaspa_address)
        || end.sender_hydra_id == identity
    {
        return Ok(false);
    }
    if end.initiator_hydra_id != identity && end.responder_hydra_id != identity {
        return Err("KKTP session_end does not name this HYDRA identity".into());
    }
    Ok(true)
}

fn verify_session_end_signature(profile: &str, end: &KktpSessionEnd) -> Result<(), String> {
    let sig = BASE64
        .decode(&end.pq_sig_b64)
        .map_err(|_| "KKTP session_end PQ signature is not valid base64".to_string())?;
    with_hydra_runtime(profile, |hydra| {
        hydra.verify_contact_application_context(
            &end.sender_hydra_id,
            &end.pq_signing_bytes()?,
            &sig,
        )
    })
}

fn verify_session_end_route(
    end: &KktpSessionEnd,
    route: Option<&session::Route>,
) -> Result<(), String> {
    if route.is_some_and(|route| route.kaspa_address != end.sender_kaspa_address) {
        return Err(
            "KKTP session_end Kaspa signer does not match the authenticated peer route".into(),
        );
    }
    Ok(())
}
