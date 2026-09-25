use crate::model::Profile;

#[cfg(target_arch = "wasm32")]
pub use ghost_runtime::{MailboxService, ReadyEnvelope};

pub fn parse_profiles(raw: &str) -> Vec<Profile> {
    serde_json::from_str(raw).unwrap_or_default()
}

pub fn serialize_profiles(profiles: &[Profile]) -> String {
    serde_json::to_string(profiles).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_profiles, serialize_profiles};
    use crate::model::Profile;

    #[test]
    fn profile_round_trip_preserves_canonical_runtime_model() {
        let profiles = vec![Profile::new("id".into(), "User".into())];
        let encoded = serialize_profiles(&profiles);
        assert_eq!(parse_profiles(&encoded), profiles);
    }
}
