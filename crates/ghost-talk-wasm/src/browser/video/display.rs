use std::{cell::RefCell, collections::HashMap};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::Element;

/// A tile with no frame for this long is hidden again.
const STALE_AFTER_MS: f64 = 2_000.0;
const SWEEP_INTERVAL_MS: u32 = 1_000;

thread_local! {
    /// Last frame time per tile key; the sweep runs while any tile is live.
    static LIVE_TILES: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
}

/// Paint one JPEG frame onto every `<img data-ghost-video="{key}">`.
///
/// Keys are restricted to `[A-Za-z0-9_-]` so they are safe inside a CSS
/// attribute selector. Tiles stop showing a sender two seconds after its
/// last frame.
pub fn show_video_frame(key: &str, jpeg: &[u8]) {
    if !valid_key(key) {
        return;
    }
    let source = format!("data:image/jpeg;base64,{}", STANDARD.encode(jpeg));
    for tile in tiles(key) {
        let _ = tile.set_attribute("src", &source);
    }
    let start_sweep = LIVE_TILES.with(|live| {
        let mut live = live.borrow_mut();
        let was_idle = live.is_empty();
        live.insert(key.to_owned(), now_ms());
        was_idle
    });
    if start_sweep {
        spawn_local(sweep());
    }
}

/// Blank every live tile, e.g. when a call or Room session ends.
pub fn clear_video_frames() {
    let keys = LIVE_TILES.with(|live| {
        live.borrow_mut()
            .drain()
            .map(|(key, _)| key)
            .collect::<Vec<_>>()
    });
    for key in keys {
        blank(&key);
    }
}

async fn sweep() {
    loop {
        TimeoutFuture::new(SWEEP_INTERVAL_MS).await;
        let now = now_ms();
        let (stale, idle) = LIVE_TILES.with(|live| {
            let mut live = live.borrow_mut();
            let stale = live
                .iter()
                .filter(|(_, seen)| now - **seen > STALE_AFTER_MS)
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            for key in &stale {
                live.remove(key);
            }
            (stale, live.is_empty())
        });
        for key in stale {
            blank(&key);
        }
        if idle {
            return;
        }
    }
}

fn blank(key: &str) {
    for tile in tiles(key) {
        let _ = tile.remove_attribute("src");
    }
}

fn tiles(key: &str) -> Vec<Element> {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return Vec::new();
    };
    let Ok(nodes) = document.query_selector_all(&format!("img[data-ghost-video=\"{key}\"]")) else {
        return Vec::new();
    };
    (0..nodes.length())
        .filter_map(|index| nodes.item(index)?.dyn_into::<Element>().ok())
        .collect()
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 200
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn now_ms() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map_or(0.0, |performance| performance.now())
}

#[cfg(test)]
mod tests {
    use super::valid_key;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn tile_keys_are_selector_safe() {
        assert!(valid_key("call-remote"));
        assert!(valid_key("room-ab12-cd34_ef"));
        assert!(!valid_key(""));
        assert!(!valid_key("a\"]"));
        assert!(!valid_key("a b"));
        assert!(!valid_key(&"a".repeat(201)));
    }
}
