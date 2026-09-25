use base64::{engine::general_purpose::STANDARD, Engine as _};

const GHOST_TALK_LOGO_PNG: &[u8] = include_bytes!("../../../../../assets/ghost-talk-logo-ui.png");

pub fn ghost_talk_logo_src() -> String {
    format!(
        "data:image/png;base64,{}",
        STANDARD.encode(GHOST_TALK_LOGO_PNG)
    )
}
