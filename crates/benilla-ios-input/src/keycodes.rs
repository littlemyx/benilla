//! USB HID keyboard/keypad usages (page 0x07) to Bevy's physical `KeyCode` and a US-layout
//! logical key. Apple's `GCKeyCode` values are these usage IDs (`GCKeyCodeKeyA` = 0x04), so the
//! table is keyed by the number and each arm names the `GCKeyCode` constant. Pure Rust: it builds
//! and tests on every platform.

use bevy::input::keyboard::{Key, KeyCode, NativeKey};
use smol_str::SmolStr;

/// The physical key for a HID usage ID, or `None` for one benilla does not know.
pub fn key_code(hid: u16) -> Option<KeyCode> {
    use KeyCode::*;
    Some(match hid {
        // GCKeyCodeKeyA ..= KeyZ
        0x04 => KeyA,
        0x05 => KeyB,
        0x06 => KeyC,
        0x07 => KeyD,
        0x08 => KeyE,
        0x09 => KeyF,
        0x0A => KeyG,
        0x0B => KeyH,
        0x0C => KeyI,
        0x0D => KeyJ,
        0x0E => KeyK,
        0x0F => KeyL,
        0x10 => KeyM,
        0x11 => KeyN,
        0x12 => KeyO,
        0x13 => KeyP,
        0x14 => KeyQ,
        0x15 => KeyR,
        0x16 => KeyS,
        0x17 => KeyT,
        0x18 => KeyU,
        0x19 => KeyV,
        0x1A => KeyW,
        0x1B => KeyX,
        0x1C => KeyY,
        0x1D => KeyZ,
        // GCKeyCodeOne ..= Nine, Zero
        0x1E => Digit1,
        0x1F => Digit2,
        0x20 => Digit3,
        0x21 => Digit4,
        0x22 => Digit5,
        0x23 => Digit6,
        0x24 => Digit7,
        0x25 => Digit8,
        0x26 => Digit9,
        0x27 => Digit0,
        0x28 => Enter,        // ReturnOrEnter
        0x29 => Escape,       // Escape
        0x2A => Backspace,    // DeleteOrBackspace
        0x2B => Tab,          // Tab
        0x2C => Space,        // Spacebar
        0x2D => Minus,        // Hyphen
        0x2E => Equal,        // EqualSign
        0x2F => BracketLeft,  // OpenBracket
        0x30 => BracketRight, // CloseBracket
        0x31 => Backslash,    // Backslash
        0x33 => Semicolon,    // Semicolon
        0x34 => Quote,        // Quote
        0x35 => Backquote,    // GraveAccentAndTilde
        0x36 => Comma,        // Comma
        0x37 => Period,       // Period
        0x38 => Slash,        // Slash
        0x39 => CapsLock,     // CapsLock
        // GCKeyCodeF1 ..= F12
        0x3A => F1,
        0x3B => F2,
        0x3C => F3,
        0x3D => F4,
        0x3E => F5,
        0x3F => F6,
        0x40 => F7,
        0x41 => F8,
        0x42 => F9,
        0x43 => F10,
        0x44 => F11,
        0x45 => F12,
        0x46 => PrintScreen, // PrintScreen
        0x47 => ScrollLock,  // ScrollLock
        0x48 => Pause,       // Pause
        0x49 => Insert,      // Insert
        0x4A => Home,        // Home
        0x4B => PageUp,      // PageUp
        0x4C => Delete,      // DeleteForward
        0x4D => End,         // End
        0x4E => PageDown,    // PageDown
        0x4F => ArrowRight,  // RightArrow
        0x50 => ArrowLeft,   // LeftArrow
        0x51 => ArrowDown,   // DownArrow
        0x52 => ArrowUp,     // UpArrow
        // Keypad
        0x53 => NumLock,        // KeypadNumLock
        0x54 => NumpadDivide,   // KeypadSlash
        0x55 => NumpadMultiply, // KeypadAsterisk
        0x56 => NumpadSubtract, // KeypadHyphen
        0x57 => NumpadAdd,      // KeypadPlus
        0x58 => NumpadEnter,    // KeypadEnter
        0x59 => Numpad1,        // Keypad1 ..= Keypad9
        0x5A => Numpad2,
        0x5B => Numpad3,
        0x5C => Numpad4,
        0x5D => Numpad5,
        0x5E => Numpad6,
        0x5F => Numpad7,
        0x60 => Numpad8,
        0x61 => Numpad9,
        0x62 => Numpad0,       // Keypad0
        0x63 => NumpadDecimal, // KeypadPeriod
        0x64 => IntlBackslash, // NonUSBackslash
        0x65 => ContextMenu,   // Application
        0x66 => Power,         // Power
        0x67 => NumpadEqual,   // KeypadEqualSign
        // GCKeyCodeF13 ..= F24 (the framework names up to F20)
        0x68 => F13,
        0x69 => F14,
        0x6A => F15,
        0x6B => F16,
        0x6C => F17,
        0x6D => F18,
        0x6E => F19,
        0x6F => F20,
        0x70 => F21,
        0x71 => F22,
        0x72 => F23,
        0x73 => F24,
        0x85 => NumpadComma, // International-keypad comma (no GCKeyCode constant)
        0x87 => IntlRo,      // International1
        0x88 => KanaMode,    // International2
        0x89 => IntlYen,     // International3
        0x8A => Convert,     // International4
        0x8B => NonConvert,  // International5
        0x90 => Lang1,       // LANG1
        0x91 => Lang2,       // LANG2
        0x92 => Lang3,       // LANG3
        0x93 => Lang4,       // LANG4
        0x94 => Lang5,       // LANG5
        0xE0 => ControlLeft, // LeftControl
        0xE1 => ShiftLeft,   // LeftShift
        0xE2 => AltLeft,     // LeftAlt
        0xE3 => SuperLeft,   // LeftGUI
        0xE4 => ControlRight, // RightControl
        0xE5 => ShiftRight,  // RightShift
        0xE6 => AltRight,    // RightAlt
        0xE7 => SuperRight,  // RightGUI
        _ => return None,
    })
}

/// The US-layout logical key and the text a press of `hid` produces, or `None` for an unknown
/// usage. Printable keys carry text (shift applied; keypad digits and operators always, as a
/// keypad with NumLock on). Text for the control keys matches winit's macOS backend
/// (`NamedKey::to_text`, which its `create_key_event` copies into `KeyEvent.text` on a press):
/// Enter and keypad Enter `"\r"`, Tab `"\t"`, Backspace `"\x08"`, Escape `"\x1b"`; every other
/// non-printable key has no text.
pub fn logical_text(hid: u16, shift: bool) -> Option<(Key, Option<SmolStr>)> {
    let code = key_code(hid)?;
    if let Some(c) = printable(hid, shift) {
        let s = SmolStr::new_inline(c.encode_utf8(&mut [0; 4]));
        let key = if c == ' ' {
            Key::Space
        } else {
            Key::Character(s.clone())
        };
        return Some((key, Some(s)));
    }
    let text = |s: &str| Some(SmolStr::new(s));
    Some(match code {
        KeyCode::Enter | KeyCode::NumpadEnter => (Key::Enter, text("\r")),
        KeyCode::Tab => (Key::Tab, text("\t")),
        KeyCode::Backspace => (Key::Backspace, text("\x08")),
        KeyCode::Escape => (Key::Escape, text("\x1b")),
        other => (named(other), None),
    })
}

/// The character a printable key types on a US layout.
fn printable(hid: u16, shift: bool) -> Option<char> {
    let pick = |plain: char, shifted: char| Some(if shift { shifted } else { plain });
    match hid {
        0x04..=0x1D => {
            let c = (b'a' + (hid - 0x04) as u8) as char;
            Some(if shift { c.to_ascii_uppercase() } else { c })
        }
        0x1E..=0x26 => pick(
            (b'1' + (hid - 0x1E) as u8) as char,
            b"!@#$%^&*("[(hid - 0x1E) as usize] as char,
        ),
        0x27 => pick('0', ')'),
        0x2C => Some(' '),
        0x2D => pick('-', '_'),
        0x2E => pick('=', '+'),
        0x2F => pick('[', '{'),
        0x30 => pick(']', '}'),
        0x31 => pick('\\', '|'),
        0x33 => pick(';', ':'),
        0x34 => pick('\'', '"'),
        0x35 => pick('`', '~'),
        0x36 => pick(',', '<'),
        0x37 => pick('.', '>'),
        0x38 => pick('/', '?'),
        0x54 => Some('/'),
        0x55 => Some('*'),
        0x56 => Some('-'),
        0x57 => Some('+'),
        0x59..=0x61 => Some((b'1' + (hid - 0x59) as u8) as char),
        0x62 => Some('0'),
        0x63 => Some('.'),
        0x64 => pick('\\', '|'),
        0x67 => Some('='),
        0x85 => Some(','),
        _ => None,
    }
}

/// The logical key of a non-printable physical key.
fn named(code: KeyCode) -> Key {
    use KeyCode::*;
    match code {
        CapsLock => Key::CapsLock,
        PrintScreen => Key::PrintScreen,
        ScrollLock => Key::ScrollLock,
        Pause => Key::Pause,
        Insert => Key::Insert,
        Home => Key::Home,
        PageUp => Key::PageUp,
        Delete => Key::Delete,
        End => Key::End,
        PageDown => Key::PageDown,
        ArrowRight => Key::ArrowRight,
        ArrowLeft => Key::ArrowLeft,
        ArrowDown => Key::ArrowDown,
        ArrowUp => Key::ArrowUp,
        NumLock => Key::NumLock,
        ContextMenu => Key::ContextMenu,
        Power => Key::Power,
        F1 => Key::F1,
        F2 => Key::F2,
        F3 => Key::F3,
        F4 => Key::F4,
        F5 => Key::F5,
        F6 => Key::F6,
        F7 => Key::F7,
        F8 => Key::F8,
        F9 => Key::F9,
        F10 => Key::F10,
        F11 => Key::F11,
        F12 => Key::F12,
        F13 => Key::F13,
        F14 => Key::F14,
        F15 => Key::F15,
        F16 => Key::F16,
        F17 => Key::F17,
        F18 => Key::F18,
        F19 => Key::F19,
        F20 => Key::F20,
        F21 => Key::F21,
        F22 => Key::F22,
        F23 => Key::F23,
        F24 => Key::F24,
        ControlLeft | ControlRight => Key::Control,
        ShiftLeft | ShiftRight => Key::Shift,
        AltLeft | AltRight => Key::Alt,
        SuperLeft | SuperRight => Key::Super,
        KanaMode => Key::KanaMode,
        Convert => Key::Convert,
        NonConvert => Key::NonConvert,
        _ => Key::Unidentified(NativeKey::Unidentified),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn spot_checks() {
        assert_eq!(key_code(0x04), Some(KeyCode::KeyA));
        assert_eq!(key_code(0x2C), Some(KeyCode::Space));
        assert_eq!(key_code(0xE1), Some(KeyCode::ShiftLeft));
        assert_eq!(key_code(0x3A), Some(KeyCode::F1));
        assert_eq!(key_code(0x58), Some(KeyCode::NumpadEnter));
        assert_eq!(key_code(0x64), Some(KeyCode::IntlBackslash));
        assert_eq!(key_code(0x27), Some(KeyCode::Digit0));
        assert_eq!(key_code(0x52), Some(KeyCode::ArrowUp));
        assert_eq!(key_code(0x03), None);
        assert_eq!(key_code(0xFFFF), None);
    }

    #[test]
    fn every_mapped_id_is_a_distinct_code() {
        let mut seen = HashSet::new();
        let mut n = 0;
        for hid in 0..=0xFFFF_u16 {
            if let Some(c) = key_code(hid) {
                n += 1;
                assert!(seen.insert(c), "{hid:#x} repeats {c:?}");
            }
        }
        assert_eq!(
            n, 130,
            "the table's size changed; update this with the table"
        );
    }

    #[test]
    fn text() {
        let t = |hid, shift| logical_text(hid, shift).map(|(_, t)| t.map(|s| s.to_string()));
        assert_eq!(t(0x04, false), Some(Some("a".into())));
        assert_eq!(t(0x04, true), Some(Some("A".into())));
        assert_eq!(t(0x1E, true), Some(Some("!".into())));
        assert_eq!(t(0x1E, false), Some(Some("1".into())));
        assert_eq!(t(0x26, true), Some(Some("(".into())));
        assert_eq!(t(0x27, true), Some(Some(")".into())));
        assert_eq!(t(0x2C, false), Some(Some(" ".into())));
        assert_eq!(t(0x5F, false), Some(Some("7".into())));
        assert_eq!(t(0x28, false), Some(Some("\r".into())));
        assert_eq!(t(0x2A, false), Some(Some("\x08".into())));
        assert_eq!(t(0x3A, false), Some(None));
        assert_eq!(t(0xE1, false), Some(None));
        assert_eq!(t(0x03, false), None);
        assert_eq!(logical_text(0x2C, false).unwrap().0, Key::Space);
        assert_eq!(logical_text(0x28, false).unwrap().0, Key::Enter);
        assert_eq!(logical_text(0x52, false).unwrap().0, Key::ArrowUp);
    }
}
