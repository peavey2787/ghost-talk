//! Camera video for the live call and Room voice. Frames are p2p-net only,
//! so the camera is offered only while a p2p-net route is connected.

use ghost_p2p::P2pRouteState;
use wasm_bindgen_futures::spawn_local;
use yew::Callback;

use crate::components::call::{CallRuntime, VideoControls};

/// Tile showing this device's own camera.
pub(crate) const LOCAL_TILE: &str = "video-local";
/// Tile showing the 1:1 call peer.
pub(in crate::components::call) const CALL_TILE: &str = "video-call";

/// Tile showing one Room participant.
pub(crate) fn room_tile(room_id: &str, hydra_id: &str) -> String {
    format!("video-room-{room_id}-{hydra_id}")
}

/// Camera controls whose frames go to `send`.
pub(in crate::components::call) fn controls(
    runtime: &CallRuntime,
    send: fn(&CallRuntime, Vec<u8>),
) -> VideoControls {
    let toggle_runtime = runtime.clone();
    VideoControls {
        on: runtime.video.is_running(),
        available: available(runtime),
        toggle: Callback::from(move |_| toggle(toggle_runtime.clone(), send)),
    }
}

fn available(runtime: &CallRuntime) -> bool {
    crate::components::call::p2p_route::route_is_automatic(runtime)
        && *runtime.p2p_state == P2pRouteState::Connected
}

fn toggle(runtime: CallRuntime, send: fn(&CallRuntime, Vec<u8>)) {
    if runtime.video.is_running() {
        stop(&runtime);
        redraw(&runtime);
        return;
    }
    if !available(&runtime) {
        runtime
            .on_error
            .emit("Video needs a connected P2P route (route: Auto).".into());
        return;
    }
    spawn_local(async move {
        let frames = runtime.clone();
        let started = runtime
            .video
            .start(move |frame| {
                ghost_talk_wasm::show_video_frame(LOCAL_TILE, &frame);
                send(&frames, frame);
            })
            .await;
        if let Err(error) = started {
            runtime.on_error.emit(format!("Camera: {error}"));
        }
        redraw(&runtime);
    });
}

/// Turn the camera off and blank every tile.
pub(in crate::components::call) fn stop(runtime: &CallRuntime) {
    runtime.video.stop();
    ghost_talk_wasm::clear_video_frames();
}

fn redraw(runtime: &CallRuntime) {
    runtime
        .render_epoch
        .set((*runtime.render_epoch).wrapping_add(1));
}
