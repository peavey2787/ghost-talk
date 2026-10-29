pub(super) fn normalized_override(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

pub(super) fn is_transport_failure(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    // Portal 1.0.1 performs its own reconnect/replay before returning these
    // failures. Classify the public error strings only after Portal gives up so
    // the application can try its next resolver endpoint.
    error.contains("websocket connection failed")
        || error.contains("websocket connect timeout")
        || error.contains("websocket rpc response timeout")
        || error.contains("websocket send failed")
        || error.contains("kaspa wrpc driver stopped")
        || error.contains("connection reset without closing handshake")
        || error.contains("closed before kaspa wrpc frame")
        || error.contains("kaspa wrpc connection ended")
        || error == super::block_pump::BLOCK_STREAM_STALLED
}

#[cfg(test)]
mod tests {
    use super::is_transport_failure;
    use crate::kaspa_gateway::block_pump::BLOCK_STREAM_STALLED;

    #[test]
    fn a_stalled_block_stream_moves_to_the_next_endpoint() {
        assert!(is_transport_failure(BLOCK_STREAM_STALLED));
        assert!(!is_transport_failure("subscription rejected"));
    }
}
