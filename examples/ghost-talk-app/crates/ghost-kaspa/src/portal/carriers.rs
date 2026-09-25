pub use ghost_core::TransactionObservation as LiveTransactionObservation;

#[cfg(feature = "upstream")]
#[derive(Clone, Debug)]
pub struct LiveBlockEvent {
    pub block_hash: String,
    pub daa_score: u64,
    pub transaction_count: usize,
    pub observations: Vec<LiveTransactionObservation>,
}

#[cfg(feature = "upstream")]
pub(crate) const GTCD_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/GTCD/v1\0";
#[cfg(feature = "upstream")]
pub(crate) const GTACK_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/GTAK/v1\0";
#[cfg(feature = "upstream")]
pub(crate) const GTCR_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/GTCR/v1\0";
#[cfg(feature = "upstream")]
pub(crate) const GTCA_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/GTCA/v1\0";
#[cfg(feature = "upstream")]
pub(crate) const KKTP_DISCOVERY_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/KKTP/discovery/v2\0";
#[cfg(feature = "upstream")]
pub(crate) const KKTP_RESPONSE_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/KKTP/response/v2\0";
#[cfg(feature = "upstream")]
pub(crate) const KKTP_SESSION_END_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/KKTP/session-end/v2\0";
#[cfg(feature = "upstream")]
pub(crate) const KKTP_CALL_SIGNAL_SIGNATURE_DOMAIN: &[u8] = b"GhostTalk/KKTP/call-signal/v2\0";

#[cfg(feature = "upstream")]
pub(crate) fn gtcd_digest(
    descriptor: &ghost_protocol::GhostContactDescriptor,
) -> Result<[u8; 32], String> {
    use sha2::{Digest, Sha256};
    let signing = descriptor.signing_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(GTCD_SIGNATURE_DOMAIN);
    hasher.update(signing);
    Ok(hasher.finalize().into())
}
