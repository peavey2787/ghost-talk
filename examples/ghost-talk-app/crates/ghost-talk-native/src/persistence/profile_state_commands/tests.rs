use super::{merge_profile_patch, validate_profile_json};

#[test]
fn profile_state_allows_only_one_auto_login_id() {
    assert!(validate_profile_json(r#"[{"autoLogin":true},{"autoLogin":false}]"#).is_ok());
    assert!(validate_profile_json(r#"[{"autoLogin":true},{"autoLogin":true}]"#).is_err());
}

#[test]
fn profile_patch_preserves_other_profiles_and_rejects_stale_revision() {
    let current = r#"[{"id":"a","stateRevision":4,"label":"new-a"},{"id":"b","stateRevision":7,"label":"new-b"}]"#;
    let stale_process = r#"[{"id":"a","stateRevision":5,"label":"next-a"},{"id":"b","stateRevision":2,"label":"stale-b"}]"#;
    let merged = merge_profile_patch(Some(current), stale_process, &["a".into()]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&merged).unwrap();
    let profiles = value.as_array().unwrap();
    let profile = |id: &str| {
        profiles
            .iter()
            .find(|profile| profile.get("id").and_then(serde_json::Value::as_str) == Some(id))
            .unwrap()
    };
    assert_eq!(
        profile("a")
            .get("label")
            .and_then(serde_json::Value::as_str),
        Some("next-a")
    );
    assert_eq!(
        profile("b")
            .get("label")
            .and_then(serde_json::Value::as_str),
        Some("new-b")
    );

    let delayed = r#"[{"id":"a","stateRevision":3,"label":"old-a"}]"#;
    let merged_again = merge_profile_patch(Some(&merged), delayed, &["a".into()]).unwrap();
    let value: serde_json::Value = serde_json::from_str(&merged_again).unwrap();
    let profile_a = value
        .as_array()
        .unwrap()
        .iter()
        .find(|profile| profile.get("id").and_then(serde_json::Value::as_str) == Some("a"))
        .unwrap();
    assert_eq!(
        profile_a.get("label").and_then(serde_json::Value::as_str),
        Some("next-a")
    );

    let conflicting = r#"[{"id":"a","stateRevision":5,"label":"different-a"}]"#;
    let error = merge_profile_patch(Some(&merged), conflicting, &["a".into()]).unwrap_err();
    assert!(error.contains("persistence conflict"));
}
