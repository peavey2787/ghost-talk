//! Announce the transport binding once per newly established session.
//!
//! Node start and address changes re-announce every eligible session
//! (`announce_active_sessions`). A session established while the node is
//! already running needs its own announcement, otherwise p2p text and voice
//! could only start after a call. Each SID is announced once per node
//! generation, so the Kaspa signal layer is used as little as possible.

use std::collections::HashSet;

use ghost_p2p::P2pRouteState;
use yew::prelude::*;

use super::announce::{session_is_eligible, start_session};
use crate::{components::call::CallRuntime, model::Profile};

#[derive(Clone, PartialEq)]
struct SessionKey {
    node_ready: bool,
    generation: u64,
    sessions: Vec<(String, String)>,
}

fn eligible_sessions(profile: &Profile) -> Vec<(String, String)> {
    if !profile.settings.route.eq_ignore_ascii_case("auto") {
        return Vec::new();
    }
    let mut sessions: Vec<(String, String)> = profile
        .chats
        .iter()
        .filter(|chat| session_is_eligible(chat))
        .filter_map(|chat| Some((chat.id.clone(), chat.session_sid()?.to_owned())))
        .collect();
    sessions.sort();
    sessions
}

fn node_ready(state: &P2pRouteState) -> bool {
    matches!(state, P2pRouteState::Connecting | P2pRouteState::Connected)
}

#[hook]
pub(in crate::components::call) fn use_session_announce_effect(
    profile: &Profile,
    runtime: CallRuntime,
) {
    let announced = use_mut_ref(|| (0u64, HashSet::<String>::new()));
    let key = SessionKey {
        node_ready: node_ready(&runtime.p2p_state),
        generation: *runtime.p2p_generation.borrow(),
        sessions: eligible_sessions(profile),
    };
    let profile = profile.clone();
    use_effect_with(key, move |key| {
        let mut announced = announced.borrow_mut();
        if announced.0 != key.generation {
            // A new node generation announces every session on start.
            *announced = (
                key.generation,
                key.sessions.iter().map(|(_, sid)| sid.clone()).collect(),
            );
        } else if key.node_ready {
            for (chat_id, sid) in &key.sessions {
                if !announced.1.insert(sid.clone()) {
                    continue;
                }
                if let Some(chat) = profile.chats.iter().find(|chat| &chat.id == chat_id) {
                    start_session(runtime.clone(), chat.clone());
                }
            }
        }
        || ()
    });
}
