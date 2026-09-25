use super::{BroadcastResult, Deserialize, WalletProjection, WalletPublic};

pub(crate) fn wallet_projection(public: &WalletPublic) -> WalletProjection {
    public.projection()
}

pub(crate) fn broadcast_projection(
    result: ghost_kaspa::wallet::KaspaBroadcastResult,
) -> BroadcastResult {
    BroadcastResult {
        transaction_id: result.transaction_id,
        fee_sompi: result.fee_sompi,
        public: wallet_projection(&result.public),
    }
}

#[tauri::command]
pub fn derivation_presets() -> Vec<ghost_api::DerivationPresetInfo> {
    ghost_api::derivation_presets()
}

#[derive(Clone, Debug, Deserialize)]
pub struct WalletNetworkOptions {
    #[serde(default)]
    pub rest_endpoint: Option<String>,
    #[serde(default)]
    pub wrpc_endpoint: Option<String>,
}

