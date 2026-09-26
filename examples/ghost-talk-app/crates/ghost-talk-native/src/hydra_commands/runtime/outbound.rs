use base64::Engine as _;
use ghost_api::HydraRealtimeEnvelope;
use ghost_protocol::REALTIME_INNER_PREFIX;

use super::super::{
    runtime_owner::HydraRuntimeState,
    runtime_state::HydraProfileRuntime,
    session_message_delivery::{fragment_kktp_wire, seal_kktp_message, seal_kktp_reaction},
    session_state::{persistent_realtime_aad, seal_persistent_payload},
    session_types::{
        KktpSessionBinding, KktpSessionState, PreparedMailbox, State, StegoProfile, BASE64,
        MAX_MAILBOX_ENVELOPE_BYTES, MAX_MAILBOX_TRANSACTIONS, PERSISTENT_REALTIME_MAGIC,
    },
};
use crate::hydra_commands::reset_kktp_after_unsent_advance;

pub(crate) fn prepare_kktp_mailbox(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
    stego_profile: &str,
) -> Result<PreparedMailbox, String> {
    if body.is_empty() || body.len() > ghost_core::MAX_EVENT_BYTES {
        return Err("message body is empty or exceeds the Ghost Talk event limit".into());
    }
    let (wire, _) = seal_kktp_message(runtime, contact_id, message_id, body, stego_profile)?;
    let prepared = match fragment_kktp_wire(&wire) {
        Ok(prepared) => prepared,
        Err(error) => {
            reset_kktp_after_unsent_advance(runtime, contact_id)?;
            return Err(format!("{error}; the secure session was reset before any KKTP sequence gap could be created"));
        }
    };
    Ok(prepared)
}

pub(crate) fn prepare_kktp_reaction_mailbox(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    reaction: &ghost_protocol::GhostReactionEvent,
    stego_profile: &str,
) -> Result<PreparedMailbox, String> {
    let (wire, _) = seal_kktp_reaction(runtime, contact_id, message_id, reaction, stego_profile)?;
    match fragment_kktp_wire(&wire) {
        Ok(prepared) => Ok(prepared),
        Err(error) => {
            reset_kktp_after_unsent_advance(runtime, contact_id)?;
            Err(format!(
                "{error}; the secure session was reset before any KKTP sequence gap could be created"
            ))
        }
    }
}

pub(crate) fn realtime_binding(
    runtime: &HydraProfileRuntime,
    contact_id: &str,
) -> Result<KktpSessionBinding, String> {
    let binding = runtime
        .kktp_sessions
        .get(contact_id)
        .cloned()
        .ok_or_else(|| "realtime transport requires an active KKTP session".to_string())?;
    if binding.state != KktpSessionState::Active {
        return Err("realtime transport requires an active KKTP session".into());
    }
    if runtime.blocked_peers.contains(contact_id) {
        return Err("realtime transport is closed for this peer".into());
    }
    Ok(binding)
}

pub(crate) fn realtime_envelope(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
    binding: &KktpSessionBinding,
) -> Result<Vec<u8>, String> {
    if runtime.hydra.session_status(contact_id)? == "active" {
        let inner = format!("{REALTIME_INNER_PREFIX}{}:{body}", binding.sid);
        let envelopes = runtime
            .hydra
            .send(contact_id, inner.as_bytes(), StegoProfile::Off)?;
        if envelopes.len() != 1 {
            return Err("realtime HYDRA send produced more than one logical envelope".into());
        }
        return envelopes
            .into_iter()
            .next()
            .ok_or_else(|| "realtime HYDRA send produced no envelope".to_string());
    }
    let transport = binding
        .persistent_transport
        .as_ref()
        .ok_or_else(|| "realtime transport has no locally persisted restart key".to_string())?;
    let aad = persistent_realtime_aad(&binding.sid, &runtime.identity_id, message_id);
    seal_persistent_payload(
        &transport.realtime_send_key,
        PERSISTENT_REALTIME_MAGIC,
        &aad,
        body.as_bytes(),
    )
}

pub(crate) fn realtime_carrier(
    runtime: &HydraProfileRuntime,
    binding: &KktpSessionBinding,
    message_id: &str,
    envelope: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if envelope.len().saturating_add(68) > MAX_MAILBOX_ENVELOPE_BYTES {
        return Err("realtime HYDRA envelope exceeds the configured carrier limit".into());
    }
    ghost_realtime::Gtr1Envelope::from_hex_ids(
        &runtime.identity_id,
        message_id,
        &binding.sid,
        envelope,
    )
    .and_then(|carrier| carrier.encode())
    .map_err(|error| error.to_string())
}

pub(crate) fn fragment_realtime_carrier(carrier: &[u8]) -> Result<PreparedMailbox, String> {
    let packet = ghost_core::Id128::new_random();
    let mut payloads_hex = Vec::new();
    for frame in ghost_protocol::fragment(packet, carrier)? {
        if payloads_hex.len() >= MAX_MAILBOX_TRANSACTIONS {
            return Err("realtime control requires too many Kaspa mailbox transactions".into());
        }
        payloads_hex.push(hex::encode(frame));
    }
    Ok(PreparedMailbox { payloads_hex })
}

pub(crate) fn prepare_realtime_carrier(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
) -> Result<HydraRealtimeEnvelope, String> {
    if body.is_empty() || body.len() > ghost_core::MAX_EVENT_BYTES {
        return Err("realtime body is empty or exceeds the Ghost Talk event limit".into());
    }
    let binding = realtime_binding(runtime, contact_id)?;
    let envelope = realtime_envelope(runtime, contact_id, message_id, body, &binding)?;
    let carrier = realtime_carrier(runtime, &binding, message_id, envelope)?;
    Ok(HydraRealtimeEnvelope {
        carrier_b64: BASE64.encode(carrier),
        session_sid: binding.sid,
        message_id: message_id.to_owned(),
    })
}

#[tauri::command]
pub async fn hydra_seal_realtime(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    contact_id: String,
    message_id: String,
    body: String,
) -> Result<HydraRealtimeEnvelope, String> {
    let runtime = state.runtime(&profile_id)?;
    let mut runtime = runtime.lock().await;
    prepare_realtime_carrier(&mut runtime, &contact_id, &message_id, &body)
}

pub(crate) fn prepare_realtime_mailbox(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
) -> Result<PreparedMailbox, String> {
    let carrier = prepare_realtime_carrier(runtime, contact_id, message_id, body)?;
    let bytes = BASE64
        .decode(carrier.carrier_b64.as_bytes())
        .map_err(|_| "internal GTR1 carrier is not valid base64".to_string())?;
    fragment_realtime_carrier(&bytes)
}
