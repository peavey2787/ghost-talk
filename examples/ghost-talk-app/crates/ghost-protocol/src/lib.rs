#![forbid(unsafe_code)]

mod protocol;

pub use protocol::{
    canonical_json, fragment, is_ghost_payload, kktp_anchor_type, kktp_mailbox_id, session_topic,
    session_topic_id, validate_kktp_message_id, CallControlV1, CarrierFrame, ChatEvent, EventKind,
    GhostCallInviteContext, GhostCallSignal, GhostContactAccept, GhostContactDescriptor,
    GhostContactRequest, GhostDeliveryAck, GhostReactionEvent, GhostRoomInviteContext, Gtr1Envelope,
    KktpDirection, KktpFirstMessage, KktpHandshakeControl, KktpInnerMessage, KktpMailboxMessage,
    KktpSessionEnd, OpusFrameV1, RealtimeBodyV1, RealtimeCapability, RealtimeProtocolError,
    RoomPresenceV1, TransportAnnounceV1, VoiceBatchV1, EVENT_VERSION,
    GHOST_KKTP_VERSION, GHST_DATA_MAX, GHST_HEADER, GHST_MAGIC, GHST_VERSION, GTACK_MAGIC,
    GTCD_MAGIC, GTCD_VERSION, GTCR_MAGIC, GTR1_HEADER_LEN, GTR1_MAGIC, REALTIME_INNER_PREFIX, GTVA_MAGIC,
    KKTP_ANCHOR_PREFIX, KKTP_MESSAGE_PREFIX,
};
