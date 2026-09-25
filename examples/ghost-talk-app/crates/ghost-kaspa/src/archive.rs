use crate::{wallet::{self, WalletPublic, WalletSecret}, PortalFacade};
use ghost_media::{content_hash, KaspaArchiveChunk};
use serde::{Deserialize, Serialize};

pub const ARCHIVE_CHUNK_BYTES: usize = 7_500;
pub const MAX_ARCHIVE_BYTES: usize = 512 * 1024 * 1024;
pub const ARCHIVE_PROGRESS_VERSION: u8 = 1;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePlanRequest {
    pub profile_id: String,
    pub password: String,
    pub sealed: Vec<u8>,
    pub public: WalletPublic,
    pub data_size: u64,
    #[serde(default)]
    pub wrpc_endpoint: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivePublishRequest {
    pub profile_id: String,
    pub password: String,
    pub sealed: Vec<u8>,
    pub public: WalletPublic,
    pub content_type: String,
    pub data_base64: String,
    pub max_cost_sompi: u64,
    pub confirmed: bool,
    #[serde(default)]
    pub wrpc_endpoint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInFlightChunk {
    pub index: u32,
    pub content_hash: String,
    pub size: u32,
    pub reserved_fee_sompi: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveProgress {
    #[serde(default = "archive_progress_version")]
    pub version: u8,
    pub profile_id: String,
    pub media_id: String,
    pub content_type: String,
    pub network: String,
    pub address: String,
    pub total_size: u64,
    pub chunk_count: u32,
    pub completed: Vec<KaspaArchiveChunk>,
    pub transaction_ids: Vec<String>,
    pub total_fee_sompi: String,
    pub fees_exact: bool,
    pub public: WalletPublic,
    #[serde(default)]
    pub in_flight: Option<ArchiveInFlightChunk>,
}

const fn archive_progress_version() -> u8 { ARCHIVE_PROGRESS_VERSION }

impl ArchiveProgress {
    pub fn new(
        profile_id: &str,
        media_id: &str,
        content_type: &str,
        address: &str,
        size: u64,
        chunk_count: u32,
        public: WalletPublic,
    ) -> Self {
        Self {
            version: ARCHIVE_PROGRESS_VERSION,
            profile_id: profile_id.into(),
            media_id: media_id.into(),
            content_type: content_type.into(),
            network: public.network.clone(),
            address: address.into(),
            total_size: size,
            chunk_count,
            completed: Vec::new(),
            transaction_ids: Vec::new(),
            total_fee_sompi: "0".into(),
            fees_exact: true,
            public,
            in_flight: None,
        }
    }

    pub fn paid_fee(&self) -> Result<u128, String> {
        self.total_fee_sompi
            .parse::<u128>()
            .map_err(|_| "archive progress contains an invalid fee".into())
    }

    pub fn mark_in_flight(&mut self, index: u32, body: &[u8], reserved_fee_sompi: u64) {
        self.in_flight = Some(ArchiveInFlightChunk {
            index,
            content_hash: content_hash(body),
            size: body.len() as u32,
            reserved_fee_sompi,
        });
    }

    pub fn clear_in_flight(&mut self) { self.in_flight = None; }
}

pub fn validate_archive_size(size: u64) -> Result<(), String> {
    if size == 0 || size > MAX_ARCHIVE_BYTES as u64 {
        Err("archive media is empty or exceeds the 512 MiB client limit".into())
    } else {
        Ok(())
    }
}

pub async fn estimate_archive_chunk_fee(
    portal: &PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    address: &str,
    bytes: usize,
) -> Result<u64, String> {
    let payload = ghost_media::encode_archive_chunk(&"00".repeat(32), 0, 1, &vec![0u8; bytes])?;
    wallet::estimate_payload_fee(portal, secret, public, address, &payload).await
}


pub async fn archive_plan(
    portal: &PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    address: &str,
    total_bytes: u64,
) -> Result<ghost_api::KaspaArchivePlan, String> {
    validate_archive_size(total_bytes)?;
    let chunks = total_bytes.div_ceil(ARCHIVE_CHUNK_BYTES as u64).max(1);
    let full_count = total_bytes / ARCHIVE_CHUNK_BYTES as u64;
    let tail = (total_bytes % ARCHIVE_CHUNK_BYTES as u64) as usize;
    let full_fee = archive_chunk_fee_if_needed(
        portal,
        secret,
        public,
        address,
        full_count > 0,
        ARCHIVE_CHUNK_BYTES,
    )
    .await?;
    let tail_fee = archive_chunk_fee_if_needed(
        portal,
        secret,
        public,
        address,
        tail > 0,
        tail,
    )
    .await?;
    Ok(ghost_api::KaspaArchivePlan {
        total_bytes,
        chunks: chunks.min(u64::from(u32::MAX)) as u32,
        transactions: chunks.min(u64::from(u32::MAX)) as u32,
        estimated_fee_per_tx_sompi: full_fee.max(tail_fee),
        estimated_cost_sompi: full_count
            .saturating_mul(full_fee)
            .saturating_add(tail_fee),
    })
}

pub async fn archive_remaining_cost(
    portal: &PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    address: &str,
    chunks: &[&[u8]],
) -> Result<u64, String> {
    let mut total = 0u64;
    for body in chunks {
        total = total.saturating_add(
            estimate_archive_chunk_fee(portal, secret, public, address, body.len()).await?,
        );
    }
    Ok(total)
}

async fn archive_chunk_fee_if_needed(
    portal: &PortalFacade,
    secret: &WalletSecret,
    public: &WalletPublic,
    address: &str,
    needed: bool,
    bytes: usize,
) -> Result<u64, String> {
    if !needed {
        return Ok(0);
    }
    estimate_archive_chunk_fee(portal, secret, public, address, bytes).await
}
