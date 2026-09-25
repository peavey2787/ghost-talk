use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::{HydraCallSignalProjection, HydraContactAcceptedProjection, HydraMailboxResult, HydraRecoveryProjection, HydraSessionEndedProjection};
use ghost_hydra::ReceivedProjection;
use ghost_protocol::{GhostContactAccept, GhostDeliveryAck, KktpFirstMessage, KktpHandshakeControl, KktpMailboxMessage, KktpSessionEnd};
use serde_json::Value;

use super::{secure_transport::with_hydra_runtime, handshake, mailbox_common::{discard, stego}, session::{self, Role, SessionState}};
use super::super::support::util::{required, required_str, to_value};

pub(in crate::native::browser_host) fn receive(args: &Value) -> Result<Value, String> {
    let profile = required_str(args, "profileId")?;
    let identity = required_str(args, "identityId")?;
    super::mailbox_send::ensure_identity(profile, identity)?;
    let local_addresses: Vec<String> = required(args, "localKaspaAddresses")?;
    let envelope = hex::decode(required_str(args, "envelopeHex")?).map_err(|_| "mailbox envelope is not valid hex".to_string())?;
    let result = dispatch(profile, identity, &local_addresses, &envelope)?;
    to_value(result)
}

fn dispatch(profile: &str, identity: &str, local: &[String], envelope: &[u8]) -> Result<HydraMailboxResult, String> {
    if envelope.starts_with(&ghost_protocol::GTCR_MAGIC) {
        return super::mailbox::receive_contact_request(profile, identity, local, envelope);
    }
    if envelope.starts_with(&ghost_protocol::GTACK_MAGIC) {
        return receive_ack(profile, identity, envelope);
    }
    if is_mailbox_message(envelope) {
        return receive_message(profile, KktpMailboxMessage::decode(envelope)?);
    }
    dispatch_anchor(profile, identity, local, envelope)
}

fn is_mailbox_message(envelope: &[u8]) -> bool {
    envelope.starts_with(ghost_protocol::KKTP_MESSAGE_PREFIX)
        && !envelope.starts_with(ghost_protocol::KKTP_ANCHOR_PREFIX)
}

fn dispatch_anchor(profile: &str, identity: &str, local: &[String], envelope: &[u8]) -> Result<HydraMailboxResult, String> {
    match ghost_protocol::kktp_anchor_type(envelope)?.as_deref() {
        Some("response") => receive_accept(profile, local, envelope),
        Some("ghost_handshake") => handshake::handle(profile, KktpHandshakeControl::decode(envelope)?),
        Some("session_end") => receive_session_end(profile, identity, local, envelope),
        Some("call_signal") => receive_call_signal(profile, identity, local, envelope),
        _ => Ok(discard()),
    }
}

fn receive_accept(profile: &str, local: &[String], envelope: &[u8]) -> Result<HydraMailboxResult, String> {
    let accepted = GhostContactAccept::decode(envelope)?;
    ghost_kaspa::verify_contact_accept(&accepted)?;
    let identity = session::with(profile, |runtime| Ok(runtime.identity_id.clone()))?;
    if accepted.responder.hydra_identity_id == identity { return Ok(discard()); }
    if !local.iter().any(|address| address == &accepted.recipient_kaspa_address) { return Err("contact acceptance is not addressed to this Ghost Talk wallet".into()); }
    let allowed = session::with(profile, |runtime| Ok(runtime.pending_contact_sids.contains(&accepted.request_id.to_ascii_lowercase()) || runtime.routes.get(&accepted.responder.hydra_identity_id).and_then(|route| route.session_sid.as_deref()) == Some(accepted.request_id.as_str())))?;
    if !allowed { return Ok(discard()); }
    let card = BASE64.decode(&accepted.responder.hydra_contact_card_b64).map_err(|_| "contact-accept HYDRA contact card is not valid base64".to_string())?;
    let peer = with_hydra_runtime(profile, |hydra| {
        let contact = hydra.add_contact(&card)?;
        if contact.handle != accepted.responder.hydra_identity_id { return Err("contact-accept HYDRA identity does not match its authenticated contact card".into()); }
        Ok(contact.handle)
    })?;
    handshake::reset_peer_crypto(profile, &peer)?;
    session::install(profile, &peer, accepted.request_id.clone(), Role::Initiator, SessionState::Discovered)?;
    session::remember_route(profile, &peer, accepted.responder.kaspa_address.clone(), accepted.responder.display_name.clone(), accepted.request_id.clone(), Role::Initiator)?;
    session::with_mut(profile, |runtime| { runtime.pending_contact_sids.remove(&accepted.request_id.to_ascii_lowercase()); Ok(()) })?;
    Ok(HydraMailboxResult { contact_accepted: Some(HydraContactAcceptedProjection { request_id: accepted.request_id, peer_address: accepted.responder.kaspa_address, acceptor_address: accepted.acceptor_kaspa_address, peer_label: accepted.responder.display_name, peer_hydra_id: peer }), ..discard() })
}

fn receive_message(profile: &str, wire: KktpMailboxMessage) -> Result<HydraMailboxResult, String> {
    let peer = wire.sender_hydra_id.clone();
    let route = session::with(profile, |runtime| Ok(runtime.routes.get(&peer).cloned()))?;
    let binding = match message_binding(profile, &peer, route.clone(), &wire)? {
        MessageBinding::Ready(binding) => binding,
        MessageBinding::Return(result) => return Ok(result),
    };
    let inner = decrypt_message(profile, &peer, &binding, &wire)?;
    advance_receive_sequence(profile, &peer, wire.seq)?;
    let projected = project_inner(profile, &peer, &wire.sid, inner)?;
    let route = session::with(profile, |runtime| Ok(runtime.routes.get(&peer).cloned()))?;
    Ok(HydraMailboxResult { received: projected, peer_address: route.as_ref().map(|route| route.kaspa_address.clone()), peer_label: route.as_ref().map(|route| route.display_name.clone()), message_id: Some(wire.message_id), ..discard() })
}


enum MessageBinding {
    Ready(session::Binding),
    Return(HydraMailboxResult),
}

fn message_binding(
    profile: &str,
    peer: &str,
    route: Option<session::Route>,
    wire: &KktpMailboxMessage,
) -> Result<MessageBinding, String> {
    let binding = session::with(profile, |runtime| Ok(runtime.sessions.get(peer).cloned()))?;
    let Some(binding) = binding else {
        return Ok(MessageBinding::Return(recover_missing_binding(profile, peer, route, wire.message_id.clone())?));
    };
    if binding.sid != wire.sid || binding.mailbox_id != wire.mailbox_id || binding.inbound() != wire.direction {
        return Ok(MessageBinding::Return(discard()));
    }
    if binding.state != SessionState::Active {
        return Ok(MessageBinding::Return(route_result(route, wire.message_id.clone(), false)?));
    }
    if wire.seq < binding.recv_next_seq {
        return Ok(MessageBinding::Return(discard()));
    }
    if wire.seq > binding.recv_next_seq {
        return Err(format!("KKTP sequence gap: expected {}, received {}", binding.recv_next_seq, wire.seq));
    }
    Ok(MessageBinding::Ready(binding))
}

fn decrypt_message(
    profile: &str,
    peer: &str,
    binding: &session::Binding,
    wire: &KktpMailboxMessage,
) -> Result<ghost_protocol::KktpInnerMessage, String> {
    let encrypted = BASE64.decode(&wire.ciphertext_b64)
        .map_err(|_| "KKTP HYDRA ciphertext is not valid base64".to_string())?;
    let received = with_hydra_runtime(profile, |hydra| hydra.receive(&encrypted, stego(&wire.profile)?))?
        .ok_or_else(|| "HYDRA did not yield plaintext for the expected KKTP sequence".to_string())?;
    if received.from != peer {
        return Err("HYDRA sender does not match the KKTP peer binding".into());
    }
    let inner = ghost_protocol::KktpInnerMessage::decode(received.plaintext.as_bytes())?;
    validate_inner(binding, wire, &inner)?;
    Ok(inner)
}

fn validate_inner(
    binding: &session::Binding,
    wire: &KktpMailboxMessage,
    inner: &ghost_protocol::KktpInnerMessage,
) -> Result<(), String> {
    let mismatch = inner.sid != binding.sid
        || inner.mailbox_id != binding.mailbox_id
        || inner.direction != wire.direction
        || inner.seq != wire.seq
        || inner.message_id != wire.message_id;
    if mismatch {
        return Err("KKTP encrypted inner metadata does not match the authenticated outer record".into());
    }
    Ok(())
}

fn advance_receive_sequence(profile: &str, peer: &str, seq: u64) -> Result<(), String> {
    let next = seq.checked_add(1).ok_or_else(|| "KKTP inbound sequence exhausted".to_string())?;
    session::with_mut(profile, |runtime| {
        if let Some(binding) = runtime.sessions.get_mut(peer) {
            binding.recv_next_seq = next;
        }
        Ok(())
    })
}

fn recover_missing_binding(profile: &str, peer: &str, route: Option<session::Route>, message_id: String) -> Result<HydraMailboxResult, String> {
    let route = route.ok_or_else(|| "KKTP message has no verified Kaspa peer route".to_string())?;
    if route.resume_required && route.session_role == Some(Role::Responder) { return route_result(Some(route), message_id, false); }
    if with_hydra_runtime(profile, |hydra| hydra.session_status(peer))? == "pending" { return route_result(Some(route), message_id, false); }
    let local = session::with(profile, |runtime| Ok(runtime.identity_id.clone()))?;
    let sid = if route.resume_required && route.session_role == Some(Role::Initiator) {
        handshake::successor_sid(route.session_sid.as_deref().ok_or_else(|| "restart route is missing its prior KKTP SID".to_string())?, &local, peer)?
    } else { ghost_core::Id128::new_random().to_string() };
    handshake::reset_peer_crypto(profile, peer)?;
    let offer = with_hydra_runtime(profile, |hydra| hydra.init_handshake(peer))?;
    session::install(profile, peer, sid.clone(), Role::Initiator, SessionState::Handshake)?;
    Ok(HydraMailboxResult { recovery: Some(HydraRecoveryProjection { destination: route.kaspa_address.clone(), offer_hex: hex::encode(offer), sid, peer_hydra_id: peer.to_owned() }), peer_address: Some(route.kaspa_address), peer_label: Some(route.display_name), message_id: Some(message_id), discard: false, ..Default::default() })
}

pub(in crate::native::browser_host) fn open_first(profile: &str, peer: &str, sid: &str, first: KktpFirstMessage) -> Result<Option<ReceivedProjection>, String> {
    let binding = session::with(profile, |runtime| runtime.sessions.get(peer).cloned().ok_or_else(|| "KKTP responder binding disappeared".to_string()))?;
    if binding.sid != sid || binding.mailbox_id != first.mailbox_id || binding.inbound() != first.direction || first.seq != binding.recv_next_seq || first.sender_hydra_id != peer { return Err("KKTP first-message metadata does not match the authenticated session".into()); }
    let envelope = BASE64.decode(&first.envelope_b64).map_err(|_| "KKTP first-message envelope is not valid base64".to_string())?;
    let received = with_hydra_runtime(profile, |hydra| hydra.receive(&envelope, stego(&first.profile)?))?.ok_or_else(|| "HYDRA did not yield the authenticated first message".to_string())?;
    if received.from != peer { return Err("HYDRA first-message sender does not match the KKTP peer".into()); }
    let inner = ghost_protocol::KktpInnerMessage::decode(received.plaintext.as_bytes())?;
    if inner.sid != sid || inner.mailbox_id != first.mailbox_id || inner.direction != first.direction || inner.seq != first.seq || inner.message_id != first.message_id { return Err("KKTP first-message inner metadata mismatch".into()); }
    session::with_mut(profile, |runtime| { if let Some(binding) = runtime.sessions.get_mut(peer) { binding.recv_next_seq = first.seq + 1; } Ok(()) })?;
    project_inner(profile, peer, sid, inner)
}

fn project_inner(profile: &str, peer: &str, sid: &str, inner: ghost_protocol::KktpInnerMessage) -> Result<Option<ReceivedProjection>, String> {
    match inner.kind.as_str() {
        "msg" => Ok(Some(ReceivedProjection { from: peer.into(), plaintext: inner.body, content_type: None, session_sid: Some(sid.into()) })),
        "reaction" => Ok(Some(ReceivedProjection { from: peer.into(), plaintext: serde_json::to_string(&inner.reaction.ok_or_else(|| "authenticated KKTP reaction metadata is missing".to_string())?).map_err(|error| error.to_string())?, content_type: Some("ghost-reaction-v1".into()), session_sid: Some(sid.into()) })),
        "session_end" => { with_hydra_runtime(profile, |hydra| hydra.close_session(peer))?; session::with_mut(profile, |runtime| { runtime.sessions.remove(peer); Ok(()) })?; Ok(None) }
        _ => Err("unknown authenticated KKTP inner message type".into()),
    }
}

fn receive_ack(profile: &str, identity: &str, envelope: &[u8]) -> Result<HydraMailboxResult, String> {
    let ack = GhostDeliveryAck::decode(envelope)?; ghost_kaspa::verify_delivery_ack(&ack)?;
    if ack.signer_hydra_id == identity || ack.destination_hydra_id != identity { return Ok(discard()); }
    let route = session::with(profile, |runtime| Ok(runtime.routes.get(&ack.signer_hydra_id).cloned()))?;
    let Some(route) = route.filter(|route| route.kaspa_address == ack.signer_kaspa_address) else { return Ok(discard()); };
    session::with_mut(profile, |runtime| { runtime.pending_outbound.remove(&ack.signer_hydra_id); runtime.prepared_completion.remove(&ack.signer_hydra_id); if let Some(binding) = runtime.sessions.get_mut(&ack.signer_hydra_id) { binding.state = SessionState::Active; } Ok(()) })?;
    Ok(HydraMailboxResult { peer_address: Some(route.kaspa_address), peer_label: Some(route.display_name), delivery_ack: Some(ack.message_id), delivery_ack_peer: Some(ack.signer_hydra_id.clone()), session_established_peer: Some(ack.signer_hydra_id), ..discard() })
}

fn receive_call_signal(profile: &str, identity: &str, local: &[String], envelope: &[u8]) -> Result<HydraMailboxResult, String> {
    let signal = ghost_protocol::GhostCallSignal::decode(envelope)?;
    if signal.sender.hydra_identity_id == identity || !local.iter().any(|address| address == &signal.recipient_kaspa_address) { return Ok(discard()); }
    ghost_kaspa::verify_call_signal(&signal)?;
    let card = BASE64.decode(&signal.sender.hydra_contact_card_b64).map_err(|_| "call-signal HYDRA contact card is not valid base64".to_string())?;
    let peer = with_hydra_runtime(profile, |hydra| hydra.preview_contact(&card).map(|contact| contact.handle))?;
    if peer != signal.sender.hydra_identity_id { return Err("call-signal HYDRA identity does not match its authenticated contact card".into()); }
    Ok(HydraMailboxResult { call_signal: Some(HydraCallSignalProjection { signal_id: signal.signal_id, call_id: signal.call_id, action: signal.action, peer_address: signal.sender.kaspa_address, local_address: signal.recipient_kaspa_address, peer_label: signal.sender.display_name, peer_hydra_id: peer }), ..discard() })
}

fn receive_session_end(profile: &str, identity: &str, local: &[String], envelope: &[u8]) -> Result<HydraMailboxResult, String> {
    let end = KktpSessionEnd::decode(envelope)?;
    ghost_kaspa::verify_kktp_session_end(&end)?;
    if !session_end_targets_local(&end, identity, local)? {
        return Ok(discard());
    }
    verify_session_end_signature(profile, &end)?;
    let route = session::with(profile, |runtime| Ok(runtime.routes.get(&end.sender_hydra_id).cloned()))?;
    verify_session_end_route(&end, route.as_ref())?;
    handshake::reset_peer_crypto(profile, &end.sender_hydra_id)?;
    session::with_mut(profile, |runtime| {
        runtime.sessions.remove(&end.sender_hydra_id);
        runtime.blocked.insert(end.sender_hydra_id.clone());
        Ok(())
    })?;
    Ok(HydraMailboxResult { peer_address: Some(end.sender_kaspa_address.clone()), peer_label: route.map(|route| route.display_name), session_ended: Some(HydraSessionEndedProjection { peer_hydra_id: end.sender_hydra_id, peer_address: end.sender_kaspa_address, sid: end.sid, reason: end.reason }), ..discard() })
}


fn session_end_targets_local(end: &KktpSessionEnd, identity: &str, local: &[String]) -> Result<bool, String> {
    if !local.iter().any(|address| address == &end.recipient_kaspa_address) || end.sender_hydra_id == identity {
        return Ok(false);
    }
    if end.initiator_hydra_id != identity && end.responder_hydra_id != identity {
        return Err("KKTP session_end does not name this HYDRA identity".into());
    }
    Ok(true)
}

fn verify_session_end_signature(profile: &str, end: &KktpSessionEnd) -> Result<(), String> {
    let sig = BASE64.decode(&end.pq_sig_b64)
        .map_err(|_| "KKTP session_end PQ signature is not valid base64".to_string())?;
    with_hydra_runtime(profile, |hydra| {
        hydra.verify_contact_application_context(
            &end.sender_hydra_id,
            &end.pq_signing_bytes()?,
            &sig,
        )
    })
}

fn verify_session_end_route(end: &KktpSessionEnd, route: Option<&session::Route>) -> Result<(), String> {
    if route.is_some_and(|route| route.kaspa_address != end.sender_kaspa_address) {
        return Err("KKTP session_end Kaspa signer does not match the authenticated peer route".into());
    }
    Ok(())
}


fn route_result(route: Option<session::Route>, message_id: String, discard_value: bool) -> Result<HydraMailboxResult, String> {
    Ok(HydraMailboxResult { peer_address: route.as_ref().map(|route| route.kaspa_address.clone()), peer_label: route.as_ref().map(|route| route.display_name.clone()), message_id: Some(message_id), discard: discard_value, ..Default::default() })
}
