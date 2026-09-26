use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_p2p::{GhostP2pTransport, P2pNetSubscription, P2pRouteState, P2pSubscription};
use ghost_realtime::{parse_session_id, Gtr1Envelope, P2pIncoming};
use wasm_bindgen_futures::spawn_local;

use super::events::current;
use crate::{
    components::call::CallRuntime,
    model::{Chat, RealtimeControl},
};

/// Subscribe once per session topic and node generation.
pub(super) async fn ensure_subscription(runtime: &CallRuntime, chat: &Chat) -> Result<(), String> {
    let generation = *runtime.p2p_generation.borrow();
    let sid_text = chat
        .session_sid()
        .ok_or_else(|| "p2p subscription requires an active SID".to_string())?
        .to_owned();
    let sid = parse_session_id(&sid_text).map_err(|error| error.to_string())?;
    // Reserve before the async subscribe so two UI effects cannot create two
    // consumers for one session topic.
    if !runtime
        .p2p_subscriptions
        .borrow_mut()
        .insert(sid_text.clone())
    {
        return Ok(());
    }
    let subscribed = subscribe(runtime, sid).await;
    adopt(runtime, sid_text, generation, subscribed)
}

fn adopt(
    runtime: &CallRuntime,
    sid_text: String,
    generation: u64,
    subscribed: Result<P2pNetSubscription, String>,
) -> Result<(), String> {
    if !current(runtime, generation) {
        return Err("p2p subscription belongs to a retired node generation".into());
    }
    match subscribed {
        Ok(subscription) => {
            spawn_local(consume(runtime.clone(), sid_text, generation, subscription));
            Ok(())
        }
        Err(error) => {
            runtime.p2p_subscriptions.borrow_mut().remove(&sid_text);
            Err(error)
        }
    }
}

async fn subscribe(runtime: &CallRuntime, sid: [u8; 16]) -> Result<P2pNetSubscription, String> {
    runtime.p2p().subscribe(sid).await
}

async fn consume(
    runtime: CallRuntime,
    sid_text: String,
    generation: u64,
    mut subscription: P2pNetSubscription,
) {
    while current(&runtime, generation) {
        match subscription.next().await {
            Ok(Some(incoming)) if current(&runtime, generation) => {
                deliver(&runtime, &sid_text, incoming).await
            }
            _ => break,
        }
    }
    if current(&runtime, generation) {
        runtime.p2p_subscriptions.borrow_mut().remove(&sid_text);
        runtime.p2p_state.set(P2pRouteState::KaspaFallback);
    }
}

/// Open an admitted p2p delivery through HYDRA and hand it to the same
/// realtime control path that Kaspa-carried realtime bodies use.
async fn deliver(runtime: &CallRuntime, sid_text: &str, incoming: P2pIncoming) {
    let Ok(carrier) = Gtr1Envelope::decode(&incoming.packet) else {
        return;
    };
    let profile_id = runtime.profile_ref.borrow().id.clone();
    let carrier_b64 = STANDARD.encode(&incoming.packet);
    let Ok(opened) = crate::controllers::call::open_realtime(&profile_id, &carrier_b64).await
    else {
        return;
    };
    let (Some(received), Some(message_id)) = (opened.received, opened.message_id) else {
        return;
    };
    // Claim only after the authenticated HYDRA envelope has opened, so a
    // malformed p2p packet cannot suppress the valid Kaspa fallback copy.
    if received.session_sid.as_deref() != Some(sid_text)
        || !crate::realtime_replay::accept_gtr1(&carrier)
    {
        return;
    }
    let Some(chat) = session_chat(runtime, &received.from, sid_text) else {
        return;
    };
    if runtime.profile_ref.borrow().settings.debug_logging {
        let kind = crate::live_voice::body_kind(&received.plaintext);
        crate::controllers::debug::record(
            "realtime",
            "realtime-received",
            format!("carrier=p2p-net kind={kind} chat={}", chat.id),
        );
    }
    let control = RealtimeControl {
        id: message_id,
        chat_id: chat.id,
        session_sid: sid_text.to_owned(),
        body: received.plaintext,
    };
    if !crate::components::call::direct_text::receive(&control, runtime) {
        crate::components::call::manager::process_realtime_control(control, runtime.clone());
    }
}

fn session_chat(runtime: &CallRuntime, from_hydra: &str, sid_text: &str) -> Option<Chat> {
    runtime
        .profile_ref
        .borrow()
        .chats
        .iter()
        .find(|chat| {
            chat.peer_hydra_handle() == Some(from_hydra) && chat.session_sid() == Some(sid_text)
        })
        .cloned()
}
