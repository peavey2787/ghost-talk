pub(super) fn log_call_signal(
    profile_id: &str,
    signal_id: &str,
    call_id: &str,
    action: &str,
    destination: &str,
    transaction_id: Option<&str>,
) {
    let (event, suffix) = match transaction_id {
        Some(txid) => ("signal-send-accepted-by-kaspa", format!(" txid={txid}")),
        None => ("signal-send-start", String::new()),
    };
    crate::debug_log::record("info", "call", event, format!("profile={profile_id} signal={signal_id} call={call_id} action={action} destination={destination}{suffix}"));
}

pub(super) fn log_contact_accept_start(profile_id: &str, identity_id: &str, request_bytes: usize) {
    crate::debug_log::record(
        "info",
        "handshake",
        "response-send-start",
        format!("profile={profile_id} identity={identity_id} signed_request_bytes={request_bytes}"),
    );
}

pub(super) fn log_contact_accept_verified(
    profile_id: &str,
    request_id: &str,
    peer_id: &str,
    destination: &str,
    acceptor: &str,
) {
    crate::debug_log::record("info", "handshake", "response-request-verified", format!("profile={profile_id} sid={request_id} peer={peer_id} destination={destination} acceptor={acceptor}"));
}

pub(super) fn log_contact_accept_sent(
    profile_id: &str,
    request_id: &str,
    txid: &str,
    destination: &str,
) {
    crate::debug_log::record(
        "info",
        "handshake",
        "response-send-accepted-by-kaspa",
        format!("profile={profile_id} sid={request_id} txid={txid} destination={destination}"),
    );
}
