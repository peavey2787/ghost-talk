//! Compact browser camera video: the sender samples the camera into bounded
//! JPEG frames; the display paints received frames onto tagged `<img>` tiles.
//!
//! Every frame is a complete image, so a lost or late frame costs nothing
//! but smoothness, and the transport may keep only the newest one.

mod display;
mod sender;

pub use display::{clear_video_frames, show_video_frame};
pub use sender::BrowserVideoSender;
