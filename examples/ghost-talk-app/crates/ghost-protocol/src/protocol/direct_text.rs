//! Ephemeral chat text carried inside a sealed realtime body over p2p-net.
//! It is authenticated by the same HYDRA session as KKTP text but, unlike
//! KKTP mailbox messages, it is never anchored on Kaspa.

use serde::{Deserialize, Serialize};

use super::validate_kktp_message_id;

pub const DIRECT_TEXT_PREFIX: &str = "\u{1e}GHOST-P2P-TEXT-V1:";
pub const MAX_DIRECT_TEXT_BYTES: usize = 192 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectTextV1 {
    pub message_id: String,
    pub body: String,
}

impl DirectTextV1 {
    pub fn validate(&self) -> Result<(), String> {
        validate_kktp_message_id(&self.message_id)?;
        if self.body.trim().is_empty() || self.body.len() > MAX_DIRECT_TEXT_BYTES {
            return Err("direct text is empty or exceeds the p2p text limit".into());
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self)
            .map(|json| format!("{DIRECT_TEXT_PREFIX}{json}"))
            .map_err(|error| error.to_string())
    }

    pub fn decode(body: &str) -> Option<Self> {
        let json = body.strip_prefix(DIRECT_TEXT_PREFIX)?;
        let text = serde_json::from_str::<Self>(json).ok()?;
        text.validate().ok()?;
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(body: &str) -> DirectTextV1 {
        DirectTextV1 {
            message_id: "ab".repeat(16),
            body: body.into(),
        }
    }

    #[test]
    fn direct_text_round_trips() {
        let encoded = text("hello").encode().unwrap();
        assert!(encoded.starts_with(DIRECT_TEXT_PREFIX));
        assert_eq!(DirectTextV1::decode(&encoded), Some(text("hello")));
    }

    #[test]
    fn invalid_direct_text_fails_closed() {
        assert!(text(" ").encode().is_err());
        assert!(text(&"x".repeat(MAX_DIRECT_TEXT_BYTES + 1))
            .encode()
            .is_err());
        let mut bad_id = text("hi");
        bad_id.message_id = "nope".into();
        assert!(bad_id.encode().is_err());
        assert_eq!(DirectTextV1::decode("hello"), None);
        assert_eq!(
            DirectTextV1::decode(&format!("{DIRECT_TEXT_PREFIX}{{}}")),
            None
        );
    }
}
