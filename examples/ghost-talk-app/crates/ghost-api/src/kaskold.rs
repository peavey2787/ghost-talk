use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KasKoldWalletSummary {
    pub index: usize,
    pub name: String,
    pub active: bool,
    pub kind: String,
    pub fingerprint: Option<String>,
    pub kpub: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KasKoldInventoryResult {
    pub sealed_inventory: Vec<u8>,
    pub wallets: Vec<KasKoldWalletSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KasKoldBackupResult {
    pub filename: String,
    pub media_type: String,
    pub text: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KasKoldSignResult {
    pub signed_pskt_hex: String,
    pub transaction_json: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KasKoldReviewOutput {
    pub index: usize,
    pub amount_sompi: String,
    pub ownership: String,
    pub address: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KasKoldReviewResult {
    pub review_token: String,
    pub network: String,
    pub input_count: usize,
    pub output_count: usize,
    pub input_total_sompi: String,
    pub output_total_sompi: String,
    pub external_total_sompi: String,
    pub change_total_sompi: String,
    pub own_receive_total_sompi: String,
    pub fee_sompi: String,
    pub outputs: Vec<KasKoldReviewOutput>,
}
