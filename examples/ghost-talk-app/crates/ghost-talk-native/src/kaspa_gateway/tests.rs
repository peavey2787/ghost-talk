use super::is_transport_failure;

#[test]
fn classifies_transport_failures_without_evicting_application_errors() {
    assert!(is_transport_failure(
        "WebSocket connection failed: Connection reset without closing handshake"
    ));
    assert!(is_transport_failure("WebSocket connect timeout (15s)"));
    assert!(is_transport_failure("WebSocket RPC response timeout (15s)"));
    assert!(is_transport_failure("WebSocket send failed"));
    assert!(!is_transport_failure("insufficient funds"));
    assert!(!is_transport_failure("recipient address is invalid"));
}
