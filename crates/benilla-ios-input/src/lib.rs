//! The iPad's hardware keyboard and mouse as Bevy input messages.
//!
//! winit 0.30 on iOS delivers no key codes (only `insertText` as an `Unidentified` key with
//! text) and no mouse events, and refuses `set_cursor_grab`. This plugin reads Apple's
//! GameController (`GCKeyboard`, `GCMouse`) instead and writes the messages the game reads:
//! `KeyboardInput` (physical code, US-layout logical key, text on press), `MouseMotion`,
//! `MouseButtonInput`, `MouseWheel` and a software `CursorMoved`.
//!
//! The cursor is the position iPadOS draws, from a `UIHoverGestureRecognizer` on winit's view
//! ([`Raw::Hover`], logical points, y down): `drain` writes `CursorMoved` and sets the window's
//! cursor position, which is what the game reads (`Window::cursor_position`). Raw `GCMouse` deltas
//! are not that position (iPadOS accelerates the drawn pointer), so they only feed `MouseMotion`
//! once a hover has been seen; until then they are integrated from the window centre as a
//! fallback. While [`PointerLock`] is locked (a mouse-look session) the cursor position is not
//! updated, so UI hover freezes where it was; `MouseMotion` flows either way. Off iOS the plugin
//! does nothing and the crate only builds, so the translation below is tested on the host.
//!
//! Setting the window's position makes bevy_winit's `changed_windows` call winit's
//! `set_cursor_position` whenever it differs from its cache (`bevy_winit/src/system.rs:401-407`);
//! winit on iOS returns `NotSupported`, so each move makes bevy log `error!("could not set cursor
//! position…")`; `benilla-world`'s log filter silences that target on iOS.
//!
//! iPadOS ends the hover the moment a trackpad button goes down and reports the pointer as a touch
//! until the release, so while a mouse button is held `HoverEnd` is ignored and the winit
//! `TouchInput` position (logical points, `bevy_winit/src/state.rs` `WindowEvent::Touch`) is the
//! cursor; that is what makes a drag work. Touches with no button held are fingers and are ignored.
//!
//! The pointer is locked by answering `prefersPointerLocked` on winit's root view controller,
//! which winit does not implement; see [`PointerLock::set_locked`].

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Mutex;

use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::input::{ButtonState, InputSystems};
use bevy::prelude::*;
use bevy::window::{CursorLeft, CursorMoved, PrimaryWindow};

#[cfg(target_os = "ios")]
mod audio;
pub mod keycodes;
#[cfg(target_os = "ios")]
pub use audio::activate_playback_session;
#[cfg(target_os = "ios")]
mod native;
#[cfg(target_os = "ios")]
pub use native::{display_max_fps, ui_pasteboard_read, ui_pasteboard_write};

pub use keycodes::{key_code, logical_text};

/// The notes queue is on: the session record wants the shim's events ([`enable_notes`]).
static NOTES_ON: AtomicBool = AtomicBool::new(false);
/// What the shim saw, `(kind, text)`, for the session record to drain each frame.
static NOTES: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());
/// The most notes held between two drains; the rest are dropped, never the app's memory.
const NOTES_MAX: usize = 4096;

/// Starts queueing the shim's events as notes ([`take_notes`]); off until asked, so a run without a
/// session record queues nothing.
pub fn enable_notes() {
    NOTES_ON.store(true, Ordering::Relaxed);
}

/// The notes queued since the last call, oldest first.
pub fn take_notes() -> Vec<(&'static str, String)> {
    std::mem::take(&mut *NOTES.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Queues one note when the queue is on; `text` is built only then.
fn note(kind: &'static str, text: impl FnOnce() -> String) {
    if !NOTES_ON.load(Ordering::Relaxed) {
        return;
    }
    let mut q = NOTES.lock().unwrap_or_else(|e| e.into_inner());
    if q.len() < NOTES_MAX {
        q.push((kind, text()));
    }
}

/// One device event, as GameController reported it, before it becomes a Bevy message.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Raw {
    /// A key change; `hid` is the `GCKeyCode`, a USB HID usage ID.
    Key {
        hid: u16,
        pressed: bool,
    },
    /// A relative mouse movement, in GameController's orientation (y up).
    Move {
        dx: f32,
        dy: f32,
    },
    Button {
        button: MouseButton,
        pressed: bool,
    },
    /// A scroll step, in GameController's orientation.
    Scroll {
        dx: f32,
        dy: f32,
    },
    /// The pointer's position as UIKit draws it, in logical points, origin top-left, y down.
    Hover {
        x: f32,
        y: f32,
    },
    /// The pointer left the view.
    HoverEnd,
    /// A device went away: release everything held, or a key stays down forever.
    Reset,
}

/// The receiving end of the device handlers' channel.
#[derive(Resource)]
pub struct RawInbox(pub Mutex<Receiver<Raw>>);

/// What the translation remembers between frames.
#[derive(Resource, Default)]
pub struct InputState {
    held_keys: HashSet<u16>,
    held_buttons: HashSet<MouseButton>,
    /// The software cursor, in logical window pixels; `None` until the first move.
    cursor: Option<Vec2>,
    /// A hover position has arrived: the deltas no longer move the cursor.
    hover_seen: bool,
    /// The kinds of [`Raw`] already logged once, for the on-device channel diagnostics.
    logged: HashSet<&'static str>,
    /// Events the drag trace may still log; armed to [`TRACE_EVENTS`] by the first Left press.
    trace_remaining: u32,
    /// The trace has armed (once per process).
    trace_armed: bool,
    /// The last [`TRACE_RING`] events before the trace armed, dumped when it does.
    trace_ring: VecDeque<TraceEv>,
}

/// How many events the drag trace logs after the first Left press.
const TRACE_EVENTS: u32 = 400;
/// How many pre-arm events the trace remembers.
const TRACE_RING: usize = 8;
const TRACE: &str = "benilla_ios_input::trace";

/// One event the drag trace remembers or logs.
#[derive(Debug, Clone, Copy)]
enum TraceEv {
    Raw(Raw),
    Touch {
        phase: TouchPhase,
        id: u64,
        pos: Vec2,
    },
}

impl InputState {
    /// Whether the drag trace is logging.
    fn tracing(&self) -> bool {
        self.trace_remaining > 0
    }

    /// Feeds the drag trace one event, before it is handled: remembered while unarmed, armed by the
    /// first Left press (dumping the ring), logged while armed until the cap.
    fn trace(&mut self, ev: TraceEv, locked: bool) {
        if !self.trace_armed {
            if matches!(
                ev,
                TraceEv::Raw(Raw::Button {
                    button: MouseButton::Left,
                    pressed: true
                })
            ) {
                self.trace_armed = true;
                self.trace_remaining = TRACE_EVENTS;
                info!(target: TRACE, "armed by the first Left press; last {} events before it:", self.trace_ring.len());
                for old in std::mem::take(&mut self.trace_ring) {
                    info!(target: TRACE, "pre {old:?}");
                }
            } else {
                if self.trace_ring.len() == TRACE_RING {
                    self.trace_ring.pop_front();
                }
                self.trace_ring.push_back(ev);
                return;
            }
        }
        if self.trace_remaining == 0 {
            return;
        }
        self.trace_remaining -= 1;
        let (held, cursor) = (&self.held_buttons, self.cursor);
        let lock = if locked { " locked" } else { "" };
        match ev {
            TraceEv::Raw(raw) => {
                info!(target: TRACE, "raw {raw:?} held={held:?} cursor={cursor:?}{lock}")
            }
            TraceEv::Touch { phase, id, pos } => {
                info!(target: TRACE, "touch {phase:?} id={id} pos={pos:?} held={held:?} cursor={cursor:?}{lock}")
            }
        }
    }

    /// Logs the first occurrence of each event kind per process; no per-event logging.
    fn first(&mut self, raw: &Raw) {
        let kind = match raw {
            Raw::Key { .. } => "Key",
            Raw::Move { .. } => "Move",
            Raw::Button {
                button: MouseButton::Left,
                ..
            } => "Button(Left)",
            Raw::Button {
                button: MouseButton::Right,
                ..
            } => "Button(Right)",
            Raw::Button {
                button: MouseButton::Middle,
                ..
            } => "Button(Middle)",
            Raw::Button { .. } => "Button(other)",
            Raw::Scroll { .. } => "Scroll",
            Raw::Hover { .. } => "Hover",
            Raw::HoverEnd => "HoverEnd",
            Raw::Reset => return,
        };
        if self.logged.insert(kind) {
            info!("ios input: first {kind}");
            note("first", || kind.to_string());
        }
    }
}

/// The pointer-lock request, set by the camera code on entering and leaving mouse-look.
#[derive(Resource, Default)]
pub struct PointerLock {
    locked: bool,
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    /// The value the root view controller was last told to re-query for.
    told: bool,
}

/// What the root view controller's `prefersPointerLocked` answers.
pub(crate) static PREFERS_LOCKED: AtomicBool = AtomicBool::new(false);

impl PointerLock {
    /// Ask iPadOS to capture the pointer (true) or release it. The plugin makes the root view
    /// controller re-query `prefersPointerLocked` on the next frame; UIKit honours it only while
    /// the scene is foreground and a pointer is attached.
    pub fn set_locked(&mut self, locked: bool) {
        if self.locked != locked {
            info!("ios pointer lock: {locked}");
            note("lock", || locked.to_string());
        }
        self.locked = locked;
        PREFERS_LOCKED.store(locked, Ordering::Relaxed);
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }
}

/// Adds the iPad hardware keyboard and mouse. A no-op off iOS.
pub struct IosInputPlugin;

impl Plugin for IosInputPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(target_os = "ios")]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            app.insert_resource(RawInbox(Mutex::new(rx)))
                .init_resource::<InputState>()
                .init_resource::<PointerLock>()
                .insert_non_send_resource(native::Native::new(tx))
                .add_systems(PreStartup, native::attach)
                .add_systems(
                    PreUpdate,
                    (
                        native::attach_hover,
                        native::attach_pointer_hider,
                        native::sync_pointer_lock,
                        drain,
                    )
                        .chain()
                        .before(InputSystems),
                );
        }
        #[cfg(not(target_os = "ios"))]
        let _ = (app, InputSystems);
    }
}

/// The note kind of a raw event; the session record budgets each kind on its own.
fn raw_kind(raw: &Raw) -> &'static str {
    match raw {
        Raw::Key { .. } => "key",
        Raw::Move { .. } => "move",
        Raw::Button { .. } => "button",
        Raw::Scroll { .. } => "scroll",
        Raw::Hover { .. } => "hover",
        Raw::HoverEnd => "hoverend",
        Raw::Reset => "reset",
    }
}

/// Moves the software cursor to `pos` (logical): the message, and the window's own position.
fn move_cursor(
    state: &mut InputState,
    win: &mut Window,
    cursor: &mut MessageWriter<CursorMoved>,
    window: Entity,
    pos: Vec2,
    scale: f32,
) {
    let delta = state.cursor.map(|prev| pos - prev);
    state.cursor = Some(pos);
    win.set_physical_cursor_position(Some((pos * scale).as_dvec2()));
    if state.tracing() {
        info!(target: TRACE, "  cursor write pos={pos:?} delta={delta:?} physical={:?}", win.physical_cursor_position());
    }
    cursor.write(CursorMoved {
        window,
        position: pos,
        delta,
    });
}

/// Turns the queued device events into Bevy messages for the primary window.
pub fn drain(
    inbox: Res<RawInbox>,
    mut state: ResMut<InputState>,
    lock: Res<PointerLock>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut keys: MessageWriter<KeyboardInput>,
    mut motion: MessageWriter<MouseMotion>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut wheel: MessageWriter<MouseWheel>,
    mut cursor: MessageWriter<CursorMoved>,
    mut left: MessageWriter<CursorLeft>,
    mut touches: MessageReader<TouchInput>,
) {
    let Ok((window, mut win)) = windows.single_mut() else {
        return;
    };
    let size = Vec2::new(win.width(), win.height());
    let scale = win.resolution.scale_factor();
    let rx = inbox.0.lock().unwrap_or_else(|e| e.into_inner());
    let state = &mut *state;
    while let Ok(raw) = rx.try_recv() {
        state.first(&raw);
        state.trace(TraceEv::Raw(raw), lock.is_locked());
        note(raw_kind(&raw), || {
            format!(
                "{raw:?} held={:?} cursor={:?} locked={}",
                state.held_buttons,
                state.cursor,
                lock.is_locked()
            )
        });
        match raw {
            Raw::Key { hid, pressed } => {
                let Some(key_code) = key_code(hid) else {
                    note("key-unmapped", || {
                        format!("hid={hid:#04x} pressed={pressed}")
                    });
                    continue;
                };
                // Auto-repeat is not reported by GameController; a held key arrives once.
                let changed = if pressed {
                    state.held_keys.insert(hid)
                } else {
                    state.held_keys.remove(&hid)
                };
                if !changed {
                    continue;
                }
                let shift = state.held_keys.contains(&0xE1) || state.held_keys.contains(&0xE5);
                let Some((logical_key, text)) = logical_text(hid, shift) else {
                    continue;
                };
                keys.write(KeyboardInput {
                    key_code,
                    logical_key,
                    state: if pressed {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    },
                    text: if pressed { text } else { None },
                    repeat: false,
                    window,
                });
            }
            Raw::Move { dx, dy } => {
                // GCMouse y is up, bevy's is down: flipped. Unverified on device.
                let delta = Vec2::new(dx, -dy);
                motion.write(MouseMotion { delta });
                if !lock.is_locked() && !state.hover_seen {
                    let pos = (state.cursor.unwrap_or(size / 2.0) + delta).clamp(Vec2::ZERO, size);
                    move_cursor(state, &mut win, &mut cursor, window, pos, scale);
                }
            }
            Raw::Hover { x, y } => {
                state.hover_seen = true;
                if !lock.is_locked() {
                    move_cursor(state, &mut win, &mut cursor, window, Vec2::new(x, y), scale);
                }
            }
            Raw::HoverEnd => {
                // iPadOS ends the hover when the trackpad button goes down (the pointer becomes
                // a touch); the position then comes from `TouchInput` until the release.
                if !lock.is_locked() && state.held_buttons.is_empty() {
                    state.cursor = None;
                    win.set_physical_cursor_position(None);
                    left.write(CursorLeft { window });
                } else if state.tracing() {
                    info!(target: TRACE, "  HoverEnd ignored (locked={}, held={:?})", lock.is_locked(), state.held_buttons);
                }
            }
            Raw::Button { button, pressed } => {
                let changed = if pressed {
                    state.held_buttons.insert(button)
                } else {
                    state.held_buttons.remove(&button)
                };
                if changed {
                    buttons.write(MouseButtonInput {
                        button,
                        state: if pressed {
                            ButtonState::Pressed
                        } else {
                            ButtonState::Released
                        },
                        window,
                    });
                } else {
                    note("button-dup", || {
                        format!("{button:?} pressed={pressed} dropped: no edge")
                    });
                }
            }
            Raw::Scroll { dx, dy } => {
                // Unit and sign unverified on device: GCDeviceCursor values are taken as notches
                // (Line) with y up, matching bevy's wheel (positive y scrolls up).
                wheel.write(MouseWheel {
                    unit: MouseScrollUnit::Line,
                    x: dx,
                    y: dy,
                    window,
                });
            }
            Raw::Reset => {
                for hid in state.held_keys.drain() {
                    if let Some((logical_key, _)) = logical_text(hid, false) {
                        if let Some(key_code) = key_code(hid) {
                            keys.write(KeyboardInput {
                                key_code,
                                logical_key,
                                state: ButtonState::Released,
                                text: None,
                                repeat: false,
                                window,
                            });
                        }
                    }
                }
                for button in state.held_buttons.drain() {
                    buttons.write(MouseButtonInput {
                        button,
                        state: ButtonState::Released,
                        window,
                    });
                }
            }
        }
    }
    // While a mouse button is held iPadOS reports the indirect pointer as a touch, and the hover
    // is silent: that touch is the cursor. With no button held a touch is a finger and is left
    // alone, so the screen does not move the software cursor.
    for touch in touches.read() {
        state.trace(
            TraceEv::Touch {
                phase: touch.phase,
                id: touch.id,
                pos: touch.position,
            },
            lock.is_locked(),
        );
        let applied = !state.held_buttons.is_empty() && !lock.is_locked();
        note(
            match touch.phase {
                TouchPhase::Moved => "touch-move",
                _ => "touch",
            },
            || {
                format!(
                    "{:?} pos={:?} applied={applied} held={:?} locked={}",
                    touch.phase,
                    touch.position,
                    state.held_buttons,
                    lock.is_locked()
                )
            },
        );
        if !applied {
            continue;
        }
        if matches!(touch.phase, TouchPhase::Started | TouchPhase::Moved) {
            if state.logged.insert("Touch") {
                info!("ios input: first Touch");
            }
            move_cursor(state, &mut win, &mut cursor, window, touch.position, scale);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{channel, Sender};

    fn app() -> (App, Sender<Raw>) {
        let (tx, rx) = channel();
        let mut app = App::new();
        app.add_message::<KeyboardInput>()
            .add_message::<MouseMotion>()
            .add_message::<MouseButtonInput>()
            .add_message::<MouseWheel>()
            .add_message::<CursorMoved>()
            .add_message::<CursorLeft>()
            .add_message::<TouchInput>()
            .insert_resource(RawInbox(Mutex::new(rx)))
            .init_resource::<InputState>()
            .init_resource::<PointerLock>()
            .add_systems(Update, drain);
        let mut window = Window::default();
        window.resolution.set_scale_factor_override(Some(2.0));
        window.resolution.set(800.0, 600.0);
        app.world_mut().spawn((window, PrimaryWindow));
        (app, tx)
    }

    fn read<M: Message + Clone>(app: &App) -> Vec<M> {
        let msgs = app.world().resource::<Messages<M>>();
        msgs.iter_current_update_messages().cloned().collect()
    }

    #[test]
    fn keys_carry_shift_and_text() {
        let (mut app, tx) = app();
        for (hid, pressed) in [
            (0xE1, true),
            (0x04, true),
            (0x04, false),
            (0xE1, false),
            (0x04, true),
        ] {
            tx.send(Raw::Key { hid, pressed }).unwrap();
        }
        app.update();
        let k = read::<KeyboardInput>(&app);
        assert_eq!(k.len(), 5);
        assert_eq!(k[1].text.as_deref(), Some("A"));
        assert_eq!(k[2].text, None);
        assert_eq!(k[4].text.as_deref(), Some("a"));
    }

    #[test]
    fn cursor_clamps_and_freezes_while_locked() {
        let (mut app, tx) = app();
        tx.send(Raw::Move { dx: 10.0, dy: 5.0 }).unwrap();
        tx.send(Raw::Move {
            dx: 5000.0,
            dy: 0.0,
        })
        .unwrap();
        app.update();
        let c = read::<CursorMoved>(&app);
        assert_eq!(c[0].position, Vec2::new(410.0, 295.0));
        assert_eq!(c[1].position, Vec2::new(800.0, 295.0));
        assert_eq!(read::<MouseMotion>(&app)[0].delta, Vec2::new(10.0, -5.0));

        app.world_mut()
            .resource_mut::<PointerLock>()
            .set_locked(true);
        tx.send(Raw::Move { dx: -3.0, dy: 0.0 }).unwrap();
        app.update();
        assert!(read::<CursorMoved>(&app).is_empty());
        assert_eq!(read::<MouseMotion>(&app).len(), 1);
        app.world_mut()
            .resource_mut::<PointerLock>()
            .set_locked(false);
    }

    fn cursor_of(app: &mut App) -> Option<Vec2> {
        let mut q = app.world_mut().query::<&Window>();
        q.single(app.world()).unwrap().physical_cursor_position()
    }

    #[test]
    fn hover_sets_logical_message_and_physical_window_position() {
        let (mut app, tx) = app();
        tx.send(Raw::Hover { x: 100.0, y: 50.0 }).unwrap();
        app.update();
        let c = read::<CursorMoved>(&app);
        assert_eq!(c[0].position, Vec2::new(100.0, 50.0));
        assert_eq!(cursor_of(&mut app), Some(Vec2::new(200.0, 100.0)));
        tx.send(Raw::Hover { x: 110.0, y: 50.0 }).unwrap();
        app.update();
        assert_eq!(
            read::<CursorMoved>(&app)[0].delta,
            Some(Vec2::new(10.0, 0.0))
        );
    }

    #[test]
    fn move_fallback_also_sets_the_window_position() {
        let (mut app, tx) = app();
        tx.send(Raw::Move { dx: 10.0, dy: 5.0 }).unwrap();
        app.update();
        assert_eq!(cursor_of(&mut app), Some(Vec2::new(820.0, 590.0)));
    }

    #[test]
    fn after_hover_moves_only_write_motion() {
        let (mut app, tx) = app();
        tx.send(Raw::Hover { x: 100.0, y: 50.0 }).unwrap();
        tx.send(Raw::Move { dx: 30.0, dy: 0.0 }).unwrap();
        app.update();
        assert_eq!(read::<CursorMoved>(&app).len(), 1);
        assert_eq!(read::<MouseMotion>(&app).len(), 1);
        assert_eq!(cursor_of(&mut app), Some(Vec2::new(200.0, 100.0)));
    }

    #[test]
    fn locked_hover_keeps_the_position_and_hover_end_clears_it() {
        let (mut app, tx) = app();
        tx.send(Raw::Hover { x: 100.0, y: 50.0 }).unwrap();
        app.update();
        app.world_mut()
            .resource_mut::<PointerLock>()
            .set_locked(true);
        tx.send(Raw::Hover { x: 300.0, y: 300.0 }).unwrap();
        app.update();
        assert!(read::<CursorMoved>(&app).is_empty());
        assert_eq!(cursor_of(&mut app), Some(Vec2::new(200.0, 100.0)));
        app.world_mut()
            .resource_mut::<PointerLock>()
            .set_locked(false);
        tx.send(Raw::HoverEnd).unwrap();
        app.update();
        assert_eq!(cursor_of(&mut app), None);
        assert_eq!(read::<CursorLeft>(&app).len(), 1);
    }

    fn touch(app: &mut App, phase: TouchPhase, x: f32, y: f32) {
        let window = {
            let mut q = app
                .world_mut()
                .query_filtered::<Entity, With<PrimaryWindow>>();
            q.single(app.world()).unwrap()
        };
        app.world_mut().write_message(TouchInput {
            phase,
            position: Vec2::new(x, y),
            window,
            force: None,
            id: 0,
        });
    }

    #[test]
    fn touch_moves_the_cursor_only_while_a_button_is_held() {
        let (mut app, tx) = app();
        touch(&mut app, TouchPhase::Moved, 300.0, 200.0);
        app.update();
        assert!(read::<CursorMoved>(&app).is_empty());
        assert_eq!(cursor_of(&mut app), None);

        tx.send(Raw::Button {
            button: MouseButton::Left,
            pressed: true,
        })
        .unwrap();
        touch(&mut app, TouchPhase::Moved, 300.0, 200.0);
        app.update();
        assert_eq!(
            read::<CursorMoved>(&app)[0].position,
            Vec2::new(300.0, 200.0)
        );
        assert_eq!(cursor_of(&mut app), Some(Vec2::new(600.0, 400.0)));
    }

    #[test]
    fn hover_end_is_ignored_while_a_button_is_held() {
        let (mut app, tx) = app();
        tx.send(Raw::Hover { x: 100.0, y: 50.0 }).unwrap();
        tx.send(Raw::Button {
            button: MouseButton::Left,
            pressed: true,
        })
        .unwrap();
        tx.send(Raw::HoverEnd).unwrap();
        app.update();
        assert_eq!(cursor_of(&mut app), Some(Vec2::new(200.0, 100.0)));
        assert!(read::<CursorLeft>(&app).is_empty());
        tx.send(Raw::Button {
            button: MouseButton::Left,
            pressed: false,
        })
        .unwrap();
        tx.send(Raw::HoverEnd).unwrap();
        app.update();
        assert_eq!(cursor_of(&mut app), None);
        assert_eq!(read::<CursorLeft>(&app).len(), 1);
    }

    #[test]
    fn trace_arms_on_left_press_and_stops_at_the_cap() {
        let (mut app, tx) = app();
        for i in 0..20 {
            tx.send(Raw::Hover {
                x: i as f32,
                y: 0.0,
            })
            .unwrap();
        }
        app.update();
        let st = app.world().resource::<InputState>();
        assert_eq!(st.trace_remaining, 0);
        assert!(!st.trace_armed);
        assert_eq!(st.trace_ring.len(), TRACE_RING);

        tx.send(Raw::Button {
            button: MouseButton::Left,
            pressed: true,
        })
        .unwrap();
        app.update();
        let st = app.world().resource::<InputState>();
        assert!(st.trace_armed);
        assert!(st.trace_ring.is_empty());
        assert_eq!(st.trace_remaining, TRACE_EVENTS - 1);

        for _ in 0..TRACE_EVENTS + 50 {
            tx.send(Raw::Move { dx: 1.0, dy: 0.0 }).unwrap();
        }
        app.update();
        assert_eq!(app.world().resource::<InputState>().trace_remaining, 0);

        // Armed once per process: a second press does not re-arm.
        tx.send(Raw::Button {
            button: MouseButton::Left,
            pressed: false,
        })
        .unwrap();
        tx.send(Raw::Button {
            button: MouseButton::Left,
            pressed: true,
        })
        .unwrap();
        app.update();
        assert_eq!(app.world().resource::<InputState>().trace_remaining, 0);
    }

    /// The shim in front of Bevy's own input systems, as the plugin orders them.
    fn app_with_input() -> (App, Sender<Raw>) {
        let (mut app, tx) = app();
        app.add_plugins(bevy::input::InputPlugin);
        app.world_mut().remove_resource::<Messages<TouchInput>>();
        app.add_message::<TouchInput>();
        app.add_systems(PreUpdate, drain.before(InputSystems));
        (app, tx)
    }

    #[test]
    fn a_press_and_a_release_in_one_drain_are_both_seen_by_the_button_plane() {
        let (mut app, tx) = app_with_input();
        tx.send(Raw::Hover { x: 120.0, y: 500.0 }).unwrap();
        for pressed in [true, false] {
            tx.send(Raw::Button {
                button: MouseButton::Left,
                pressed,
            })
            .unwrap();
        }
        app.update();
        let b = app.world().resource::<ButtonInput<MouseButton>>();
        assert!(b.just_pressed(MouseButton::Left) && b.just_released(MouseButton::Left));
        assert!(!b.pressed(MouseButton::Left));
        // The pointer stayed where the hover put it, in logical points.
        let mut q = app.world_mut().query::<&Window>();
        let w = q.single(app.world()).unwrap();
        assert_eq!(w.cursor_position(), Some(Vec2::new(120.0, 500.0)));
    }

    #[test]
    fn a_release_without_a_press_and_a_second_press_make_no_edge() {
        let (mut app, tx) = app_with_input();
        tx.send(Raw::Button {
            button: MouseButton::Left,
            pressed: false,
        })
        .unwrap();
        app.update();
        assert!(read::<MouseButtonInput>(&app).is_empty());
        for _ in 0..2 {
            tx.send(Raw::Button {
                button: MouseButton::Left,
                pressed: true,
            })
            .unwrap();
        }
        app.update();
        assert_eq!(read::<MouseButtonInput>(&app).len(), 1);
    }

    #[test]
    fn a_movement_key_reaches_the_key_plane_and_the_notes_name_the_events() {
        let (mut app, tx) = app_with_input();
        enable_notes();
        let _ = take_notes();
        tx.send(Raw::Key {
            hid: 0x1A,
            pressed: true,
        })
        .unwrap();
        tx.send(Raw::Button {
            button: MouseButton::Left,
            pressed: true,
        })
        .unwrap();
        app.update();
        assert!(app
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW));
        let notes = take_notes();
        let kinds: Vec<_> = notes.iter().map(|(k, _)| *k).collect();
        assert!(
            kinds.contains(&"key") && kinds.contains(&"button"),
            "{kinds:?}"
        );
        assert!(kinds.contains(&"first"), "{kinds:?}");
        NOTES_ON.store(false, Ordering::Relaxed);
    }

    #[test]
    fn reset_releases_what_is_held() {
        let (mut app, tx) = app();
        tx.send(Raw::Key {
            hid: 0x1A,
            pressed: true,
        })
        .unwrap();
        tx.send(Raw::Button {
            button: MouseButton::Left,
            pressed: true,
        })
        .unwrap();
        tx.send(Raw::Reset).unwrap();
        app.update();
        let k = read::<KeyboardInput>(&app);
        assert_eq!(k.last().unwrap().state, ButtonState::Released);
        assert_eq!(
            read::<MouseButtonInput>(&app).last().unwrap().state,
            ButtonState::Released
        );
    }
}
