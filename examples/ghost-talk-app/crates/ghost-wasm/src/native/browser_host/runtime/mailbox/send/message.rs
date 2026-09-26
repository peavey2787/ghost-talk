use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::MailboxSendResult;
use ghost_protocol::{
    GhostReactionEvent, KktpFirstMessage, KktpInnerMessage, KktpMailboxMessage, GHOST_KKTP_VERSION,
};
use serde_json::Value;

use super::{active_binding, ensure_identity, validate_route};
use crate::native::browser_host::{
    runtime::{
        handshake,
        mailbox::common::{frame, stego, BrowserWallet},
        secure_transport::with_hydra_runtime,
        session::{self, Binding, PendingSend, Role, SessionState},
    },
    support::util::required_str,
};

/// Text or reaction to send inside one KKTP message.
struct Outgoing<'a> {
    peer: &'a str,
    destination: &'a str,
    body: &'a str,
    message_id: &'a str,
    profile_name: &'a str,
}

pub(super) async fn send_message(args: &Value) -> Result<MailboxSendResult, String> {
    let mut wallet = BrowserWallet::open(args)?;
    let profile = wallet.profile_id.clone();
    ensure_identity(&profile, required_str(args, "identityId")?)?;
    let outgoing = Outgoing {
        peer: required_str(args, "contactId")?,
        destination: required_str(args, "destination")?,
        body: required_str(args, "body")?,
        message_id: required_str(args, "messageId")?,
        profile_name: required_str(args, "stegoProfile")?,
    };
    validate_route(&profile, outgoing.peer, outgoing.destination)?;
    ghost_protocol::validate_kktp_message_id(outgoing.message_id)?;
    let status = with_hydra_runtime(&profile, |hydra| hydra.session_status(outgoing.peer))?;
    if status == "active" && active_binding(&profile, outgoing.peer).is_ok() {
        let reaction = message_reaction(args)?;
        let wire = seal_message(&profile, &outgoing, reaction.as_ref())?;
        return wallet
            .send(
                args,
                outgoing.destination,
                frame(&wire.encode()?)?,
                false,
                None,
            )
            .await;
    }
    if !args
        .get("allowHandshake")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err("Secure peer transport is not active; no new handshake was started".into());
    }
    prepare_handshake_send(args, wallet, &outgoing).await
}

fn message_reaction(args: &Value) -> Result<Option<GhostReactionEvent>, String> {
    args.get("reaction")
        .filter(|value| !value.is_null())
        .map(|value| serde_json::from_value(value.clone()).map_err(|error| error.to_string()))
        .transpose()
}

async fn prepare_handshake_send(
    args: &Value,
    mut wallet: BrowserWallet,
    outgoing: &Outgoing<'_>,
) -> Result<MailboxSendResult, String> {
    let profile = wallet.profile_id.clone();
    let peer = outgoing.peer;
    if !with_hydra_runtime(&profile, |hydra| hydra.has_contact(peer))? {
        return Err("The recipient has not accepted this Ghost Talk chat yet".into());
    }
    if with_hydra_runtime(&profile, |hydra| hydra.session_status(peer))? == "pending" {
        with_hydra_runtime(&profile, |hydra| hydra.abort_handshake(peer))?;
    }
    let sid = handshake_sid(&profile, peer)?;
    handshake::reset_peer_crypto(&profile, peer)?;
    let binding = session::install(
        &profile,
        peer,
        sid.clone(),
        Role::Initiator,
        SessionState::Handshake,
    )?;
    let offer = with_hydra_runtime(&profile, |hydra| hydra.init_handshake(peer))?;
    let carrier = handshake::control(&profile, &binding, "pq_init", &offer, None)?;
    let pending_id = remember_pending(&profile, sid, outgoing)?;
    wallet
        .send(
            args,
            outgoing.destination,
            frame(&carrier)?,
            true,
            Some(pending_id),
        )
        .await
}

/// Restart successors derive their SID; a discovered initiator reuses its
/// consent SID; everything else starts fresh.
fn handshake_sid(profile: &str, peer: &str) -> Result<String, String> {
    session::with(profile, |runtime| {
        let route = runtime
            .routes
            .get(peer)
            .ok_or_else(|| "secure peer has no verified Kaspa route".to_string())?;
        if route.resume_required && route.session_role == Some(Role::Initiator) {
            let prior = route
                .session_sid
                .as_deref()
                .ok_or_else(|| "restart route is missing its prior KKTP SID".to_string())?;
            return handshake::successor_sid(prior, &runtime.identity_id, peer);
        }
        let discovered = runtime.sessions.get(peer).filter(|binding| {
            binding.role == Role::Initiator && binding.state == SessionState::Discovered
        });
        Ok(discovered
            .map(|binding| binding.sid.clone())
            .unwrap_or_else(|| ghost_core::Id128::new_random().to_string()))
    })
}

fn remember_pending(profile: &str, sid: String, outgoing: &Outgoing<'_>) -> Result<String, String> {
    let pending_id = ghost_core::Id128::new_random().to_string();
    let pending = PendingSend {
        id: pending_id.clone(),
        sid,
        destination: outgoing.destination.to_owned(),
        body: outgoing.body.to_owned(),
        message_id: outgoing.message_id.to_owned(),
        stego_profile: outgoing.profile_name.to_owned(),
    };
    session::with_mut(profile, |runtime| {
        runtime
            .pending_outbound
            .insert(outgoing.peer.to_owned(), pending);
        Ok(())
    })?;
    Ok(pending_id)
}

pub(in crate::native::browser_host) fn first_message(
    profile: &str,
    peer: &str,
    message_id: &str,
    body: &str,
    profile_name: &str,
) -> Result<KktpFirstMessage, String> {
    let outgoing = Outgoing {
        peer,
        destination: "",
        body,
        message_id,
        profile_name,
    };
    let wire = seal_message(profile, &outgoing, None)?;
    let envelope = BASE64
        .decode(&wire.ciphertext_b64)
        .map_err(|_| "KKTP ciphertext is not valid base64".to_string())?;
    Ok(KktpFirstMessage {
        direction: wire.direction,
        mailbox_id: wire.mailbox_id,
        message_id: wire.message_id,
        profile: wire.profile,
        seq: wire.seq,
        sender_hydra_id: wire.sender_hydra_id,
        envelope_b64: BASE64.encode(envelope),
    })
}

fn seal_message(
    profile: &str,
    outgoing: &Outgoing<'_>,
    reaction: Option<&GhostReactionEvent>,
) -> Result<KktpMailboxMessage, String> {
    let binding = active_binding(profile, outgoing.peer)?;
    let direction = binding.role.outbound();
    let inner = inner_message(&binding, outgoing, reaction);
    let envelopes = with_hydra_runtime(profile, |hydra| {
        hydra.send(
            outgoing.peer,
            &inner.encode()?,
            stego(outgoing.profile_name)?,
        )
    })?;
    let [envelope] = <[Vec<u8>; 1]>::try_from(envelopes).map_err(|_| {
        "HYDRA must produce exactly one logical envelope per KKTP message".to_string()
    })?;
    advance_send_sequence(profile, outgoing.peer)?;
    let sender = session::with(profile, |runtime| Ok(runtime.identity_id.clone()))?;
    Ok(KktpMailboxMessage {
        kind: "msg".into(),
        version: GHOST_KKTP_VERSION,
        sid: binding.sid,
        mailbox_id: binding.mailbox_id,
        direction,
        seq: binding.send_seq,
        sender_hydra_id: sender,
        message_id: outgoing.message_id.to_owned(),
        profile: outgoing.profile_name.to_owned(),
        ciphertext_b64: BASE64.encode(envelope),
    })
}

fn inner_message(
    binding: &Binding,
    outgoing: &Outgoing<'_>,
    reaction: Option<&GhostReactionEvent>,
) -> KktpInnerMessage {
    let (sid, mailbox) = (binding.sid.clone(), binding.mailbox_id.clone());
    let direction = binding.role.outbound();
    let message_id = outgoing.message_id.to_owned();
    match reaction {
        Some(reaction) => KktpInnerMessage::reaction(
            sid,
            mailbox,
            direction,
            binding.send_seq,
            message_id,
            reaction.clone(),
        ),
        None => KktpInnerMessage::text(
            sid,
            mailbox,
            direction,
            binding.send_seq,
            message_id,
            outgoing.body.to_owned(),
        ),
    }
}

fn advance_send_sequence(profile: &str, peer: &str) -> Result<(), String> {
    session::with_mut(profile, |runtime| {
        let current = runtime
            .sessions
            .get_mut(peer)
            .ok_or_else(|| "KKTP binding disappeared during send".to_string())?;
        current.send_seq = current
            .send_seq
            .checked_add(1)
            .ok_or_else(|| "KKTP outbound sequence exhausted".to_string())?;
        Ok(())
    })
}
