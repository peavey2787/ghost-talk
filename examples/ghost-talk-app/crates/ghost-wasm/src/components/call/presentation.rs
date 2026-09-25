use ghost_domain::call::{CallPhase, CallRecord};
use ghost_p2p::P2pRouteState;
use yew::prelude::*;

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
) -> Html {
    let Some(call) = current else {
        return html! {};
    };
    let route = p2p_route_label(p2p_state);
    let phase = call_phase_label(call.phase);
    html! {
      <div class="call-modal-backdrop">
        <section class="call-modal" role="dialog" aria-modal="true" aria-label="Voice call">
          <h3>{call.peer_label.clone()}</h3>
          <p class="call-state">{phase}</p>
          <p class="call-route">{route}</p>
          {call_error(&call)}
          {call_controls(&call, actions)}
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

fn call_phase_label(phase: CallPhase) -> &'static str {
    match phase {
        CallPhase::OutgoingRinging => "Calling…",
        CallPhase::IncomingRinging => "Incoming call",
        CallPhase::Connecting => "Connecting…",
        CallPhase::Connected => "Connected",
        CallPhase::Ended => "Call ended",
        CallPhase::Failed => "Call failed",
    }
}

fn call_error(call: &CallRecord) -> Html {
    call.error
        .as_ref()
        .map(|error| html! { <p class="call-error">{error}</p> })
        .unwrap_or_default()
}

fn call_controls(call: &CallRecord, actions: &CallActions) -> Html {
    match call.phase {
        CallPhase::IncomingRinging => html! {
          <div class="call-controls">
            <button onclick={actions.accept.clone()}> {"Accept"} </button>
            <button onclick={actions.decline.clone()}> {"Decline"} </button>
          </div>
        },
        CallPhase::OutgoingRinging | CallPhase::Connecting | CallPhase::Connected => html! {
          <div class="call-controls">
            if call.phase == CallPhase::Connected {
              <button onclick={actions.toggle_mute.clone()}>
                {if call.muted { "Unmute" } else { "Mute" }}
              </button>
            }
            <button onclick={actions.hangup.clone()}> {"Hang up"} </button>
          </div>
        },
        CallPhase::Ended | CallPhase::Failed => html! {
          <div class="call-controls">
            <button onclick={actions.close_error.clone()}> {"Close"} </button>
          </div>
        },
    }
}
