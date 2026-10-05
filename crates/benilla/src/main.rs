//! The `benilla` launcher: a shim that carries the build stamp, so a new commit recompiles these
//! few lines and relinks instead of rebuilding `benilla-app`.

use benilla_app::BuildId;

fn main() -> benilla_app::AppExit {
    // The iOS bundle is read-only, and the `current_exe()`-relative defaults of the install search
    // and of `local_state` point into it; `$HOME` is the app container, whose `Documents` the
    // player fills through Files (`UIFileSharingEnabled`).
    #[cfg(target_os = "ios")]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        for (key, tail) in [
            ("WOW_DATA", "Documents/WoW/Data"),
            ("BENILLA_HOME", "Documents/benilla-config"),
        ] {
            if std::env::var_os(key).is_none() {
                // First statement of `main`: no thread exists yet to race the environment.
                std::env::set_var(key, format!("{home}/{tail}"));
            }
        }
        // A relative `WOW_SESSION_RECORD` (devicectl cannot know the container path) names a
        // folder under `Documents`, which Files shows and `devicectl` can copy from.
        if let Some(rel) = std::env::var_os("WOW_SESSION_RECORD").filter(|v| !v.is_empty()) {
            if std::path::Path::new(&rel).is_relative() {
                let mut full = std::path::PathBuf::from(&home);
                full.push("Documents");
                full.push(rel);
                std::env::set_var("WOW_SESSION_RECORD", full);
            }
        }
    }
    benilla_app::run(BuildId {
        version: env!("CARGO_PKG_VERSION"),
        describe: env!("BENILLA_GIT_DESCRIBE"),
        sha: env!("BENILLA_GIT_SHA"),
        short: env!("BENILLA_GIT_SHORT"),
        date: env!("BENILLA_GIT_DATE"),
        profile: env!("BENILLA_PROFILE"),
        project_dir: env!("BENILLA_PROJECT_DIR"),
        ..Default::default()
    })
}
