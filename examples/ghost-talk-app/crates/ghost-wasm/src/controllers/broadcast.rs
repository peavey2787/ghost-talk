mod relay;

pub(crate) use crate::native::BroadcastStopResult;

pub(crate) async fn start(
    session_id: &str,
    record_local: bool,
    rtmp_server: Option<&str>,
    rtmp_stream_key: Option<&str>,
    relay_url: Option<&str>,
) -> Result<(), String> {
    ghost_broadcast::validate_rtmp_configuration(rtmp_server, rtmp_stream_key)?;
    relay::start(session_id, relay_url, rtmp_server, rtmp_stream_key)?;
    if let Err(error) = crate::native::start_broadcast(
        session_id,
        record_local,
        rtmp_server,
        rtmp_stream_key,
        relay_url,
    )
    .await
    {
        relay::stop(session_id);
        return Err(error);
    }
    Ok(())
}

pub(crate) async fn push(
    session_id: &str,
    sequence: u64,
    timestamp_ms: u64,
    encoded: Vec<u8>,
) -> Result<(), String> {
    let relay_result = relay::push(session_id, sequence, timestamp_ms, &encoded);
    let native_result =
        crate::native::push_broadcast(session_id, sequence, timestamp_ms, encoded).await;
    native_result.and(relay_result)
}

pub(crate) async fn stop(session_id: &str) -> Result<BroadcastStopResult, String> {
    let relay_failures = relay::stop(session_id);
    let mut result = crate::native::stop_broadcast(session_id).await?;
    result.failures.extend(relay_failures);
    Ok(result)
}
