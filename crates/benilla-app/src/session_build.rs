//! The client build the session plays as, held once per app and handed to every protocol entry
//! point the net layer calls (`logon_as`, `connect_queued_as`; the session it opens carries the
//! build into its reader and writer).

use benilla_build::{ClientBuild, VANILLA_1_12_1};
use bevy::prelude::*;

/// The build detected from the install at startup; 1.12.1 when none was detected.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SessionBuild(pub(crate) ClientBuild);

impl Default for SessionBuild {
    fn default() -> Self {
        Self(VANILLA_1_12_1)
    }
}

/// The app's session build: what the launch inserted, else 1.12.1.
pub(crate) fn of_app(app: &App) -> SessionBuild {
    app.world()
        .get_resource::<SessionBuild>()
        .copied()
        .unwrap_or_default()
}

/// What the launch does with the install's build.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Admission {
    /// A playable build: no line.
    Play,
    /// A known build the client cannot play: the line, and the launch stops.
    Refuse(String),
    /// A known unplayable build let through by the development switch: the line, and the launch
    /// goes on.
    Proceed(String),
}

/// Decides the launch for `build`; `allow_unplayable` is the development switch, which only a
/// `dev` build ever passes as true ([`allow_unplayable_env`]).
pub(crate) fn admit(build: &ClientBuild, allow_unplayable: bool) -> Admission {
    let Some(refusal) = crate::unplayable_notice(build) else {
        return Admission::Play;
    };
    if allow_unplayable {
        Admission::Proceed(format!(
            "{refusal}; going on because WOW_ALLOW_UNPLAYABLE=1 (development switch)"
        ))
    } else {
        Admission::Refuse(refusal)
    }
}

/// `WOW_ALLOW_UNPLAYABLE=1`, honoured by dev builds only; the player build keeps the refusal.
pub(crate) fn allow_unplayable_env() -> bool {
    crate::run_mode::dev_affordances()
        && std::env::var("WOW_ALLOW_UNPLAYABLE").as_deref() == Ok("1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_build::TBC_2_4_3;

    #[test]
    fn the_default_build_is_1_12_1() {
        assert_eq!(SessionBuild::default().0, VANILLA_1_12_1);
    }

    #[test]
    fn the_app_holds_the_build_the_launch_inserted_and_defaults_to_1_12_1() {
        let mut app = App::new();
        assert_eq!(of_app(&app).0, VANILLA_1_12_1);
        app.insert_resource(SessionBuild(TBC_2_4_3));
        assert_eq!(of_app(&app).0, TBC_2_4_3);
        // Per app, not a process global: another app is unaffected.
        assert_eq!(of_app(&App::new()).0, VANILLA_1_12_1);
    }

    #[test]
    fn a_playable_build_plays_with_or_without_the_switch() {
        assert_eq!(admit(&VANILLA_1_12_1, false), Admission::Play);
        assert_eq!(admit(&VANILLA_1_12_1, true), Admission::Play);
    }

    #[test]
    fn an_unplayable_build_is_refused_without_the_switch() {
        assert_eq!(
            admit(&TBC_2_4_3, false),
            Admission::Refuse(
                "benilla: install is 2.4.3 (build 8606), which this client cannot play yet".into()
            )
        );
    }

    #[test]
    fn the_switch_lets_an_unplayable_build_through_and_says_so() {
        match admit(&TBC_2_4_3, true) {
            Admission::Proceed(line) => {
                assert!(line.contains("2.4.3 (build 8606)"), "{line}");
                assert!(line.contains("WOW_ALLOW_UNPLAYABLE=1"), "{line}");
            }
            other => panic!("{other:?}"),
        }
        // The switch changes nothing about what the build is.
        assert!(!TBC_2_4_3.playable());
    }
}
