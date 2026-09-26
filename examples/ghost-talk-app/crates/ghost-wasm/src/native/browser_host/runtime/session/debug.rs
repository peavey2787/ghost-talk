//! Redacted protocol-debug projection of the browser KKTP transport.

use super::{with, BrowserTransport};

pub(in crate::native::browser_host) fn debug_value(
    profile_id: &str,
) -> Result<serde_json::Value, String> {
    with(profile_id, |runtime| {
        Ok(serde_json::json!({
            "available": true,
            "identity_id": runtime.identity_id,
            "sessions": debug_sessions(runtime),
            "pending_outbound": joined(runtime.pending_outbound.values().map(|item| format!(
                "sid={} peer={} pending={} message={}",
                item.sid, item.destination, item.id, item.message_id
            ))),
            "prepared_completion": joined(runtime.prepared_completion.iter().map(
                |(peer, value)| format!("peer={peer} message={}", value.message_id)
            )),
            "pending_inbound": serde_json::Value::Null,
            "pending_recovery": serde_json::Value::Null,
            "prepared_recovery_finish": serde_json::Value::Null,
            "retained_delivery_count": 0,
            "retired_sid_count": 0,
        }))
    })
}

fn debug_sessions(runtime: &BrowserTransport) -> Vec<serde_json::Value> {
    let mut sessions = runtime
        .sessions
        .values()
        .map(|binding| {
            let peer_kaspa_address = runtime
                .routes
                .get(&binding.peer)
                .map(|route| route.kaspa_address.clone());
            serde_json::json!({
                "peer_hydra_id": binding.peer,
                "sid": binding.sid,
                "role": binding.role.label(),
                "state": format!("{:?}", binding.state),
                "hydra_status": format!("{:?}", binding.state),
                "mailbox_id": binding.mailbox_id,
                "send_seq": binding.send_seq,
                "recv_next_seq": binding.recv_next_seq,
                "peer_kaspa_address": peer_kaspa_address,
            })
        })
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        left["peer_hydra_id"]
            .as_str()
            .cmp(&right["peer_hydra_id"].as_str())
    });
    sessions
}

/// Sorted " | "-joined summary, or null when empty.
fn joined(items: impl Iterator<Item = String>) -> Option<String> {
    let mut items: Vec<String> = items.collect();
    items.sort();
    (!items.is_empty()).then(|| items.join(" | "))
}
