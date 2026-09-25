use base64::{engine::general_purpose::STANDARD, Engine as _};
use ghost_p2p::{
    GhostP2pTransport, P2pNetEvent, P2pNetSubscription, P2pRouteState, P2pSubscription,
    SessionPeerBinding,
};
use ghost_protocol::{RealtimeBodyV1, RealtimeCapability, TransportAnnounceV1};
use wasm_bindgen_futures::spawn_local;

use crate::{
    live_voice,
    model::{Chat, RealtimeControl},
};

use super::CallRuntime;

pub(super) fn start_session(runtime: CallRuntime, chat: Chat) {
    if !chat_is_p2p_eligible(&runtime, &chat) {
        return;
    }
    spawn_local(async move {
        if let Err(error) = ensure_subscription(&runtime, &chat).await {
            runtime.on_error.emit(format!("p2p-net subscription: {error}"));
            return;
        }
        if let Err(error) = announce(&runtime, &chat, false).await {
            runtime.on_error.emit(format!("p2p-net announcement: {error}"));
        }
    });
}

pub(super) async fn announce_active_sessions(runtime: &CallRuntime) {
    if !runtime
        .profile_ref
        .borrow()
        .settings
        .route
        .eq_ignore_ascii_case("auto")
    {
        return;
    }
    let chats: Vec<Chat> = runtime
        .profile_ref
        .borrow()
        .chats
        .iter()
        .filter(|chat| chat.bootstrap_complete() && chat.session_sid().is_some())
        .filter(|chat| !chat.archived() && !chat.left() && !chat.peer_left())
        .cloned()
        .collect();
    for chat in chats {
        if ensure_subscription(runtime, &chat).await.is_ok() {
            let _ = announce(runtime, &chat, false).await;
        }
    }
}

pub(super) async fn process_transport_control(
    control: RealtimeControl,
    body: RealtimeBodyV1,
    runtime: CallRuntime,
) -> Result<(), String> {
    let (announcement, is_ack) = match body {
        RealtimeBodyV1::TransportAnnounce(value) => (value, false),
        RealtimeBodyV1::TransportAck(value) => (value, true),
        _ => return Err("unsupported p2p transport control body".into()),
    };
    announcement.validate().map_err(|error| error.to_string())?;
    validate_announcement_addresses(&announcement)?;
    if !announcement
        .capabilities
        .contains(&RealtimeCapability::AddressedDelivery)
    {
        return Err("p2p peer does not advertise addressed delivery".into());
    }

    let chat = runtime
        .profile_ref
        .borrow()
        .chats
        .iter()
        .find(|chat| chat.id == control.chat_id)
        .cloned()
        .ok_or_else(|| "p2p transport control refers to an unknown chat".to_string())?;
    let sid_text = chat
        .session_sid()
        .ok_or_else(|| "p2p transport control has no active SID".to_string())?;
    if sid_text != control.session_sid {
        return Err("p2p transport control SID is stale".into());
    }
    let peer_hydra = chat
        .peer_hydra_handle()
        .ok_or_else(|| "p2p transport control has no authenticated HYDRA peer".to_string())?;
    let sid = decode_hex_array::<16>(sid_text, "p2p session SID")?;
    let hydra_id = decode_hex_array::<32>(peer_hydra, "p2p peer HYDRA id")?;

    {
        let mut p2p = runtime
            .p2p
            .try_borrow_mut()
            .map_err(|_| "p2p-net is busy with another lifecycle operation".to_string())?;
        p2p.bind_session(SessionPeerBinding {
            sid,
            hydra_id,
            peer_id: announcement.peer_id.clone(),
        })
        .await?;
    }

    ensure_subscription(&runtime, &chat).await?;
    runtime.p2p_state.set(P2pRouteState::Connecting);
    let mut dial_started = false;
    for address in &announcement.dial_addresses {
        let result = match runtime.p2p.try_borrow_mut() {
            Ok(mut p2p) => p2p.connect(address).await,
            Err(_) => Err("p2p-net is busy with another lifecycle operation".into()),
        };
        if result.is_ok() {
            dial_started = true;
            break;
        }
    }
    if !dial_started && !announcement.dial_addresses.is_empty() {
        runtime.p2p_state.set(P2pRouteState::KaspaFallback);
    }

    if !is_ack {
        announce(&runtime, &chat, true).await?;
    }
    Ok(())
}

pub(super) fn spawn_event_loop(runtime: CallRuntime, generation: u64) -> Result<(), String> {
    let mut events = runtime
        .p2p
        .try_borrow()
        .map_err(|_| "p2p-net is busy during event subscription".to_string())?
        .subscribe_events()?;
    spawn_local(async move {
        loop {
            if *runtime.p2p_generation.borrow() != generation {
                break;
            }
            let event = match events.next().await {
                Ok(event) => event,
                Err(error) => {
                    if *runtime.p2p_generation.borrow() == generation {
                        runtime.p2p_state.set(P2pRouteState::KaspaFallback);
                        runtime.on_error.emit(format!("p2p-net events: {error}"));
                    }
                    break;
                }
            };
            if *runtime.p2p_generation.borrow() != generation {
                break;
            }
            match event {
                P2pNetEvent::LocalBindingChanged(_) => {
                    announce_active_sessions(&runtime).await;
                }
                P2pNetEvent::PeerConnected(peer_id) => {
                    let bound = runtime
                        .p2p
                        .try_borrow()
                        .map(|p2p| p2p.has_bound_peer(&peer_id))
                        .unwrap_or(false);
                    if bound {
                        runtime.p2p_state.set(P2pRouteState::Connected);
                    }
                }
                P2pNetEvent::PeerDisconnected(peer_id) => {
                    let bound = runtime
                        .p2p
                        .try_borrow()
                        .map(|p2p| p2p.has_bound_peer(&peer_id))
                        .unwrap_or(false);
                    if bound {
                        runtime.p2p_state.set(P2pRouteState::KaspaFallback);
                    }
                }
                P2pNetEvent::Online => {
                    if *runtime.p2p_state == P2pRouteState::Unavailable {
                        runtime.p2p_state.set(P2pRouteState::Connecting);
                    }
                }
                P2pNetEvent::Offline => runtime.p2p_state.set(P2pRouteState::KaspaFallback),
            }
        }
    });
    Ok(())
}

async fn announce(runtime: &CallRuntime, chat: &Chat, ack: bool) -> Result<(), String> {
    if !chat_is_p2p_eligible(runtime, chat) {
        return Ok(());
    }
    let binding = match runtime.p2p.try_borrow() {
        Ok(p2p) => p2p.local_binding().await?,
        Err(_) => return Err("p2p-net is busy with another lifecycle operation".into()),
    };
    let announcement = TransportAnnounceV1 {
        peer_id: binding.peer_id,
        dial_addresses: binding.dial_addresses,
        capabilities: vec![RealtimeCapability::AddressedDelivery, RealtimeCapability::Voice],
    };
    announcement.validate().map_err(|error| error.to_string())?;
    let body = if ack {
        RealtimeBodyV1::TransportAck(announcement)
    } else {
        RealtimeBodyV1::TransportAnnounce(announcement)
    };
    runtime
        .realtime
        .enqueue_kaspa(chat.id.clone(), live_voice::encode_p2p_control(&body)?);
    Ok(())
}

async fn ensure_subscription(runtime: &CallRuntime, chat: &Chat) -> Result<(), String> {
    let generation = *runtime.p2p_generation.borrow();
    let sid_text = chat
        .session_sid()
        .ok_or_else(|| "p2p subscription requires an active SID".to_string())?;
    let sid = decode_hex_array::<16>(sid_text, "p2p session SID")?;
    if runtime.p2p_subscriptions.borrow().contains(sid_text) {
        return Ok(());
    }
    // Reserve before the async subscribe so two UI effects cannot create two
    // consumers for one session topic.
    runtime
        .p2p_subscriptions
        .borrow_mut()
        .insert(sid_text.to_string());
    let subscription = match runtime.p2p.try_borrow() {
        Ok(p2p) => p2p.subscribe(sid).await,
        Err(_) => Err("p2p-net is busy with another lifecycle operation".into()),
    };
    let subscription = match subscription {
        Ok(subscription) => subscription,
        Err(error) => {
            if *runtime.p2p_generation.borrow() == generation {
                runtime.p2p_subscriptions.borrow_mut().remove(sid_text);
            }
            return Err(error);
        }
    };
    if *runtime.p2p_generation.borrow() != generation {
        return Err("p2p subscription belongs to a retired node generation".into());
    }
    spawn_subscription(runtime.clone(), sid_text.to_string(), generation, subscription);
    Ok(())
}

fn spawn_subscription(
    runtime: CallRuntime,
    sid_text: String,
    generation: u64,
    mut subscription: P2pNetSubscription,
) {
    spawn_local(async move {
        while *runtime.p2p_generation.borrow() == generation {
            let incoming = match subscription.next().await {
                Ok(Some(incoming)) => incoming,
                _ => break,
            };
            if *runtime.p2p_generation.borrow() != generation {
                break;
            }
            let Ok(carrier) = ghost_protocol::Gtr1Envelope::decode(&incoming.packet) else {
                continue;
            };
            let carrier_b64 = STANDARD.encode(&incoming.packet);
            let profile_id = runtime.profile_ref.borrow().id.clone();
            let result = match crate::controllers::call::open_realtime(
                &profile_id,
                &carrier_b64,
            )
            .await
            {
                Ok(result) => result,
                Err(_) => continue,
            };
            let (Some(received), Some(message_id)) = (result.received, result.message_id) else {
                continue;
            };
            if received.session_sid.as_deref() != Some(sid_text.as_str()) {
                continue;
            }
            // Claim only after the authenticated HYDRA envelope has opened.
            // A malformed p2p packet therefore cannot suppress a valid Kaspa
            // fallback carrying the same logical message id.
            if !crate::realtime_replay::accept_gtr1(&carrier) {
                continue;
            }
            let chat = runtime
                .profile_ref
                .borrow()
                .chats
                .iter()
                .find(|chat| {
                    chat.peer_hydra_handle() == Some(received.from.as_str())
                        && chat.session_sid() == Some(sid_text.as_str())
                })
                .cloned();
            let Some(chat) = chat else {
                continue;
            };
            let control = RealtimeControl {
                id: message_id,
                chat_id: chat.id,
                session_sid: sid_text.clone(),
                body: received.plaintext,
            };
            let _ = super::manager::process_realtime_control(control, runtime.clone());
        }
        if *runtime.p2p_generation.borrow() == generation {
            runtime.p2p_subscriptions.borrow_mut().remove(&sid_text);
            runtime.p2p_state.set(P2pRouteState::KaspaFallback);
        }
    });
}

fn chat_is_p2p_eligible(runtime: &CallRuntime, chat: &Chat) -> bool {
    runtime
        .profile_ref
        .borrow()
        .settings
        .route
        .eq_ignore_ascii_case("auto")
        && chat.bootstrap_complete()
        && chat.session_sid().is_some()
        && chat.peer_hydra_handle().is_some()
        && !chat.archived()
        && !chat.left()
        && !chat.peer_left()
}

fn validate_announcement_addresses(announcement: &TransportAnnounceV1) -> Result<(), String> {
    for address in &announcement.dial_addresses {
        let Some((_, target_peer)) = address.rsplit_once("/p2p/") else {
            return Err("p2p announcement address is missing its destination PeerId".into());
        };
        if target_peer != announcement.peer_id {
            return Err("p2p announcement address PeerId does not match the authenticated binding".into());
        }
    }
    Ok(())
}

fn decode_hex_array<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{label} is not valid hex"));
    }
    let bytes = hex::decode(value).map_err(|_| format!("{label} is not valid hex"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{label} has an invalid length"))
}
