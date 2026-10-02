//! The iOS side: GameController handlers feeding the channel, the pointer-lock hook on winit's
//! root view controller, and the pasteboard.
//!
//! Every UIKit and GameController call here runs on the main thread: the systems take `NonSend`
//! resources (which bevy runs on the main thread) because Objective-C objects are not `Send`.
//! The handler blocks themselves may run on GameController's queue, so they only send on the
//! channel.

use std::ffi::c_char;
use std::ptr::NonNull;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, RawHandleWrapper};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, ProtocolObject, Sel};
use objc2::sel;
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol, NSString};
use objc2_game_controller::{
    GCControllerAxisInput, GCControllerButtonInput, GCKeyCode, GCKeyboard,
    GCKeyboardDidConnectNotification, GCKeyboardDidDisconnectNotification, GCKeyboardInput,
    GCMouse, GCMouseDidConnectNotification, GCMouseDidDisconnectNotification, GCMouseInput,
};
use objc2_ui_kit::{UIPasteboard, UIViewController};
use raw_window_handle::RawWindowHandle;

use crate::{Raw, PREFERS_LOCKED};

/// What must stay alive: the observer tokens and the coalesced keyboard.
pub struct Native {
    tx: Sender<Raw>,
    tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    keyboard: Option<Retained<GCKeyboard>>,
    /// The root view controller `prefersPointerLocked` was installed on.
    root_vc: Option<Retained<UIViewController>>,
}

impl Native {
    pub fn new(tx: Sender<Raw>) -> Self {
        Self {
            tx,
            tokens: Vec::new(),
            keyboard: None,
            root_vc: None,
        }
    }
}

/// Registers the connect and disconnect observers and attaches to what is already connected.
pub fn attach(mut native: NonSendMut<Native>) {
    let tx = native.tx.clone();
    native.keyboard = attach_keyboard(&tx);
    attach_mice(&tx);

    let center = NSNotificationCenter::defaultCenter();
    let mut observe = |name: &NSString, f: Box<dyn Fn() + 'static>| {
        let block = RcBlock::new(move |_: NonNull<NSNotification>| f());
        // SAFETY: `name` is a GameController notification name; `queue` None runs the block on the
        // posting thread; the block only touches `Send` state or GameController objects, and the
        // returned token is kept for the process's life.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
        };
        native.tokens.push(token);
    };
    let t = tx.clone();
    // SAFETY (all four statics): extern NSString constants in GameController, valid for 'static.
    observe(
        unsafe { GCKeyboardDidConnectNotification },
        Box::new(move || {
            // The keyboard is coalesced: one object for all keyboards, so re-attaching is idempotent.
            // Its retained handle in `Native` is the same object.
            attach_keyboard(&t);
        }),
    );
    let t = tx.clone();
    observe(
        unsafe { GCKeyboardDidDisconnectNotification },
        Box::new(move || {
            let _ = t.send(Raw::Reset);
        }),
    );
    let t = tx.clone();
    observe(
        unsafe { GCMouseDidConnectNotification },
        Box::new(move || attach_mice(&t)),
    );
    let t = tx;
    observe(
        unsafe { GCMouseDidDisconnectNotification },
        Box::new(move || {
            let _ = t.send(Raw::Reset);
        }),
    );
}

fn attach_keyboard(tx: &Sender<Raw>) -> Option<Retained<GCKeyboard>> {
    // SAFETY: class method; None when no keyboard is attached.
    let keyboard = unsafe { GCKeyboard::coalescedKeyboard() }?;
    // SAFETY: plain getter on a live object.
    let input = unsafe { keyboard.keyboardInput() }?;
    let tx = tx.clone();
    let handler = RcBlock::new(
        move |_: NonNull<GCKeyboardInput>,
              _: NonNull<GCControllerButtonInput>,
              code: GCKeyCode,
              pressed: Bool| {
            if let Ok(hid) = u16::try_from(code) {
                let _ = tx.send(Raw::Key {
                    hid,
                    pressed: pressed.as_bool(),
                });
            }
        },
    );
    // SAFETY: the setter copies the block, so ours may drop; the handler only sends on a channel.
    unsafe { input.setKeyChangedHandler(RcBlock::as_ptr(&handler)) };
    Some(keyboard)
}

fn attach_mice(tx: &Sender<Raw>) {
    // SAFETY: class method returning the connected mice.
    let mice = unsafe { GCMouse::mice() };
    for mouse in mice.iter() {
        attach_mouse(&mouse, tx);
    }
}

fn button_handler(
    tx: &Sender<Raw>,
    button: MouseButton,
) -> RcBlock<dyn Fn(NonNull<GCControllerButtonInput>, f32, Bool)> {
    let tx = tx.clone();
    RcBlock::new(
        move |_: NonNull<GCControllerButtonInput>, _: f32, pressed: Bool| {
            let _ = tx.send(Raw::Button {
                button,
                pressed: pressed.as_bool(),
            });
        },
    )
}

fn attach_mouse(mouse: &GCMouse, tx: &Sender<Raw>) {
    // SAFETY: getter on a live GCMouse.
    let Some(input) = (unsafe { mouse.mouseInput() }) else {
        return;
    };
    let t = tx.clone();
    let moved = RcBlock::new(move |_: NonNull<GCMouseInput>, dx: f32, dy: f32| {
        let _ = t.send(Raw::Move { dx, dy });
    });
    // SAFETY: every setter below copies its block; the handlers only send on the channel; every
    // getter is called on a live GCMouseInput.
    unsafe {
        input.setMouseMovedHandler(RcBlock::as_ptr(&moved));
        input
            .leftButton()
            .setPressedChangedHandler(RcBlock::as_ptr(&button_handler(tx, MouseButton::Left)));
        if let Some(b) = input.rightButton() {
            b.setPressedChangedHandler(RcBlock::as_ptr(&button_handler(tx, MouseButton::Right)));
        }
        if let Some(b) = input.middleButton() {
            b.setPressedChangedHandler(RcBlock::as_ptr(&button_handler(tx, MouseButton::Middle)));
        }
        if let Some(aux) = input.auxiliaryButtons() {
            for (b, which) in aux.iter().zip([MouseButton::Back, MouseButton::Forward]) {
                b.setPressedChangedHandler(RcBlock::as_ptr(&button_handler(tx, which)));
            }
        }
        // Scroll: the pad reports per-event values; whether they are deltas or accumulated is
        // unverified on device, deltas assumed.
        let scroll = input.scroll();
        let t = tx.clone();
        let x = RcBlock::new(move |_: NonNull<GCControllerAxisInput>, v: f32| {
            let _ = t.send(Raw::Scroll { dx: v, dy: 0.0 });
        });
        scroll.xAxis().setValueChangedHandler(RcBlock::as_ptr(&x));
        let t = tx.clone();
        let y = RcBlock::new(move |_: NonNull<GCControllerAxisInput>, v: f32| {
            let _ = t.send(Raw::Scroll { dx: 0.0, dy: v });
        });
        scroll.yAxis().setValueChangedHandler(RcBlock::as_ptr(&y));
    }
}

/// `-(BOOL)prefersPointerLocked`, answering the flag [`crate::PointerLock`] sets.
extern "C" fn prefers_pointer_locked(_this: *mut AnyObject, _cmd: Sel) -> Bool {
    Bool::new(PREFERS_LOCKED.load(Ordering::Relaxed))
}

/// Installs `prefersPointerLocked` on the root view controller once it exists, and tells UIKit
/// to re-query it whenever the request changes. winit's `WinitUIViewController` does not
/// implement the selector, so adding it to that leaf class is a plain addition.
pub fn sync_pointer_lock(
    mut native: NonSendMut<Native>,
    mut lock: ResMut<crate::PointerLock>,
    window: Query<&RawHandleWrapper, With<PrimaryWindow>>,
) {
    if native.root_vc.is_none() {
        let Ok(handle) = window.single() else { return };
        let RawWindowHandle::UiKit(h) = handle.get_window_handle() else {
            return;
        };
        let Some(vc_ptr) = h.ui_view_controller else {
            return;
        };
        // SAFETY: winit's `ui_view_controller` is a live UIViewController owned by the window; this
        // system is NonSend, so on the main thread.
        let vc: &AnyObject = unsafe { vc_ptr.cast::<AnyObject>().as_ref() };
        let cls = vc.class() as *const AnyClass as *mut AnyClass;
        // SAFETY: `prefers_pointer_locked` matches the selector's `B@:` signature (BOOL return,
        // self, _cmd); transmuting it to the untyped `Imp` is how the runtime takes it.
        unsafe {
            let imp: Imp = std::mem::transmute::<extern "C" fn(*mut AnyObject, Sel) -> Bool, Imp>(
                prefers_pointer_locked,
            );
            objc2::ffi::class_addMethod(
                cls,
                sel!(prefersPointerLocked),
                imp,
                c"B@:".as_ptr().cast::<c_char>(),
            );
        }
        // SAFETY: as above; retaining the live object.
        native.root_vc = unsafe { Retained::retain(vc_ptr.as_ptr().cast::<UIViewController>()) };
    }
    if lock.told != lock.locked {
        lock.told = lock.locked;
        if let Some(vc) = &native.root_vc {
            vc.setNeedsUpdateOfPrefersPointerLocked();
        }
    }
}

/// The general pasteboard's string, if it holds one. Main thread only.
pub fn ui_pasteboard_read() -> Option<String> {
    // SAFETY: plain getters; the caller is on the main thread.
    unsafe { UIPasteboard::generalPasteboard().string() }.map(|s| s.to_string())
}

/// Puts `text` on the general pasteboard. Main thread only.
pub fn ui_pasteboard_write(text: &str) {
    // SAFETY: plain setter; the caller is on the main thread.
    unsafe { UIPasteboard::generalPasteboard().setString(Some(&NSString::from_str(text))) };
}
