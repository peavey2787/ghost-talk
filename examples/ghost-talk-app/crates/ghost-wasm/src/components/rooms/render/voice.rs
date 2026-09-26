use yew::prelude::*;

use crate::components::rooms::state::{Room, RoomsUi};

/// Join/leave Room voice. Voice travels over the Room's authenticated realtime
/// sessions (p2p-net under Auto, otherwise Kaspa).
pub(super) fn render_room_voice(ui: &RoomsUi, room: &Room) -> Html {
    let Some(voice) = ui.voice.clone() else {
        return Html::default();
    };
    if room.audio_policy() == ghost_rooms::ChannelPolicy::Off || room.pending_acceptance() {
        return Html::default();
    }
    if voice.active_room_id.as_deref() == Some(room.id.as_str()) {
        let leave = voice.leave.clone();
        return html! {
            <div class="card room-voice active"><b>{"In Room voice"}</b>
              <button type="button" class="danger" onclick={Callback::from(move |_| leave.emit(()))}>{"Leave voice"}</button></div>
        };
    }
    let join = voice.join.clone();
    let room_id = room.id.clone();
    html! {
        <div class="card room-voice"><span>{"Room voice"}</span>
          <button type="button" class="primary" onclick={Callback::from(move |_| join.emit(room_id.clone()))}>{"Join voice"}</button></div>
    }
}
