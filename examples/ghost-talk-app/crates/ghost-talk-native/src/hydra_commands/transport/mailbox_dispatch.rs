use super::super::{
    contact_accept_carriers::handle_contact_accept_carrier,
    handshake_dispatch::{handle_kktp_handshake, handle_kktp_mailbox_message},
    inbound::{handle_delivery_ack_carrier, handle_realtime_carrier},
    runtime_owner::HydraRuntimeState,
    runtime_state::HydraProfileRuntime,
    session_carriers::{
        allowed_session_map, decode_mailbox_envelope, handle_call_signal_carrier,
        handle_contact_request_carrier, handle_kktp_session_end, mailbox_anchor_kind,
        mailbox_carrier_kind,
    },
    session_types::{
        ActiveSessionRegistration, AppHandle, HydraMailboxResult, KktpHandshakeControl,
        KktpMailboxMessage, KktpSessionEnd, PreparedMailbox, State, StegoProfile,
        KKTP_ANCHOR_PREFIX, KKTP_MESSAGE_PREFIX, MAX_MAILBOX_ENVELOPE_BYTES,
        MAX_MAILBOX_TRANSACTIONS,
    },
};
use crate::validation::require_min_password;
use std::path::PathBuf;
fn dispatch_anchor_envelope(
    runtime: &mut HydraProfileRuntime,
    profile_id: &str,
    envelope: &[u8],
    local_kaspa_addresses: &[String],
    anchor_kind: &str,
) -> Result<Option<HydraMailboxResult>, String> {
    if anchor_kind == "call_signal" {
        return handle_call_signal_carrier(runtime, profile_id, envelope, local_kaspa_addresses)
            .map(Some);
    }
    if anchor_kind == "discovery" {
        return handle_contact_request_carrier(
            runtime,
            profile_id,
            envelope,
            local_kaspa_addresses,
        )
        .map(Some);
    }
    dispatch_session_anchor(
        runtime,
        profile_id,
        envelope,
        local_kaspa_addresses,
        anchor_kind,
    )
}

fn dispatch_session_anchor(
    runtime: &mut HydraProfileRuntime,
    profile_id: &str,
    envelope: &[u8],
    local_kaspa_addresses: &[String],
    anchor_kind: &str,
) -> Result<Option<HydraMailboxResult>, String> {
    if anchor_kind == "response" {
        return handle_contact_accept_carrier(runtime, profile_id, envelope, local_kaspa_addresses)
            .map(Some);
    }
    if anchor_kind == "session_end" {
        return handle_kktp_session_end(
            runtime,
            KktpSessionEnd::decode(envelope)?,
            local_kaspa_addresses,
        )
        .map(Some);
    }
    if anchor_kind == "ghost_handshake" {
        return handle_kktp_handshake(runtime, KktpHandshakeControl::decode(envelope)?).map(Some);
    }
    Ok(None)
}

fn dispatch_binary_envelope(
    runtime: &mut HydraProfileRuntime,
    profile_id: &str,
    identity_id: &str,
    envelope: &[u8],
    local_kaspa_addresses: &[String],
) -> Result<HydraMailboxResult, String> {
    if let Some(result) =
        dispatch_contact_envelope(runtime, profile_id, envelope, local_kaspa_addresses)?
    {
        return Ok(result);
    }
    dispatch_message_envelope(runtime, identity_id, envelope)
}

fn dispatch_contact_envelope(
    runtime: &mut HydraProfileRuntime,
    profile_id: &str,
    envelope: &[u8],
    local_kaspa_addresses: &[String],
) -> Result<Option<HydraMailboxResult>, String> {
    if envelope.starts_with(&ghost_protocol::GTCR_MAGIC) {
        return handle_contact_request_carrier(
            runtime,
            profile_id,
            envelope,
            local_kaspa_addresses,
        )
        .map(Some);
    }
    if ghost_protocol::kktp_anchor_type(envelope)
        .ok()
        .flatten()
        .as_deref()
        == Some("response")
    {
        return handle_contact_accept_carrier(runtime, profile_id, envelope, local_kaspa_addresses)
            .map(Some);
    }
    Ok(None)
}

fn dispatch_message_envelope(
    runtime: &mut HydraProfileRuntime,
    identity_id: &str,
    envelope: &[u8],
) -> Result<HydraMailboxResult, String> {
    if envelope.starts_with(KKTP_MESSAGE_PREFIX) && !envelope.starts_with(KKTP_ANCHOR_PREFIX) {
        return handle_kktp_mailbox_message(runtime, KktpMailboxMessage::decode(envelope)?);
    }
    if envelope.starts_with(&ghost_protocol::GTACK_MAGIC) {
        return handle_delivery_ack_carrier(runtime, envelope, identity_id);
    }
    if envelope.starts_with(&ghost_protocol::GTR1_MAGIC) {
        return handle_realtime_carrier(runtime, envelope);
    }
    Ok(discard_result())
}

pub(crate) fn dispatch_mailbox_envelope(
    runtime: &mut HydraProfileRuntime,
    profile_id: &str,
    identity_id: &str,
    envelope: &[u8],
    local_kaspa_addresses: &[String],
    anchor_kind: Option<&str>,
) -> Result<HydraMailboxResult, String> {
    if let Some(kind) = anchor_kind {
        if let Some(result) =
            dispatch_anchor_envelope(runtime, profile_id, envelope, local_kaspa_addresses, kind)?
        {
            return Ok(result);
        }
    }
    dispatch_binary_envelope(
        runtime,
        profile_id,
        identity_id,
        envelope,
        local_kaspa_addresses,
    )
}

#[tauri::command]
pub async fn hydra_receive_mailbox(
    state: State<'_, HydraRuntimeState>,
    profile_id: String,
    password: String,
    identity_id: String,
    envelope_hex: String,
    local_kaspa_addresses: Vec<String>,
    active_sessions: Vec<ActiveSessionRegistration>,
) -> Result<HydraMailboxResult, String> {
    require_min_password(&password, "ID")?;
    let envelope = decode_mailbox_envelope(&envelope_hex)?;
    let runtime = state.runtime(&profile_id)?;
    let mut runtime = runtime.lock().await;
    if runtime.identity_id != identity_id {
        return Err("selected HYDRA identity does not match the unlocked Ghost Talk ID".into());
    }
    runtime.allowed_kktp_sids = allowed_session_map(active_sessions)?;
    let anchor_kind = mailbox_anchor_kind(&envelope)?;
    crate::debug_log::record(
        "info",
        "mailbox",
        "carrier-processing",
        format!(
            "profile={} kind={} bytes={}",
            profile_id,
            mailbox_carrier_kind(&envelope, anchor_kind.as_deref()),
            envelope.len()
        ),
    );
    dispatch_mailbox_envelope(
        &mut runtime,
        &profile_id,
        &identity_id,
        &envelope,
        &local_kaspa_addresses,
        anchor_kind.as_deref(),
    )
}

pub(crate) fn discard_result() -> HydraMailboxResult {
    HydraMailboxResult {
        received: None,
        control: None,
        recovery: None,
        incoming_request: None,
        call_signal: None,
        contact_accepted: None,
        peer_address: None,
        peer_label: None,
        message_id: None,
        delivery_ack: None,
        delivery_ack_peer: None,
        session_established_peer: None,
        session_ended: None,
        discard: true,
    }
}

pub(crate) fn frame_control(carrier: Vec<u8>) -> Result<PreparedMailbox, String> {
    if carrier.len() > MAX_MAILBOX_ENVELOPE_BYTES {
        return Err("HYDRA control envelope exceeds the configured carrier limit".into());
    }
    let packet = ghost_core::Id128::new_random();
    let frames = ghost_protocol::fragment(packet, &carrier)?;
    if frames.len() > MAX_MAILBOX_TRANSACTIONS {
        return Err("HYDRA control envelope requires too many Kaspa mailbox transactions".into());
    }
    Ok(PreparedMailbox {
        payloads_hex: frames.into_iter().map(hex::encode).collect(),
    })
}
pub(crate) fn decode_fixed_hex<const N: usize>(
    value: &str,
    label: &str,
) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{label} must be exactly {} hexadecimal characters",
            N * 2
        ));
    }
    let decoded = hex::decode(value).map_err(|_| format!("{label} is not valid hex"))?;
    decoded
        .try_into()
        .map_err(|_| format!("{label} has the wrong size"))
}

pub(crate) fn profile_path(app: &AppHandle, profile_id: &str) -> Result<PathBuf, String> {
    if profile_id.is_empty()
        || profile_id.len() > 80
        || !profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("invalid local profile id".into());
    }
    Ok(crate::storage_root::data_root(app)?
        .join("hydra")
        .join(profile_id))
}

pub(crate) fn parse_stego_profile(value: &str) -> Result<StegoProfile, String> {
    StegoProfile::parse_ui_label(value)
}
