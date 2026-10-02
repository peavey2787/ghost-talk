use ghost_p2p::{GhostP2pTransport, LocalP2pBinding};
use ghost_protocol::{RealtimeBodyV1, RealtimeCapability, TransportAnnounceV1};
use wasm_bindgen_futures::spawn_local;

use super::{route_is_automatic, subscription::ensure_subscription, trace};
use crate::{components::call::CallRuntime, live_voice, model::Chat};

/// Open the p2p route for a conversation whose authenticated session is active.
pub(in crate::components::call) fn start_session(runtime: CallRuntime, chat: Chat) {
    if !chat_is_eligible(&runtime, &chat) {
        return;
    }
    spawn_local(async move {
        if let Err(error) = open_session(&runtime, &chat).await {
            runtime.on_error.emit(error);
        }
    });
}

async fn open_session(runtime: &CallRuntime, chat: &Chat) -> Result<(), String> {
    ensure_subscription(runtime, chat)
        .await
        .map_err(|error| format!("p2p-net subscription: {error}"))?;
    announce(runtime, chat, false, false)
        .await
        .map_err(|error| format!("p2p-net announcement: {error}"))
}

/// Re-announce every eligible conversation, e.g. after the node started or its
/// advertised addresses changed.
pub(super) async fn announce_active_sessions(runtime: &CallRuntime) {
    let chats: Vec<Chat> = runtime
        .profile_ref
        .borrow()
        .chats
        .iter()
        .filter(|chat| chat_is_eligible(runtime, chat))
        .cloned()
        .collect();
    for chat in chats {
        if ensure_subscription(runtime, &chat).await.is_ok() {
            let _ = announce(runtime, &chat, false, false).await;
        }
    }
}

thread_local! {
    /// Last binding announced per chat session (chat id + SID). Every
    /// announcement is a paid Kaspa transaction, so an unchanged binding is
    /// never sent twice for one session; a new session is announced afresh
    /// because the peer binds our PeerId per SID.
    static SENT: std::cell::RefCell<std::collections::HashMap<(String, String), LocalP2pBinding>> =
        std::cell::RefCell::default();
}

/// Record `binding` as announced for the chat session; false when it was
/// already sent for that session.
pub(super) fn mark_sent(chat: &Chat, binding: &LocalP2pBinding) -> bool {
    let key = (
        chat.id.clone(),
        chat.session_sid().unwrap_or_default().to_owned(),
    );
    SENT.with(|sent| {
        let mut sent = sent.borrow_mut();
        if sent.get(&key) == Some(binding) {
            return false;
        }
        sent.insert(key, binding.clone());
        true
    })
}

/// Send this node's transport binding over the authenticated Kaspa realtime
/// carrier, so p2p-net never bootstraps its own identity assertion.
///
/// Skipped when the binding has no dial addresses yet (a later binding change
/// announces it) or when this exact binding was already announced for the
/// chat. `force` answers a peer's new announcement even if ours is unchanged,
/// because that peer may have restarted and lost it.
pub(super) async fn announce(
    runtime: &CallRuntime,
    chat: &Chat,
    ack: bool,
    force: bool,
) -> Result<(), String> {
    if !chat_is_eligible(runtime, chat) {
        return Ok(());
    }
    let binding = local_binding(runtime).await?;
    if binding.dial_addresses.is_empty() || !(mark_sent(chat, &binding) || force) {
        return Ok(());
    }
    let addresses = binding.dial_addresses.len();
    let announcement = TransportAnnounceV1 {
        peer_id: binding.peer_id,
        dial_addresses: binding.dial_addresses,
        capabilities: vec![
            RealtimeCapability::AddressedDelivery,
            RealtimeCapability::Voice,
        ],
    };
    let (event, body) = if ack {
        ("announce-ack", RealtimeBodyV1::TransportAck(announcement))
    } else {
        ("announce", RealtimeBodyV1::TransportAnnounce(announcement))
    };
    let encoded = live_voice::encode_p2p_control(&body)?;
    trace(
        runtime,
        event,
        format!("chat={} dialAddresses={addresses}", chat.id),
    );
    runtime.realtime.enqueue_kaspa(chat.id.clone(), encoded);
    Ok(())
}

async fn local_binding(runtime: &CallRuntime) -> Result<LocalP2pBinding, String> {
    runtime.p2p().local_binding().await
}

pub(super) fn chat_is_eligible(runtime: &CallRuntime, chat: &Chat) -> bool {
    route_is_automatic(runtime) && session_is_eligible(chat)
}

/// An authenticated, live conversation that can carry a transport binding.
pub(super) fn session_is_eligible(chat: &Chat) -> bool {
    chat.bootstrap_complete()
        && chat.session_sid().is_some()
        && chat.peer_hydra_handle().is_some()
        && !chat.archived()
        && !chat.left()
        && !chat.peer_left()
}
