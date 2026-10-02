//! The iPad's hardware keyboard and mouse as Bevy input messages.
//!
//! winit 0.30 on iOS delivers no key codes (only `insertText` as an `Unidentified` key with
//! text) and no mouse events, and refuses `set_cursor_grab`. This plugin reads Apple's
//! GameController (`GCKeyboard`, `GCMouse`) instead and writes the messages the game reads:
//! `KeyboardInput` (physical code, US-layout logical key, text on press), `MouseMotion`,
//! `MouseButtonInput`, `MouseWheel` and a software `CursorMoved`.
//!
//! The software cursor integrates the mouse deltas from the window centre and clamps them to the
//! window. While [`PointerLock`] is locked (a mouse-look session) no `CursorMoved` is written, so
//! UI hover freezes where it was; `MouseMotion` flows either way. Off iOS the plugin does nothing
//! and the crate only builds, so the translation below is tested on the host.
//!
//! The pointer is locked by answering `prefersPointerLocked` on winit's root view controller,
//! which winit does not implement; see [`PointerLock::set_locked`].

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Mutex;

use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::{ButtonState, InputSystems};
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow};

#[cfg(target_os = "ios")]
mod audio;
pub mod keycodes;
#[cfg(target_os = "ios")]
pub use audio::activate_playback_session;
#[cfg(target_os = "ios")]
mod native;
#[cfg(target_os = "ios")]
pub use native::{ui_pasteboard_read, ui_pasteboard_write};

pub use keycodes::{key_code, logical_text};

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
                    (native::sync_pointer_lock, drain)
                        .chain()
                        .before(InputSystems),
                );
        }
        #[cfg(not(target_os = "ios"))]
        let _ = (app, InputSystems);
    }
}

/// Turns the queued device events into Bevy messages for the primary window.
pub fn drain(
    inbox: Res<RawInbox>,
    mut state: ResMut<InputState>,
    lock: Res<PointerLock>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut keys: MessageWriter<KeyboardInput>,
    mut motion: MessageWriter<MouseMotion>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut wheel: MessageWriter<MouseWheel>,
    mut cursor: MessageWriter<CursorMoved>,
) {
    let Ok((window, win)) = windows.single() else {
        return;
    };
    let size = Vec2::new(win.width(), win.height());
    let rx = inbox.0.lock().unwrap_or_else(|e| e.into_inner());
    let state = &mut *state;
    while let Ok(raw) = rx.try_recv() {
        match raw {
            Raw::Key { hid, pressed } => {
                let Some(key_code) = key_code(hid) else {
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
                if !lock.is_locked() {
                    let pos = (state.cursor.unwrap_or(size / 2.0) + delta).clamp(Vec2::ZERO, size);
                    state.cursor = Some(pos);
                    cursor.write(CursorMoved {
                        window,
                        position: pos,
                        delta: Some(delta),
                    });
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
            .insert_resource(RawInbox(Mutex::new(rx)))
            .init_resource::<InputState>()
            .init_resource::<PointerLock>()
            .add_systems(Update, drain);
        let mut window = Window::default();
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
