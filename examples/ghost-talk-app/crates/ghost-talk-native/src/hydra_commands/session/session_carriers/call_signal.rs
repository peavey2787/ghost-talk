use super::{
    discard_result, HydraCallSignalProjection, HydraMailboxResult, HydraProfileRuntime, BASE64,
    MAX_CONTACT_CARD_BYTES,
};
use base64::Engine as _;

pub(crate) fn decode_descriptor_contact_card(
    descriptor: &ghost_protocol::GhostContactDescriptor,
    label: &str,
) -> Result<Vec<u8>, String> {
    let card = BASE64
        .decode(&descriptor.hydra_contact_card_b64)
        .map_err(|_| format!("{label} HYDRA contact card is not valid base64"))?;
    if card.is_empty() || card.len() > MAX_CONTACT_CARD_BYTES {
        return Err(format!("{label} HYDRA contact card size is invalid"));
    }
    Ok(card)
}

fn decode_call_signal(
    profile_id: &str,
    envelope: &[u8],
) -> Option<ghost_protocol::GhostCallSignal> {
    match ghost_protocol::GhostCallSignal::decode(envelope) {
        Ok(signal) => Some(signal),
        Err(error) => {
            crate::debug_log::record(
                "warn",
                "call",
                "signal-discarded-invalid-envelope",
                format!("profile={} error={}", profile_id, error),
            );
            None
        }
    }
}

fn call_signal_targets_profile(
    runtime: &HydraProfileRuntime,
    signal: &ghost_protocol::GhostCallSignal,
    local_kaspa_addresses: &[String],
) -> bool {
    signal.sender.hydra_identity_id != runtime.identity_id
        && local_kaspa_addresses
            .iter()
            .any(|address| address == &signal.recipient_kaspa_address)
}

fn verify_call_signal_signature(
    profile_id: &str,
    signal: &ghost_protocol::GhostCallSignal,
) -> bool {
    if let Err(error) = ghost_kaspa::verify_call_signal(signal) {
        crate::debug_log::record(
            "warn",
            "call",
            "signal-discarded-invalid-signature",
            format!(
                "profile={} signal={} call={} error={}",
                profile_id, signal.signal_id, signal.call_id, error
            ),
        );
        return false;
    }
    true
}

fn verified_call_peer_handle(
    runtime: &mut HydraProfileRuntime,
    profile_id: &str,
    signal: &ghost_protocol::GhostCallSignal,
) -> Option<String> {
    let card = decode_descriptor_contact_card(&signal.sender, "call-signal")
        .map_err(|error| {
            record_call_signal_discard(
                profile_id,
                signal,
                "signal-discarded-invalid-contact-card",
                &error,
            );
        })
        .ok()?;
    let contact = runtime
        .hydra
        .preview_contact(&card)
        .map_err(|error| {
            record_call_signal_discard(
                profile_id,
                signal,
                "signal-discarded-invalid-hydra-contact",
                &error,
            );
        })
        .ok()?;
    if !signal.sender.hydra_identity_id.is_empty()
        && signal.sender.hydra_identity_id != contact.handle
    {
        crate::debug_log::record(
            "warn",
            "call",
            "signal-discarded-identity-mismatch",
            format!(
                "profile={} signal={} call={} descriptor_peer={} card_peer={}",
                profile_id,
                signal.signal_id,
                signal.call_id,
                signal.sender.hydra_identity_id,
                contact.handle
            ),
        );
        return None;
    }
    (contact.handle != runtime.identity_id).then_some(contact.handle)
}

fn record_call_signal_discard(
    profile_id: &str,
    signal: &ghost_protocol::GhostCallSignal,
    event: &str,
    error: &str,
) {
    crate::debug_log::record(
        "warn",
        "call",
        event,
        format!(
            "profile={} signal={} call={} error={}",
            profile_id, signal.signal_id, signal.call_id, error
        ),
    );
}

pub(crate) fn handle_call_signal_carrier(
    runtime: &mut HydraProfileRuntime,
    profile_id: &str,
    envelope: &[u8],
    local_kaspa_addresses: &[String],
) -> Result<HydraMailboxResult, String> {
    let Some(signal) = decode_call_signal(profile_id, envelope) else {
        return Ok(discard_result());
    };
    // Block-added is a network-wide carrier feed. Traffic for another wallet is
    // normal input and must never block later envelopes for this profile.
    if !call_signal_targets_profile(runtime, &signal, local_kaspa_addresses) {
        return Ok(discard_result());
    }
    if !verify_call_signal_signature(profile_id, &signal) {
        return Ok(discard_result());
    }
    let Some(peer_hydra_id) = verified_call_peer_handle(runtime, profile_id, &signal) else {
        return Ok(discard_result());
    };
    crate::debug_log::record(
        "info",
        "call",
        "signal-received-verified",
        format!(
            "profile={} signal={} call={} action={} peer={} destination={}",
            profile_id,
            signal.signal_id,
            signal.call_id,
            signal.action,
            peer_hydra_id,
            signal.recipient_kaspa_address
        ),
    );
    Ok(HydraMailboxResult {
        call_signal: Some(HydraCallSignalProjection {
            signal_id: signal.signal_id,
            call_id: signal.call_id,
            action: signal.action,
            peer_address: signal.sender.kaspa_address,
            local_address: signal.recipient_kaspa_address,
            peer_label: signal.sender.display_name,
            peer_hydra_id,
        }),
        ..discard_result()
    })
}
