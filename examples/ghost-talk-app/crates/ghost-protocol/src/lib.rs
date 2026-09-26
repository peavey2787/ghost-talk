#![forbid(unsafe_code)]

mod protocol;

pub use protocol::{
    canonical_json, fragment, is_ghost_payload, kktp_anchor_type, kktp_mailbox_id,
    validate_kktp_message_id, CarrierFrame, ChatEvent, DirectTextV1, EventKind,
    GhostCallInviteContext, GhostCallSignal, GhostContactAccept, GhostContactDescriptor,
    GhostContactRequest, GhostDeliveryAck, GhostReactionEvent, GhostRoomInviteContext,
    KktpDirection, KktpFirstMessage, KktpHandshakeControl, KktpInnerMessage, KktpMailboxMessage,
    KktpSessionEnd, RealtimeBodyV1, RealtimeCapability, RealtimeProtocolError, TransportAnnounceV1,
    DIRECT_TEXT_PREFIX, EVENT_VERSION, GHOST_KKTP_VERSION, GHST_DATA_MAX, GHST_HEADER, GHST_MAGIC,
    GHST_VERSION, GTACK_MAGIC, GTCD_MAGIC, GTCD_VERSION, GTCR_MAGIC, GTVA_MAGIC,
    KKTP_ANCHOR_PREFIX, KKTP_MESSAGE_PREFIX, MAX_DIRECT_TEXT_BYTES, MAX_P2P_DIAL_ADDRESSES,
    MAX_P2P_DIAL_ADDRESS_BYTES, REALTIME_INNER_PREFIX,
};
