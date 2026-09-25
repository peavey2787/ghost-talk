#![cfg(target_arch = "wasm32")]

mod media_api;
pub use media_api::{
    blob_to_bytes, create_opus_media_recorder, microphone_stream, preferred_opus_mime,
    stop_stream_tracks,
};
mod receiver;
mod sender;

pub use receiver::BrowserVoiceReceiver;
pub use sender::BrowserVoiceSender;
