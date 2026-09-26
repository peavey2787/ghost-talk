mod view;
pub(crate) use view::ChatView;
mod chat_actions;
mod composer;
mod direct_send;
mod lifecycle;
mod message_rendering;
pub(crate) mod reaction_ui;

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests;

pub(crate) use super::form::{prevent_submit, text_input as input};
