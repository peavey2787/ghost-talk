//! Raise a system notification when a newer incoming message arrives for the
//! active profile while Ghost Talk is in the background.

use super::AppState;
mod system;

use self::system as notifications;
use crate::app::active_profile;
pub(crate) use system::request_permission;
use yew::prelude::*;

#[hook]
pub(crate) fn use_message_notifications(state: &AppState) {
    let seen = use_mut_ref(|| None::<(String, f64)>);
    let key = active_profile(state).and_then(|profile| {
        let newest = notifications::newest_incoming(&profile)?;
        Some((profile.id, profile.settings.message_notifications, newest))
    });
    use_effect_with(key, move |key| {
        if let Some((profile_id, enabled, (at, source))) = key {
            if advance(&mut seen.borrow_mut(), profile_id, *at) && *enabled {
                notifications::notify_new_message(source);
            }
        }
    });
}

/// Record the newest incoming message time and report whether it is new.
/// The first time seen for a profile only sets the baseline, so opening the
/// app never replays history as notifications.
fn advance(seen: &mut Option<(String, f64)>, profile_id: &str, at: f64) -> bool {
    let newer = matches!(seen.as_ref(), Some((id, last)) if id == profile_id && at > *last);
    let baseline = !matches!(seen.as_ref(), Some((id, _)) if id == profile_id);
    if newer || baseline {
        *seen = Some((profile_id.to_owned(), at));
    }
    newer
}

#[cfg(test)]
mod tests {
    use super::advance;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn only_messages_newer_than_the_baseline_notify() {
        let mut seen = None;
        assert!(
            !advance(&mut seen, "a", 10.0),
            "first sight is the baseline"
        );
        assert!(!advance(&mut seen, "a", 10.0));
        assert!(advance(&mut seen, "a", 11.0));
        assert!(
            !advance(&mut seen, "a", 5.0),
            "older history never notifies"
        );
        assert!(
            !advance(&mut seen, "b", 99.0),
            "switching profile re-baselines"
        );
        assert!(advance(&mut seen, "b", 100.0));
    }
}
