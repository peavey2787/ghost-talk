use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Returns the single writable root for Ghost Talk runtime state.
///
/// Release builds use Tauri's normal per-application data directory directly.
/// Development/debug builds use an isolated child namespace so `cargo tauri dev`
/// can run beside an installed/release Ghost Talk instance without opening the
/// same HYDRA state directory or mutating the release profile store.
///
/// The policy is intentionally build-mode based rather than OS based. Tauri
/// still chooses the correct writable application-data location on Windows,
/// Linux, macOS, Android, and iOS.
pub(crate) fn data_root(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve Ghost Talk application-data directory: {error}"))?;
    Ok(build_mode_data_root(base, cfg!(debug_assertions)))
}

fn build_mode_data_root(base: PathBuf, development: bool) -> PathBuf {
    if development {
        base.join("development")
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::build_mode_data_root;
    use std::path::PathBuf;

    #[test]
    fn debug_builds_are_isolated_from_release_application_data() {
        let base = PathBuf::from("ghost-talk-app-data");
        assert_eq!(
            build_mode_data_root(base.clone(), true),
            base.join("development")
        );
    }

    #[test]
    fn release_builds_keep_the_canonical_application_data_root() {
        let base = PathBuf::from("ghost-talk-app-data");
        assert_eq!(build_mode_data_root(base.clone(), false), base);
    }
}
