use crate::model::Profile;
use crate::{
    components::{
        call::RoomVoiceContext,
        form::{checkbox_input, prevent_submit, text_input},
    },
    controllers::broadcast::BroadcastStopResult,
};
use ghost_media::MediaReference;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

#[derive(Clone)]
pub(super) struct LiveUi {
    pub(super) record_local: UseStateHandle<bool>,
    pub(super) rtmp_server: UseStateHandle<String>,
    pub(super) rtmp_key: UseStateHandle<String>,
    pub(super) relay_url: UseStateHandle<String>,
    pub(super) recording: UseStateHandle<Option<MediaReference>>,
    pub(super) started_at_ms: UseStateHandle<Option<f64>>,
    pub(super) duration_ms: UseStateHandle<u64>,
    pub(super) status: UseStateHandle<String>,
}

pub(super) fn render(profile: &Profile, ui: &LiveUi, context: Option<RoomVoiceContext>) -> Html {
    let Some(context) = context else {
        return html! { <div class="card muted">{"Room voice runtime is unavailable."}</div> };
    };
    let active_room = context.active_room_id.clone();
    let active_session = context.broadcast_session.clone();
    let allowed = broadcast_allowed(profile, active_room.as_deref());
    let start = start_callback(ui.clone(), context.clone());
    let stop = stop_callback(ui.clone(), context);
    let recording = recording_summary(&ui.recording);
    html! {
        <form class="card form-grid" onsubmit={prevent_submit()}>
            <h3>{"Live broadcast"}</h3>
            <small>{"Uses the active Room voice capture. External RTMP leaves Ghost's security boundary."}</small>
            {security_state(profile, active_room.as_deref(), ui)}
            <label class="checkbox-row"><input type="checkbox" checked={*ui.record_local} onchange={checkbox_input(ui.record_local.clone())}/>{"Record locally"}</label>
            <label>{"RTMP/RTMPS server"}<input value={(*ui.rtmp_server).clone()} oninput={text_input(ui.rtmp_server.clone())} placeholder="rtmps://server/app" /></label>
            <label>{"Stream key"}<input type="password" autocomplete="off" value={(*ui.rtmp_key).clone()} oninput={text_input(ui.rtmp_key.clone())} /></label>
            <label>{"Broadcast relay"}<input value={(*ui.relay_url).clone()} oninput={text_input(ui.relay_url.clone())} placeholder="wss://relay.example/ingest" /></label>
            <small>{"Web/mobile live RTMP uses the configured relay. Desktop may publish RTMP directly when Relay is blank."}</small>
            <div class="button-row">
                <button type="button" class="primary" disabled={!allowed || active_session.is_some()} onclick={start}>{"Go live"}</button>
                <button type="button" disabled={active_session.is_none()} onclick={stop}>{"End broadcast"}</button>
            </div>
            {broadcast_hint(active_room.as_deref(), allowed)}
            {recording}
        </form>
    }
}

fn security_state(profile: &Profile, room_id: Option<&str>, ui: &LiveUi) -> Html {
    let room = room_id.and_then(|id| profile.rooms.iter().find(|room| room.id == id));
    let room_state = room
        .map(|room| match room.access() {
            ghost_rooms::RoomAccess::Public => html! { <span>{"🌐 Public Broadcast"}</span> },
            _ => html! { <span>{"🎙 Private Room"}</span> },
        })
        .unwrap_or_default();
    let rtmp = if ui.rtmp_server.trim().is_empty() {
        Html::default()
    } else {
        html! { <span>{"📡 External RTMP"}</span> }
    };
    let relay = if ui.relay_url.trim().is_empty() {
        Html::default()
    } else {
        html! { <span>{"🔁 Broadcast Relay"}</span> }
    };
    let recording = if *ui.record_local {
        html! { <span>{"💾 Local Recording"}</span> }
    } else {
        Html::default()
    };
    html! { <div class="security-state-row">{room_state}{rtmp}{relay}{recording}</div> }
}

fn broadcast_allowed(profile: &Profile, room_id: Option<&str>) -> bool {
    let Some(room_id) = room_id else {
        return false;
    };
    let Some(hydra_id) = profile.hydra_identity_id.as_deref() else {
        return false;
    };
    profile
        .rooms
        .iter()
        .find(|room| room.id == room_id)
        .is_some_and(|room| room.can_broadcast(hydra_id))
}

fn broadcast_hint(room_id: Option<&str>, allowed: bool) -> Html {
    match (room_id, allowed) {
        (None, _) => html! { <small>{"Join voice in a Room first."}</small> },
        (Some(_), false) => {
            html! { <small>{"This Room policy or your role does not permit broadcasting."}</small> }
        }
        (Some(_), true) => Html::default(),
    }
}

fn start_callback(ui: LiveUi, context: RoomVoiceContext) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let record_local = *ui.record_local;
        let server = normalized_optional(&ui.rtmp_server);
        let key = normalized_optional(&ui.rtmp_key);
        let relay = normalized_optional(&ui.relay_url);
        if !record_local && server.is_none() && relay.is_none() {
            ui.status
                .set("Enable local recording, configure RTMP/RTMPS, or configure a broadcast relay.".into());
            return;
        }
        let session_id = match crate::random_id() {
            Ok(id) => id,
            Err(error) => {
                ui.status.set(error);
                return;
            }
        };
        let status = ui.status.clone();
        let started_at = ui.started_at_ms.clone();
        let set_session = context.set_broadcast_session.clone();
        let started = crate::now_ms();
        spawn_local(async move {
            match crate::controllers::broadcast::start(
                &session_id,
                record_local,
                server.as_deref(),
                key.as_deref(),
                relay.as_deref(),
            )
            .await
            {
                Ok(()) => {
                    started_at.set(Some(started));
                    set_session.emit(Some(session_id));
                    status.set("Broadcast started.".into());
                }
                Err(error) => status.set(error),
            }
        });
    })
}

fn stop_callback(ui: LiveUi, context: RoomVoiceContext) -> Callback<MouseEvent> {
    Callback::from(move |_| {
        let Some(session_id) = context.broadcast_session.clone() else {
            return;
        };
        let status = ui.status.clone();
        let recording = ui.recording.clone();
        let started_at = ui.started_at_ms.clone();
        let duration = ui.duration_ms.clone();
        let set_session = context.set_broadcast_session.clone();
        let ended = crate::now_ms();
        spawn_local(async move {
            match crate::controllers::broadcast::stop(&session_id).await {
                Ok(result) => {
                    if let Some(started) = *started_at {
                        duration.set((ended - started).max(0.0) as u64);
                    }
                    started_at.set(None);
                    apply_stop_result(result, &recording, &status);
                }
                Err(error) => status.set(error),
            }
            set_session.emit(None);
        });
    })
}

fn apply_stop_result(
    result: BroadcastStopResult,
    recording: &UseStateHandle<Option<MediaReference>>,
    status: &UseStateHandle<String>,
) {
    if let Some(media) = result.recording {
        recording.set(Some(media));
    }
    if result.failures.is_empty() {
        status.set("Broadcast ended.".into());
    } else {
        let sinks = result
            .failures
            .iter()
            .map(|failure| failure.sink.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        status.set(format!("Broadcast ended with sink failures: {sinks}"));
    }
}

fn normalized_optional(value: &UseStateHandle<String>) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn recording_summary(recording: &UseStateHandle<Option<MediaReference>>) -> Html {
    let Some(media) = recording.as_ref() else {
        return Html::default();
    };
    html! { <small>{format!("Recorded {} bytes · {}", media.size, media.media_id)}</small> }
}
