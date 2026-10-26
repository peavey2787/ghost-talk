#![forbid(unsafe_code)]

mod contact_preview;
mod descriptor;
mod history;
mod live;
mod node_session;
mod room_announcement;
pub mod wallet;
pub use contact_preview::preview_contact_request;
pub use descriptor::{build_private_descriptor, build_signed_descriptor};
pub use history::project_wallet_history_entry;
pub use live::{fallback_live_event_id, is_live_ghost_carrier, live_mailbox_event};
pub use node_session::NodeSession;
pub use room_announcement::{sign_room_announcement, verify_room_announcement};

mod portal;
pub use portal::upstream;
pub use portal::{
    p2pk_compressed_public_key, sign_call_signal, sign_contact_accept, sign_contact_request,
    sign_delivery_ack, sign_domain_message, sign_gtcd, sign_kktp_session_end, validate_destination,
    verify_call_signal, verify_contact_accept, verify_contact_request, verify_delivery_ack,
    verify_domain_message, verify_gtcd, verify_kktp_session_end, LiveBlockEvent,
    LiveTransactionObservation, Outpoint, PortalCurrentUtxo, PortalFacade, PortalMassAnalysis,
    UtxoReservations,
};
