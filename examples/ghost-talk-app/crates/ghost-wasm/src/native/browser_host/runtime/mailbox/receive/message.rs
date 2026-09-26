use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::{HydraMailboxResult, HydraRecoveryProjection};
use ghost_hydra::ReceivedProjection;
use ghost_protocol::{KktpFirstMessage, KktpMailboxMessage};

use super::route_result;
use crate::native::browser_host::runtime::{
    handshake,
    mailbox::common::{discard, stego},
    secure_transport::with_hydra_runtime,
    session::{self, Role, SessionState},
};

pub(super) fn receive_message(
    profile: &str,
    wire: KktpMailboxMessage,
) -> Result<HydraMailboxResult, String> {
    let peer = wire.sender_hydra_id.clone();
    let route = session::with(profile, |runtime| Ok(runtime.routes.get(&peer).cloned()))?;
    let binding = match message_binding(profile, &peer, route.clone(), &wire)? {
        MessageBinding::Ready(binding) => binding,
        MessageBinding::Return(result) => return Ok(*result),
    };
    let inner = decrypt_message(profile, &peer, &binding, &wire)?;
    advance_receive_sequence(profile, &peer, wire.seq)?;
    let projected = project_inner(profile, &peer, &wire.sid, inner)?;
    let route = session::with(profile, |runtime| Ok(runtime.routes.get(&peer).cloned()))?;
    Ok(HydraMailboxResult {
        received: projected,
        peer_address: route.as_ref().map(|route| route.kaspa_address.clone()),
        peer_label: route.as_ref().map(|route| route.display_name.clone()),
        message_id: Some(wire.message_id),
        ..discard()
    })
}

enum MessageBinding {
    Ready(session::Binding),
    Return(Box<HydraMailboxResult>),
}

fn message_binding(
    profile: &str,
    peer: &str,
    route: Option<session::Route>,
    wire: &KktpMailboxMessage,
) -> Result<MessageBinding, String> {
    let binding = session::with(profile, |runtime| Ok(runtime.sessions.get(peer).cloned()))?;
    let Some(binding) = binding else {
        return Ok(MessageBinding::Return(Box::new(recover_missing_binding(
            profile,
            peer,
            route,
            wire.message_id.clone(),
        )?)));
    };
    if binding.sid != wire.sid
        || binding.mailbox_id != wire.mailbox_id
        || binding.inbound() != wire.direction
    {
        return Ok(MessageBinding::Return(Box::new(discard())));
    }
    if binding.state != SessionState::Active {
        return Ok(MessageBinding::Return(Box::new(route_result(
            route,
            wire.message_id.clone(),
            false,
        )?)));
    }
    if wire.seq < binding.recv_next_seq {
        return Ok(MessageBinding::Return(Box::new(discard())));
    }
    if wire.seq > binding.recv_next_seq {
        return Err(format!(
            "KKTP sequence gap: expected {}, received {}",
            binding.recv_next_seq, wire.seq
        ));
    }
    Ok(MessageBinding::Ready(binding))
}

fn decrypt_message(
    profile: &str,
    peer: &str,
    binding: &session::Binding,
    wire: &KktpMailboxMessage,
) -> Result<ghost_protocol::KktpInnerMessage, String> {
    let encrypted = BASE64
        .decode(&wire.ciphertext_b64)
        .map_err(|_| "KKTP HYDRA ciphertext is not valid base64".to_string())?;
    let received = with_hydra_runtime(profile, |hydra| {
        hydra.receive(&encrypted, stego(&wire.profile)?)
    })?
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
        return Err(
            "KKTP encrypted inner metadata does not match the authenticated outer record".into(),
        );
    }
    Ok(())
}

fn advance_receive_sequence(profile: &str, peer: &str, seq: u64) -> Result<(), String> {
    let next = seq
        .checked_add(1)
        .ok_or_else(|| "KKTP inbound sequence exhausted".to_string())?;
    session::with_mut(profile, |runtime| {
        if let Some(binding) = runtime.sessions.get_mut(peer) {
            binding.recv_next_seq = next;
        }
        Ok(())
    })
}

fn recover_missing_binding(
    profile: &str,
    peer: &str,
    route: Option<session::Route>,
    message_id: String,
) -> Result<HydraMailboxResult, String> {
    let route = route.ok_or_else(|| "KKTP message has no verified Kaspa peer route".to_string())?;
    if route.resume_required && route.session_role == Some(Role::Responder) {
        return route_result(Some(route), message_id, false);
    }
    if with_hydra_runtime(profile, |hydra| hydra.session_status(peer))? == "pending" {
        return route_result(Some(route), message_id, false);
    }
    let local = session::with(profile, |runtime| Ok(runtime.identity_id.clone()))?;
    let sid = if route.resume_required && route.session_role == Some(Role::Initiator) {
        handshake::successor_sid(
            route
                .session_sid
                .as_deref()
                .ok_or_else(|| "restart route is missing its prior KKTP SID".to_string())?,
            &local,
            peer,
        )?
    } else {
        ghost_core::Id128::new_random().to_string()
    };
    handshake::reset_peer_crypto(profile, peer)?;
    let offer = with_hydra_runtime(profile, |hydra| hydra.init_handshake(peer))?;
    session::install(
        profile,
        peer,
        sid.clone(),
        Role::Initiator,
        SessionState::Handshake,
    )?;
    Ok(HydraMailboxResult {
        recovery: Some(HydraRecoveryProjection {
            destination: route.kaspa_address.clone(),
            offer_hex: hex::encode(offer),
            sid,
            peer_hydra_id: peer.to_owned(),
        }),
        peer_address: Some(route.kaspa_address),
        peer_label: Some(route.display_name),
        message_id: Some(message_id),
        discard: false,
        ..Default::default()
    })
}

pub(in crate::native::browser_host) fn open_first(
    profile: &str,
    peer: &str,
    sid: &str,
    first: KktpFirstMessage,
) -> Result<Option<ReceivedProjection>, String> {
    let binding = session::with(profile, |runtime| {
        runtime
            .sessions
            .get(peer)
            .cloned()
            .ok_or_else(|| "KKTP responder binding disappeared".to_string())
    })?;
    if binding.sid != sid
        || binding.mailbox_id != first.mailbox_id
        || binding.inbound() != first.direction
        || first.seq != binding.recv_next_seq
        || first.sender_hydra_id != peer
    {
        return Err("KKTP first-message metadata does not match the authenticated session".into());
    }
    let envelope = BASE64
        .decode(&first.envelope_b64)
        .map_err(|_| "KKTP first-message envelope is not valid base64".to_string())?;
    let received = with_hydra_runtime(profile, |hydra| {
        hydra.receive(&envelope, stego(&first.profile)?)
    })?
    .ok_or_else(|| "HYDRA did not yield the authenticated first message".to_string())?;
    if received.from != peer {
        return Err("HYDRA first-message sender does not match the KKTP peer".into());
    }
    let inner = ghost_protocol::KktpInnerMessage::decode(received.plaintext.as_bytes())?;
    if inner.sid != sid
        || inner.mailbox_id != first.mailbox_id
        || inner.direction != first.direction
        || inner.seq != first.seq
        || inner.message_id != first.message_id
    {
        return Err("KKTP first-message inner metadata mismatch".into());
    }
    session::with_mut(profile, |runtime| {
        if let Some(binding) = runtime.sessions.get_mut(peer) {
            binding.recv_next_seq = first.seq + 1;
        }
        Ok(())
    })?;
    project_inner(profile, peer, sid, inner)
}

fn project_inner(
    profile: &str,
    peer: &str,
    sid: &str,
    inner: ghost_protocol::KktpInnerMessage,
) -> Result<Option<ReceivedProjection>, String> {
    match inner.kind.as_str() {
        "msg" => Ok(Some(ReceivedProjection {
            from: peer.into(),
            plaintext: inner.body,
            content_type: None,
            session_sid: Some(sid.into()),
        })),
        "reaction" => {
            Ok(Some(ReceivedProjection {
                from: peer.into(),
                plaintext: serde_json::to_string(&inner.reaction.ok_or_else(|| {
                    "authenticated KKTP reaction metadata is missing".to_string()
                })?)
                .map_err(|error| error.to_string())?,
                content_type: Some("ghost-reaction-v1".into()),
                session_sid: Some(sid.into()),
            }))
        }
        "session_end" => {
            with_hydra_runtime(profile, |hydra| hydra.close_session(peer))?;
            session::with_mut(profile, |runtime| {
                runtime.sessions.remove(peer);
                Ok(())
            })?;
            Ok(None)
        }
        _ => Err("unknown authenticated KKTP inner message type".into()),
    }
}
