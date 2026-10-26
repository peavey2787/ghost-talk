use yew::prelude::*;

use crate::components::call::{room_tile, video_button, LOCAL_TILE};
use crate::components::rooms::state::{Room, RoomsUi};

/// Join/leave the Room call. Voice travels over the Room's authenticated
/// realtime sessions (p2p-net under Auto, otherwise Kaspa); video is
/// p2p-net only.
pub(super) fn render_room_voice(ui: &RoomsUi, room: &Room, local_hydra: Option<&str>) -> Html {
    let Some(voice) = ui.voice.clone() else {
        return Html::default();
    };
    if room.pending_acceptance() {
        return Html::default();
    }
    if voice.active_room_id.as_deref() == Some(room.id.as_str()) {
        let leave = voice.leave.clone();
        return html! {
            <div class="card room-voice active">
              <div class="room-voice-bar"><b>{"In Room voice"}</b>
                {video_button(&voice.video)}
                <button type="button" class="danger" onclick={Callback::from(move |_| leave.emit(()))}>{"Leave voice"}</button></div>
              {render_room_video(room, local_hydra)}
            </div>
        };
    }
    let join = voice.join.clone();
    let room_id = room.id.clone();
    html! {
        <div class="card room-voice"><span>{"Room voice"}</span>
          <button type="button" class="primary" onclick={Callback::from(move |_| join.emit(room_id.clone()))}>{"Join voice"}</button></div>
    }
}

/// One tile per other participant plus this device's preview; each stays
/// hidden until that participant's camera frames arrive.
fn render_room_video(room: &Room, local_hydra: Option<&str>) -> Html {
    let others = std::iter::once(room.owner_hydra_id.as_str())
        .chain(
            room.members()
                .iter()
                .filter_map(|member| member.hydra_handle.as_deref()),
        )
        .filter(|hydra| Some(*hydra) != local_hydra)
        .collect::<Vec<_>>();
    html! {
        <div class="video-grid">
          {for others.into_iter().map(|hydra| html! {
            <img class="video-tile" data-ghost-video={room_tile(&room.id, hydra)} alt="Participant video"/>
          })}
          <img class="video-tile local" data-ghost-video={LOCAL_TILE} alt="Your camera"/>
        </div>
    }
}
