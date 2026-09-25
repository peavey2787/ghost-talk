use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub max_kaspa_payload: usize,
}

pub fn app_info() -> AppInfo {
    AppInfo {
        name: ghost_core::APP_NAME.to_owned(),
        version: ghost_core::APP_VERSION.to_owned(),
        max_kaspa_payload: ghost_core::MAX_GHOST_TX_PAYLOAD,
    }
}
