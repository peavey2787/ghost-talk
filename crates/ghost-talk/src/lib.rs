//! Transport-neutral realtime voice/media primitives for Ghost Talk.
//!
//! The crate owns media framing, sender/receiver sequencing, defensive resource
//! limits, reorder handling, playback scheduling, and statistics. Network
//! transport, encryption, signaling, identity, and user-interface policy remain
//! responsibilities of the embedding application.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod chunk;
mod config;
mod error;
mod limits;
mod media;
mod receiver;
mod reorder;
mod sender;
mod sequence;
mod stats;
mod stream;
mod timing;

pub use chunk::{VoiceChunk, VOICE_WIRE_VERSION};
pub use config::{VoiceReceiverConfig, VoiceSenderConfig};
pub use error::VoiceError;
pub use limits::VoiceLimits;
pub use media::capture::EncodedAudioChunk;
pub use media::codec::VoiceCodec;
pub use media::playback::{PlaybackBackend, PlaybackReceipt};
pub use receiver::{ReceiveOutcome, VoiceReceiver};
pub use sender::VoiceSender;
pub use stats::{VoiceReceiverStats, VoiceSenderStats};
pub use stream::StreamId;
