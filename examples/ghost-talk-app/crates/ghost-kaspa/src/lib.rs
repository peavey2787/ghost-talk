#![forbid(unsafe_code)]

mod archive;
mod contact_preview;
mod descriptor;
mod history;
mod live;
mod node_session;
mod resolver;
pub mod wallet;
pub use archive::{
    archive_plan, archive_remaining_cost, estimate_archive_chunk_fee, validate_archive_size,
    ArchiveInFlightChunk, ArchivePlanRequest, ArchiveProgress, ArchivePublishRequest,
    ARCHIVE_CHUNK_BYTES, ARCHIVE_PROGRESS_VERSION, MAX_ARCHIVE_BYTES,
};
pub use contact_preview::preview_contact_request;
pub use descriptor::{build_private_descriptor, build_signed_descriptor};
pub use history::project_wallet_history_entry;
pub use live::{fallback_live_event_id, is_live_ghost_carrier};
pub use node_session::NodeSession;
pub use resolver::{resolver_query_url, PUBLIC_WRPC_RESOLVERS};

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
