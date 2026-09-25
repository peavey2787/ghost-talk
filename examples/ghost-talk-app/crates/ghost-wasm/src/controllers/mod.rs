//! Application orchestration boundary between Yew components and adapters.
//!
//! Controllers own no durable domain state. Components invoke controller
//! operations; controllers coordinate or delegate to domain/native adapters.
pub(crate) mod account;
pub(crate) mod broadcast;
pub(crate) mod call;
pub(crate) mod chat;
pub(crate) mod contact;
pub(crate) mod debug;
pub(crate) mod discover;
pub(crate) mod kasia;
pub(crate) mod media;
pub(crate) mod room;

pub(crate) mod studio;
