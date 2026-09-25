use crate::KasiaContactMapping;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KasiaSendRequest<P> {
    pub profile_id: String,
    pub password: String,
    pub sealed: Vec<u8>,
    pub public: P,
    pub mapping: KasiaContactMapping,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub is_response: bool,
    #[serde(default)]
    pub fee_sompi: u64,
    #[serde(default)]
    pub wrpc_endpoint: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KasiaHistoryRequest<P> {
    pub profile_id: String,
    pub password: String,
    pub sealed: Vec<u8>,
    pub public: P,
    pub mapping: KasiaContactMapping,
    pub indexer_url: String,
    #[serde(default)]
    pub block_time: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KasiaIdentityProjection {
    pub public_key_hex: String,
}
