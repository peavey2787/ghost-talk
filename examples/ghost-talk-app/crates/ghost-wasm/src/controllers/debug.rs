use crate::model::DebugLogSnapshot;
use serde_json::Value;

pub(crate) async fn debug_log_snapshot(
    since_sequence: Option<u64>,
) -> Result<DebugLogSnapshot, String> {
    crate::native::debug_log_snapshot(since_sequence).await
}

pub(crate) async fn hydra_debug_state(profile_id: &str) -> Result<Value, String> {
    crate::native::hydra_debug_state(profile_id).await
}

pub(crate) async fn clear_debug_log() -> Result<(), String> {
    crate::native::clear_debug_log().await
}

/// Best-effort protocol-debug record. Callers check the profile's
/// `debug_logging` setting first so disabled logging costs nothing.
pub(crate) fn record(category: &'static str, event: &'static str, details: String) {
    wasm_bindgen_futures::spawn_local(async move {
        let _ = crate::native::record_debug(category, event, &details).await;
    });
}
