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
