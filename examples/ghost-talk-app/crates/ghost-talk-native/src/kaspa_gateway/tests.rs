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

#[tokio::test(flavor = "current_thread")]
async fn a_non_websocket_override_is_rejected_before_any_connection() {
    let error = super::KaspaGatewayState::default()
        .portal_for_network("testnet-10", Some("https://node.example.org"))
        .await
        .err()
        .expect("only ws:// or wss:// endpoints are accepted");
    assert_eq!(error, "custom Kaspa RPC endpoint must use ws:// or wss://");
}

#[tokio::test(flavor = "current_thread")]
async fn an_unreachable_explicit_endpoint_reports_the_portal_failure() {
    // Nothing listens on a port that was just released.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("ephemeral port")
        .port();
    let state = super::KaspaGatewayState::default();
    let error = state
        .portal_for_network("testnet-10", Some(&format!("ws://127.0.0.1:{port}")))
        .await
        .err()
        .expect("no node listens on the released port");
    assert!(error.contains("WebSocket"), "{error}");
    assert!(state.inner.read().await.connection.is_none());
}
