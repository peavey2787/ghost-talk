use super::super::super::session_types::{
    KktpFirstMessage, KktpMailboxMessage, PreparedMailbox, BASE64, MAX_MAILBOX_TRANSACTIONS,
};
use super::seal_kktp_message;
use crate::hydra_commands::HydraProfileRuntime;
use base64::Engine as _;

pub(crate) fn kktp_first_message(
    runtime: &mut HydraProfileRuntime,
    contact_id: &str,
    message_id: &str,
    body: &str,
    stego_profile: &str,
) -> Result<KktpFirstMessage, String> {
    let (wire, envelope) = seal_kktp_message(runtime, contact_id, message_id, body, stego_profile)?;
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

pub(crate) fn fragment_kktp_wire(wire: &KktpMailboxMessage) -> Result<PreparedMailbox, String> {
    let carrier = wire.encode()?;
    let packet = ghost_core::Id128::new_random();
    let frames = ghost_protocol::fragment(packet, &carrier)?;
    if frames.len() > MAX_MAILBOX_TRANSACTIONS {
        return Err("message requires too many Kaspa mailbox transactions".into());
    }
    Ok(PreparedMailbox {
        payloads_hex: frames.into_iter().map(hex::encode).collect(),
    })
}
