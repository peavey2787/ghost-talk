#![forbid(unsafe_code)]

mod catalog;
mod live;
mod podcast;
mod profile;
mod relay;
mod recording_sink;
mod rtmp;
mod rtmp_sink;

pub use catalog::BroadcastCatalog;
pub use live::{BroadcastFrame, BroadcastPipeline, BroadcastSink, SinkFailure};
pub use podcast::{PodcastEpisode, PodcastShow};
pub use profile::{CreatorProfile, StationProfile};
pub use relay::{encode_relay_frame, validate_relay_url, RelayControl, RELAY_FRAME_MAGIC, RELAY_PROTOCOL_VERSION};
pub use recording_sink::FileRecordingSink;
pub use rtmp::{validate_rtmp_configuration, RtmpDestination, RtmpSecret, RtmpTransport};
pub use rtmp_sink::FfmpegRtmpSink;

/// Validate the stable 128-bit hexadecimal session id shared by every broadcast host.
pub fn validate_session_id(value: &str) -> Result<(), String> {
    if value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("broadcast session id must be 32 hexadecimal characters".into())
    }
}
