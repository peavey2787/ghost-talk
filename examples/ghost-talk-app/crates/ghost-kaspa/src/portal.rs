#[cfg(feature = "upstream")]
pub mod upstream {
    pub use kaspa_portal as portal;
}

#[cfg(test)]
mod bootstrap_tests;
mod carriers;
mod client;
mod contact_signatures;
#[cfg(all(test, feature = "upstream"))]
mod contract_tests;
mod reservations;

pub use carriers::{LiveBlockEvent, LiveTransactionObservation};
pub use client::{PortalCurrentUtxo, PortalFacade, PortalMassAnalysis};
pub use contact_signatures::{
    p2pk_compressed_public_key, sign_call_signal, sign_contact_accept, sign_contact_request,
    sign_delivery_ack, sign_domain_message, sign_gtcd, sign_kktp_session_end, verify_call_signal,
    verify_contact_accept, verify_contact_request, verify_delivery_ack, verify_domain_message,
    verify_gtcd, verify_kktp_session_end,
};
pub use reservations::{validate_destination, Outpoint, UtxoReservations};
