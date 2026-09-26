use ghost_talk_wasm::BrowserVoiceSender;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    live_voice,
    model::{Profile, RealtimeControl},
};

use super::{active_call, CallRuntime, RoomVoiceContext};
use crate::controllers::call::RealtimeSender;

pub(super) fn context(runtime: CallRuntime) -> RoomVoiceContext {
    let join_runtime = runtime.clone();
    let leave_runtime = runtime.clone();
    let session_state = runtime.room_broadcast_session.clone();
    RoomVoiceContext {
        active_room_id: (*runtime.room_voice_id).clone(),
        join: Callback::from(move |room_id| join(join_runtime.clone(), room_id)),
        leave: Callback::from(move |_| leave(&leave_runtime)),
        broadcast_session: (*runtime.room_broadcast_session).clone(),
        set_broadcast_session: Callback::from(move |session| session_state.set(session)),
    }
}

fn join(runtime: CallRuntime, room_id: String) {
    if active_call(&runtime).is_some() {
        runtime
            .on_error
            .emit("End the direct call before joining Room voice.".into());
        return;
    }
    let profile = runtime.profile_ref.borrow().clone();
    let Some(room) = profile
        .rooms
        .iter()
        .find(|room| room.id == room_id)
        .cloned()
    else {
        runtime.on_error.emit("Room is no longer available.".into());
        return;
    };
    if room.audio_policy() == ghost_rooms::ChannelPolicy::Off {
        runtime
            .on_error
            .emit("Voice is disabled for this Room.".into());
        return;
    }
    runtime.room_voice_id.set(Some(room_id));
    let Some(local_hydra) = profile.hydra_identity_id.clone() else {
        return;
    };
    if !room.can_speak(&local_hydra) {
        return;
    }
    let capture_runtime = runtime.clone();
    spawn_local(async move {
        if let Err(error) = start_capture(capture_runtime).await {
            runtime.on_error.emit(error);
            runtime.room_voice_id.set(None);
        }
    });
}

async fn start_capture(runtime: CallRuntime) -> Result<(), String> {
    // Bind first so the `borrow()` is released before any `borrow_mut()` below.
    let existing = runtime.sender_ref.borrow().clone();
    let sender = if let Some(existing) = existing {
        existing
    } else {
        let created = BrowserVoiceSender::new()?;
        *runtime.sender_ref.borrow_mut() = Some(created.clone());
        created
    };
    sender
        .start(move |encoded| send_encoded(runtime.clone(), encoded))
        .await
}

fn send_encoded(runtime: CallRuntime, encoded: Vec<u8>) {
    let Some(room_id) = (*runtime.room_voice_id).clone() else {
        return;
    };
    let profile = runtime.profile_ref.borrow().clone();
    let Some(local_hydra) = profile.hydra_identity_id.clone() else {
        return;
    };
    let Some(room) = profile.rooms.iter().find(|room| room.id == room_id) else {
        return;
    };
    if !room.can_speak(&local_hydra) {
        return;
    }
    let sequence = next_sequence(&runtime);
    let timestamp_ms = crate::now_ms().max(0.0) as u64;
    let packet =
        live_voice::RoomVoicePacket::audio(room_id, local_hydra, sequence, timestamp_ms, &encoded);
    let Ok(body) = live_voice::encode_room_voice(&packet) else {
        return;
    };
    fan(&profile, room, None, &body, &runtime.realtime);
    push_broadcast(runtime, sequence, timestamp_ms, encoded);
}

fn next_sequence(runtime: &CallRuntime) -> u64 {
    let mut value = runtime.room_sequence.borrow_mut();
    *value = value.wrapping_add(1);
    *value
}

fn push_broadcast(runtime: CallRuntime, sequence: u64, timestamp_ms: u64, encoded: Vec<u8>) {
    let Some(session_id) = (*runtime.room_broadcast_session).clone() else {
        return;
    };
    spawn_local(async move {
        let _ =
            crate::controllers::broadcast::push(&session_id, sequence, timestamp_ms, encoded).await;
    });
}

pub(super) fn process(
    control: RealtimeControl,
    packet: live_voice::RoomVoicePacket,
    runtime: CallRuntime,
) {
    let profile = runtime.profile_ref.borrow().clone();
    let Some(room) = profile
        .rooms
        .iter()
        .find(|room| room.id == packet.room_id)
        .cloned()
    else {
        return;
    };
    if (*runtime.room_voice_id).as_deref() != Some(packet.room_id.as_str()) {
        return;
    }
    if !room.can_speak(&packet.speaker_hydra_id) {
        return;
    }
    if !valid_hop(&profile, &control, &room, &packet.speaker_hydra_id) {
        return;
    }
    play(&runtime, &packet);
    relay_if_owner(&profile, &room, &packet, &runtime.realtime);
}

fn valid_hop(
    profile: &Profile,
    control: &RealtimeControl,
    room: &crate::model::Room,
    speaker: &str,
) -> bool {
    let peer = profile
        .chats
        .iter()
        .find(|chat| chat.id == control.chat_id)
        .and_then(|chat| chat.peer_hydra_handle());
    peer == Some(speaker) || peer == Some(room.owner_hydra_id.as_str())
}

fn play(runtime: &CallRuntime, packet: &live_voice::RoomVoicePacket) {
    let Some(bytes) = packet.audio_bytes() else {
        return;
    };
    if let Err(error) = runtime.receiver.borrow().receive_encoded(&bytes) {
        runtime.on_error.emit(error);
    }
}

fn relay_if_owner(
    profile: &Profile,
    room: &crate::model::Room,
    packet: &live_voice::RoomVoicePacket,
    realtime: &RealtimeSender,
) {
    if profile.hydra_identity_id.as_deref() != Some(room.owner_hydra_id.as_str())
        || packet.speaker_hydra_id == room.owner_hydra_id
    {
        return;
    }
    if let Ok(body) = live_voice::encode_room_voice(packet) {
        fan(
            profile,
            room,
            Some(&packet.speaker_hydra_id),
            &body,
            realtime,
        );
    }
}

fn fan(
    profile: &Profile,
    room: &crate::model::Room,
    exclude_hydra: Option<&str>,
    body: &str,
    realtime: &RealtimeSender,
) {
    if profile.hydra_identity_id.as_deref() == Some(room.owner_hydra_id.as_str()) {
        for member in room.members() {
            if member.hydra_handle.as_deref() == exclude_hydra {
                continue;
            }
            if let Some(chat_id) = realtime_chat_for(profile, member.hydra_handle.as_deref()) {
                realtime.enqueue(chat_id, body.to_owned());
            }
        }
    } else if let Some(chat_id) = realtime_chat_for(profile, Some(&room.owner_hydra_id)) {
        realtime.enqueue(chat_id, body.to_owned());
    }
}

fn realtime_chat_for(profile: &Profile, hydra: Option<&str>) -> Option<String> {
    let hydra = hydra?;
    profile
        .chats
        .iter()
        .find(|chat| {
            chat.peer_hydra_handle() == Some(hydra)
                && chat.bootstrap_complete()
                && !chat.left()
                && !chat.peer_left()
        })
        .map(|chat| chat.id.clone())
}

fn leave(runtime: &CallRuntime) {
    runtime.room_voice_id.set(None);
    if active_call(runtime).is_none() {
        if let Some(sender) = runtime.sender_ref.borrow_mut().take() {
            sender.close();
        }
        runtime.receiver.borrow().reset();
    }
}
