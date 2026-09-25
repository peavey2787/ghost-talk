use super::*;
use crate::model::PublicGhostProfile;

fn profile_with_dotk_alias() -> Profile {
    let mut profile = Profile::new("profile-a".into(), "A".into());
    profile.public_directory.push(PublicGhostProfile {
        kaspa_address: "kaspa:qcached".into(),
        dotk_name: Some("alice.k".into()),
        username: "Alice".into(),
        ..Default::default()
    });
    profile
}

#[test]
fn explicit_dotk_input_bypasses_cached_alias_addresses() {
    assert_eq!(
        resolve_recipient_target(&profile_with_dotk_alias(), "ALICE.K").unwrap(),
        "ALICE.K"
    );
}

#[test]
fn dotk_candidate_selection_preserves_name_for_fresh_verification() {
    let candidates = recipient_candidates(&profile_with_dotk_alias(), "alice.k");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].value, "alice.k");
}
