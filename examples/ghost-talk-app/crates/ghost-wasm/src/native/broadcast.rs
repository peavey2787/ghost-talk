pub(crate) use ghost_api::BroadcastStopResult;
use serde_json::json;

pub(crate) async fn start_broadcast(
    session_id: &str,
    record_local: bool,
    rtmp_server: Option<&str>,
    rtmp_stream_key: Option<&str>,
    relay_url: Option<&str>,
) -> Result<(), String> {
    super::invoke::invoke_unit(
        "broadcast_start",
        json!({
            "request": {
                "sessionId": session_id,
                "recordLocal": record_local,
                "rtmpServer": rtmp_server,
                "rtmpStreamKey": rtmp_stream_key,
                "relayUrl": relay_url,
            }
        }),
    )
    .await
}

pub(crate) async fn push_broadcast(
    session_id: &str,
    sequence: u64,
    timestamp_ms: u64,
    encoded: Vec<u8>,
) -> Result<(), String> {
    super::invoke::invoke_unit(
        "broadcast_push",
        json!({
            "sessionId": session_id,
            "sequence": sequence,
            "timestampMs": timestamp_ms,
            "encoded": encoded,
        }),
    )
    .await
}

pub(crate) async fn stop_broadcast(session_id: &str) -> Result<BroadcastStopResult, String> {
    super::invoke::invoke("broadcast_stop", json!({ "sessionId": session_id })).await
}
