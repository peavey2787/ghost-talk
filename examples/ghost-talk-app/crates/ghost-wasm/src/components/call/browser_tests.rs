use wasm_bindgen_test::*;

use super::video;
use crate::components::browser_test_support::{click, settle, test_root};
use crate::model::CallPhase;

wasm_bindgen_test_configure!(run_in_browser);

mod harness;
use harness::{take_events, CallHarness, CallHarnessProps};

#[wasm_bindgen_test(async)]
async fn rendered_mute_unmute_and_hangup_keep_exact_call_id_and_local_end() {
    let _ = take_events();
    let root = test_root();
    yew::Renderer::<CallHarness>::with_root_and_props(
        root.clone(),
        CallHarnessProps {
            call_id: "call-c".into(),
            phase: CallPhase::Connected,
            muted: false,
            video_available: false,
        },
    )
    .render();
    settle().await;

    click(&root, ".call-actions button:not(.danger)");
    settle().await;
    assert!(root.text_content().unwrap_or_default().contains("Unmute"));
    click(&root, ".call-actions button:not(.danger)");
    settle().await;
    assert!(root.text_content().unwrap_or_default().contains("Mute"));

    click(&root, ".call-actions .danger");
    settle().await;
    assert!(root.query_selector(".voice-call-modal").unwrap().is_none());
    assert_eq!(
        take_events(),
        vec![
            "mute:call-c:true",
            "mute:call-c:false",
            "local-end:call-c",
            "signal-failed:call-c",
        ]
    );
    root.remove();
}

#[wasm_bindgen_test(async)]
async fn rendered_accept_and_decline_bind_the_visible_incoming_call() {
    let _ = take_events();
    let root = test_root();
    yew::Renderer::<CallHarness>::with_root_and_props(
        root.clone(),
        CallHarnessProps {
            call_id: "incoming-c".into(),
            phase: CallPhase::IncomingRinging,
            muted: false,
            video_available: false,
        },
    )
    .render();
    settle().await;
    click(&root, ".call-actions .primary");
    settle().await;
    assert_eq!(take_events(), vec!["accept:incoming-c"]);
    root.remove();

    let root = test_root();
    yew::Renderer::<CallHarness>::with_root_and_props(
        root.clone(),
        CallHarnessProps {
            call_id: "incoming-d".into(),
            phase: CallPhase::IncomingRinging,
            muted: false,
            video_available: false,
        },
    )
    .render();
    settle().await;
    click(&root, ".call-actions .danger");
    settle().await;
    assert!(root.query_selector(".voice-call-modal").unwrap().is_none());
    assert_eq!(take_events(), vec!["decline:incoming-d"]);
    root.remove();
}

#[wasm_bindgen_test(async)]
async fn connected_call_offers_video_only_over_a_p2p_route_and_paints_frames() {
    let _ = take_events();
    let root = test_root();
    yew::Renderer::<CallHarness>::with_root_and_props(
        root.clone(),
        CallHarnessProps {
            call_id: "video-c".into(),
            phase: CallPhase::Connected,
            muted: false,
            video_available: false,
        },
    )
    .render();
    settle().await;
    let button = root.query_selector(".video-toggle").unwrap().unwrap();
    assert!(button.has_attribute("disabled"), "no p2p route, no video");
    let tile_selector = format!("img[data-ghost-video=\"{}\"]", video::CALL_TILE);
    let tile = root.query_selector(&tile_selector).unwrap().unwrap();
    assert!(
        !tile.has_attribute("src"),
        "tiles stay blank until frames arrive"
    );
    ghost_talk_wasm::show_video_frame(video::CALL_TILE, &[0xFF, 0xD8, 0xFF, 0xD9]);
    assert!(tile
        .get_attribute("src")
        .is_some_and(|src| src.starts_with("data:image/jpeg;base64,")));
    ghost_talk_wasm::clear_video_frames();
    assert!(!tile.has_attribute("src"));
    root.remove();

    let root = test_root();
    yew::Renderer::<CallHarness>::with_root_and_props(
        root.clone(),
        CallHarnessProps {
            call_id: "video-d".into(),
            phase: CallPhase::Connected,
            muted: false,
            video_available: true,
        },
    )
    .render();
    settle().await;
    click(&root, ".video-toggle");
    settle().await;
    assert_eq!(take_events(), vec!["video-toggle"]);
    root.remove();
}
