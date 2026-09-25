use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::MailboxSendResult;
use ghost_protocol::{GhostContactAccept, GhostDeliveryAck, GhostReactionEvent, KktpFirstMessage, KktpInnerMessage, KktpMailboxMessage, GHOST_KKTP_VERSION};
use serde_json::Value;
use zeroize::Zeroize;

use super::{
    secure_transport::with_hydra_runtime,
    handshake,
    mailbox_common::{frame, stego, BrowserWallet},
    session::{self, Binding, PendingSend, Role, SessionState},
};
use super::super::support::util::{required, required_str, to_value};

pub(in crate::native::browser_host) async fn invoke(command: &str, args: &Value) -> Result<Value, String> {
    let sent = match command {
        "mailbox_send_contact_request" => super::mailbox::send_contact_request(args).await?,
        "mailbox_send_contact_accept" | "mailbox_send_message" | "mailbox_send_control" | "mailbox_send_recovery_offer" | "mailbox_send_realtime_carrier" => {
            invoke_primary(command, args).await?
        }
        _ => invoke_secondary(command, args).await?,
    };
    to_value(sent)
}

async fn invoke_primary(command: &str, args: &Value) -> Result<MailboxSendResult, String> {
    match command {
        "mailbox_send_contact_accept" => send_contact_accept(args).await,
        "mailbox_send_message" => send_message(args).await,
        "mailbox_send_control" => send_control(args).await,
        "mailbox_send_recovery_offer" => send_recovery_offer(args).await,
        "mailbox_send_realtime_carrier" => send_realtime_carrier(args).await,
        _ => Err(format!("unknown primary browser mailbox send command: {command}")),
    }
}

async fn invoke_secondary(command: &str, args: &Value) -> Result<MailboxSendResult, String> {
    match command {
        "mailbox_send_delivery_ack" => send_delivery_ack(args).await,
        "mailbox_retry_handshake_finish" => retry_handshake_finish(args).await,
        "mailbox_send_session_end" => super::mailbox_control::send_session_end(args).await,
        "mailbox_send_call_signal" => super::mailbox_control::send_call_signal(args).await,
        _ => Err(format!("unknown browser mailbox send command: {command}")),
    }
}

pub(in crate::native::browser_host) async fn send_contact_accept(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let profile = wallet.profile_id.clone();
    let identity = required_str(args, "identityId")?;
    ensure_identity(&profile, identity)?;
    let encoded = hex::decode(required_str(args, "signedRequestHex")?).map_err(|_| "signed Ghost Talk contact request is not valid hex".to_string())?;
    let request = ghost_protocol::GhostContactRequest::decode(&encoded)?;
    ghost_kaspa::verify_contact_request(&request)?;
    let local = wallet.public.receive_addresses.iter().chain(wallet.public.change_addresses.iter()).any(|address| address == &request.recipient_kaspa_address);
    if !local { return Err("contact request is not addressed to this Ghost Talk wallet".into()); }
    let card = BASE64.decode(&request.sender.hydra_contact_card_b64).map_err(|_| "contact-request HYDRA contact card is not valid base64".to_string())?;
    let peer = with_hydra_runtime(&profile, |hydra| {
        let preview = hydra.preview_contact(&card)?;
        if preview.handle != request.sender.hydra_identity_id { return Err("contact-request HYDRA identity does not match its authenticated contact card".into()); }
        Ok(hydra.add_contact(&card)?.handle)
    })?;
    handshake::reset_peer_crypto(&profile, &peer)?;
    session::install(&profile, &peer, request.request_id.clone(), Role::Responder, SessionState::Discovered)?;
    session::remember_route(&profile, &peer, request.sender.kaspa_address.clone(), request.sender.display_name.clone(), request.request_id.clone(), Role::Responder)?;
    let descriptor = private_descriptor(&profile, identity, &wallet, required_str(args, "senderDisplayName")?)?;
    let destination = request.sender.kaspa_address.clone();
    let acceptor = request.recipient_kaspa_address.clone();
    let mut accepted = GhostContactAccept { version: GHOST_KKTP_VERSION, request_id: request.request_id, recipient_kaspa_address: destination.clone(), acceptor_kaspa_address: acceptor.clone(), responder: descriptor, signature_hex: String::new() };
    let mut key = ghost_kaspa::wallet::private_key_for_address(&wallet.secret, &wallet.public, &acceptor)?;
    let signed = ghost_kaspa::sign_contact_accept(&mut accepted, &key); key.zeroize(); signed?;
    wallet.send(args, &destination, frame(&accepted.encode()?)?, true, None).await
}

pub(in crate::native::browser_host) async fn send_message(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let profile = wallet.profile_id.clone();
    let identity = required_str(args, "identityId")?;
    let peer = required_str(args, "contactId")?;
    let destination = required_str(args, "destination")?;
    ensure_identity(&profile, identity)?;
    validate_route(&profile, peer, destination)?;
    let body = required_str(args, "body")?;
    let message_id = required_str(args, "messageId")?;
    ghost_protocol::validate_kktp_message_id(message_id)?;
    let profile_name = required_str(args, "stegoProfile")?;
    let reaction: Option<GhostReactionEvent> = args.get("reaction").filter(|value| !value.is_null()).map(|value| serde_json::from_value(value.clone()).map_err(|error| error.to_string())).transpose()?;
    let allow = args.get("allowHandshake").and_then(Value::as_bool).unwrap_or(false);
    let status = with_hydra_runtime(&profile, |hydra| hydra.session_status(peer))?;
    if status == "active" && active_binding(&profile, peer).is_ok() {
        let wire = seal_message(&profile, peer, message_id, body, reaction.as_ref(), profile_name)?;
        return wallet.send(args, destination, frame(&wire.encode()?)?, false, None).await;
    }
    if !allow { return Err("Secure peer transport is not active; no new handshake was started".into()); }
    prepare_handshake_send(args, wallet, peer, destination, body, message_id, profile_name).await
}

async fn prepare_handshake_send(args: &Value, mut wallet: BrowserWallet, peer: &str, destination: &str, body: &str, message_id: &str, profile_name: &str) -> Result<MailboxSendResult, String> {
    if !with_hydra_runtime(&wallet.profile_id, |hydra| hydra.has_contact(peer))? { return Err("The recipient has not accepted this Ghost Talk chat yet".into()); }
    if with_hydra_runtime(&wallet.profile_id, |hydra| hydra.session_status(peer))? == "pending" { with_hydra_runtime(&wallet.profile_id, |hydra| hydra.abort_handshake(peer))?; }
    let local = session::with(&wallet.profile_id, |runtime| Ok(runtime.identity_id.clone()))?;
    let sid = session::with(&wallet.profile_id, |runtime| {
        let route = runtime.routes.get(peer).ok_or_else(|| "secure peer has no verified Kaspa route".to_string())?;
        if route.resume_required && route.session_role == Some(Role::Initiator) {
            let prior = route.session_sid.as_deref().ok_or_else(|| "restart route is missing its prior KKTP SID".to_string())?;
            handshake::successor_sid(prior, &local, peer)
        } else if let Some(binding) = runtime.sessions.get(peer).filter(|binding| binding.role == Role::Initiator && binding.state == SessionState::Discovered) {
            Ok(binding.sid.clone())
        } else { Ok(ghost_core::Id128::new_random().to_string()) }
    })?;
    handshake::reset_peer_crypto(&wallet.profile_id, peer)?;
    let binding = session::install(&wallet.profile_id, peer, sid.clone(), Role::Initiator, SessionState::Handshake)?;
    let offer = with_hydra_runtime(&wallet.profile_id, |hydra| hydra.init_handshake(peer))?;
    let carrier = handshake::control(&wallet.profile_id, &binding, "pq_init", &offer, None)?;
    let payloads = frame(&carrier)?;
    let pending_id = ghost_core::Id128::new_random().to_string();
    session::with_mut(&wallet.profile_id, |runtime| {
        runtime.pending_outbound.insert(peer.to_owned(), PendingSend { id: pending_id.clone(), sid, destination: destination.to_owned(), body: body.to_owned(), message_id: message_id.to_owned(), stego_profile: profile_name.to_owned() });
        Ok(())
    })?;
    wallet.send(args, destination, payloads, true, Some(pending_id)).await
}

pub(in crate::native::browser_host) fn first_message(profile: &str, peer: &str, message_id: &str, body: &str, profile_name: &str) -> Result<KktpFirstMessage, String> {
    let wire = seal_message(profile, peer, message_id, body, None, profile_name)?;
    let envelope = BASE64.decode(&wire.ciphertext_b64).map_err(|_| "KKTP ciphertext is not valid base64".to_string())?;
    Ok(KktpFirstMessage { direction: wire.direction, mailbox_id: wire.mailbox_id, message_id: wire.message_id, profile: wire.profile, seq: wire.seq, sender_hydra_id: wire.sender_hydra_id, envelope_b64: BASE64.encode(envelope) })
}

fn seal_message(profile: &str, peer: &str, message_id: &str, body: &str, reaction: Option<&GhostReactionEvent>, profile_name: &str) -> Result<KktpMailboxMessage, String> {
    let binding = active_binding(profile, peer)?;
    let direction = binding.role.outbound();
    let inner = match reaction { Some(reaction) => KktpInnerMessage::reaction(binding.sid.clone(), binding.mailbox_id.clone(), direction, binding.send_seq, message_id.to_owned(), reaction.clone()), None => KktpInnerMessage::text(binding.sid.clone(), binding.mailbox_id.clone(), direction, binding.send_seq, message_id.to_owned(), body.to_owned()) };
    let envelopes = with_hydra_runtime(profile, |hydra| hydra.send(peer, &inner.encode()?, stego(profile_name)?))?;
    if envelopes.len() != 1 { return Err("HYDRA produced multiple logical envelopes for one KKTP message".into()); }
    let envelope = envelopes.into_iter().next().ok_or_else(|| "HYDRA produced no KKTP envelope".to_string())?;
    session::with_mut(profile, |runtime| { let current = runtime.sessions.get_mut(peer).ok_or_else(|| "KKTP binding disappeared during send".to_string())?; current.send_seq = current.send_seq.checked_add(1).ok_or_else(|| "KKTP outbound sequence exhausted".to_string())?; Ok(()) })?;
    let sender = session::with(profile, |runtime| Ok(runtime.identity_id.clone()))?;
    Ok(KktpMailboxMessage { kind: "msg".into(), version: GHOST_KKTP_VERSION, sid: binding.sid, mailbox_id: binding.mailbox_id, direction, seq: binding.send_seq, sender_hydra_id: sender, message_id: message_id.to_owned(), profile: profile_name.to_owned(), ciphertext_b64: BASE64.encode(envelope) })
}

fn active_binding(profile: &str, peer: &str) -> Result<Binding, String> { session::with(profile, |runtime| { let binding = runtime.sessions.get(peer).cloned().ok_or_else(|| "KKTP session binding is missing for this peer".to_string())?; if binding.state != SessionState::Active { return Err("KKTP session is not active for this peer".into()); } Ok(binding) }) }

async fn send_control(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let destination = required_str(args, "destination")?.to_owned();
    let values: Vec<String> = required(args, "payloadsHex")?;
    let payloads = super::mailbox_common::decode_payloads(&values)?;
    let pending = args.get("completesPendingId").and_then(Value::as_str).map(str::to_owned);
    let sent = wallet.send(args, &destination, payloads, false, None).await?;
    if let Some(pending_id) = pending { session::with_mut(&wallet.profile_id, |runtime| { runtime.pending_outbound.retain(|_, pending| pending.id != pending_id); Ok(()) })?; }
    Ok(sent)
}

async fn send_recovery_offer(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let profile = wallet.profile_id.clone();
    let peer = required_str(args, "contactId")?;
    let destination = required_str(args, "destination")?;
    let offer = hex::decode(required_str(args, "offerHex")?).map_err(|_| "HYDRA recovery offer is not valid hex".to_string())?;
    let binding = session::with(&profile, |runtime| runtime.sessions.get(peer).cloned().ok_or_else(|| "KKTP recovery session binding is missing".to_string()))?;
    let carrier = handshake::control(&profile, &binding, "pq_init", &offer, None)?;
    wallet.send(args, destination, frame(&carrier)?, true, None).await
}

async fn retry_handshake_finish(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let identity = required_str(args, "identityId")?;
    let peer = required_str(args, "contactId")?;
    let message_id = required_str(args, "messageId")?;
    ghost_protocol::validate_kktp_message_id(message_id)?;
    ensure_identity(&wallet.profile_id, identity)?;
    let prepared = session::with(&wallet.profile_id, |runtime| {
        runtime.prepared_completion.get(peer).cloned().ok_or_else(||
            "no peer-unacknowledged KKTP FINISH is retained for retry".to_string())
    })?;
    if prepared.message_id != message_id {
        return Err("retained KKTP FINISH does not match this chat message".into());
    }
    let payloads = super::mailbox_common::decode_payloads(&prepared.payloads_hex)?;
    wallet.send(args, &prepared.destination, payloads, true, None).await
}

async fn send_delivery_ack(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let identity = required_str(args, "identityId")?;
    let peer = required_str(args, "destinationHydraId")?;
    let destination = required_str(args, "destination")?;
    validate_route(&wallet.profile_id, peer, destination)?;
    let mut ack = GhostDeliveryAck { version: 1, signer_kaspa_address: wallet.public.receive_addresses.first().cloned().ok_or_else(|| "wallet has no stable Ghost Talk receive address".to_string())?, signer_hydra_id: identity.to_owned(), destination_hydra_id: peer.to_owned(), message_id: required_str(args, "messageId")?.to_owned(), signature_hex: String::new() };
    let mut key = ghost_kaspa::wallet::receive_private_key(&wallet.secret, 0)?; let result = ghost_kaspa::sign_delivery_ack(&mut ack, &key); key.zeroize(); result?;
    wallet.send(args, destination, frame(&ack.encode()?)?, false, None).await
}

pub(in crate::native::browser_host) fn private_descriptor(profile: &str, identity: &str, wallet: &BrowserWallet, label: &str) -> Result<ghost_protocol::GhostContactDescriptor, String> {
    let card = with_hydra_runtime(profile, |hydra| hydra.contact_card())?;
    ghost_kaspa::build_private_descriptor(&wallet.secret, &wallet.public, &card, identity, label)
}


pub(in crate::native::browser_host) async fn send_realtime_carrier(
    args: &Value,
) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let profile = wallet.profile_id.clone();
    let peer = required_str(args, "contactId")?;
    let destination = required_str(args, "destination")?;
    validate_route(&profile, peer, destination)?;
    let carrier = BASE64
        .decode(required_str(args, "carrierB64")?)
        .map_err(|_| "GTR1 realtime carrier is not valid base64".to_string())?;
    let decoded = ghost_protocol::Gtr1Envelope::decode(&carrier).map_err(|error| error.to_string())?;
    let (identity, sid) = session::with(&profile, |runtime| {
        let binding = runtime
            .sessions
            .get(peer)
            .ok_or_else(|| "realtime carrier has no active KKTP session".to_string())?;
        if binding.state != SessionState::Active {
            return Err("realtime carrier requires an active KKTP session".into());
        }
        Ok((runtime.identity_id.clone(), binding.sid.clone()))
    })?;
    if decoded.sender_hex() != identity || decoded.sid_hex() != sid {
        return Err("GTR1 realtime carrier does not match the active authenticated session".into());
    }
    wallet.send(args, destination, frame(&carrier)?, true, None).await
}

pub(in crate::native::browser_host) fn ensure_identity(profile: &str, identity: &str) -> Result<(), String> { session::with(profile, |runtime| if runtime.identity_id == identity { Ok(()) } else { Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into()) }) }

pub(in crate::native::browser_host) fn validate_route(profile: &str, peer: &str, destination: &str) -> Result<(), String> { session::with(profile, |runtime| { let route = runtime.routes.get(peer).ok_or_else(|| "secure peer has no verified Kaspa route".to_string())?; if route.kaspa_address != destination { return Err("mailbox destination does not match the verified peer route".into()); } if runtime.blocked.contains(peer) { return Err("secure peer transport is blocked".into()); } Ok(()) }) }
