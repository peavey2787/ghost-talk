//! Classify Portal errors that a send may retry.

/// True when a Portal error means the connection to the node failed, so the
/// operation may be retried on a fresh connection or another endpoint. Portal
/// performs its own reconnect/replay before returning these, so they are final
/// for the current connection.
pub fn is_portal_transport_failure(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("websocket connection failed")
        || error.contains("websocket connect timeout")
        || error.contains("websocket rpc response timeout")
        || error.contains("websocket send failed")
        || error.contains("kaspa wrpc driver stopped")
        || error.contains("connection reset without closing handshake")
        || error.contains("closed before kaspa wrpc frame")
        || error.contains("kaspa wrpc connection ended")
}

/// True when the node rejected a transaction only because it does not know a
/// parent yet (typically change from a send submitted through another node
/// that has not propagated). Resubmitting shortly after succeeds.
pub fn is_orphan_rejection(error: &str) -> bool {
    error.to_ascii_lowercase().contains("is an orphan")
}

#[cfg(test)]
mod tests {
    use super::{is_orphan_rejection, is_portal_transport_failure};

    #[test]
    fn socket_failures_are_transport_failures() {
        assert!(is_portal_transport_failure(
            "transaction error: WebSocket connection failed: WebSocket error"
        ));
        assert!(is_portal_transport_failure("Kaspa wRPC driver stopped"));
    }

    #[test]
    fn node_rejections_are_not_transport_failures() {
        assert!(!is_portal_transport_failure("subscription rejected"));
        assert!(!is_portal_transport_failure(
            "transaction error: insufficient funds"
        ));
    }

    #[test]
    fn a_missing_parent_is_an_orphan_rejection() {
        assert!(is_orphan_rejection(
            "RPC error: Rejected transaction b5a5: transaction b5a5 is an orphan where orphan is disallowed"
        ));
        assert!(!is_orphan_rejection(
            "transaction error: insufficient funds"
        ));
    }
}
