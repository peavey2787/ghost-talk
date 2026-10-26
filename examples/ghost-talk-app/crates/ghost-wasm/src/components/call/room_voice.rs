use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::{
    live_voice::{self, RoomMediaPacket},
    model::{Profile, RealtimeControl, Room},
};

use super::{active_call, video, CallRuntime, RoomVoiceContext};

mod fanout;
use fanout::{fan, relay_if_owner, valid_hop};

/// The two media a Room participant can send. Voice may fall back to Kaspa
/// under the Room's route; video is p2p-net only.
#[derive(Clone, Copy)]
enum Medium {
    Voice,
    Video,
}

impl Medium {
    fn encode(self, packet: &RoomMediaPacket) -> Result<String, String> {
        match self {
            Medium::Voice => live_voice::encode_room_voice(packet),
            Medium::Video => live_voice::encode_room_video(packet),
        }
    }
}

pub(super) fn context(runtime: CallRuntime) -> RoomVoiceContext {
    let join_runtime = runtime.clone();
    let leave_runtime = runtime.clone();
    RoomVoiceContext {
        active_room_id: (*runtime.room_voice_id).clone(),
        join: Callback::from(move |room_id| join(join_runtime.clone(), room_id)),
        leave: Callback::from(move |_| leave(&leave_runtime)),
        video: video::controls(&runtime, send_video_frame),
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
    set_active_room(&runtime, Some(room_id));
    let Some(local_hydra) = profile.hydra_identity_id.clone() else {
        return;
    };
    if !room.is_participant(&local_hydra) {
        return;
    }
    let capture_runtime = runtime.clone();
    spawn_local(async move {
        if let Err(error) = start_capture(capture_runtime).await {
            runtime.on_error.emit(error);
            set_active_room(&runtime, None);
        }
    });
}

async fn start_capture(runtime: CallRuntime) -> Result<(), String> {
    let sender = runtime.voice_sender()?;
    sender
        .start(move |encoded| send_media(&runtime, Medium::Voice, &encoded))
        .await
}

fn send_video_frame(runtime: &CallRuntime, frame: Vec<u8>) {
    send_media(runtime, Medium::Video, &frame);
}

fn send_media(runtime: &CallRuntime, medium: Medium, bytes: &[u8]) {
    let Some(room_id) = runtime.room_voice_ref.borrow().clone() else {
        return;
    };
    let profile = runtime.profile_ref.borrow().clone();
    let Some(local_hydra) = profile.hydra_identity_id.clone() else {
        return;
    };
    let Some(room) = profile.rooms.iter().find(|room| room.id == room_id) else {
        return;
    };
    if !room.is_participant(&local_hydra) {
        return;
    }
    let sequence = next_sequence(runtime);
    let timestamp_ms = crate::now_ms().max(0.0) as u64;
    let packet = RoomMediaPacket::new(room_id, local_hydra, sequence, timestamp_ms, bytes);
    if let Ok(body) = medium.encode(&packet) {
        fan(&profile, room, None, medium, &body, &runtime.realtime);
    }
}

fn next_sequence(runtime: &CallRuntime) -> u64 {
    let mut value = runtime.room_sequence.borrow_mut();
    *value = value.wrapping_add(1);
    *value
}

pub(super) fn process_voice(
    control: RealtimeControl,
    packet: RoomMediaPacket,
    runtime: CallRuntime,
) {
    process(control, packet, Medium::Voice, runtime);
}

pub(super) fn process_video(
    control: RealtimeControl,
    packet: RoomMediaPacket,
    runtime: CallRuntime,
) {
    process(control, packet, Medium::Video, runtime);
}

fn process(
    control: RealtimeControl,
    packet: RoomMediaPacket,
    medium: Medium,
    runtime: CallRuntime,
) {
    let profile = runtime.profile_ref.borrow().clone();
    let Some(room) = admitted_room(&profile, &control, &packet, &runtime) else {
        return;
    };
    match medium {
        Medium::Voice => play(&runtime, &packet),
        Medium::Video => show(&packet),
    }
    relay_if_owner(&profile, &room, &packet, medium, &runtime.realtime);
}

/// The Room a packet belongs to, when this device is in that Room's voice,
/// the sender is a participant and the hop is the sender or the owner.
fn admitted_room(
    profile: &Profile,
    control: &RealtimeControl,
    packet: &RoomMediaPacket,
    runtime: &CallRuntime,
) -> Option<Room> {
    if runtime.room_voice_ref.borrow().as_deref() != Some(packet.room_id.as_str()) {
        return None;
    }
    let room = profile
        .rooms
        .iter()
        .find(|room| room.id == packet.room_id)?;
    (room.is_participant(&packet.speaker_hydra_id)
        && valid_hop(profile, control, room, &packet.speaker_hydra_id))
    .then(|| room.clone())
}

fn play(runtime: &CallRuntime, packet: &RoomMediaPacket) {
    let Some(bytes) = packet.bytes() else {
        return;
    };
    if let Err(error) = runtime.receiver.borrow().receive_encoded(&bytes) {
        runtime.on_error.emit(error);
    }
}

fn show(packet: &RoomMediaPacket) {
    if let Some(frame) = packet.bytes() {
        let tile = video::room_tile(&packet.room_id, &packet.speaker_hydra_id);
        ghost_talk_wasm::show_video_frame(&tile, &frame);
    }
}

fn leave(runtime: &CallRuntime) {
    set_active_room(runtime, None);
    if active_call(runtime).is_none() {
        super::close_media(runtime);
    }
}

/// Update the rendered state and the live mirror the audio callbacks read.
fn set_active_room(runtime: &CallRuntime, room_id: Option<String>) {
    *runtime.room_voice_ref.borrow_mut() = room_id.clone();
    runtime.room_voice_id.set(room_id);
}
