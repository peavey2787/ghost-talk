//! While the swarm is online, advertise the public rooms this profile owns on
//! the room-directory topic. Each announcement is signed by the owner's Kaspa
//! address key (no transaction, no fee) and re-signed before listeners would
//! consider it stale.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use ghost_realtime::ROOM_DIRECTORY_TOPIC;
use ghost_rooms::{RoomAnnouncement, ROOM_LISTING_TTL_MS};
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use super::trace;
use crate::{
    components::call::{CallRuntime, SwarmStatus},
    model::{Profile, Room},
};

const ANNOUNCE_INTERVAL_MS: u32 = 60_000;
const RESIGN_AFTER_MS: u64 = ROOM_LISTING_TTL_MS / 2;

type SignedCache = Rc<RefCell<HashMap<String, RoomAnnouncement>>>;

#[hook]
pub(in crate::components::call) fn use_room_announcer(runtime: CallRuntime) {
    let cache: SignedCache = use_mut_ref(HashMap::new);
    let online = matches!(runtime.swarm.status(), SwarmStatus::Online { .. });
    let profile_id = runtime.profile_ref.borrow().id.clone();
    use_effect_with((profile_id, online), move |(_, online)| {
        let interval = online.then(|| {
            let tick = move || spawn_local(announce(runtime.clone(), cache.clone()));
            tick();
            gloo_timers::callback::Interval::new(ANNOUNCE_INTERVAL_MS, tick)
        });
        move || drop(interval)
    });
}

async fn announce(runtime: CallRuntime, cache: SignedCache) {
    let profile = runtime.profile_ref.borrow().clone();
    for room in crate::controllers::room::owned_rooms(&profile) {
        let Some(signed) = signed(&runtime, &profile, &cache, &room).await else {
            continue;
        };
        let published = match signed.encode() {
            Ok(packet) => crate::swarm_carrier::publish(ROOM_DIRECTORY_TOPIC, &packet).await,
            Err(error) => Err(error),
        };
        if let Err(error) = published {
            trace(&runtime, "room-announce-failed", error);
        }
    }
}

/// The cached signature while it still matches the room and is fresh;
/// otherwise a newly signed announcement. `None` for unannounceable rooms.
async fn signed(
    runtime: &CallRuntime,
    profile: &Profile,
    cache: &SignedCache,
    room: &Room,
) -> Option<RoomAnnouncement> {
    let now = crate::now_ms().max(0.0) as u64;
    let cached = cache.borrow().get(&room.id).cloned();
    if let Some(cached) = cached.filter(|cached| still_current(cached, room, now)) {
        return Some(cached);
    }
    let unsigned = RoomAnnouncement::for_room(room, now)?;
    match crate::controllers::room::sign_announcement(profile, &runtime.password, unsigned).await {
        Ok(signed) => {
            cache.borrow_mut().insert(room.id.clone(), signed.clone());
            Some(signed)
        }
        Err(error) => {
            trace(runtime, "room-announce-sign-failed", error);
            None
        }
    }
}

fn still_current(cached: &RoomAnnouncement, room: &Room, now: u64) -> bool {
    let unsigned = RoomAnnouncement {
        signature_hex: String::new(),
        ..cached.clone()
    };
    now.saturating_sub(cached.issued_at_ms) < RESIGN_AFTER_MS
        && RoomAnnouncement::for_room(room, cached.issued_at_ms).as_ref() == Some(&unsigned)
}
