use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use ghost_api::{HydraMailboxResult, HydraRealtimeEnvelope, ReceivedProjection};
use ghost_hydra::StegoProfile;
use serde_json::Value;

use super::super::support::util::{required_str, to_value};
use super::{
    secure_transport::{require_active_binding, with_hydra_runtime},
    session::{self, SessionState},
};

pub(in crate::native::browser_host) fn seal(args: &Value) -> Result<Value, String> {
    let profile = required_str(args, "profileId")?;
    let peer = required_str(args, "contactId")?;
    let message_id = required_str(args, "messageId")?;
    let body = required_str(args, "body")?;
    if body.is_empty() || body.len() > ghost_core::MAX_EVENT_BYTES {
        return Err("realtime body is empty or exceeds the Ghost Talk event limit".into());
    }
    ghost_protocol::validate_kktp_message_id(message_id)?;
    let (identity, sid) = session::with(profile, |runtime| {
        let binding = runtime
            .sessions
            .get(peer)
            .ok_or_else(|| "realtime transport requires an active KKTP session".to_string())?;
        if binding.state != SessionState::Active || runtime.blocked.contains(peer) {
            return Err("realtime transport requires an active KKTP session".into());
        }
        Ok((runtime.identity_id.clone(), binding.sid.clone()))
    })?;
    require_active_binding(profile, peer, &sid)?;
    let inner = format!("{}{sid}:{body}", ghost_protocol::REALTIME_INNER_PREFIX);
    let envelope = with_hydra_runtime(profile, |hydra| {
        let envelopes = hydra.send(peer, inner.as_bytes(), StegoProfile::Off)?;
        if envelopes.len() != 1 {
            return Err("realtime HYDRA send produced more than one logical envelope".into());
        }
        envelopes
            .into_iter()
            .next()
            .ok_or_else(|| "realtime HYDRA send produced no envelope".to_string())
    })?;
    let carrier = ghost_realtime::Gtr1Envelope::from_hex_ids(&identity, message_id, &sid, envelope)
        .and_then(|carrier| carrier.encode())
        .map_err(|error| error.to_string())?;
    to_value(HydraRealtimeEnvelope {
        carrier_b64: BASE64.encode(carrier),
        session_sid: sid,
        message_id: message_id.to_owned(),
    })
}

pub(in crate::native::browser_host) fn open(args: &Value) -> Result<Value, String> {
    let profile = required_str(args, "profileId")?;
    let bytes = BASE64
        .decode(required_str(args, "carrierB64")?)
        .map_err(|_| "GTR1 realtime carrier is not valid base64".to_string())?;
    to_value(open_carrier(profile, &bytes)?)
}

/// Open one sealed GTR1 carrier, whichever carrier (p2p-net or Kaspa) brought it.
pub(in crate::native::browser_host) fn open_carrier(
    profile: &str,
    bytes: &[u8],
) -> Result<HydraMailboxResult, String> {
    let carrier = ghost_realtime::Gtr1Envelope::decode(bytes).map_err(|error| error.to_string())?;
    let (sender, sid) = (carrier.sender_hex(), carrier.sid_hex());
    let route = active_route(profile, &sender, &sid)?;
    let result = HydraMailboxResult {
        peer_address: route.as_ref().map(|value| value.kaspa_address.clone()),
        peer_label: route.as_ref().map(|value| value.display_name.clone()),
        message_id: Some(carrier.message_id_hex()),
        ..Default::default()
    };
    let received = with_hydra_runtime(profile, |hydra| {
        hydra.receive(&carrier.ciphertext, StegoProfile::Off)
    })?;
    let Some(received) = received else {
        return Ok(HydraMailboxResult {
            discard: true,
            ..result
        });
    };
    if received.from != sender {
        return Err("realtime HYDRA sender does not match the authenticated peer".into());
    }
    let expected = format!("{}{sid}:", ghost_protocol::REALTIME_INNER_PREFIX);
    let body = received
        .plaintext
        .strip_prefix(&expected)
        .ok_or_else(|| "realtime HYDRA inner SID binding is invalid".to_string())?
        .to_owned();
    Ok(HydraMailboxResult {
        received: Some(ReceivedProjection {
            from: sender,
            plaintext: body,
            content_type: None,
            session_sid: Some(sid),
        }),
        ..result
    })
}

/// Realtime is admitted only for the exact active, unblocked KKTP session.
fn active_route(profile: &str, sender: &str, sid: &str) -> Result<Option<session::Route>, String> {
    session::with(profile, |runtime| {
        let binding = runtime
            .sessions
            .get(sender)
            .ok_or_else(|| "realtime carrier has no active KKTP binding".to_string())?;
        if binding.state != SessionState::Active
            || binding.sid != sid
            || runtime.blocked.contains(sender)
        {
            return Err("realtime carrier SID does not match the active KKTP session".into());
        }
        Ok(runtime.routes.get(sender).cloned())
    })
}
