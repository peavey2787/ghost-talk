use std::{cell::RefCell, rc::Rc};

use gloo_timers::future::TimeoutFuture;
use wasm_bindgen_test::*;
use yew::prelude::*;

use super::{presentation, CallEvent, CallManager};
use crate::components::browser_test_support::{click, settle, test_root};
use crate::model::CallPhase;
use ghost_p2p::P2pRouteState;

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    static EVENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn record(event: String) {
    EVENTS.with(|events| events.borrow_mut().push(event));
}

fn take_events() -> Vec<String> {
    EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

#[derive(Properties, PartialEq)]
struct CallHarnessProps {
    call_id: String,
    phase: CallPhase,
    #[prop_or(false)]
    muted: bool,
}

#[component(CallHarness)]
fn call_harness(props: &CallHarnessProps) -> Html {
    let render_epoch = use_state(|| 0u64);
    let manager = use_mut_ref(|| initial_manager(props));
    let p2p_state = use_state(|| P2pRouteState::KaspaFallback);
    let exact_id = props.call_id.clone();

    let actions = call_actions(manager.clone(), render_epoch, exact_id);
    let current = manager.borrow().visible().cloned();
    presentation::call_modal(current, &p2p_state, &actions)
}

fn call_actions(
    manager: Rc<RefCell<CallManager>>,
    render_epoch: UseStateHandle<u64>,
    call_id: String,
) -> presentation::CallActions {
    presentation::CallActions {
        accept: transition_callback(
            manager.clone(),
            render_epoch.clone(),
            call_id.clone(),
            CallEvent::AcceptLocal,
            "accept",
            false,
        ),
        decline: transition_callback(
            manager.clone(),
            render_epoch.clone(),
            call_id.clone(),
            CallEvent::Decline,
            "decline",
            true,
        ),
        hangup: hangup_callback(manager.clone(), render_epoch.clone(), call_id.clone()),
        close_error: Callback::from(|_| {}),
        toggle_mute: mute_callback(manager, render_epoch, call_id),
    }
}

fn transition_callback(
    manager: Rc<RefCell<CallManager>>,
    render_epoch: UseStateHandle<u64>,
    call_id: String,
    event: CallEvent,
    label: &'static str,
    completes: bool,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        record(format!("{label}:{call_id}"));
        crate::controllers::call::transition(&mut manager.borrow_mut(), &call_id, event).unwrap();
        if completes {
            crate::controllers::call::transition(
                &mut manager.borrow_mut(),
                &call_id,
                CallEvent::CompleteEnd,
            )
            .unwrap();
        }
        render_epoch.set((*render_epoch).wrapping_add(1));
    })
}

fn hangup_callback(
    manager: Rc<RefCell<CallManager>>,
    render_epoch: UseStateHandle<u64>,
    call_id: String,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        record(format!("local-end:{call_id}"));
        crate::controllers::call::transition(
            &mut manager.borrow_mut(),
            &call_id,
            CallEvent::Hangup,
        )
        .unwrap();
        crate::controllers::call::transition(
            &mut manager.borrow_mut(),
            &call_id,
            CallEvent::CompleteEnd,
        )
        .unwrap();
        render_epoch.set((*render_epoch).wrapping_add(1));
        let call_id = call_id.clone();
        wasm_bindgen_futures::spawn_local(async move {
            TimeoutFuture::new(0).await;
            record(format!("signal-failed:{call_id}"));
        });
    })
}

fn mute_callback(
    manager: Rc<RefCell<CallManager>>,
    render_epoch: UseStateHandle<u64>,
    call_id: String,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let muted = manager
            .borrow()
            .get(&call_id)
            .is_some_and(|call| call.muted);
        crate::controllers::call::transition(
            &mut manager.borrow_mut(),
            &call_id,
            CallEvent::SetMuted(!muted),
        )
        .unwrap();
        record(format!("mute:{call_id}:{}", !muted));
        render_epoch.set((*render_epoch).wrapping_add(1));
    })
}

fn initial_manager(props: &CallHarnessProps) -> CallManager {
    let mut manager = CallManager::default();
    if props.phase == CallPhase::IncomingRinging {
        let signal = ghost_domain::call::CallSignal {
            call_id: props.call_id.clone(),
            action: "request".into(),
            peer_address: "kaspatest:peer-c".into(),
            peer_hydra_id: "hydra-c".into(),
            peer_label: "Peer C".into(),
        };
        crate::controllers::call::receive_signal(&mut manager, &signal);
        return manager;
    }
    crate::controllers::call::begin_outgoing(
        &mut manager,
        props.call_id.clone(),
        "chat-c".into(),
        "kaspatest:peer-c".into(),
        "hydra-c".into(),
        "Peer C".into(),
    )
    .unwrap();
    if props.phase == CallPhase::Connected {
        crate::controllers::call::transition(&mut manager, &props.call_id, CallEvent::AcceptRemote)
            .unwrap();
        crate::controllers::call::transition(
            &mut manager,
            &props.call_id,
            CallEvent::TransportConnected,
        )
        .unwrap();
        if props.muted {
            crate::controllers::call::transition(
                &mut manager,
                &props.call_id,
                CallEvent::SetMuted(true),
            )
            .unwrap();
        }
    }
    manager
}

#[wasm_bindgen_test(async)]
async fn rendered_mute_unmute_and_hangup_keep_exact_call_id_and_local_end() {
    let _ = take_events();
    let root = test_root();
    yew::Renderer::<CallHarness>::with_root_and_props(
        root.clone(),
        CallHarnessProps {
            call_id: "call-c".into(),
            phase: CallPhase::Connected,
            muted: false,
        },
    )
    .render();
    settle().await;

    click(&root, ".call-actions button:not(.danger)");
    settle().await;
    assert!(root.text_content().unwrap_or_default().contains("Unmute"));
    click(&root, ".call-actions button:not(.danger)");
    settle().await;
    assert!(root.text_content().unwrap_or_default().contains("Mute"));

    click(&root, ".call-actions .danger");
    settle().await;
    assert!(root.query_selector(".voice-call-modal").unwrap().is_none());
    assert_eq!(
        take_events(),
        vec![
            "mute:call-c:true",
            "mute:call-c:false",
            "local-end:call-c",
            "signal-failed:call-c",
        ]
    );
    root.remove();
}

#[wasm_bindgen_test(async)]
async fn rendered_accept_and_decline_bind_the_visible_incoming_call() {
    let _ = take_events();
    let root = test_root();
    yew::Renderer::<CallHarness>::with_root_and_props(
        root.clone(),
        CallHarnessProps {
            call_id: "incoming-c".into(),
            phase: CallPhase::IncomingRinging,
            muted: false,
        },
    )
    .render();
    settle().await;
    click(&root, ".call-actions .primary");
    settle().await;
    assert_eq!(take_events(), vec!["accept:incoming-c"]);
    root.remove();

    let root = test_root();
    yew::Renderer::<CallHarness>::with_root_and_props(
        root.clone(),
        CallHarnessProps {
            call_id: "incoming-d".into(),
            phase: CallPhase::IncomingRinging,
            muted: false,
        },
    )
    .render();
    settle().await;
    click(&root, ".call-actions .danger");
    settle().await;
    assert!(root.query_selector(".voice-call-modal").unwrap().is_none());
    assert_eq!(take_events(), vec!["decline:incoming-d"]);
    root.remove();
}
