mod call_signal;
mod canonical_json;
mod events;
mod kktp_codec;
mod kktp_types;
mod kktp_validation_types;
mod realtime;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod reaction_protocol_tests;
#[cfg(test)]
mod realtime_tests;

pub use call_signal::GhostCallSignal;
pub use canonical_json::canonical_json;
pub use events::{
    fragment, CarrierFrame, ChatEvent, EventKind, GhostCallInviteContext, GhostContactDescriptor,
    GhostContactRequest, GhostRoomInviteContext, EVENT_VERSION,
    GHOST_KKTP_VERSION, GHST_DATA_MAX, GHST_HEADER, GHST_MAGIC, GHST_VERSION, GTACK_MAGIC,
    GTCD_MAGIC, GTCD_VERSION, GTCR_MAGIC, GTVA_MAGIC, KKTP_ANCHOR_PREFIX, KKTP_MESSAGE_PREFIX,
};
pub use kktp_codec::{kktp_anchor_type, kktp_mailbox_id, validate_kktp_message_id};
pub use kktp_types::{
    GhostContactAccept, GhostDeliveryAck, GhostReactionEvent, KktpDirection, KktpFirstMessage,
    KktpHandshakeControl, KktpInnerMessage, KktpMailboxMessage, KktpSessionEnd,
};

/// True when a Kaspa transaction payload carries a Ghost Talk protocol envelope.
pub fn is_ghost_payload(payload: &[u8]) -> bool {
    payload.starts_with(b"KKTP:")
        || payload.starts_with(b"GHST")
        || payload.starts_with(b"GTCD")
        || payload.starts_with(b"GTCR")
        || payload.starts_with(b"GTCA")
        || payload.starts_with(b"GTAK")
        || payload.starts_with(b"GTVA")
        || payload.starts_with(b"GTBK")
        || payload.starts_with(b"GMAR1")
}

pub use realtime::{
    session_topic, session_topic_id, CallControlV1, Gtr1Envelope, OpusFrameV1,
    RealtimeBodyV1, RealtimeCapability, RealtimeProtocolError, RoomPresenceV1,
    TransportAnnounceV1, VoiceBatchV1, GTR1_HEADER_LEN, GTR1_MAGIC, REALTIME_INNER_PREFIX,
    MAX_OPUS_FRAME_BYTES, MAX_P2P_DIAL_ADDRESSES, MAX_P2P_DIAL_ADDRESS_BYTES,
    MAX_REALTIME_CIPHERTEXT_BYTES, MAX_VOICE_BATCH_FRAMES,
};
