use super::super::{
    runtime_state::HydraProfileRuntime,
    session_state::{open_persistent_payload, persistent_durable_aad},
    session_types::{
        KktpDirection, KktpInnerMessage, KktpRole, KktpSessionBinding, KktpSessionState,
        ReceivedProjection, BASE64, PERSISTENT_TRANSPORT_MAGIC,
    },
    transport_persistence::{
        persist_transport_state, persistent_message_key, persistent_next_chain_key,
        persistent_transport_from_seed,
    },
};
use super::projection::project_kktp_inner;
use base64::Engine as _;
use zeroize::Zeroize;

pub(crate) fn validate_kktp_binding_header(
    binding: &KktpSessionBinding,
    sid: &str,
    mailbox_id: &str,
    direction: KktpDirection,
    sender_hydra_id: &str,
) -> Result<(), String> {
    [
        (
            binding.sid == sid,
            "KKTP sid does not match the active session",
        ),
        (
            binding.mailbox_id == mailbox_id,
            "KKTP mailbox id does not match the active session",
        ),
        (
            binding.inbound_direction() == direction,
            "KKTP direction does not match the peer role",
        ),
        (
            binding.peer_hydra_id == sender_hydra_id,
            "KKTP sender HYDRA id does not match the bound peer",
        ),
    ]
    .into_iter()
    .find(|(valid, _)| !valid)
    .map(|(_, message)| Err(message.into()))
    .unwrap_or(Ok(()))
}

pub(crate) fn validate_inbound_sequence(
    binding: &KktpSessionBinding,
    seq: u64,
) -> Result<bool, String> {
    if binding.state != KktpSessionState::Active {
        return Err("KKTP message arrived before the session became active".into());
    }
    if seq < binding.recv_next_seq {
        return Ok(false);
    }
    if seq > binding.recv_next_seq {
        return Err(format!(
            "KKTP sequence gap: expected {}, received {seq}",
            binding.recv_next_seq
        ));
    }
    Ok(true)
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit authenticated envelope fields"
)]
pub(crate) fn receive_kktp_inner(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
    sid: &str,
    mailbox_id: &str,
    direction: KktpDirection,
    seq: u64,
    message_id: &str,
    profile_name: &str,
    encrypted_envelope: &[u8],
) -> Result<(KktpInnerMessage, bool), String> {
    if encrypted_envelope.starts_with(PERSISTENT_TRANSPORT_MAGIC) {
        let binding = runtime
            .kktp_sessions
            .get(peer_hydra_id)
            .cloned()
            .ok_or_else(|| "persistent KKTP message has no session binding".to_string())?;
        let transport = binding.persistent_transport.as_ref().ok_or_else(|| {
            "persistent KKTP message arrived without locally restored transport state".to_string()
        })?;
        let aad = persistent_durable_aad(sid, mailbox_id, direction, seq, message_id);
        let mut message_key = persistent_message_key(&transport.recv_chain_key);
        let opened = open_persistent_payload(
            &message_key,
            PERSISTENT_TRANSPORT_MAGIC,
            &aad,
            encrypted_envelope,
        );
        message_key.zeroize();
        let mut plaintext = opened?;
        let decoded = KktpInnerMessage::decode(&plaintext);
        plaintext.zeroize();
        return decoded.map(|inner| (inner, true));
    }
    let profile = parse_stego_profile(profile_name)?;
    let received = runtime
        .hydra
        .receive(encrypted_envelope, profile)?
        .ok_or_else(|| {
            "HYDRA did not yield plaintext for the expected KKTP sequence".to_string()
        })?;
    if received.from != peer_hydra_id {
        return Err("HYDRA sender does not match the KKTP peer binding".into());
    }
    KktpInnerMessage::decode(received.plaintext.as_bytes()).map(|inner| (inner, false))
}

pub(crate) fn validate_kktp_inner_metadata(
    inner: &KktpInnerMessage,
    sid: &str,
    mailbox_id: &str,
    direction: KktpDirection,
    seq: u64,
    message_id: &str,
) -> Result<(), String> {
    let valid = inner.sid == sid
        && inner.mailbox_id == mailbox_id
        && inner.direction == direction
        && inner.seq == seq
        && inner.message_id == message_id;
    if valid {
        Ok(())
    } else {
        Err("KKTP encrypted inner metadata does not match the authenticated outer record".into())
    }
}

pub(crate) fn advance_kktp_receive_sequence(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
    seq: u64,
) -> Result<(), String> {
    let next_seq = seq
        .checked_add(1)
        .ok_or_else(|| "KKTP inbound sequence exhausted".to_string())?;
    if let Some(current) = runtime.kktp_sessions.get_mut(peer_hydra_id) {
        current.recv_next_seq = next_seq;
    }
    Ok(())
}

fn install_received_restart_state(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
    seq: u64,
    inner: &KktpInnerMessage,
    persistent_envelope: bool,
) -> Result<(), String> {
    if persistent_envelope {
        let current = runtime
            .kktp_sessions
            .get_mut(peer_hydra_id)
            .ok_or_else(|| "persistent KKTP binding disappeared during receive".to_string())?;
        let transport = current
            .persistent_transport
            .as_mut()
            .ok_or_else(|| "persistent KKTP receive state disappeared".to_string())?;
        transport.recv_chain_key = persistent_next_chain_key(&transport.recv_chain_key);
        return Ok(());
    }
    let Some(seed_b64) = inner.resume_seed_b64.as_deref() else {
        return Ok(());
    };
    install_first_message_seed(runtime, peer_hydra_id, seq, seed_b64)
}

fn install_first_message_seed(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
    seq: u64,
    seed_b64: &str,
) -> Result<(), String> {
    let mut seed = BASE64
        .decode(seed_b64)
        .map_err(|_| "KKTP persistent-transport seed is not valid base64".to_string())?;
    if seed.len() != 32 {
        seed.zeroize();
        return Err("KKTP persistent-transport seed must be exactly 32 bytes".into());
    }
    let mut seed_array: [u8; 32] = seed
        .as_slice()
        .try_into()
        .map_err(|_| "KKTP persistent-transport seed has invalid length".to_string())?;
    let current = runtime
        .kktp_sessions
        .get_mut(peer_hydra_id)
        .ok_or_else(|| "KKTP binding disappeared while installing restart state".to_string())?;
    if current.role != KktpRole::Responder || seq != 0 {
        seed.zeroize();
        seed_array.zeroize();
        return Err("KKTP persistent-transport seed arrived outside the authenticated first-message transition".into());
    }
    if current.persistent_transport.is_none() {
        current.persistent_transport =
            Some(persistent_transport_from_seed(&seed_array, current.role));
    }
    seed.zeroize();
    seed_array.zeroize();
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "explicit authenticated envelope fields"
)]
pub(crate) fn open_kktp_envelope(
    runtime: &mut HydraProfileRuntime,
    peer_hydra_id: &str,
    sid: &str,
    mailbox_id: &str,
    direction: KktpDirection,
    seq: u64,
    message_id: &str,
    profile_name: &str,
    encrypted_envelope: &[u8],
) -> Result<Option<ReceivedProjection>, String> {
    let binding = runtime
        .kktp_sessions
        .get(peer_hydra_id)
        .cloned()
        .ok_or_else(|| "KKTP session binding is missing for the sender".to_string())?;
    validate_kktp_binding_header(&binding, sid, mailbox_id, direction, peer_hydra_id)?;
    if !validate_inbound_sequence(&binding, seq)? {
        return Ok(None);
    }
    let (inner, persistent_envelope) = receive_kktp_inner(
        runtime,
        peer_hydra_id,
        sid,
        mailbox_id,
        direction,
        seq,
        message_id,
        profile_name,
        encrypted_envelope,
    )?;
    validate_kktp_inner_metadata(&inner, sid, mailbox_id, direction, seq, message_id)?;
    install_received_restart_state(runtime, peer_hydra_id, seq, &inner, persistent_envelope)?;
    advance_kktp_receive_sequence(runtime, peer_hydra_id, seq)?;
    let projected = project_kktp_inner(runtime, peer_hydra_id, sid, inner)?;
    persist_transport_state(runtime)?;
    Ok(projected)
}
use super::super::mailbox_dispatch::parse_stego_profile;
