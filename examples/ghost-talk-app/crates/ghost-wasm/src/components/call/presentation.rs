use ghost_domain::call::{CallPhase, CallRecord};
use ghost_p2p::P2pRouteState;
use yew::prelude::*;

use super::{video, VideoControls};

#[derive(Clone)]
pub(super) struct CallActions {
    pub(super) accept: Callback<MouseEvent>,
    pub(super) decline: Callback<MouseEvent>,
    pub(super) hangup: Callback<MouseEvent>,
    pub(super) close_error: Callback<MouseEvent>,
    pub(super) toggle_mute: Callback<MouseEvent>,
}

pub(super) fn call_modal(
    current: Option<CallRecord>,
    p2p_state: &UseStateHandle<P2pRouteState>,
    actions: &CallActions,
    video: &VideoControls,
) -> Html {
    let Some(call) = current else {
        return html! {};
    };
    let route = p2p_route_label(p2p_state);
    let phase = call_phase_label(call.phase);
    html! {
      <div class="call-modal-backdrop">
        <section class="voice-call-modal" role="dialog" aria-modal="true" aria-label="Call">
          <h3>{call.peer_label.clone()}</h3>
          <p class="call-state">{phase}</p>
          <p class="call-state call-route">{route}</p>
          {call_video(&call)}
          {call_error(&call)}
          {call_controls(&call, actions, video)}
        </section>
      </div>
    }
}

fn p2p_route_label(state: &UseStateHandle<P2pRouteState>) -> &'static str {
    match **state {
        P2pRouteState::Connected => "P2P connected",
        P2pRouteState::Starting | P2pRouteState::Connecting => "P2P connecting",
        P2pRouteState::KaspaFallback | P2pRouteState::Unavailable => "Kaspa fallback",
    }
}

const PHASE_LABELS: [(CallPhase, &str); 8] = [
    (CallPhase::OutgoingRinging, "Calling…"),
    (CallPhase::IncomingRinging, "Incoming call"),
    (CallPhase::Answering, "Answering…"),
    (CallPhase::Connecting, "Connecting…"),
    (CallPhase::Connected, "Connected"),
    (CallPhase::Ending, "Ending call…"),
    (CallPhase::Ended, "Call ended"),
    (CallPhase::Failed, "Call failed"),
];

fn call_phase_label(phase: CallPhase) -> &'static str {
    PHASE_LABELS
        .iter()
        .find(|(candidate, _)| *candidate == phase)
        .map_or("Call", |(_, label)| label)
}

fn call_error(call: &CallRecord) -> Html {
    call.error
        .as_ref()
        .map(|error| html! { <p class="call-error">{error}</p> })
        .unwrap_or_default()
}

/// The peer's camera and this device's own preview; a tile stays hidden
/// until frames arrive.
fn call_video(call: &CallRecord) -> Html {
    if call.phase != CallPhase::Connected {
        return Html::default();
    }
    html! {
      <div class="video-grid">
        <img class="video-tile" data-ghost-video={video::CALL_TILE} alt={format!("{} video", call.peer_label)}/>
        <img class="video-tile local" data-ghost-video={video::LOCAL_TILE} alt="Your camera"/>
      </div>
    }
}

/// Start/stop the camera; disabled while no p2p-net route can carry video.
pub(crate) fn video_button(video: &VideoControls) -> Html {
    let toggle = video.toggle.clone();
    let title = (!video.available && !video.on).then_some("Video needs a connected P2P route");
    html! {
      <button class="video-toggle" disabled={!video.available && !video.on} {title}
        onclick={Callback::from(move |_| toggle.emit(()))}>
        {if video.on { "Stop video" } else { "Start video" }}
      </button>
    }
}

fn call_controls(call: &CallRecord, actions: &CallActions, video: &VideoControls) -> Html {
    match call.phase {
        CallPhase::IncomingRinging => html! {
          <div class="call-actions">
            <button class="primary" onclick={actions.accept.clone()}> {"Accept"} </button>
            <button class="danger" onclick={actions.decline.clone()}> {"Decline"} </button>
          </div>
        },
        CallPhase::OutgoingRinging
        | CallPhase::Answering
        | CallPhase::Connecting
        | CallPhase::Connected => html! {
          <div class="call-actions">
            if call.phase == CallPhase::Connected {
              <button onclick={actions.toggle_mute.clone()}>
                {if call.muted { "Unmute" } else { "Mute" }}
              </button>
              {video_button(video)}
            }
            <button class="danger" onclick={actions.hangup.clone()}> {"Hang up"} </button>
          </div>
        },
        CallPhase::Ending | CallPhase::Ended | CallPhase::Failed => html! {
          <div class="call-actions">
            <button onclick={actions.close_error.clone()}> {"Close"} </button>
          </div>
        },
    }
}
