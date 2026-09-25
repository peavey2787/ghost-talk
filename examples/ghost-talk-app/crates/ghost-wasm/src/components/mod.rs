mod account;
pub mod authenticated_profile;
pub mod avatar;
#[cfg(all(test, target_arch = "wasm32"))]
mod browser_test_support;
pub mod call;
pub mod chat;
pub mod contacts;
pub mod discover;
mod form;
pub mod kasia;
pub mod media_upload;
pub mod recipient_input;
pub mod rooms;
mod shell;
pub mod studio;

pub use account::{identity, settings, wallet};
pub use shell::{debug, sidebar, simple};
