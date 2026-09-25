use std::collections::HashSet;

pub(crate) fn merge_profile_patch(
    current: Option<&str>,
    incoming: &str,
    changed_profile_ids: &[String],
) -> Result<String, String> {
    let incoming_profiles = parse_profile_array(incoming, "Ghost Talk profile state")?;
    let mut merged = match current {
        Some(raw) => parse_profile_array(raw, "existing Ghost Talk profile state")?,
        None => Vec::new(),
    };
    for profile_id in changed_profile_ids.iter().cloned().collect::<HashSet<_>>() {
        merge_changed_profile(&mut merged, &incoming_profiles, &profile_id)?;
    }
    serde_json::to_string(&merged)
        .map_err(|error| format!("serialize merged Ghost Talk profile state: {error}"))
}

fn profile_revision(profile: &serde_json::Value) -> u64 {
    profile
        .get("stateRevision")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_default()
}

fn parse_profile_array(raw: &str, label: &str) -> Result<Vec<serde_json::Value>, String> {
    serde_json::from_str::<serde_json::Value>(raw)
        .map_err(|error| format!("{label} is invalid JSON: {error}"))?
        .as_array()
        .cloned()
        .ok_or_else(|| format!("{label} must be a JSON array"))
}

fn profile_index(profiles: &[serde_json::Value], profile_id: &str) -> Option<usize> {
    profiles.iter().position(|profile| {
        profile.get("id").and_then(serde_json::Value::as_str) == Some(profile_id)
    })
}

fn incoming_profile<'a>(
    profiles: &'a [serde_json::Value],
    profile_id: &str,
) -> Option<&'a serde_json::Value> {
    profiles
        .iter()
        .find(|profile| profile.get("id").and_then(serde_json::Value::as_str) == Some(profile_id))
}

fn merge_changed_profile(
    merged: &mut Vec<serde_json::Value>,
    incoming: &[serde_json::Value],
    profile_id: &str,
) -> Result<(), String> {
    let candidate = incoming_profile(incoming, profile_id);
    let current_index = profile_index(merged, profile_id);
    match (candidate, current_index) {
        (Some(candidate), Some(index)) => {
            replace_profile_if_newer(merged, index, candidate, profile_id)
        }
        (Some(candidate), None) => {
            merged.push(candidate.clone());
            Ok(())
        }
        (None, Some(index)) => {
            merged.remove(index);
            Ok(())
        }
        (None, None) => Ok(()),
    }
}

fn replace_profile_if_newer(
    merged: &mut [serde_json::Value],
    index: usize,
    candidate: &serde_json::Value,
    profile_id: &str,
) -> Result<(), String> {
    let candidate_revision = profile_revision(candidate);
    let current_revision = profile_revision(&merged[index]);
    if candidate_revision > current_revision {
        merged[index] = candidate.clone();
        return Ok(());
    }
    if candidate_revision < current_revision || merged[index] == *candidate {
        return Ok(());
    }
    Err(format!(
        "Ghost Talk profile persistence conflict for ID {profile_id}: revision {candidate_revision} has different contents"
    ))
}
