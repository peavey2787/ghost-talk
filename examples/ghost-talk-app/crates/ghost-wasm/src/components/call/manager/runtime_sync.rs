//! Builds the call runtime from the manager's props and keeps its long-lived
//! refs (profile, call owner, realtime sender) in step with them.

use crate::controllers::call::RealtimeSender;
use crate::model::{Profile, ProfilePatch, WalletProjection};
use ghost_domain::call::CallManager;
use ghost_p2p::{P2pNetTransport, P2pRouteState};
use ghost_talk_wasm::{BrowserVoiceReceiver, BrowserVoiceSender};
use std::{cell::RefCell, rc::Rc};
use yew::prelude::*;

use super::super::CallRuntime;
use super::LiveCallManagerProps;

/// Build the call runtime for this render, keeping long-lived state in refs.
#[hook]
pub(super) fn use_call_runtime(
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
    let room_voice_ref = use_mut_ref(|| None::<String>);
    let room_sequence = use_mut_ref(|| 0u64);
    let room_broadcast_session = use_state(|| None::<String>);
    let room_broadcast_ref = use_mut_ref(|| None::<String>);
    let p2p = use_mut_ref(P2pNetTransport::default);
    let p2p_state = use_state(|| P2pRouteState::Unavailable);
    let p2p_subscriptions = use_mut_ref(std::collections::HashSet::<String>::new);
    let p2p_generation = use_mut_ref(|| 0u64);
    let swarm = super::super::swarm::use_swarm_tracker();
    let rooms = super::super::swarm::use_room_directory_tracker();
    let realtime = use_realtime_sender(props, &profile_ref, &render_epoch, &p2p, &p2p_state);
    CallRuntime {
        profile_ref,
        call_manager,
        render_epoch,
        sender_ref,
        receiver,
        room_voice_id,
        room_voice_ref,
        room_sequence,
        room_broadcast_session,
        room_broadcast_ref,
        p2p,
        p2p_state,
        p2p_subscriptions,
        p2p_generation,
        swarm,
        rooms,
        realtime,
        password: props.password.clone(),
        on_select: props.on_select.clone(),
        on_update: props.on_update.clone(),
        on_error: props.on_error.clone(),
        on_swarm_mailbox: props.on_swarm_mailbox.clone(),
    }
}

fn reset_call_owner(
    manager: &Rc<RefCell<CallManager>>,
    owner: &Rc<RefCell<String>>,
    profile_id: &str,
) {
    if *owner.borrow() != profile_id {
        manager.borrow_mut().reset();
        *owner.borrow_mut() = profile_id.to_owned();
    }
}

fn sync_profile_ref(profile_ref: &Rc<RefCell<Profile>>, incoming: &Profile) {
    let replace = {
        let current = profile_ref.borrow();
        current.id != incoming.id || incoming.state_revision() > current.state_revision()
    };
    if replace {
        *profile_ref.borrow_mut() = incoming.clone();
    }
}

fn realtime_wallet_progress_callback(
    profile_ref: Rc<RefCell<Profile>>,
    render_epoch: UseStateHandle<u64>,
    on_update: Callback<ProfilePatch>,
) -> Callback<WalletProjection> {
    Callback::from(move |public| {
        let before = profile_ref.borrow().clone();
        let Some((profile, patch)) =
            crate::controllers::account::apply_wallet_progress(&before, public)
        else {
            return;
        };
        *profile_ref.borrow_mut() = profile;
        render_epoch.set((*render_epoch).wrapping_add(1));
        on_update.emit(patch);
    })
}

/// One realtime sender per call runtime, kept in sync with the live profile.
#[hook]
fn use_realtime_sender(
    props: &LiveCallManagerProps,
    profile_ref: &Rc<RefCell<Profile>>,
    render_epoch: &UseStateHandle<u64>,
    p2p: &Rc<RefCell<ghost_p2p::P2pNetTransport>>,
    p2p_state: &UseStateHandle<ghost_p2p::P2pRouteState>,
) -> RealtimeSender {
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
        p2p_state.clone(),
        on_wallet_progress,
        props.on_error.clone(),
    );
    let realtime = realtime_ref.borrow().clone();
    realtime
}
