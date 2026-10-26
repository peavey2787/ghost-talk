#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Stable content-addressed identity for media bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaReference {
    pub media_id: String,
    pub content_hash: String,
    pub content_type: String,
    pub size: u64,
    pub location: MediaLocation,
}

/// Retrieval hint. Integrity never depends on this location being trusted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum MediaLocation {
    #[default]
    Local,
    Remote(String),
}

/// Compute Ghost's canonical media content hash.
pub fn content_hash(bytes: &[u8]) -> String {
    hex::encode(blake3::hash(bytes).as_bytes())
}

/// Verify bytes against a signed/reference hash without trusting their source.
pub fn verify_content(bytes: &[u8], expected_hash: &str) -> bool {
    content_hash(bytes).eq_ignore_ascii_case(expected_hash.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_verification_detects_substitution() {
        let hash = content_hash(b"avatar");
        assert!(verify_content(b"avatar", &hash));
        assert!(!verify_content(b"substitute", &hash));
    }
}
