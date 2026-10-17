use ghost_api::MailboxEvent;
use ghost_protocol::{swarm_frame_id, SwarmMailboxPacket};

/// Hold a swarm mailbox packet for other peers and, when it is addressed to
/// one of `addresses`, return the same mailbox event a Kaspa block would
/// deliver, marked as swarm-carried and stamped with its original send time.
pub(crate) fn mailbox_event(
    packet: &[u8],
    addresses: &[String],
    received_at_secs: u64,
) -> Option<MailboxEvent> {
    let packet = SwarmMailboxPacket::decode(packet).ok()?;
    super::retention::hold(&packet);
    packet.addressed_to(addresses.iter()).then(|| MailboxEvent {
        transaction_id: swarm_frame_id(&packet.frame),
        blue_score: String::new(),
        payload_hex: hex::encode(&packet.frame),
        block_time: Some(packet.sent_at_secs.min(received_at_secs)),
        via_swarm: true,
    })
}
