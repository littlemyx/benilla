//! The session record's input rows (`WOW_SESSION_RECORD`), a dev instrument: every mouse button
//! edge and key edge with the layer that took it, the iOS shim's own events, and a status row every
//! five seconds, so a session of clicks that did nothing says where each one went.
//!
//! Format, after `t=<ms> input `:
//! `btn <Button> press|release pos=(x,y) ui=(x,y) win=<w>x<h>@<scale> layer=<layer> ...`;
//! `key press|release <KeyCode> typing=<bool> consumed=<bool> covered=<bool> bind[...] move=<0x..>`;
//! `shim <kind> <text>` (iOS only: GameController, hover and touch events as the shim saw them);
//! `status ...`. The layer is the first that took the press: `ui:<frame>` (a mouse-enabled
//! interface frame under the pointer), `ui-drop` (a payload dropped on empty world), `camera:<Left|
//! Right>` (a look session the press started, with the pick it latched), `none`, or `no-cursor`
//! (the window had no pointer position, so the UI's mouse half never ran).
//!
//! Budget: the first [`FIRST`] rows of the shared kinds, then one row in [`EVERY`]; the shim's
//! high-rate kinds (`move`, `hover`, `scroll`, `touch-move`) have their own, smaller allowance, and
//! the one-shot markers (`first`, `lock`) are never dropped.

use std::collections::HashMap;
use std::time::Duration;

use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::session_record::{enabled, line};

/// Rows of the shared kinds written in full before the sampling starts.
const FIRST: u32 = 500;
/// After [`FIRST`], one row in this many.
const EVERY: u32 = 100;
/// Rows of a high-rate shim kind written in full.
#[cfg(any(target_os = "ios", test))]
const FIRST_HIGH_RATE: u32 = 20;
/// After [`FIRST_HIGH_RATE`], one row in this many.
#[cfg(any(target_os = "ios", test))]
const EVERY_HIGH_RATE: u32 = 200;
/// How long a button held without a release is called stuck, once per press.
const STUCK: Duration = Duration::from_secs(3);
/// The status row's period.
const STATUS_EVERY: Duration = Duration::from_secs(5);
/// The frame-time window.
const FRAMES_EVERY: Duration = Duration::from_secs(10);

/// How many rows each kind has had, to ration them.
#[derive(Resource, Default)]
struct Budget(HashMap<&'static str, u32>);

impl Budget {
    /// Whether the next row of `kind` is written: the first `first`, then one in `every`.
    fn allow(&mut self, kind: &'static str, first: u32, every: u32) -> bool {
        let n = self.0.entry(kind).or_insert(0);
        *n += 1;
        *n <= first || n.is_multiple_of(every)
    }

    /// The shared allowance of the app's own rows and the shim's rare events.
    fn shared(&mut self) -> bool {
        self.allow("shared", FIRST, EVERY)
    }
}

/// When each held button went down, and whether its stuck row is out.
#[derive(Resource, Default)]
struct Held(HashMap<MouseButton, (Duration, bool)>);

/// The window as a row names it: logical size at the scale factor.
fn window_text(window: &Window) -> String {
    format!(
        "{}x{}@{}",
        window.width(),
        window.height(),
        window.resolution.scale_factor()
    )
}

/// A point as a row names it.
fn point_text(p: Option<Vec2>) -> String {
    match p {
        Some(p) => format!("({:.1},{:.1})", p.x, p.y),
        None => "none".into(),
    }
}

/// The layer that took a press, from the state the frame ended in; see the module doc.
fn layer(
    has_cursor: bool,
    ui_frame: Option<&str>,
    click_consumed: bool,
    look: &str,
    covered: bool,
) -> String {
    if covered {
        "cover".into()
    } else if !has_cursor {
        "no-cursor".into()
    } else if let Some(name) = ui_frame {
        format!("ui:{name}")
    } else if click_consumed {
        "ui-drop".into()
    } else if look != "none" {
        format!("camera:{look}")
    } else {
        "none".into()
    }
}

/// What the world pick held, a unit's guid or an object's.
fn pick_text(pick: &crate::target::PressPick) -> String {
    match (pick.hovered.guid, pick.object.guid) {
        (Some(g), _) => format!("unit:{g:#x}"),
        (None, Some(g)) => format!("object:{g:#x}"),
        _ => "none".into(),
    }
}

/// The player's state, as the movement and the pick read it.
fn player_text(player: &crate::player::Player) -> String {
    format!(
        "active={} detached={} settling={} control_lost={} reseat={} flags={:#x}",
        player.active,
        player.detached,
        player.settling,
        player.control_lost,
        player.reseat,
        player.move_flags()
    )
}

/// One row per mouse button edge: where, what the interface hit-test said, and who took it.
#[allow(clippy::too_many_arguments)]
fn record_buttons(
    mut edges: MessageReader<MouseButtonInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    probe: crate::ui_script::PointerFrameProbe,
    ui_scale: Res<crate::ui_script::UiScaleCvar>,
    hover: Res<crate::ui_script::PlayerUiHover>,
    consumed: Res<crate::ui_script::PlayerUiClickConsumed>,
    over_ui: Res<crate::ui_script::PointerOverUi>,
    rig: Res<crate::player::CameraControl>,
    pick: Res<crate::target::PressPick>,
    cover: Res<crate::loading_screen::LoadingScreen>,
    mut world_clicks: (
        MessageReader<benilla_world::interact::WorldClick>,
        MessageReader<benilla_world::interact::WorldRightClick>,
    ),
    time: Res<Time<Real>>,
    mut held: ResMut<Held>,
    mut budget: ResMut<Budget>,
) {
    let Ok(window) = windows.single() else { return };
    let clicked = (
        world_clicks.0.read().count() > 0,
        world_clicks.1.read().count() > 0,
    );
    let pos = window.cursor_position();
    let ui = pos.map(|c| {
        let s = crate::ui_script::seam_scale(window.height(), ui_scale.0);
        Vec2::new(c.x / s, (window.height() - c.y) / s)
    });
    let frame = ui.and_then(|p| probe.name_at(p.x, p.y));
    // The world frame is hit-tested like any frame but is the world's, not the interface's.
    let ui_frame = frame.as_deref().filter(|n| *n != "WorldFrame");
    let look = rig.look_label();
    let now = time.elapsed();
    for ev in edges.read() {
        let pressed = ev.state == ButtonState::Pressed;
        if pressed {
            held.0.insert(ev.button, (now, false));
        }
        let held_for = (!pressed)
            .then(|| held.0.remove(&ev.button))
            .flatten()
            .map(|(t0, _)| now.saturating_sub(t0));
        if !budget.shared() {
            continue;
        }
        let layer = layer(pos.is_some(), ui_frame, consumed.0, &look, cover.covering());
        line(
            "input",
            &format!(
                "btn {:?} {} pos={} ui={} win={} layer={layer} ui-hit={} ui-hover={:?} over-ui={} \
                 look={look} pick={} world-click={}/{}{}",
                ev.button,
                if pressed { "press" } else { "release" },
                point_text(pos),
                point_text(ui),
                window_text(window),
                frame.as_deref().unwrap_or("none"),
                hover.0,
                over_ui.0,
                pick_text(&pick),
                clicked.0,
                clicked.1,
                held_for.map_or(String::new(), |d| format!(" held-ms={}", d.as_millis())),
            ),
        );
    }
    // A press the OS never released: the next click cannot happen until it does.
    for (button, (t0, said)) in held.0.iter_mut() {
        if !*said && now.saturating_sub(*t0) >= STUCK {
            *said = true;
            line(
                "input",
                &format!(
                    "btn {button:?} held {}s with no release look={look} cursor={}",
                    STUCK.as_secs(),
                    point_text(pos)
                ),
            );
        }
    }
}

/// A key edge: whether the keyboard capture or the cover took it, and what the bindings did.
fn record_keys(
    mut edges: MessageReader<KeyboardInput>,
    capture: Res<crate::ui_script::UiKeyboardCapture>,
    cover: Res<crate::loading_screen::LoadingScreen>,
    bindings: Res<crate::bindings::BindingsState>,
    player: Res<crate::player::Player>,
    mut budget: ResMut<Budget>,
) {
    for ev in edges.read() {
        if !budget.shared() {
            continue;
        }
        line(
            "input",
            &format!(
                "key {} {:?} typing={} consumed={} covered={} bind[{}] {}",
                if ev.state == ButtonState::Pressed {
                    "press"
                } else {
                    "release"
                },
                ev.key_code,
                capture.typing,
                capture.consumed.contains(&ev.key_code),
                cover.covering(),
                bindings.summary(),
                player_text(&player),
            ),
        );
    }
}

/// The iOS shim's events, as it queued them: the record's view of what the device delivered.
#[cfg(target_os = "ios")]
fn record_shim(mut budget: ResMut<Budget>) {
    for (kind, text) in benilla_ios_input::take_notes() {
        let write = match kind {
            "first" | "lock" | "key-unmapped" | "button-dup" => true,
            "move" | "hover" | "scroll" | "touch-move" => {
                budget.allow(kind, FIRST_HIGH_RATE, EVERY_HIGH_RATE)
            }
            _ => budget.shared(),
        };
        if write {
            line("input", &format!("shim {kind} {text}"));
        }
    }
}

/// Every [`STATUS_EVERY`], and on a change of lifecycle state: the frame's whole input situation.
#[allow(clippy::too_many_arguments)]
fn record_status(
    time: Res<Time<Real>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    state: Res<State<crate::char_select::ClientState>>,
    cover: Res<crate::loading_screen::LoadingScreen>,
    rig: Res<crate::player::CameraControl>,
    capture: Res<crate::ui_script::UiKeyboardCapture>,
    hover: Res<crate::ui_script::PlayerUiHover>,
    player: Res<crate::player::Player>,
    mut last: Local<Option<Duration>>,
) {
    let now = time.elapsed();
    let due = last.is_none_or(|t| now.saturating_sub(t) >= STATUS_EVERY);
    if !due {
        return;
    }
    *last = Some(now);
    let Ok(window) = windows.single() else { return };
    line(
        "input",
        &format!(
            "status state={:?} win={} focused={} cursor={} covered={} look={} \
             typing={} ui-hover={:?} {} pos={:.0},{:.0},{:.0}",
            state.get(),
            window_text(window),
            window.focused,
            point_text(window.cursor_position()),
            cover.covering(),
            rig.look_label(),
            capture.typing,
            hover.0,
            player_text(&player),
            player.pos.x,
            player.pos.y,
            player.pos.z,
        ),
    );
}

/// One `frames` row per [`FRAMES_EVERY`]: the count, the rate, the mean and p95 frame time.
fn record_frames(
    time: Res<Time<Real>>,
    state: Res<State<crate::char_select::ClientState>>,
    mut window: Local<(Vec<f32>, Duration)>,
) {
    window.0.push(time.delta_secs() * 1000.0);
    let now = time.elapsed();
    if now.saturating_sub(window.1) < FRAMES_EVERY {
        return;
    }
    let secs = now.saturating_sub(window.1).as_secs_f32().max(0.001);
    window.0.sort_by(f32::total_cmp);
    let n = window.0.len();
    let mean = window.0.iter().sum::<f32>() / n as f32;
    let p95 = window.0[((n * 95).div_ceil(100)).saturating_sub(1).min(n - 1)];
    line(
        "frames",
        &format!(
            "n={n} fps={:.1} mean-ms={mean:.2} p95-ms={p95:.2} max-ms={:.2} state={:?}",
            n as f32 / secs,
            window.0[n - 1],
            state.get()
        ),
    );
    *window = (Vec::new(), now);
}

/// The display's refresh ceiling, once: what the OS offers, against the `frames` rows' rate.
#[cfg(target_os = "ios")]
fn record_display() {
    match benilla_ios_input::display_max_fps() {
        Some(hz) => line("display", &format!("max-fps={hz}")),
        None => line("display", "max-fps=unknown"),
    }
}

/// Adds the input rows when the session record is on.
pub(crate) struct InputRecordPlugin;

impl Plugin for InputRecordPlugin {
    fn build(&self, app: &mut App) {
        if !enabled() {
            return;
        }
        #[cfg(target_os = "ios")]
        benilla_ios_input::enable_notes();
        app.init_resource::<Budget>()
            .init_resource::<Held>()
            .add_systems(
                PostUpdate,
                (record_buttons, record_keys, record_status, record_frames),
            );
        #[cfg(target_os = "ios")]
        app.add_systems(PostUpdate, record_shim)
            .add_systems(Startup, record_display);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_writes_the_first_rows_then_one_in_a_hundred() {
        let mut b = Budget::default();
        let written = (0..FIRST + 1000).filter(|_| b.shared()).count() as u32;
        assert_eq!(written, FIRST + (FIRST + 1000) / EVERY - FIRST / EVERY);
        // A kind has its own count.
        assert!(b.allow("move", FIRST_HIGH_RATE, EVERY_HIGH_RATE));
    }

    #[test]
    fn a_press_names_the_first_layer_that_took_it() {
        assert_eq!(
            layer(true, Some("ActionButton1"), false, "none", false),
            "ui:ActionButton1"
        );
        assert_eq!(layer(true, None, true, "none", false), "ui-drop");
        assert_eq!(layer(true, None, false, "Left", false), "camera:Left");
        assert_eq!(layer(true, None, false, "none", false), "none");
        assert_eq!(layer(false, Some("X"), false, "Left", false), "no-cursor");
        assert_eq!(layer(true, Some("X"), false, "Left", true), "cover");
    }

    #[test]
    fn a_point_is_one_decimal_or_none() {
        assert_eq!(
            point_text(Some(Vec2::new(204.01569, 808.7285))),
            "(204.0,808.7)"
        );
        assert_eq!(point_text(None), "none");
    }
}
