pub use ghost_domain::wallet::WalletProjection;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletHistoryEntry {
    pub transaction_id: String,
    pub blue_score: String,
    pub block_time: Option<u64>,
    pub ghost_payload: bool,
    #[serde(default)]
    pub addresses: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletHistoryResult {
    #[serde(default)]
    pub history: Vec<WalletHistoryEntry>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub used_addresses: Vec<String>,
    #[serde(default)]
    pub recommended_receive_index: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletSnapshot {
    pub balance_sompi: String,
    pub utxo_count: String,
    pub blue_score: String,
    #[serde(default)]
    pub history: Vec<WalletHistoryEntry>,
    #[serde(default)]
    pub active_addresses: Vec<String>,
    #[serde(default)]
    pub recommended_receive_index: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletRecovery {
    pub mnemonic: String,
    pub passphrase: String,
    pub account_path: String,
    pub network: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WalletCreateResponse {
    #[serde(default)]
    pub sealed: Vec<u8>,
    pub mnemonic: String,
    pub public: WalletProjection,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WalletImportResponse {
    #[serde(default)]
    pub sealed: Vec<u8>,
    pub public: WalletProjection,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BroadcastResult {
    pub transaction_id: String,
    #[serde(default)]
    pub fee_sompi: String,
    pub public: WalletProjection,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MailboxSendResult {
    pub transaction_id: String,
    #[serde(default)]
    pub fee_sompi: String,
    #[serde(default)]
    pub mailbox_output_sompi: String,
    pub public: WalletProjection,
    #[serde(default)]
    pub pending_handshake: bool,
    #[serde(default)]
    pub pending_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PublishedGhostDescriptor {
    pub kaspa_address: String,
    #[serde(default)]
    pub transaction_id: Option<String>,
    #[serde(default)]
    pub already_current: bool,
    #[serde(default)]
    pub discoverable: bool,
    pub public: WalletProjection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivationPresetInfo {
    pub label: String,
    pub path: Option<String>,
    pub supported: bool,
    pub note: String,
}

pub fn derivation_presets() -> Vec<DerivationPresetInfo> {
    [
        (
            "Kaspa Standard (CLI / Kaspium / KasWare / OneKey / Tangem)",
            Some("m/44'/111111'/0'"),
            "BIP44 account root; receive /0/index and change /1/index.",
        ),
        (
            "Kaspa account #1",
            Some("m/44'/111111'/1'"),
            "Second standard Kaspa BIP44 account.",
        ),
        (
            "Kaspa account #2",
            Some("m/44'/111111'/2'"),
            "Third standard Kaspa BIP44 account.",
        ),
        (
            "Custom account path",
            None,
            "Enter an account-level BIP32 path. Ghost Talk appends /0/index and /1/index.",
        ),
    ]
    .into_iter()
    .map(|(label, path, note)| DerivationPresetInfo {
        label: label.to_owned(),
        path: path.map(str::to_owned),
        supported: true,
        note: note.to_owned(),
    })
    .collect()
}
