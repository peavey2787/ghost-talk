use futures_util::future::Either;
use ghost_p2p::{GhostP2pTransport, P2pRouteState, SessionPeerBinding};
use ghost_protocol::{RealtimeBodyV1, RealtimeCapability, TransportAnnounceV1};
use ghost_realtime::{parse_hydra_id, parse_session_id};

use super::{announce::announce, subscription::ensure_subscription, trace};
use crate::{
    components::call::CallRuntime,
    model::{Chat, RealtimeControl},
};

/// A route that never answers (e.g. a NAT that does not hairpin) must not
/// block the remaining announced routes.
const DIAL_TIMEOUT_MS: u32 = 10_000;

/// Browsers reach a peer only through a WebRTC-direct (with certhash),
/// WebSocket or WebTransport hop; raw TCP/QUIC routes cannot be dialed.
fn browser_dialable(address: &str) -> bool {
    let first_hop = address.split("/p2p-circuit").next().unwrap_or(address);
    first_hop.contains("/webrtc-direct/certhash/")
        || first_hop.contains("/ws/")
        || first_hop.contains("/wss/")
        || first_hop.ends_with("/ws")
        || first_hop.ends_with("/wss")
        || first_hop.contains("/webtransport")
}

/// Handle a peer's authenticated transport announcement (or its ack): bind the
/// announced PeerId to the session's SID/HYDRA identity, subscribe, dial, and
/// acknowledge with our own binding.
pub(in crate::components::call) async fn process_transport_control(
    control: RealtimeControl,
    body: RealtimeBodyV1,
    runtime: CallRuntime,
) -> Result<(), String> {
    let announcement = body.announcement();
    validate(announcement)?;
    let chat = control_chat(&runtime, &control)?;
    bind(&runtime, &chat, announcement).await?;
    ensure_subscription(&runtime, &chat).await?;
    trace(
        &runtime,
        if body.is_ack() {
            "announce-ack-received"
        } else {
            "announce-received"
        },
        format!(
            "chat={} sid={} peer={}",
            chat.id,
            chat.session_sid().unwrap_or_default(),
            announcement.peer_id
        ),
    );
    dial(&runtime, announcement).await;
    mark_if_connected(&runtime, &announcement.peer_id);
    super::events::schedule_redial(&runtime, &announcement.peer_id);
    // Announcements are sent once per binding and session, so one arriving
    // again means the peer's node restarted and lost our binding: always
    // answer it. (Kaspa redelivery never reaches here twice; the realtime
    // replay window processes each packet once.)
    if !body.is_ack() {
        announce(&runtime, &chat, true, true).await?;
    }
    Ok(())
}

fn validate(announcement: &TransportAnnounceV1) -> Result<(), String> {
    announcement.validate().map_err(|error| error.to_string())?;
    if !announcement
        .capabilities
        .contains(&RealtimeCapability::AddressedDelivery)
    {
        return Err("p2p peer does not advertise addressed delivery".into());
    }
    let foreign = announcement.dial_addresses.iter().any(|address| {
        address
            .rsplit_once("/p2p/")
            .is_none_or(|(_, target)| target != announcement.peer_id)
    });
    if foreign {
        return Err("p2p announcement address does not target the announced PeerId".into());
    }
    Ok(())
}

fn control_chat(runtime: &CallRuntime, control: &RealtimeControl) -> Result<Chat, String> {
    let chat = runtime
        .profile_ref
        .borrow()
        .chats
        .iter()
        .find(|chat| chat.id == control.chat_id)
        .cloned()
        .ok_or_else(|| "p2p transport control refers to an unknown chat".to_string())?;
    if chat.session_sid() != Some(control.session_sid.as_str()) {
        return Err("p2p transport control SID is stale".into());
    }
    Ok(chat)
}

async fn bind(
    runtime: &CallRuntime,
    chat: &Chat,
    announcement: &TransportAnnounceV1,
) -> Result<(), String> {
    let sid = parse_session_id(chat.session_sid().unwrap_or_default())
        .map_err(|error| error.to_string())?;
    let peer_hydra = chat
        .peer_hydra_handle()
        .ok_or_else(|| "p2p transport control has no authenticated HYDRA peer".to_string())?;
    let hydra_id = parse_hydra_id(peer_hydra).map_err(|error| error.to_string())?;
    let binding = SessionPeerBinding {
        sid,
        hydra_id,
        peer_id: announcement.peer_id.clone(),
    };
    runtime.p2p().bind_session(binding).await
}

thread_local! {
    /// Last authenticated dial addresses per bound PeerId, for re-dialing.
    static PEER_ADDRESSES: std::cell::RefCell<std::collections::HashMap<String, Vec<String>>> =
        std::cell::RefCell::default();
}

async fn dial(runtime: &CallRuntime, announcement: &TransportAnnounceV1) {
    PEER_ADDRESSES.with(|peers| {
        peers.borrow_mut().insert(
            announcement.peer_id.clone(),
            announcement.dial_addresses.clone(),
        )
    });
    dial_addresses(runtime, &announcement.dial_addresses).await;
}

/// Re-dial a bound peer from its last announcement (after an idle drop).
pub(super) async fn redial(runtime: &CallRuntime, peer_id: &str) {
    let addresses = PEER_ADDRESSES.with(|peers| peers.borrow().get(peer_id).cloned());
    if let Some(addresses) = addresses {
        dial_addresses(runtime, &addresses).await;
    }
}

async fn dial_addresses(runtime: &CallRuntime, addresses: &[String]) {
    if addresses.is_empty() {
        return;
    }
    if !addresses.iter().any(|address| browser_dialable(address)) {
        trace(
            runtime,
            "dial-skipped",
            format!("no browser-dialable address in {addresses:?}"),
        );
    }
    runtime.p2p_state.set(P2pRouteState::Connecting);
    for address in addresses.iter().filter(|address| browser_dialable(address)) {
        if dial_one(runtime, address).await {
            return;
        }
    }
    runtime.p2p_state.set(P2pRouteState::KaspaFallback);
}

/// Dial one address within the dial timeout; true when it connected.
async fn dial_one(runtime: &CallRuntime, address: &str) -> bool {
    let mut p2p = runtime.p2p();
    let dial = p2p.connect(address);
    let timeout = gloo_timers::future::TimeoutFuture::new(DIAL_TIMEOUT_MS);
    futures_util::pin_mut!(dial);
    let failure = match futures_util::future::select(dial, timeout).await {
        Either::Left((Ok(()), _)) => {
            trace(runtime, "dialed", address.to_owned());
            return true;
        }
        Either::Left((Err(error), _)) => error,
        Either::Right(_) => "timed out".to_owned(),
    };
    trace(runtime, "dial-failed", format!("{address}: {failure}"));
    false
}

/// The remote side may have dialed first; its PeerConnected event could have
/// arrived before this binding existed, so check the connected peers.
pub(super) fn mark_if_connected(runtime: &CallRuntime, peer_id: &str) {
    if runtime.swarm.is_connected(peer_id) {
        runtime.p2p_state.set(P2pRouteState::Connected);
    }
}
