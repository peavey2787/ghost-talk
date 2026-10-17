use ghost_api::MailboxEvent;
use ghost_protocol::{swarm_frame_id, SwarmMailboxPacket};
use ghost_rooms::RoomAnnouncement;

/// A swarm mailbox packet addressed to one of `addresses`, as the same
/// mailbox event a Kaspa block would deliver, marked as swarm-carried.
pub(crate) fn mailbox_event(
    packet: &[u8],
    addresses: &[String],
    received_at_secs: u64,
) -> Option<MailboxEvent> {
    let packet = SwarmMailboxPacket::decode(packet).ok()?;
    packet.addressed_to(addresses.iter()).then(|| MailboxEvent {
        transaction_id: swarm_frame_id(&packet.frame),
        blue_score: String::new(),
        payload_hex: hex::encode(&packet.frame),
        block_time: Some(received_at_secs),
        via_swarm: true,
    })
}

/// A well-formed room announcement whose owner signature verifies.
pub(crate) fn room_announcement(packet: &[u8]) -> Option<RoomAnnouncement> {
    let announcement = RoomAnnouncement::decode(packet).ok()?;
    ghost_kaspa::verify_room_announcement(&announcement).ok()?;
    Some(announcement)
}
