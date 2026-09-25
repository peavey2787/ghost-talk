use ghost_contacts::ContactService;
use ghost_domain::identity::PeerBinding;

use ghost_domain::call::{CallEvent, CallManager};
use ghost_p2p::{GhostP2pTransport, P2pNetTransport, P2pRouteState, P2pStartConfig};
use ghost_talk_wasm::{BrowserVoiceReceiver, BrowserVoiceSender};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::visible_call;
use crate::model::CallRecord;
use crate::{
    live_voice::{self},
    model::{CallPhase, Chat, Profile, ProfilePatch, RealtimeControl},
    random_id,
};
mod runtime_sync;
use runtime_sync::{realtime_wallet_progress_callback, reset_call_owner, sync_profile_ref};

use super::{
    active_call, apply_call_command, can_start_call, close_media, media, p2p_route, presentation,
    signaling, termination, transport, CallContext, CallRuntime, RoomVoiceContext,
};
use crate::controllers::call::RealtimeSender;

mod signal_processing;
use signal_processing::process_signed_call_signal;

#[derive(Properties, PartialEq)]
pub struct LiveCallManagerProps {
    pub profile: Profile,
    pub password: String,
    pub realtime_controls: Vec<RealtimeControl>,
    pub on_control_handled: Callback<String>,
    pub call_events: Vec<crate::model::CallRuntimeEvent>,
    pub on_call_event_handled: Callback<String>,
    pub on_select: Callback<String>,
    pub on_update: Callback<ProfilePatch>,
    pub on_error: Callback<String>,
    #[prop_or_default]
    pub children: Children,
}

#[derive(Properties, PartialEq)]
pub struct CallButtonProps {
    pub chat: Chat,
}

#[component(CallButton)]
pub fn call_button(props: &CallButtonProps) -> Html {
    let context = use_context::<CallContext>();
    let enabled = context
        .as_ref()
        .is_some_and(|context| !context.active && can_start_call(&props.chat));
    let chat = props.chat.clone();
    let onclick = context
        .map(|context| {
            Callback::from(move |_| {
                context.start.emit(chat.clone());
            })
        })
        .unwrap_or_else(|| Callback::from(|_| {}));
    html! { <button class="call-button" disabled={!enabled} {onclick}>{"Call"}</button> }
}

#[component(LiveCallManager)]
pub fn live_call_manager(props: &LiveCallManagerProps) -> Html {
    let render_epoch = use_state(|| 0u64);
    let runtime = use_call_runtime(props, render_epoch);
    use_call_event_effect(props, runtime.clone());
    use_p2p_effect(props, runtime.clone());
    use_realtime_effect(props, runtime.clone());
    use_connecting_effect(runtime.clone());
    let context = CallContext {
        active: active_call(&runtime).is_some(),
        start: start_call_callback(runtime.clone()),
    };
    let room_voice = super::room_voice::context(runtime.clone());
    let current = visible_call(&runtime);
    let actions = call_actions(&runtime, current.clone());
    html! {
        <ContextProvider<CallContext> context={context}>
          <ContextProvider<RoomVoiceContext> context={room_voice}>
            {for props.children.iter()}
            {presentation::call_modal(current, &runtime.p2p_state, &actions)}
          </ContextProvider<RoomVoiceContext>>
        </ContextProvider<CallContext>>
    }
}

#[hook]
fn use_call_runtime(
    props: &LiveCallManagerProps,
    render_epoch: UseStateHandle<u64>,
) -> CallRuntime {
    let profile_ref = use_mut_ref(|| props.profile.clone());
    sync_profile_ref(&profile_ref, &props.profile);
    let call_manager = use_mut_ref(CallManager::default);
    let call_owner = use_mut_ref(|| props.profile.id.clone());
    reset_call_owner(&call_manager, &call_owner, &props.profile.id);
    let sender_ref = use_mut_ref(|| None::<BrowserVoiceSender>);
    let receiver = use_mut_ref(BrowserVoiceReceiver::new);
    let room_voice_id = use_state(|| None::<String>);
    let room_sequence = use_mut_ref(|| 0u64);
    let room_broadcast_session = use_state(|| None::<String>);
    let p2p = use_mut_ref(P2pNetTransport::default);
    let p2p_state = use_state(|| P2pRouteState::Unavailable);
    let p2p_subscriptions = use_mut_ref(std::collections::HashSet::<String>::new);
    let p2p_generation = use_mut_ref(|| 0u64);
    let on_wallet_progress = realtime_wallet_progress_callback(
        profile_ref.clone(),
        render_epoch.clone(),
        props.on_update.clone(),
    );
    let realtime_ref = use_mut_ref(|| {
        RealtimeSender::new(
            props.profile.clone(),
            props.password.clone(),
            p2p.clone(),
            p2p_state.clone(),
            on_wallet_progress.clone(),
            props.on_error.clone(),
        )
    });
    realtime_ref.borrow().sync(
        profile_ref.borrow().clone(),
        props.password.clone(),
        on_wallet_progress,
        props.on_error.clone(),
    );
    let realtime = realtime_ref.borrow().clone();
    CallRuntime {
        profile_ref,
        call_manager,
        render_epoch,
        sender_ref,
        receiver,
        room_voice_id,
        room_sequence,
        room_broadcast_session,
        p2p,
        p2p_state,
        p2p_subscriptions,
        p2p_generation,
        realtime,
        password: props.password.clone(),
        on_select: props.on_select.clone(),
        on_update: props.on_update.clone(),
        on_error: props.on_error.clone(),
    }
}

fn call_actions(runtime: &CallRuntime, current: Option<CallRecord>) -> presentation::CallActions {
    presentation::CallActions {
        accept: signaling::accept_callback(runtime.clone()),
        decline: termination::end_callback(runtime.clone(), current.clone(), CallEvent::Decline),
        hangup: termination::end_callback(runtime.clone(), current.clone(), CallEvent::Hangup),
        close_error: termination::close_error_callback(runtime.clone()),
        toggle_mute: media::toggle_mute_callback(runtime.clone(), current.clone()),
    }
}

fn start_call_callback(runtime: CallRuntime) -> Callback<Chat> {
    Callback::from(move |chat: Chat| {
        if active_call(&runtime).is_some() || !can_start_call(&chat) {
            return;
        }
        let call_id = match random_id() {
            Ok(value) => value,
            Err(error) => {
                runtime.on_error.emit(error);
                return;
            }
        };
        let peer_address = chat
            .peer_kaspa_address()
            .map(str::to_owned)
            .unwrap_or_default();
        let peer_hydra_id = chat
            .peer_hydra_handle()
            .map(str::to_owned)
            .unwrap_or_default();
        if let Err(error) = apply_call_command(&runtime, |calls| {
            crate::controllers::call::begin_outgoing(
                calls,
                call_id.clone(),
                chat.id.clone(),
                peer_address,
                peer_hydra_id,
                chat.label.clone(),
            )
        }) {
            runtime.on_error.emit(error);
            return;
        }
        let runtime = runtime.clone();
        spawn_local(async move {
            signaling::send_initial_ring(runtime, call_id).await;
        });
    })
}

#[hook]
fn use_p2p_effect(props: &LiveCallManagerProps, runtime: CallRuntime) {
    let network = props
        .profile
        .wallet
        .as_ref()
        .map(|wallet| wallet.public.network.clone());
    let key = (
        props.profile.id.clone(),
        network.clone(),
        props.profile.settings.route.clone(),
    );
    use_effect_with(key, move |(profile_id, network_id, route)| {
        let generation = {
            let mut value = runtime.p2p_generation.borrow_mut();
            *value = value.wrapping_add(1);
            *value
        };
        let enabled = route.eq_ignore_ascii_case("auto") && network_id.is_some();
        if enabled {
            runtime.p2p_state.set(P2pRouteState::Starting);
            let runtime_for_start = runtime.clone();
            let config = P2pStartConfig {
                network_id: network_id.clone().expect("enabled p2p has a configured network"),
                profile_id: profile_id.clone(),
            };
            spawn_local(async move {
                let started = match runtime_for_start.p2p.try_borrow_mut() {
                    Ok(mut p2p) => p2p.start(config).await,
                    Err(_) => Err("p2p-net lifecycle is already busy".to_string()),
                };
                if *runtime_for_start.p2p_generation.borrow() != generation {
                    // This start completed after its profile/network effect was
                    // retired. A newer generation cannot have started while the
                    // mutable p2p borrow above was held, so this node is stale.
                    let old = runtime_for_start
                        .p2p
                        .try_borrow_mut()
                        .ok()
                        .map(|mut p2p| std::mem::take(&mut *p2p));
                    if let Some(mut old) = old {
                        let _ = old.shutdown().await;
                    }
                    return;
                }
                match started {
                    Ok(_) => {
                        runtime_for_start.p2p_state.set(P2pRouteState::Connecting);
                        if p2p_route::spawn_event_loop(runtime_for_start.clone(), generation).is_err() {
                            runtime_for_start.p2p_state.set(P2pRouteState::KaspaFallback);
                            return;
                        }
                        p2p_route::announce_active_sessions(&runtime_for_start).await;
                    }
                    Err(_) => runtime_for_start.p2p_state.set(P2pRouteState::KaspaFallback),
                }
            });
        } else {
            runtime.p2p_state.set(P2pRouteState::Unavailable);
        }

        let runtime_for_cleanup = runtime.clone();
        move || {
            {
                let mut value = runtime_for_cleanup.p2p_generation.borrow_mut();
                *value = value.wrapping_add(1);
            }
            runtime_for_cleanup.p2p_subscriptions.borrow_mut().clear();
            runtime_for_cleanup.p2p_state.set(P2pRouteState::Unavailable);
            let old = runtime_for_cleanup
                .p2p
                .try_borrow_mut()
                .ok()
                .map(|mut p2p| std::mem::take(&mut *p2p));
            if let Some(mut old) = old {
                spawn_local(async move {
                    let _ = old.shutdown().await;
                });
            }
        }
    });
}

#[hook]
fn use_realtime_effect(props: &LiveCallManagerProps, runtime: CallRuntime) {
    let controls = props.realtime_controls.clone();
    let on_handled = props.on_control_handled.clone();
    let key = (
        controls.clone(),
        props.profile.state_revision(),
        *runtime.render_epoch,
    );
    use_effect_with(key, move |_| {
        for control in controls.iter().cloned() {
            if process_realtime_control(control.clone(), runtime.clone()) {
                on_handled.emit(control.id);
            }
        }
    });
}

pub(super) fn process_realtime_control(control: RealtimeControl, runtime: CallRuntime) -> bool {
    if let Some(packet) = live_voice::decode_room_voice(&control.body) {
        super::room_voice::process(control, packet, runtime);
        return true;
    }
    if let Some(body) = live_voice::decode_p2p_control(&control.body) {
        spawn_local(async move {
            if let Err(error) =
                p2p_route::process_transport_control(control, body, runtime.clone()).await
            {
                runtime.on_error.emit(format!("p2p-net binding: {error}"));
            }
        });
        return true;
    }
    let Some(packet) = live_voice::decode_live(&control.body) else {
        return true;
    };
    signaling::process_live_packet(&control, packet, &runtime)
}

#[hook]
fn use_connecting_effect(runtime: CallRuntime) {
    let connecting_started = use_mut_ref(|| None::<String>);
    let call = active_call(&runtime);
    let key = call.as_ref().map(|call| (call.call_id.clone(), call.phase));
    use_effect_with(key, move |state| {
        let Some((call_id, CallPhase::Connecting)) = state.clone() else {
            *connecting_started.borrow_mut() = None;
            return;
        };
        if connecting_started.borrow().as_deref() == Some(call_id.as_str()) {
            return;
        }
        *connecting_started.borrow_mut() = Some(call_id.clone());
        let runtime = runtime.clone();
        spawn_local(async move {
            transport::establish_secure_transport(runtime, call_id).await;
        });
    });
}

#[hook]
fn use_call_event_effect(props: &LiveCallManagerProps, runtime: CallRuntime) {
    let events = props.call_events.clone();
    let on_handled = props.on_call_event_handled.clone();
    let key = (
        events.clone(),
        props.profile.id.clone(),
        *runtime.render_epoch,
    );
    use_effect_with(key, move |_| {
        for event in events.iter().cloned() {
            let id = event.id();
            match event {
                crate::model::CallRuntimeEvent::SignedSignal(signal) => {
                    process_signed_call_signal(signal, &runtime);
                }
                crate::model::CallRuntimeEvent::ContactAccepted(accepted) => {
                    if runtime
                        .call_manager
                        .borrow()
                        .find_by_bootstrap_request(&accepted.request_id)
                        .is_some()
                    {
                        let runtime = runtime.clone();
                        spawn_local(async move {
                            if let Err(error) =
                                transport::complete_contact_bootstrap(runtime.clone(), accepted)
                                    .await
                            {
                                runtime.on_error.emit(format!("Call bootstrap: {error}"));
                            }
                        });
                    }
                }
            }
            on_handled.emit(id);
        }
    });
}
