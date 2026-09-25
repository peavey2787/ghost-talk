use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedMedia {
    pub content_type: String,
    pub data_base64: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastStartRequest {
    pub session_id: String,
    #[serde(default)]
    pub record_local: bool,
    pub rtmp_server: Option<String>,
    pub rtmp_stream_key: Option<String>,
    #[serde(default)]
    pub relay_url: Option<String>,
}

pub use ghost_broadcast::SinkFailure as BroadcastSinkFailure;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastStopResult {
    pub failures: Vec<BroadcastSinkFailure>,
    #[serde(default)]
    pub recording: Option<ghost_media::MediaReference>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KaspaArchivePlan {
    pub total_bytes: u64,
    pub chunks: u32,
    pub transactions: u32,
    pub estimated_fee_per_tx_sompi: u64,
    pub estimated_cost_sompi: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KaspaArchivePublishResult {
    pub reference: ghost_media::MediaReference,
    pub transaction_ids: Vec<String>,
    pub actual_fee_sompi: String,
    pub fee_is_exact: bool,
    pub public: crate::BroadcastResult,
}
