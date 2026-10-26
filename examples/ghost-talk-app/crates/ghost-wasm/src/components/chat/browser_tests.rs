use std::cell::{Cell, RefCell};

use wasm_bindgen_test::*;
use web_sys::Element;
use yew::prelude::*;

use super::{
    composer::render_composer,
    outgoing::OutgoingText,
    retry::render_retry,
    view::{ChatProps, ChatUiState, ChatView},
};
use crate::components::browser_test_support::{click, input, settle, test_root};
use crate::model::{Chat, ChatDelta, IncomingRequest, Profile, ProfileDelta, ProfilePatch};

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    static STARTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static SENDS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static SELECTIONS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static UPDATES: RefCell<Vec<ProfilePatch>> = const { RefCell::new(Vec::new()) };
    static ACCEPT: Cell<bool> = const { Cell::new(true) };
    static RETRIES: RefCell<Vec<OutgoingText>> = const { RefCell::new(Vec::new()) };
}

fn reset_events() {
    STARTS.with(|events| events.borrow_mut().clear());
    SENDS.with(|events| events.borrow_mut().clear());
    SELECTIONS.with(|events| events.borrow_mut().clear());
    UPDATES.with(|events| events.borrow_mut().clear());
    RETRIES.with(|events| events.borrow_mut().clear());
    ACCEPT.with(|accept| accept.set(true));
}

fn take_strings(slot: &'static std::thread::LocalKey<RefCell<Vec<String>>>) -> Vec<String> {
    slot.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

fn profile_with_chat(chat: Chat) -> Profile {
    let mut profile = Profile::new("profile-a".into(), "A".into());
    ghost_chat::ChatService::push(&mut profile.chats, chat);
    profile
}

fn render_chat(root: Element, profile: Profile, selected_chat_id: String) {
    let on_select =
        Callback::from(|id: String| SELECTIONS.with(|events| events.borrow_mut().push(id)));
    let on_update = Callback::from(|patch: ProfilePatch| {
        UPDATES.with(|events| events.borrow_mut().push(patch))
    });
    let on_wallet_public = Callback::from(|_| {});
    let on_start_chat =
        Callback::from(|target: String| STARTS.with(|events| events.borrow_mut().push(target)));
    yew::Renderer::<ChatView>::with_root_and_props(
        root,
        ChatProps {
            profile,
            password: "test-password".into(),
            selected_chat_id,
            on_select,
            on_update,
            on_wallet_public,
            on_start_chat,
        },
    )
    .render();
}

#[component(ComposerHarness)]
fn composer_harness() -> Html {
    let state = ChatUiState {
        target: use_state(String::new),
        draft: use_state(String::new),
        status: use_state(String::new),
        busy: use_state(|| false),
        recording: use_state(|| false),
        show_archived: use_state(|| false),
        mask_messages: use_state(|| false),
        show_advanced: use_state(|| false),
        messages_ref: use_node_ref(),
        direct: None,
    };
    let chat = ghost_chat::ChatService::new_basic("chat-c".into(), "C".into());
    let send = Callback::from(|text: OutgoingText| {
        SENDS.with(|events| events.borrow_mut().push(text.body));
        ACCEPT.with(Cell::get)
    });
    render_composer(&state, &chat, send)
}

#[component(RetryHarness)]
fn retry_harness() -> Html {
    let mut message = crate::model::Message {
        id: "m1".into(),
        wire_id: Some("wire-1".into()),
        direction: "out".into(),
        body: "lost text".into(),
        ..Default::default()
    };
    message.mark_failed("node rejected".into());
    let send = Callback::from(|text: OutgoingText| {
        RETRIES.with(|events| events.borrow_mut().push(text));
        true
    });
    render_retry(&message, &send)
}

#[wasm_bindgen_test(async)]
async fn rendered_incoming_chat_notification_keeps_unknown_peer_identity() {
    reset_events();
    let root = test_root();
    let chat = ghost_chat::ChatService::new_incoming_request(ghost_chat::IncomingRequestChatSpec {
        direct: ghost_chat::DirectChatSpec {
            id: "chat-c".into(),
            label: "kaspatest:c".into(),
            contact_id: None,
            kaspa_address: "kaspatest:c".into(),
            hydra_handle: Some("hydra-c".into()),
            kns_name: None,
            dotk_name: None,
            verified_public: false,
        },
        request: IncomingRequest {
            request_id: "request-c".into(),
            peer_hydra_id: "hydra-c".into(),
            peer_address: "kaspatest:c".into(),
            local_address: "kaspatest:a".into(),
            signed_request_hex: "00".into(),
            ..Default::default()
        },
        room_transport_only: false,
    });
    render_chat(root.clone(), profile_with_chat(chat), "chat-c".into());
    settle().await;
    let text = root.text_content().unwrap_or_default();
    assert!(text.contains("Incoming secure chat request"));
    assert!(text.contains("kaspatest:c"));
    assert!(!text.contains("Peer B"));
    root.remove();
}

#[wasm_bindgen_test(async)]
async fn rendered_start_chat_emits_exact_typed_target() {
    reset_events();
    let root = test_root();
    render_chat(
        root.clone(),
        Profile::new("profile-a".into(), "A".into()),
        String::new(),
    );
    settle().await;
    input(&root, ".compact-start input", "kaspatest:c");
    settle().await;
    click(&root, ".compact-start .primary");
    settle().await;
    assert_eq!(take_strings(&STARTS), vec!["kaspatest:c"]);
    root.remove();
}

#[wasm_bindgen_test(async)]
async fn rendered_composer_sends_exact_message_body() {
    reset_events();
    let root = test_root();
    yew::Renderer::<ComposerHarness>::with_root(root.clone()).render();
    settle().await;
    input(&root, ".composer input", "hello C");
    settle().await;
    click(&root, ".composer .primary");
    settle().await;
    assert_eq!(take_strings(&SENDS), vec!["hello C"]);
    assert_eq!(composer_draft(&root), "");
    root.remove();
}

fn composer_draft(root: &Element) -> String {
    use wasm_bindgen::JsCast;
    root.query_selector(".composer input")
        .ok()
        .flatten()
        .and_then(|node| node.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .unwrap_or_default()
}

#[wasm_bindgen_test(async)]
async fn rendered_composer_keeps_the_draft_when_the_send_is_refused() {
    reset_events();
    ACCEPT.with(|accept| accept.set(false));
    let root = test_root();
    yew::Renderer::<ComposerHarness>::with_root(root.clone()).render();
    settle().await;
    input(&root, ".composer input", "keep me");
    settle().await;
    click(&root, ".composer .primary");
    settle().await;
    assert_eq!(take_strings(&SENDS), vec!["keep me"]);
    assert_eq!(composer_draft(&root), "keep me");
    root.remove();
}

#[wasm_bindgen_test(async)]
async fn rendered_retry_resends_the_failed_text_under_its_wire_id() {
    reset_events();
    let root = test_root();
    yew::Renderer::<RetryHarness>::with_root(root.clone()).render();
    settle().await;
    click(&root, ".message-retry");
    settle().await;
    let retries = RETRIES.with(|events| std::mem::take(&mut *events.borrow_mut()));
    assert_eq!(
        retries,
        vec![OutgoingText {
            body: "lost text".into(),
            retry_wire_id: Some("wire-1".into()),
        }]
    );
    root.remove();
}

#[wasm_bindgen_test(async)]
async fn rendered_leave_commits_local_chat_transition_before_network_cleanup() {
    reset_events();
    let root = test_root();
    let chat = ghost_chat::ChatService::new_direct_chat(ghost_chat::DirectChatSpec {
        id: "chat-c".into(),
        label: "C".into(),
        contact_id: None,
        kaspa_address: "kaspatest:c".into(),
        hydra_handle: None,
        kns_name: None,
        dotk_name: None,
        verified_public: false,
    });
    render_chat(root.clone(), profile_with_chat(chat), "chat-c".into());
    settle().await;
    click(&root, ".thread-actions .danger-link");
    settle().await;

    assert_eq!(take_strings(&SELECTIONS), vec![String::new()]);
    let patches = UPDATES.with(|events| std::mem::take(&mut *events.borrow_mut()));
    assert_eq!(patches.len(), 1);
    let mut found_leave = false;
    for delta in patches.into_iter().next().unwrap().into_deltas() {
        if let ProfileDelta::Chat(ChatDelta::Upsert { after, .. }) = delta {
            found_leave |= after.id == "chat-c"
                && after.left()
                && after.archived()
                && after.session_sid().is_none()
                && !after.bootstrap_complete();
        }
    }
    assert!(
        found_leave,
        "leave must publish the locally-ended chat before any remote cleanup"
    );
    root.remove();
}
