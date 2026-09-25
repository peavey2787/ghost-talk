use super::super::{
    mailbox_dispatch::parse_stego_profile,
    runtime_state::HydraProfileRuntime,
    session_state::{persistent_durable_aad, seal_persistent_payload},
    session_types::{
        GhostReactionEvent, KktpInnerMessage, KktpMailboxMessage, KktpRole, KktpSessionState,
        OsRng, BASE64, GHOST_KKTP_VERSION, PERSISTENT_TRANSPORT_MAGIC,
    },
    transport_persistence::{
        persistent_message_key, persistent_next_chain_key, persistent_transport_from_seed,
    },
};
use crate::hydra_commands::reset_kktp_after_unsent_advance;
use base64::Engine as _;
use rand::RngCore;
use zeroize::Zeroize;

pub(crate) fn seal_kktp_message(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
    stego_profile: &str,
) -> Result<(KktpMailboxMessage, Vec<u8>), String> {
    seal_kktp_content(runtime, contact_id, message_id, body, None, stego_profile)
}

pub(crate) fn seal_kktp_reaction(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    reaction: &GhostReactionEvent,
    stego_profile: &str,
) -> Result<(KktpMailboxMessage, Vec<u8>), String> {
    seal_kktp_content(
        runtime,
        contact_id,
        message_id,
        "",
        Some(reaction),
        stego_profile,
    )
}

fn seal_kktp_content(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
    reaction: Option<&GhostReactionEvent>,
    stego_profile: &str,
) -> Result<(KktpMailboxMessage, Vec<u8>), String> {
    let profile = parse_stego_profile(stego_profile)?;
    let hydra_status = runtime.hydra.session_status(contact_id)?;
    let resume_seed_b64 = ensure_persistent_seed(runtime, contact_id, &hydra_status)?;
    let binding = active_binding(runtime, contact_id)?;
    let direction = binding.outbound_direction();
    let seq = binding.send_seq;
    let inner_bytes = encoded_inner(
        &binding,
        direction,
        seq,
        message_id,
        body,
        reaction,
        resume_seed_b64,
    )?;
    let envelope = seal_for_transport(
        runtime,
        contact_id,
        &binding,
        &hydra_status,
        profile,
        direction,
        seq,
        message_id,
        &inner_bytes,
    )?;
    let wire = KktpMailboxMessage {
        kind: "msg".into(),
        version: GHOST_KKTP_VERSION,
        sid: binding.sid.clone(),
        mailbox_id: binding.mailbox_id.clone(),
        direction,
        seq,
        sender_hydra_id: runtime.identity_id.clone(),
        message_id: message_id.to_owned(),
        profile: stego_profile.to_owned(),
        ciphertext_b64: BASE64.encode(&envelope),
    };
    advance_send_sequence(runtime, contact_id, seq)?;
    Ok((wire, envelope))
}

fn ensure_persistent_seed(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    hydra_status: &str,
) -> Result<Option<String>, String> {
    let seed_needed = runtime
        .kktp_sessions
        .get(contact_id)
        .is_some_and(|binding| {
            binding.state == KktpSessionState::Active
                && binding.role == KktpRole::Initiator
                && binding.send_seq == 0
                && binding.persistent_transport.is_none()
        })
        && hydra_status == "active";
    if !seed_needed {
        return Ok(None);
    }
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    if let Some(current) = runtime.kktp_sessions.get_mut(contact_id) {
        current.persistent_transport = Some(persistent_transport_from_seed(&seed, current.role));
    }
    let encoded = BASE64.encode(seed);
    seed.zeroize();
    Ok(Some(encoded))
}

fn active_binding(
    runtime: &HydraProfileRuntime,
    contact_id: &str,
) -> Result<super::super::session_types::KktpSessionBinding, String> {
    let binding = runtime
        .kktp_sessions
        .get(contact_id)
        .cloned()
        .ok_or_else(|| "KKTP session binding is missing for this peer".to_string())?;
    if binding.state != KktpSessionState::Active {
        return Err("KKTP session is not active for this peer".into());
    }
    Ok(binding)
}

fn encoded_inner(
    binding: &super::super::session_types::KktpSessionBinding,
    direction: super::super::session_types::KktpDirection,
    seq: u64,
    message_id: &str,
    body: &str,
    reaction: Option<&GhostReactionEvent>,
    resume_seed_b64: Option<String>,
) -> Result<Vec<u8>, String> {
    let mut inner = match reaction {
        Some(reaction) => KktpInnerMessage::reaction(
            binding.sid.clone(),
            binding.mailbox_id.clone(),
            direction,
            seq,
            message_id.to_owned(),
            reaction.clone(),
        ),
        None => KktpInnerMessage::text(
            binding.sid.clone(),
            binding.mailbox_id.clone(),
            direction,
            seq,
            message_id.to_owned(),
            body.to_owned(),
        ),
    };
    inner.resume_seed_b64 = resume_seed_b64;
    inner.encode()
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit authenticated envelope fields"
)]
fn seal_for_transport(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    binding: &super::super::session_types::KktpSessionBinding,
    hydra_status: &str,
    profile: super::super::session_types::StegoProfile,
    direction: super::super::session_types::KktpDirection,
    seq: u64,
    message_id: &str,
    inner_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    if hydra_status == "active" {
        return seal_with_live_hydra(runtime, contact_id, profile, inner_bytes);
    }
    seal_with_persistent_transport(
        runtime,
        contact_id,
        binding,
        direction,
        seq,
        message_id,
        inner_bytes,
    )
}

fn seal_with_live_hydra(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    profile: super::super::session_types::StegoProfile,
    inner_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let envelopes = runtime.hydra.send(contact_id, inner_bytes, profile)?;
    if envelopes.len() != 1 {
        reset_kktp_after_unsent_advance(runtime, contact_id)?;
        return Err("HYDRA produced multiple logical envelopes for one KKTP direct message; the secure session was reset before publication".into());
    }
    envelopes
        .into_iter()
        .next()
        .ok_or_else(|| "HYDRA produced no KKTP envelope".to_string())
}

fn seal_with_persistent_transport(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    binding: &super::super::session_types::KktpSessionBinding,
    direction: super::super::session_types::KktpDirection,
    seq: u64,
    message_id: &str,
    inner_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let transport = binding.persistent_transport.as_ref().ok_or_else(|| {
        "HYDRA session is unavailable and this chat has no persisted restart transport".to_string()
    })?;
    let aad = persistent_durable_aad(
        &binding.sid,
        &binding.mailbox_id,
        direction,
        seq,
        message_id,
    );
    let mut message_key = persistent_message_key(&transport.send_chain_key);
    let envelope =
        seal_persistent_payload(&message_key, PERSISTENT_TRANSPORT_MAGIC, &aad, inner_bytes);
    message_key.zeroize();
    let envelope = envelope?;
    if let Some(current) = runtime.kktp_sessions.get_mut(contact_id) {
        let persistent = current
            .persistent_transport
            .as_mut()
            .ok_or_else(|| "persistent restart transport disappeared during send".to_string())?;
        persistent.send_chain_key = persistent_next_chain_key(&persistent.send_chain_key);
    }
    Ok(envelope)
}

fn advance_send_sequence(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    seq: u64,
) -> Result<(), String> {
    let next_seq = seq.checked_add(1).ok_or_else(|| {
        "KKTP outbound sequence exhausted; the secure session was reset".to_string()
    });
    let next_seq = match next_seq {
        Ok(value) => value,
        Err(error) => {
            reset_kktp_after_unsent_advance(runtime, contact_id)?;
            return Err(error);
        }
    };
    if let Some(current) = runtime.kktp_sessions.get_mut(contact_id) {
        current.send_seq = next_seq;
    }
    Ok(())
}

mod wire;
pub(crate) use wire::{fragment_kktp_wire, kktp_first_message};
