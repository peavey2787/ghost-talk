//! One queued realtime job: seal once, then choose the physical carrier.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::future::Either;
use ghost_p2p::{GhostP2pTransport, P2pRouteState};
use ghost_realtime::{parse_session_id, route, RealtimeCarrier, RouteDecision, RoutePreference};

use super::{RealtimeDelivery, RealtimeJob, SenderSnapshot};
use crate::{
    model::{HydraRealtimeEnvelope, MailboxSendResult, Profile},
    random_id,
};

pub(super) struct Target {
    peer: String,
    destination: String,
    message_id: String,
}

pub(super) fn target(profile: &Profile, job: &RealtimeJob) -> Result<Target, String> {
    let chat = profile
        .chats
        .iter()
        .find(|chat| chat.id == job.chat_id)
        .ok_or_else(|| "Realtime chat no longer exists.".to_string())?;
    let peer = chat
        .peer_hydra_handle()
        .map(str::to_owned)
        .ok_or_else(|| "Realtime chat has no authenticated HYDRA peer.".to_string())?;
    let destination = chat
        .peer_kaspa_address()
        .map(str::to_owned)
        .ok_or_else(|| "Realtime chat has no Kaspa destination.".to_string())?;
    Ok(Target {
        peer,
        destination,
        message_id: random_id()?,
    })
}

/// `Ok(None)` means the job was delivered over p2p-net (no wallet change).
pub(super) async fn send(
    snapshot: &SenderSnapshot,
    job: &RealtimeJob,
    target: &Target,
) -> Result<Option<MailboxSendResult>, String> {
    if job.delivery == RealtimeDelivery::DurableControl {
        return crate::controllers::call::send_call_control_message(
            &snapshot.profile,
            &snapshot.password,
            &target.peer,
            &target.destination,
            &job.body,
            &target.message_id,
        )
        .await
        .map(Some);
    }
    // Advance HYDRA exactly once. Every physical carrier gets these exact GTR1
    // bytes, so p2p failure cannot cause a second ratchet send or a second
    // ciphertext for the same logical realtime message.
    let sealed = crate::controllers::call::seal_realtime(
        &snapshot.profile.id,
        &target.peer,
        &target.message_id,
        &job.body,
    )
    .await?;
    if try_p2p(snapshot, decision(snapshot, job.delivery), &sealed).await? {
        trace_sent(snapshot, job, "p2p-net");
        return Ok(None);
    }
    trace_sent(snapshot, job, "kaspa");
    crate::controllers::call::send_realtime_carrier(
        &snapshot.profile,
        &snapshot.password,
        &target.peer,
        &target.destination,
        &sealed.carrier_b64,
    )
    .await
    .map(Some)
}

/// Protocol-debug record of the carrier that took a realtime body.
fn trace_sent(snapshot: &SenderSnapshot, job: &RealtimeJob, carrier: &str) {
    if snapshot.profile.settings.debug_logging {
        let kind = crate::live_voice::body_kind(&job.body);
        crate::controllers::debug::record(
            "realtime",
            "realtime-sent",
            format!("carrier={carrier} kind={kind} chat={}", job.chat_id),
        );
    }
}

fn decision(snapshot: &SenderSnapshot, delivery: RealtimeDelivery) -> RouteDecision {
    let connected = *snapshot.p2p_state == P2pRouteState::Connected;
    let automatic = snapshot.profile.settings.route.eq_ignore_ascii_case("auto");
    match delivery {
        RealtimeDelivery::DirectOnly => RouteDecision {
            primary: RealtimeCarrier::P2pNet,
            fallback: None,
        },
        RealtimeDelivery::Realtime if automatic => route(RoutePreference::Automatic, connected),
        _ => route(RoutePreference::KaspaOnly, connected),
    }
}

/// `Ok(true)` when p2p-net delivered; `Ok(false)` when Kaspa must carry the
/// same sealed bytes; `Err` when p2p-net failed and no fallback is allowed.
async fn try_p2p(
    snapshot: &SenderSnapshot,
    decision: RouteDecision,
    sealed: &HydraRealtimeEnvelope,
) -> Result<bool, String> {
    if decision.primary != RealtimeCarrier::P2pNet {
        return Ok(false);
    }
    match send_p2p_bounded(snapshot, sealed).await {
        Ok(()) => Ok(true),
        Err(error) if decision.fallback.is_none() => Err(error),
        Err(error) => {
            fall_back_to_kaspa(snapshot, error);
            Ok(false)
        }
    }
}

fn fall_back_to_kaspa(snapshot: &SenderSnapshot, error: String) {
    if snapshot.profile.settings.debug_logging {
        crate::controllers::debug::record("realtime", "p2p-send-failed", error);
    }
    snapshot.p2p_state.set(P2pRouteState::KaspaFallback);
}

/// A stalled p2p send must not hold the serial realtime queue.
const P2P_SEND_TIMEOUT_MS: u32 = 3_000;

async fn send_p2p_bounded(
    snapshot: &SenderSnapshot,
    sealed: &HydraRealtimeEnvelope,
) -> Result<(), String> {
    let send = send_p2p(snapshot, sealed);
    let timeout = gloo_timers::future::TimeoutFuture::new(P2P_SEND_TIMEOUT_MS);
    futures_util::pin_mut!(send);
    match futures_util::future::select(send, timeout).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => Err("p2p-net send timed out".to_string()),
    }
}

async fn send_p2p(snapshot: &SenderSnapshot, sealed: &HydraRealtimeEnvelope) -> Result<(), String> {
    let sid = parse_session_id(&sealed.session_sid).map_err(|error| error.to_string())?;
    let packet = STANDARD
        .decode(sealed.carrier_b64.as_bytes())
        .map_err(|_| "sealed GTR1 carrier is not valid base64".to_string())?;
    let p2p = snapshot.p2p.borrow().clone();
    p2p.send(sid, &packet).await
}
