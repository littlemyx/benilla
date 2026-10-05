//! The Lua a script VM speaks: 1.12.1's Lua 5.0 grammar and library, or 2.4.3's Lua 5.1.
//!
//! One VM binary serves both. The vendored Lua carries a per-state flag (`benilla_setdialect`,
//! `third_party/lua-src/BENILLA.md`) that defaults to the 1.12.1 dialect; [`UiScript::new`] keeps
//! that default and [`UiScript::with_dialect`] picks the other, so no existing caller changes.
//!
//! [`UiScript::new`]: super::UiScript::new
//! [`UiScript::with_dialect`]: super::UiScript::with_dialect

use std::os::raw::c_int;

use mlua::Lua;

unsafe extern "C" {
    /// The vendored `lapi.c`'s setter: 0 is 1.12.1's Lua 5.0, 1 is stock Lua 5.1.
    fn benilla_setdialect(state: *mut mlua::ffi::lua_State, dialect: c_int);
}

/// Which client's Lua a [`super::UiScript`] runs: the grammar, the library and the handler calling
/// convention. Derived from the client build by the caller; the default is 1.12.1's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScriptDialect {
    /// 1.12.1: Lua 5.0 as the client embeds it, handlers called with no arguments.
    #[default]
    Lua50,
    /// 2.4.3: Lua 5.1.1 as the client embeds it, handlers called with `self` and their arguments.
    Lua51,
}

impl ScriptDialect {
    /// The code `benilla_setdialect` takes.
    fn code(self) -> c_int {
        match self {
            ScriptDialect::Lua50 => 0,
            ScriptDialect::Lua51 => 1,
        }
    }

    /// Switch the VM's compiler and runtime to this dialect. Chunks compiled before keep what they
    /// were compiled as, so [`super::UiScript::with_dialect`] calls it after its own Lua is in.
    pub(super) fn apply(self, lua: &Lua) -> mlua::Result<()> {
        // SAFETY: `state` is the live VM's own state, handed in by `exec_raw` under mlua's lock.
        unsafe { lua.exec_raw::<()>((), |state| benilla_setdialect(state, self.code())) }
    }

    /// The dialect the VM was created with; the 1.12.1 one for a bare `Lua`.
    pub(crate) fn of(lua: &Lua) -> ScriptDialect {
        lua.app_data_ref::<ScriptDialect>()
            .map(|d| *d)
            .unwrap_or_default()
    }

    /// The parameters after `self` of a handler, as 2.4.3 compiles an inline XML body
    /// (`return function(self,<params>) <body> end`); empty for a handler that takes `self` alone.
    ///
    /// Not 1.12.1's: there the body is the chunk itself and reads `this`, `event` and `arg1..`.
    pub(crate) fn handler_params(script: &str) -> &'static str {
        let is = |n: &str| script.eq_ignore_ascii_case(n);
        if is("OnEvent") {
            "event,..."
        } else if is("OnUpdate") {
            "elapsed"
        } else if is("OnEnter") || is("OnLeave") {
            "motion"
        } else if is("OnMouseDown") || is("OnMouseUp") || is("OnDoubleClick") {
            "button"
        } else if is("OnClick") || is("PreClick") || is("PostClick") {
            "button,down"
        } else if is("OnMouseWheel") {
            "delta"
        } else if is("OnKeyDown") || is("OnKeyUp") {
            "key"
        } else if is("OnChar") {
            "text"
        } else if is("OnSizeChanged") {
            "w,h"
        } else if is("OnAttributeChanged") {
            "name,value"
        } else if is("OnValueChanged") {
            "value"
        } else if is("OnInputLanguageChanged") {
            "language"
        } else if is("OnCursorChanged") {
            "x,y,w,h"
        } else if is("OnScrollRangeChanged") {
            "xrange,yrange"
        } else if is("OnVerticalScroll") || is("OnHorizontalScroll") {
            "offset"
        } else if is("OnColorSelect") {
            "r,g,b"
        } else if is("OnHyperlinkClick") {
            "link,text,button"
        } else if is("OnHyperlinkEnter") || is("OnHyperlinkLeave") {
            "link,text"
        } else if is("OnTooltipAddMoney") {
            "cost"
        } else {
            ""
        }
    }
}
