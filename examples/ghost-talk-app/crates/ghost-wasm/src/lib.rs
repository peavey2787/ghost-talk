#![forbid(unsafe_code)]

mod model;

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod browser_js;
#[cfg(target_arch = "wasm32")]
mod components;
#[cfg(target_arch = "wasm32")]
mod controllers;
#[cfg(target_arch = "wasm32")]
mod live_voice;
#[cfg(target_arch = "wasm32")]
mod realtime_replay;
#[cfg(target_arch = "wasm32")]
mod native;
#[cfg(any(target_arch = "wasm32", test))]
mod storage;
#[cfg(target_arch = "wasm32")]
mod ui_assets;
#[cfg(target_arch = "wasm32")]
mod view_models;
#[cfg(target_arch = "wasm32")]
mod voice;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
pub(crate) fn random_id() -> Result<String, String> {
    let window = web_sys::window().ok_or_else(|| "window is unavailable".to_string())?;
    let crypto = window
        .crypto()
        .map_err(|_| "secure browser RNG is unavailable".to_string())?;
    let mut bytes = [0u8; 16];
    crypto
        .get_random_values_with_u8_array(&mut bytes)
        .map_err(|_| "secure browser RNG failed".to_string())?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn format_message_timestamp(timestamp_ms: f64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(timestamp_ms));
    let month = date.get_month() + 1;
    let day = date.get_date();
    let year = date.get_full_year();
    let hour24 = date.get_hours();
    let minute = date.get_minutes();
    let suffix = if hour24 >= 12 { "PM" } else { "AM" };
    let hour12 = match hour24 % 12 {
        0 => 12,
        value => value,
    };
    format!("{month}/{day}/{year} {hour12}:{minute:02} {suffix}")
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() {
    yew::Renderer::<app::App>::new().render();
}

#[cfg(test)]
mod tests {
    use crate::model::Profile;

    #[test]
    fn application_identity_remains_ghost_talk() {
        assert_eq!(ghost_core::APP_NAME, "Ghost Talk");
    }

    #[test]
    fn automatic_login_defaults_off_and_is_single_explicit_flag() {
        let mut profile = Profile::new("0".repeat(32), "Alice".into());
        assert!(!profile.auto_login());
        profile.set_auto_login(true);
        assert!(profile.auto_login());
    }

    #[test]
    fn room_tombstone_is_monotonic_and_can_be_cleared_for_a_new_invite() {
        let mut profile = Profile::new("0".repeat(32), "Alice".into());
        ghost_rooms::RoomTombstoneService::record(
            &mut profile.room_tombstones,
            "room-1",
            "owner-1",
            7,
        );
        ghost_rooms::RoomTombstoneService::record(
            &mut profile.room_tombstones,
            "room-1",
            "owner-1",
            5,
        );
        assert_eq!(
            ghost_rooms::RoomTombstoneService::revision(
                &profile.room_tombstones,
                "room-1",
                "owner-1"
            ),
            Some(7)
        );
        ghost_rooms::RoomTombstoneService::record(
            &mut profile.room_tombstones,
            "room-1",
            "owner-1",
            9,
        );
        assert_eq!(
            ghost_rooms::RoomTombstoneService::revision(
                &profile.room_tombstones,
                "room-1",
                "owner-1"
            ),
            Some(9)
        );
        ghost_rooms::RoomTombstoneService::clear(&mut profile.room_tombstones, "room-1", "owner-1");
        assert_eq!(
            ghost_rooms::RoomTombstoneService::revision(
                &profile.room_tombstones,
                "room-1",
                "owner-1"
            ),
            None
        );
    }
}
