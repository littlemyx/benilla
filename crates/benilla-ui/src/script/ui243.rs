//! What the 2.4.3 interface's widgets have that 1.12.1's lack, installed only for the 5.1 dialect
//! ([`super::ScriptDialect::Lua51`]), so a 1.12.1 VM's classes, method tables and handler kinds are
//! the ones its client has: the `Cooldown` class here.

use mlua::{Lua, Table, Value};

use super::object::frame_handle_of;
use super::Model;
use crate::widget::{CooldownState, KindState};

/// Registry key of the `Cooldown` method table.
pub(super) const REG_COOLDOWN_METHODS: &str = "__benilla_cooldown_methods";

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    if super::ScriptDialect::of(lua) != super::ScriptDialect::Lua51 {
        return Ok(());
    }
    install_cooldown(lua)
}

/// Run `f` over a frame's Cooldown state; errors unless `this` is a live Cooldown.
fn with_cooldown<T>(
    lua: &Lua,
    this: &Table,
    f: impl FnOnce(&mut CooldownState) -> T,
) -> mlua::Result<T> {
    let h = frame_handle_of(lua, this)?;
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    let frame = model
        .arena
        .frame_mut(h)
        .ok_or_else(|| mlua::Error::runtime("stale frame handle"))?;
    match &mut frame.kind_state {
        KindState::Cooldown(s) => Ok(f(s)),
        _ => Err(mlua::Error::runtime("not a Cooldown")),
    }
}

/// `drawEdge="true"` from the XML; the class has no Lua verb for it (the exe's method names for
/// the class are `SetCooldown` and `SetReverse`).
pub(crate) fn set_draw_edge(lua: &Lua, this: &Table, on: bool) -> mlua::Result<()> {
    with_cooldown(lua, this, |s| s.draw_edge = on)
}

/// `reverse="true"` from the XML, the same state `SetReverse` sets.
pub(crate) fn set_reverse(lua: &Lua, this: &Table, on: bool) -> mlua::Result<()> {
    with_cooldown(lua, this, |s| s.reverse = on)
}

fn install_cooldown(lua: &Lua) -> mlua::Result<()> {
    let m = lua.create_table()?;
    // The exe's own usage string, `"Usage: %s:SetCooldown(start, duration)"`, over the frame's name.
    m.set(
        "SetCooldown",
        lua.create_function(|lua, (this, start, duration): (Table, Value, Value)| {
            let number = |v: &Value| match v {
                Value::Number(n) => Some(*n),
                Value::Integer(i) => Some(*i as f64),
                Value::String(s) => s.to_str().ok().and_then(|s| s.trim().parse::<f64>().ok()),
                _ => None,
            };
            let (Some(start), Some(duration)) = (number(&start), number(&duration)) else {
                let h = frame_handle_of(lua, &this)?;
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                let who = model
                    .arena
                    .frame(h)
                    .and_then(|f| f.name.clone())
                    .unwrap_or_else(|| "<unnamed>".to_string());
                return Err(mlua::Error::runtime(format!(
                    "Usage: {who}:SetCooldown(start, duration)"
                )));
            };
            with_cooldown(lua, &this, |s| {
                s.start = start;
                s.duration = duration;
            })
        })?,
    )?;
    m.set(
        "SetReverse",
        lua.create_function(|lua, (this, on): (Table, Value)| {
            let on = !matches!(on, Value::Nil | Value::Boolean(false));
            set_reverse(lua, &this, on)
        })?,
    )?;
    lua.set_named_registry_value(REG_COOLDOWN_METHODS, m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::load;
    use crate::script::{ScriptDialect, UiScript};

    fn s51() -> UiScript {
        UiScript::with_dialect(ScriptDialect::Lua51).unwrap()
    }

    fn cooldown_of(s: &UiScript, name: &str) -> CooldownState {
        let wrapper: Table = s.lua().globals().get(name).unwrap();
        with_cooldown(s.lua(), &wrapper, |c| *c).unwrap()
    }

    #[test]
    fn a_cooldown_holds_what_it_is_given_and_answers_as_its_class() {
        let s = s51();
        s.run("cd = CreateFrame('Cooldown', 'CD1')").unwrap();
        assert_eq!(
            s.eval::<String>("return cd:GetObjectType()").unwrap(),
            "Cooldown"
        );
        assert_eq!(
            s.eval::<i64>("return cd:IsObjectType('Frame') and 1 or 0")
                .unwrap(),
            1
        );
        s.run("CD1:SetCooldown(12.5, 30) CD1:SetReverse(1)")
            .unwrap();
        let c = cooldown_of(&s, "CD1");
        assert_eq!(
            (c.start, c.duration, c.reverse, c.draw_edge),
            (12.5, 30.0, true, false)
        );
        s.run("CD1:SetReverse(nil)").unwrap();
        assert!(!cooldown_of(&s, "CD1").reverse);
        let e = s.run("CD1:SetCooldown(1)").unwrap_err().to_string();
        assert!(e.contains("Usage: CD1:SetCooldown(start, duration)"), "{e}");
    }

    #[test]
    fn the_xml_flags_reach_the_widget_and_the_methods_exist_on_a_cooldown_alone() {
        let s = s51();
        let doc = crate::framexml::parse(
            r#"<Ui><Cooldown name="CD2" reverse="true" drawEdge="true" hidden="true"/>
                <Frame name="F2"/></Ui>"#,
        )
        .unwrap();
        let report = load(&s, &doc, &|_| None);
        assert!(
            report.errors.is_empty() && report.warnings.is_empty(),
            "{report:?}"
        );
        assert!(report.unknown_attributes.is_empty(), "{report:?}");
        let c = cooldown_of(&s, "CD2");
        assert!(c.reverse && c.draw_edge);
        assert_eq!(s.eval::<i64>("return CD2:IsShown() and 1 or 0").unwrap(), 0);
        assert!(s.eval::<bool>("return F2.SetCooldown == nil").unwrap());
    }

    #[test]
    fn the_1_12_1_vm_has_no_cooldown_type() {
        let s = UiScript::new().unwrap();
        let e = s
            .run("CreateFrame('Cooldown', 'CD3')")
            .unwrap_err()
            .to_string();
        assert!(e.contains("unknown frame type 'Cooldown'"), "{e}");
        let doc = crate::framexml::parse(r#"<Ui><Cooldown name="CD4"/></Ui>"#).unwrap();
        let report = load(&s, &doc, &|_| None);
        assert_eq!(report.warnings, ["Unknown frame type: Cooldown"]);
    }
}
