use super::super::{
    delivery_state::delivery_ack_matches_prepared_completion,
    mailbox_dispatch::discard_result,
    runtime_session_queries::kktp_sid_is_current,
    runtime_state::HydraProfileRuntime,
    session_state::{
        mark_peer_route_session_active, open_persistent_payload, persistent_realtime_aad,
    },
    session_types::{
        HydraMailboxResult, KktpSessionState, PeerRoute, ReceivedProjection, StegoProfile,
        PERSISTENT_REALTIME_MAGIC,
    },
    transport_persistence::persist_transport_state,
};
pub(crate) fn decode_verified_delivery_ack(
    envelope: &[u8],
) -> Option<ghost_protocol::GhostDeliveryAck> {
    let ack = match ghost_protocol::GhostDeliveryAck::decode(envelope) {
        Ok(ack) => ack,
        Err(error) => {
            crate::debug_log::record(
                "warn",
                "mailbox",
                "delivery-ack-discarded",
                format!("reason=decode error={error}"),
            );
            return None;
        }
    };
    if let Err(error) = ghost_kaspa::verify_delivery_ack(&ack) {
        crate::debug_log::record(
            "warn",
            "mailbox",
            "delivery-ack-discarded",
            format!(
                "reason=signature peer={} message={} error={error}",
                ack.signer_hydra_id, ack.message_id
            ),
        );
        return None;
    }
    Some(ack)
}

pub(crate) fn current_ack_route<'a>(
    runtime: &'a HydraProfileRuntime,
    ack: &ghost_protocol::GhostDeliveryAck,
) -> Option<&'a PeerRoute> {
    runtime
        .peer_routes
        .get(&ack.signer_hydra_id)
        .filter(|route| route.kaspa_address == ack.signer_kaspa_address)
}

pub(crate) fn delivery_ack_is_current(
    runtime: &HydraProfileRuntime,
    ack: &ghost_protocol::GhostDeliveryAck,
    identity_id: &str,
) -> Result<bool, String> {
    if ack.signer_hydra_id == identity_id || ack.destination_hydra_id != identity_id {
        return Ok(false);
    }
    if !runtime.hydra.has_contact(&ack.signer_hydra_id)? {
        return Ok(false);
    }
    if current_ack_route(runtime, ack).is_none() {
        return Ok(false);
    }
    Ok(delivery_ack_matches_prepared_completion(
        runtime.prepared_completion.get(&ack.signer_hydra_id),
        ack,
    ))
}

pub(crate) fn handle_delivery_ack_carrier(
    runtime: &mut HydraProfileRuntime,
    envelope: &[u8],
    identity_id: &str,
) -> Result<HydraMailboxResult, String> {
    let Some(ack) = decode_verified_delivery_ack(envelope) else {
        return Ok(discard_result());
    };
    if !delivery_ack_is_current(runtime, &ack, identity_id)? {
        crate::debug_log::record(
            "info",
            "mailbox",
            "stale-delivery-ack-discarded",
            format!("peer={} message={}", ack.signer_hydra_id, ack.message_id),
        );
        return Ok(discard_result());
    }
    let route = current_ack_route(runtime, &ack).cloned().ok_or_else(|| {
        "authenticated delivery acknowledgement lost its current route".to_string()
    })?;
    runtime.pending_outbound.remove(&ack.signer_hydra_id);
    runtime.prepared_completion.remove(&ack.signer_hydra_id);
    mark_peer_route_session_active(runtime, &ack.signer_hydra_id);
    persist_transport_state(runtime)?;
    crate::debug_log::record(
        "info",
        "handshake",
        "bootstrap-ack-received-session-active",
        format!("peer={} message={}", ack.signer_hydra_id, ack.message_id),
    );
    let peer = ack.signer_hydra_id;
    Ok(HydraMailboxResult {
        peer_address: Some(route.kaspa_address),
        peer_label: Some(route.display_name),
        delivery_ack: Some(ack.message_id),
        delivery_ack_peer: Some(peer.clone()),
        session_established_peer: Some(peer),
        ..discard_result()
    })
}

pub(crate) fn realtime_route_result(
    runtime: &HydraProfileRuntime,
    sender_id: &str,
    message_id: String,
) -> HydraMailboxResult {
    let route = runtime.peer_routes.get(sender_id);
    HydraMailboxResult {
        peer_address: route.map(|value| value.kaspa_address.clone()),
        peer_label: route.map(|value| value.display_name.clone()),
        message_id: Some(message_id),
        ..discard_result()
    }
}

pub(crate) fn handle_realtime_carrier(
    runtime: &mut HydraProfileRuntime,
    envelope: &[u8],
) -> Result<HydraMailboxResult, String> {
    let carrier = ghost_protocol::Gtr1Envelope::decode(envelope).map_err(|error| error.to_string())?;
    let sender_id = carrier.sender_hex();
    let message_id = carrier.message_id_hex();
    let sid = carrier.sid_hex();
    if !kktp_sid_is_current(runtime, &sender_id, &sid) {
        crate::debug_log::record(
            "info",
            "realtime",
            "stale-realtime-sid-discarded",
            format!("peer={} sid={} message={}", sender_id, sid, message_id),
        );
        return Ok(realtime_route_result(runtime, &sender_id, message_id));
    }
    let binding = runtime
        .kktp_sessions
        .get(&sender_id)
        .cloned()
        .ok_or_else(|| "realtime carrier has no active KKTP binding".to_string())?;
    if binding.state != KktpSessionState::Active || binding.sid != sid {
        return Err("realtime carrier SID does not match the active KKTP session".into());
    }
    let Some(body) =
        open_realtime_body(runtime, &binding, envelope, &sender_id, &sid, &message_id)?
    else {
        return Ok(realtime_route_result(runtime, &sender_id, message_id));
    };
    let route = runtime.peer_routes.get(&sender_id);
    Ok(HydraMailboxResult {
        received: Some(ReceivedProjection {
            from: sender_id,
            plaintext: body,
            content_type: None,
            session_sid: Some(sid),
        }),
        peer_address: route.map(|value| value.kaspa_address.clone()),
        peer_label: route.map(|value| value.display_name.clone()),
        message_id: Some(message_id),
        ..discard_result()
    })
}

fn open_realtime_body(
    runtime: &mut HydraProfileRuntime,
    binding: &super::super::session_types::KktpSessionBinding,
    envelope: &[u8],
    sender_id: &str,
    sid: &str,
    message_id: &str,
) -> Result<Option<String>, String> {
    let carrier = ghost_protocol::Gtr1Envelope::decode(envelope).map_err(|error| error.to_string())?;
    if carrier.ciphertext.starts_with(PERSISTENT_REALTIME_MAGIC) {
        let transport = binding.persistent_transport.as_ref().ok_or_else(|| {
            "persistent realtime carrier arrived without restored transport state".to_string()
        })?;
        let aad = persistent_realtime_aad(sid, sender_id, message_id);
        let mut plaintext = open_persistent_payload(
            &transport.realtime_recv_key,
            PERSISTENT_REALTIME_MAGIC,
            &aad,
            &carrier.ciphertext,
        )?;
        let body = String::from_utf8(plaintext.clone())
            .map_err(|_| "persistent realtime body is not UTF-8".to_string())?;
        plaintext.zeroize();
        return Ok(Some(body));
    }
    let Some(received) = runtime.hydra.receive(&carrier.ciphertext, StegoProfile::Off)? else {
        return Ok(None);
    };
    if received.from != sender_id {
        return Err("realtime HYDRA sender does not match the authenticated peer".into());
    }
    let expected_prefix = format!("{REALTIME_INNER_PREFIX}{sid}:");
    received
        .plaintext
        .strip_prefix(&expected_prefix)
        .ok_or_else(|| "realtime HYDRA inner SID binding is invalid".to_string())
        .map(|body| Some(body.to_owned()))
}
use ghost_protocol::REALTIME_INNER_PREFIX;
use zeroize::Zeroize;
