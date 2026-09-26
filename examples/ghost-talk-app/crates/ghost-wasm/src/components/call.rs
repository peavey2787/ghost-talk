use crate::model::{CallRecord, Chat, Profile, ProfilePatch};
use ghost_domain::call::{CallEvent, CallManager};
use ghost_p2p::{P2pNetTransport, P2pRouteState};
use ghost_talk_wasm::{BrowserVoiceReceiver, BrowserVoiceSender};
use std::{cell::RefCell, collections::HashSet, rc::Rc};
use yew::prelude::*;

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests;
mod direct_text;
pub(crate) mod identity;
mod manager;
mod media;
mod p2p_route;
mod presentation;
mod room_voice;
mod signaling;
mod termination;
mod transport;

use crate::controllers::call::RealtimeSender;
pub(crate) use direct_text::{DirectTextContext, DirectTextRequest};
pub use manager::{CallButton, LiveCallManager};

#[derive(Clone)]
struct CallRuntime {
    profile_ref: Rc<RefCell<Profile>>,
    call_manager: Rc<RefCell<CallManager>>,
    render_epoch: UseStateHandle<u64>,
    sender_ref: Rc<RefCell<Option<BrowserVoiceSender>>>,
    receiver: Rc<RefCell<BrowserVoiceReceiver>>,
    room_voice_id: UseStateHandle<Option<String>>,
    room_sequence: Rc<RefCell<u64>>,
    room_broadcast_session: UseStateHandle<Option<String>>,
    p2p: Rc<RefCell<P2pNetTransport>>,
    p2p_state: UseStateHandle<P2pRouteState>,
    p2p_subscriptions: Rc<RefCell<HashSet<String>>>,
    p2p_generation: Rc<RefCell<u64>>,
    realtime: RealtimeSender,
    password: String,
    on_select: Callback<String>,
    on_update: Callback<ProfilePatch>,
    on_error: Callback<String>,
}

#[derive(Clone, PartialEq)]
struct CallContext {
    active: bool,
    start: Callback<Chat>,
}

#[derive(Clone, PartialEq)]
pub(crate) struct RoomVoiceContext {
    pub(crate) active_room_id: Option<String>,
    pub(crate) join: Callback<String>,
    pub(crate) leave: Callback<()>,
    pub(crate) broadcast_session: Option<String>,
    pub(crate) set_broadcast_session: Callback<Option<String>>,
}

impl CallRuntime {
    /// Clone the shared p2p-net handle so no UI borrow is held across awaits.
    fn p2p(&self) -> P2pNetTransport {
        self.p2p.borrow().clone()
    }
}

fn publish_profile(runtime: &CallRuntime, profile: Profile, patch: ProfilePatch) {
    *runtime.profile_ref.borrow_mut() = profile;
    runtime
        .render_epoch
        .set((*runtime.render_epoch).wrapping_add(1));
    runtime.on_update.emit(patch);
}

fn visible_call(runtime: &CallRuntime) -> Option<CallRecord> {
    runtime.call_manager.borrow().visible().cloned()
}

fn active_call(runtime: &CallRuntime) -> Option<CallRecord> {
    runtime.call_manager.borrow().active().cloned()
}

fn apply_call_command<F>(runtime: &CallRuntime, apply: F) -> Result<(), String>
where
    F: FnOnce(&mut CallManager) -> Result<(), String>,
{
    apply(&mut runtime.call_manager.borrow_mut())?;
    runtime
        .render_epoch
        .set((*runtime.render_epoch).wrapping_add(1));
    Ok(())
}

fn call_chat(runtime: &CallRuntime, call: &CallRecord) -> Option<Chat> {
    let chat_id = call.chat_id.as_deref()?;
    runtime
        .profile_ref
        .borrow()
        .chats
        .iter()
        .find(|chat| chat.id == chat_id)
        .cloned()
}

fn can_start_call(chat: &Chat) -> bool {
    chat.peer_kaspa_address()
        .is_some_and(|address| !address.trim().is_empty())
        && !chat.archived()
        && !chat.left()
        && !chat.peer_left()
        && !chat
            .incoming_request()
            .is_some_and(|request| request.state == "pending" && request.call_id.is_none())
}

fn close_media(runtime: &CallRuntime) {
    let sender = runtime.sender_ref.borrow_mut().take();
    if let Some(sender) = sender {
        sender.close();
    }
    runtime.receiver.borrow().reset();
}

fn fail_runtime_call(runtime: &CallRuntime, call_id: &str, message: String) {
    close_media(runtime);
    let result = apply_call_command(runtime, |calls| {
        crate::controllers::call::fail(calls, call_id, message.clone())
    });
    if let Err(error) = result {
        runtime.on_error.emit(error);
    }
}

fn end_runtime_call(runtime: &CallRuntime, call_id: &str, event: CallEvent) {
    // Make the local state transition authoritative before touching browser media.
    // MediaRecorder/AudioContext cleanup is best-effort process-local work and
    // must never be able to strand the UI in Connected/Answering if cleanup
    // throws or re-enters a browser callback.
    if let Err(error) = apply_call_command(runtime, |calls| {
        crate::controllers::call::end_local(calls, call_id, event)
    }) {
        runtime.on_error.emit(error);
        return;
    }
    close_media(runtime);
}
