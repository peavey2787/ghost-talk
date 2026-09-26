//! Inbound mailbox envelopes: contact requests/acceptances, KKTP messages,
//! delivery acks, call signals, and session ends.

mod accept;
mod message;
mod signals;

use ghost_api::HydraMailboxResult;
use ghost_protocol::{KktpHandshakeControl, KktpMailboxMessage};
use serde_json::Value;

use super::common::discard;
use crate::native::browser_host::{
    runtime::{handshake, realtime, session},
    support::util::{required, required_str, to_value},
};

pub(in crate::native::browser_host) use message::open_first;

pub(in crate::native::browser_host) fn receive(args: &Value) -> Result<Value, String> {
    let profile = required_str(args, "profileId")?;
    let identity = required_str(args, "identityId")?;
    super::send::ensure_identity(profile, identity)?;
    let local_addresses: Vec<String> = required(args, "localKaspaAddresses")?;
    let envelope = hex::decode(required_str(args, "envelopeHex")?)
        .map_err(|_| "mailbox envelope is not valid hex".to_string())?;
    let result = dispatch(profile, identity, &local_addresses, &envelope)?;
    to_value(result)
}

fn dispatch(
    profile: &str,
    identity: &str,
    local: &[String],
    envelope: &[u8],
) -> Result<HydraMailboxResult, String> {
    if envelope.starts_with(&ghost_protocol::GTCR_MAGIC) {
        return super::request::receive_contact_request(profile, identity, local, envelope);
    }
    if envelope.starts_with(&ghost_realtime::GTR1_MAGIC) {
        // Kaspa-carried realtime (announcements, voice fallback). Carriers for
        // other or ended sessions (including our own) are simply dropped.
        return Ok(realtime::open_carrier(profile, envelope).unwrap_or_else(|_| discard()));
    }
    if envelope.starts_with(&ghost_protocol::GTACK_MAGIC) {
        return signals::receive_ack(profile, identity, envelope);
    }
    if is_mailbox_message(envelope) {
        return message::receive_message(profile, KktpMailboxMessage::decode(envelope)?);
    }
    dispatch_anchor(profile, identity, local, envelope)
}

fn is_mailbox_message(envelope: &[u8]) -> bool {
    envelope.starts_with(ghost_protocol::KKTP_MESSAGE_PREFIX)
        && !envelope.starts_with(ghost_protocol::KKTP_ANCHOR_PREFIX)
}

fn dispatch_anchor(
    profile: &str,
    identity: &str,
    local: &[String],
    envelope: &[u8],
) -> Result<HydraMailboxResult, String> {
    match ghost_protocol::kktp_anchor_type(envelope)?.as_deref() {
        // Contact requests travel as signed KKTP discovery anchors.
        Some("discovery") => {
            super::request::receive_contact_request(profile, identity, local, envelope)
        }
        Some("response") => accept::receive_accept(profile, local, envelope),
        Some(kind) => dispatch_session_anchor(profile, identity, local, envelope, kind),
        None => Ok(discard()),
    }
}

fn dispatch_session_anchor(
    profile: &str,
    identity: &str,
    local: &[String],
    envelope: &[u8],
    kind: &str,
) -> Result<HydraMailboxResult, String> {
    match kind {
        "ghost_handshake" => handshake::handle(profile, KktpHandshakeControl::decode(envelope)?),
        "session_end" => signals::receive_session_end(profile, identity, local, envelope),
        "call_signal" => signals::receive_call_signal(profile, identity, local, envelope),
        _ => Ok(discard()),
    }
}

fn route_result(
    route: Option<session::Route>,
    message_id: String,
    discard_value: bool,
) -> Result<HydraMailboxResult, String> {
    Ok(HydraMailboxResult {
        peer_address: route.as_ref().map(|route| route.kaspa_address.clone()),
        peer_label: route.as_ref().map(|route| route.display_name.clone()),
        message_id: Some(message_id),
        discard: discard_value,
        ..Default::default()
    })
}
