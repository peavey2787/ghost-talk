use crate::model::{DebugLogEntry, DebugLogSnapshot};
use gloo_timers::future::TimeoutFuture;
use serde_json::Value;
use std::{cell::Cell, rc::Rc};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DebugLogProps {
    pub profile_id: String,
    pub open: bool,
    pub on_close: Callback<()>,
}

#[component(DebugLogWindow)]
pub fn debug_log_window(props: &DebugLogProps) -> Html {
    let snapshot = use_state(DebugLogSnapshot::default);
    let hydra_state = use_state(|| None::<Value>);
    let error = use_state(String::new);
    let copied = use_state(|| false);
    use_debug_polling(
        props.open,
        props.profile_id.clone(),
        snapshot.clone(),
        hydra_state.clone(),
        error.clone(),
    );
    if !props.open {
        return Html::default();
    }
    let actions = debug_actions(props, &snapshot, &hydra_state, &error, &copied);
    debug_window_view(&snapshot, &hydra_state, &error, &copied, actions)
}

#[derive(Clone)]
struct DebugActions {
    refresh: Callback<MouseEvent>,
    clear: Callback<MouseEvent>,
    copy: Callback<MouseEvent>,
    close: Callback<MouseEvent>,
}

#[hook]
fn use_debug_polling(
    open: bool,
    profile_id: String,
    snapshot: UseStateHandle<DebugLogSnapshot>,
    hydra_state: UseStateHandle<Option<Value>>,
    error: UseStateHandle<String>,
) {
    use_effect_with((open, profile_id), move |(open, profile_id)| {
        let alive = Rc::new(Cell::new(true));
        if *open {
            spawn_debug_poll(
                alive.clone(),
                profile_id.clone(),
                snapshot.clone(),
                hydra_state.clone(),
                error.clone(),
            );
        }
        move || alive.set(false)
    });
}

fn spawn_debug_poll(
    alive: Rc<Cell<bool>>,
    profile_id: String,
    snapshot: UseStateHandle<DebugLogSnapshot>,
    hydra_state: UseStateHandle<Option<Value>>,
    error: UseStateHandle<String>,
) {
    spawn_local(async move {
        while alive.get() {
            refresh_debug_state(&profile_id, &snapshot, &hydra_state, &error).await;
            TimeoutFuture::new(500).await;
        }
    });
}

async fn refresh_debug_state(
    profile_id: &str,
    snapshot: &UseStateHandle<DebugLogSnapshot>,
    hydra_state: &UseStateHandle<Option<Value>>,
    error: &UseStateHandle<String>,
) {
    match crate::controllers::debug::debug_log_snapshot(None).await {
        Ok(value) => snapshot.set(value),
        Err(message) => error.set(message),
    }
    match crate::controllers::debug::hydra_debug_state(profile_id).await {
        Ok(value) => hydra_state.set(Some(value)),
        Err(message) => error.set(message),
    }
}

fn debug_actions(
    props: &DebugLogProps,
    snapshot: &UseStateHandle<DebugLogSnapshot>,
    hydra_state: &UseStateHandle<Option<Value>>,
    error: &UseStateHandle<String>,
    copied: &UseStateHandle<bool>,
) -> DebugActions {
    DebugActions {
        refresh: debug_refresh_callback(
            props.profile_id.clone(),
            snapshot.clone(),
            hydra_state.clone(),
            error.clone(),
        ),
        clear: debug_clear_callback(snapshot.clone(), error.clone()),
        copy: debug_copy_callback(
            (**snapshot).clone(),
            (**hydra_state).clone(),
            copied.clone(),
        ),
        close: debug_close_callback(props.on_close.clone()),
    }
}

fn debug_refresh_callback(
    profile_id: String,
    snapshot: UseStateHandle<DebugLogSnapshot>,
    hydra_state: UseStateHandle<Option<Value>>,
    error: UseStateHandle<String>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let profile_id = profile_id.clone();
        let snapshot = snapshot.clone();
        let hydra_state = hydra_state.clone();
        let error = error.clone();
        spawn_local(async move {
            refresh_debug_state(&profile_id, &snapshot, &hydra_state, &error).await;
        });
    })
}

fn debug_clear_callback(
    snapshot: UseStateHandle<DebugLogSnapshot>,
    error: UseStateHandle<String>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let snapshot = snapshot.clone();
        let error = error.clone();
        spawn_local(async move {
            match crate::controllers::debug::clear_debug_log().await {
                Ok(()) => snapshot.set(DebugLogSnapshot::default()),
                Err(message) => error.set(message),
            }
        });
    })
}

fn debug_copy_callback(
    snapshot: DebugLogSnapshot,
    state: Option<Value>,
    copied: UseStateHandle<bool>,
) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let transcript = format_transcript(&snapshot.entries, state.as_ref());
        let Some(window) = web_sys::window() else {
            return;
        };
        let _ = window.navigator().clipboard().write_text(&transcript);
        copied.set(true);
        let copied = copied.clone();
        spawn_local(async move {
            TimeoutFuture::new(1_200).await;
            copied.set(false);
        });
    })
}

fn debug_close_callback(on_close: Callback<()>) -> Callback<MouseEvent> {
    Callback::from(move |_| on_close.emit(()))
}

fn debug_window_view(
    snapshot: &UseStateHandle<DebugLogSnapshot>,
    hydra_state: &UseStateHandle<Option<Value>>,
    error: &UseStateHandle<String>,
    copied: &UseStateHandle<bool>,
    actions: DebugActions,
) -> Html {
    let state_text = hydra_state
        .as_ref()
        .and_then(|value| serde_json::to_string_pretty(value).ok())
        .unwrap_or_else(|| "Waiting for native protocol state…".into());
    html! {
      <div class="modal-backdrop debug-window-backdrop" role="presentation">
        <section class="modal debug-window" role="dialog" aria-modal="true" aria-label="Protocol and Kaspa debug log">
          <header class="debug-window-head">
            <div><h3>{"Protocol & Kaspa Debug"}</h3><small>{"Redacted protocol metadata only. Message plaintext, passwords, private keys, seed words, and ciphertext are not logged."}</small></div>
            <button onclick={actions.close.clone()} aria-label="Close protocol debug window">{"×"}</button>
          </header>
          <div class="debug-state"><h4>{"Live protocol state"}</h4><pre>{state_text}</pre></div>
          <div class="debug-log">{debug_entries_view(snapshot)}</div>
          {debug_error_view(error)}
          <div class="button-row debug-actions">
            <button onclick={actions.refresh}>{"Refresh"}</button>
            <button onclick={actions.copy}>{if **copied {"Copied"} else {"Copy log"}}</button>
            <button onclick={actions.clear}>{"Clear"}</button>
            <button class="primary" onclick={actions.close}>{"Close"}</button>
          </div>
        </section>
      </div>
    }
}

fn debug_entries_view(snapshot: &UseStateHandle<DebugLogSnapshot>) -> Html {
    if snapshot.entries.is_empty() {
        html! { <div class="muted">{"No protocol/network events logged yet."}</div> }
    } else {
        html! { <>{for snapshot.entries.iter().map(render_entry)}</> }
    }
}

fn debug_error_view(error: &UseStateHandle<String>) -> Html {
    if error.is_empty() {
        Html::default()
    } else {
        html! { <p class="status">{(**error).clone()}</p> }
    }
}

fn render_entry(entry: &DebugLogEntry) -> Html {
    html! {
      <div class={classes!("debug-log-entry", entry.level.clone())}>
        <code>{format!("{} #{} [{}] [{}] {}", entry.timestamp_ms, entry.sequence, entry.level, entry.category, entry.event)}</code>
        {if entry.details.is_empty(){Html::default()}else{html!{<pre>{entry.details.clone()}</pre>}}}
      </div>
    }
}

fn format_transcript(entries: &[DebugLogEntry], state: Option<&Value>) -> String {
    let mut output = String::from("Ghost Talk Protocol Debug\nSensitive payloads are intentionally excluded.\n\nLIVE PROTOCOL STATE\n");
    output.push_str(
        &state
            .and_then(|value| serde_json::to_string_pretty(value).ok())
            .unwrap_or_else(|| "unavailable".into()),
    );
    output.push_str("\n\nEVENT LOG\n");
    for entry in entries {
        output.push_str(&format!(
            "{} #{} [{}] [{}] {} {}\n",
            entry.timestamp_ms,
            entry.sequence,
            entry.level,
            entry.category,
            entry.event,
            entry.details
        ));
    }
    output
}
