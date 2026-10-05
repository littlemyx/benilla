//! The iOS side: GameController handlers feeding the channel, the pointer position from a
//! `UIHoverGestureRecognizer` on winit's view, the pointer-lock hook on winit's root view
//! controller, and the pasteboard.
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
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, NSObject, ProtocolObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol, NSString};
use objc2_game_controller::{
    GCControllerAxisInput, GCControllerButtonInput, GCKeyCode, GCKeyboard,
    GCKeyboardDidConnectNotification, GCKeyboardDidDisconnectNotification, GCKeyboardInput,
    GCMouse, GCMouseDidConnectNotification, GCMouseDidDisconnectNotification, GCMouseInput,
};
use objc2_ui_kit::{
    UIGestureRecognizerState, UIHoverGestureRecognizer, UIPasteboard, UIPointerInteraction,
    UIPointerInteractionDelegate, UIPointerRegion, UIPointerStyle, UIView, UIViewController,
};
use raw_window_handle::RawWindowHandle;

use crate::{Raw, PREFERS_LOCKED};

/// What must stay alive: the observer tokens and the coalesced keyboard.
pub struct Native {
    tx: Sender<Raw>,
    tokens: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    keyboard: Option<Retained<GCKeyboard>>,
    /// The root view controller `prefersPointerLocked` was installed on.
    root_vc: Option<Retained<UIViewController>>,
    /// The hover recognizer on winit's view and its target (UIKit holds the target unretained).
    hover: Option<(Retained<UIHoverGestureRecognizer>, Retained<HoverTarget>)>,
    /// The pointer interaction on winit's view that hides the system pointer, and its delegate
    /// (UIKit holds the delegate unretained).
    pointer: Option<(Retained<UIPointerInteraction>, Retained<PointerHider>)>,
}

impl Native {
    pub fn new(tx: Sender<Raw>) -> Self {
        Self {
            tx,
            tokens: Vec::new(),
            keyboard: None,
            root_vc: None,
            hover: None,
            pointer: None,
        }
    }
}

/// Registers the connect and disconnect observers and attaches to what is already connected.
pub fn attach(mut native: NonSendMut<Native>) {
    let tx = native.tx.clone();
    native.keyboard = attach_keyboard(&tx);
    info!(
        "ios input: GCKeyboard {}",
        if native.keyboard.is_some() {
            "found"
        } else {
            "not connected yet"
        }
    );
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
            let found = attach_keyboard(&t).is_some();
            info!("ios input: GCKeyboard connected, attached: {found}");
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
        Box::new(move || {
            info!("ios input: GCMouse connected");
            attach_mice(&t);
        }),
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
    info!("ios input: GCMouse::mice().count() = {}", mice.count());
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
        info!("ios input: a GCMouse has no mouseInput; skipped");
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
        } else {
            info!("ios input: GCMouseInput.rightButton is None on this mouse");
        }
        if let Some(b) = input.middleButton() {
            b.setPressedChangedHandler(RcBlock::as_ptr(&button_handler(tx, MouseButton::Middle)));
        } else {
            info!("ios input: GCMouseInput.middleButton is None on this mouse");
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

define_class!(
    /// The Objective-C target of the hover recognizer's action; main thread only, like the view.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BenillaHoverTarget"]
    #[ivars = Sender<Raw>]
    struct HoverTarget;

    impl HoverTarget {
        /// `-(void)hover:(UIHoverGestureRecognizer *)r`: the indirect pointer's position in the
        /// recognizer's view, in points with the origin top-left and y down, as drawn on screen.
        #[unsafe(method(hover:))]
        fn hover(&self, r: &UIHoverGestureRecognizer) {
            // `state` is a property UIKit declares on UIGestureRecognizer; objc2-ui-kit generates
            // only its subclass setter, so the getter is sent by hand.
            // SAFETY: `state` returns `UIGestureRecognizerState` (an NSInteger enum).
            let state: UIGestureRecognizerState = unsafe { msg_send![r, state] };
            let tx = self.ivars();
            if state == UIGestureRecognizerState::Ended
                || state == UIGestureRecognizerState::Cancelled
            {
                let _ = tx.send(Raw::HoverEnd);
                return;
            }
            let view = r.view();
            let p = r.locationInView(view.as_deref());
            let _ = tx.send(Raw::Hover {
                x: p.x as f32,
                y: p.y as f32,
            });
        }
    }
);

impl HoverTarget {
    fn new(mtm: MainThreadMarker, tx: Sender<Raw>) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(tx);
        // SAFETY: NSObject's designated initialiser.
        unsafe { msg_send![super(this), init] }
    }
}

/// Attaches a `UIHoverGestureRecognizer` to winit's `UIView` once the window exists; retried each
/// frame until it does. Hover fires for the indirect pointer (trackpad, mouse) at the position
/// iPadOS draws it, acceleration included. Main thread only (NonSend).
pub fn attach_hover(
    mut native: NonSendMut<Native>,
    window: Query<&RawHandleWrapper, With<PrimaryWindow>>,
) {
    if native.hover.is_some() {
        return;
    }
    let Ok(handle) = window.single() else { return };
    let RawWindowHandle::UiKit(h) = handle.get_window_handle() else {
        return;
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: winit's `ui_view` is a live UIView owned by the window; NonSend, so main thread.
    let view: &UIView = unsafe { h.ui_view.cast::<UIView>().as_ref() };
    let target = HoverTarget::new(mtm, native.tx.clone());
    // SAFETY: `target` implements `hover:` taking the recognizer, matching the selector.
    let recognizer = unsafe {
        UIHoverGestureRecognizer::initWithTarget_action(
            mtm.alloc::<UIHoverGestureRecognizer>(),
            Some(&target),
            Some(sel!(hover:)),
        )
    };
    view.addGestureRecognizer(&recognizer);
    native.hover = Some((recognizer, target));
    info!("ios input: hover recognizer attached to the view");
}

define_class!(
    /// The delegate of the view's pointer interaction: every region is styled hidden.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BenillaPointerHider"]
    struct PointerHider;

    unsafe impl NSObjectProtocol for PointerHider {}

    // Conformance only: the one optional method is below, because `define_class!` cannot yet
    // return a `Retained` from a protocol method.
    unsafe impl UIPointerInteractionDelegate for PointerHider {}

    impl PointerHider {
        /// `-pointerInteraction:styleForRegion:`: no system pointer over the view. The returned
        /// style is autoreleased, as an Objective-C getter's result is.
        #[unsafe(method(pointerInteraction:styleForRegion:))]
        fn style_for_region(
            &self,
            _interaction: &UIPointerInteraction,
            _region: &UIPointerRegion,
        ) -> *mut UIPointerStyle {
            Retained::autorelease_return(UIPointerStyle::hiddenPointerStyle(self.mtm()))
        }
    }
);

impl PointerHider {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(());
        // SAFETY: NSObject's designated initialiser.
        unsafe { msg_send![super(this), init] }
    }
}

/// Hides the system pointer over winit's view with a `UIPointerInteraction` whose style is
/// hidden, once the window exists; retried each frame until it does. The game draws its own
/// cursor (`benilla-app`'s `cursor.rs`), as the 1.12 client does. A hidden pointer style only
/// suppresses the drawing: the pointer still exists, so `UIHoverGestureRecognizer` keeps
/// delivering hover (documented UIKit behaviour; unverified on device). Main thread only
/// (NonSend).
pub fn attach_pointer_hider(
    mut native: NonSendMut<Native>,
    window: Query<&RawHandleWrapper, With<PrimaryWindow>>,
) {
    if native.pointer.is_some() {
        return;
    }
    let Ok(handle) = window.single() else { return };
    let RawWindowHandle::UiKit(h) = handle.get_window_handle() else {
        return;
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: winit's `ui_view` is a live UIView owned by the window; NonSend, so main thread.
    let view: &UIView = unsafe { h.ui_view.cast::<UIView>().as_ref() };
    let delegate = PointerHider::new(mtm);
    let interaction = UIPointerInteraction::initWithDelegate(
        mtm.alloc::<UIPointerInteraction>(),
        Some(ProtocolObject::from_ref(&*delegate)),
    );
    view.addInteraction(ProtocolObject::from_ref(&*interaction));
    native.pointer = Some((interaction, delegate));
    info!("ios input: system pointer hidden over the view");
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
            let added = objc2::ffi::class_addMethod(
                cls,
                sel!(prefersPointerLocked),
                imp,
                c"B@:".as_ptr().cast::<c_char>(),
            );
            info!(
                "ios input: prefersPointerLocked injected: {}",
                added.as_bool()
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
