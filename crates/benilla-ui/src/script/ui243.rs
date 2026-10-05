//! What the 2.4.3 interface's widgets have that 1.12.1's lack, installed only for the 5.1 dialect
//! ([`super::ScriptDialect::Lua51`]), so a 1.12.1 VM's classes, method tables and handler kinds are
//! the ones its client has: the `Cooldown` class and the frame attribute store here.

use mlua::{Lua, Table, Value, Variadic};

use super::object::{decode_id, frame_handle_of};
use super::Model;
use crate::widget::{CooldownState, KindState};

/// Registry key of the `Cooldown` method table.
pub(super) const REG_COOLDOWN_METHODS: &str = "__benilla_cooldown_methods";

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    if super::ScriptDialect::of(lua) != super::ScriptDialect::Lua51 {
        return Ok(());
    }
    lua.app_data_mut::<Model>()
        .expect("model app_data")
        .client_interface = super::addon_gate::CLIENT_INTERFACE_2_4_3;
    install_cooldown(lua)?;
    install_attributes(lua)?;
    install_text_methods(lua)?;
    super::aura::install_tbc(lua)?;
    super::ui243_combatlog::install(lua)?;
    super::ui243_verbs::install(lua)
}

/// The 2.4.3 name and argument list of a 1.12.1 event whose trigger the engine already has: the
/// cast events moved to `UNIT_SPELLCAST_*` and take the unit first (`CastingBarFrame.lua` reads
/// `arg1` as the unit and asks `UnitCastingInfo` / `UnitChannelInfo` for the rest; `ChatFrame.lua`
/// reads `unit, spellName, rank`). Only the player casts here, so the unit is `"player"`; the rank
/// is not carried, `""`; the 1.12.1 millisecond arguments are state the info verbs answer.
pub(super) fn renamed_event(
    event: &str,
    args: &[super::ScriptValue],
) -> Option<(&'static str, Vec<super::ScriptValue>)> {
    use super::ScriptValue::Str;
    let player = || Str("player".into());
    let spell = |i: usize| match args.get(i) {
        Some(Str(name)) => Some(Str(name.clone())),
        _ => None,
    };
    Some(match event {
        // `SPELLCAST_START(name, ms)`.
        "SPELLCAST_START" => (
            "UNIT_SPELLCAST_START",
            vec![player(), spell(0)?, Str(String::new())],
        ),
        // `SPELLCAST_CHANNEL_START(ms, name)`.
        "SPELLCAST_CHANNEL_START" => (
            "UNIT_SPELLCAST_CHANNEL_START",
            vec![player(), spell(1)?, Str(String::new())],
        ),
        "SPELLCAST_STOP" => ("UNIT_SPELLCAST_STOP", vec![player()]),
        "SPELLCAST_FAILED" => ("UNIT_SPELLCAST_FAILED", vec![player()]),
        "SPELLCAST_INTERRUPTED" => ("UNIT_SPELLCAST_INTERRUPTED", vec![player()]),
        "SPELLCAST_DELAYED" => ("UNIT_SPELLCAST_DELAYED", vec![player()]),
        "SPELLCAST_CHANNEL_UPDATE" => ("UNIT_SPELLCAST_CHANNEL_UPDATE", vec![player()]),
        "SPELLCAST_CHANNEL_STOP" => ("UNIT_SPELLCAST_CHANNEL_STOP", vec![player()]),
        _ => return None,
    })
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

/// The text and caret methods the 2.4.3 stock files call that 1.12.1's classes lack, each thin over
/// what the widget already does: `SetFormattedText` is `SetText(string.format(...))` on a
/// FontString and a Button; `SetCursorPosition(n)` moves an EditBox's caret to letter `n` (the
/// count of letters before it, clamped to the text); `IsInIMECompositionMode` answers false, there
/// being no IME (`GetInputLanguage` answers "ROMAN" the same way).
fn install_text_methods(lua: &Lua) -> mlua::Result<()> {
    use mlua::ObjectLike;
    for reg in [
        super::REG_FONTSTRING_METHODS,
        super::button::REG_BUTTON_METHODS,
    ] {
        let m: Table = lua.named_registry_value(reg)?;
        m.set(
            "SetFormattedText",
            lua.create_function(|lua, (this, args): (Table, Variadic<Value>)| {
                let format: mlua::Function = lua.globals().get::<Table>("string")?.get("format")?;
                let text: Value = format.call(args)?;
                this.call_method::<()>("SetText", text)
            })?,
        )?;
    }
    let m: Table = lua.named_registry_value(super::editbox::REG_EDITBOX_METHODS)?;
    m.set(
        "SetCursorPosition",
        lua.create_function(|lua, (this, pos): (Table, f64)| {
            let h = frame_handle_of(lua, &this)?;
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            match model.arena.frame_mut(h).map(|f| &mut f.kind_state) {
                Some(KindState::EditBox(eb)) => {
                    let letters = if pos.is_finite() && pos > 0.0 {
                        pos as usize
                    } else {
                        0
                    };
                    let byte = eb
                        .text
                        .char_indices()
                        .nth(letters)
                        .map_or(eb.text.len(), |(i, _)| i);
                    eb.move_caret_to(byte, false);
                    Ok(())
                }
                _ => Err(mlua::Error::runtime("not an EditBox")),
            }
        })?,
    )?;
    m.set(
        "IsInIMECompositionMode",
        lua.create_function(|lua, this: Table| {
            frame_handle_of(lua, &this)?;
            Ok(false)
        })?,
    )
}

/// Registry key of the attribute store: frame id to a table of that frame's attributes.
const REG_ATTRS: &str = "__benilla_attributes";

/// The name a usage error reads, `<unnamed>` for a nameless frame.
fn who(lua: &Lua, this: &Table) -> String {
    frame_handle_of(lua, this)
        .ok()
        .and_then(|h| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            model.arena.frame(h).and_then(|f| f.name.clone())
        })
        .unwrap_or_else(|| "<unnamed>".to_string())
}

/// `SetAttribute(name, value)`: stores `value` (nil removes it) under `name` verbatim, and fires
/// `OnAttributeChanged(name, value)` when it differs from what was stored. Both are the 2.4.3 client's
/// shape per its usage string and handler parameters; that an equal value fires nothing is unverified.
pub(crate) fn set_attribute(lua: &Lua, this: &Table, name: &str, value: Value) -> mlua::Result<()> {
    frame_handle_of(lua, this)?;
    let id = decode_id(this)?;
    let store: Table = lua.named_registry_value(REG_ATTRS)?;
    let per: Table = match store.get::<Value>(id)? {
        Value::Table(t) => t,
        _ => {
            let t = lua.create_table()?;
            store.set(id, t.clone())?;
            t
        }
    };
    let old: Value = per.get(name)?;
    if old.equals(&value)? {
        return Ok(());
    }
    per.set(name, value.clone())?;
    let args = vec![Value::String(lua.create_string(name)?), value];
    if let Err(e) = super::event::fire_widget_handler(lua, id, "OnAttributeChanged", args) {
        lua.app_data_mut::<Model>()
            .expect("model app_data")
            .record_script_error(e.to_string());
    }
    Ok(())
}

/// The attribute `name` answers for a lookup, the wildcard forms in the order the stock secure
/// templates document (`prefix` is a modifier such as `shift-`, `suffix` a button number): the exact
/// `prefixNAMEsuffix`, then `*NAMEsuffix`, then `prefixNAME*`, then `*NAME*`, then plain `NAME`
/// (setting a name alone is "equivalent to `*attribute*`"). A value of `""` is a value, found.
fn lookup_attribute(per: &Table, prefix: &str, name: &str, suffix: &str) -> mlua::Result<Value> {
    for key in [
        format!("{prefix}{name}{suffix}"),
        format!("*{name}{suffix}"),
        format!("{prefix}{name}*"),
        format!("*{name}*"),
        name.to_string(),
    ] {
        let v: Value = per.get(key)?;
        if !v.is_nil() {
            return Ok(v);
        }
    }
    Ok(Value::Nil)
}

/// What a 2.4.3 layout element names for drawing that is not applied here yet: the text-wrapping
/// attributes (`nonspacewrap`, `bytes`, `maxLines`, `indented`), a font's `monochrome`, a button's
/// `<PushedTextOffset>` and a scrolling message frame's `insertMode`. The loader reads and keeps
/// them under the owner's key (`id:<n>` for a frame or region, `font:<name>` for a font object) for
/// the renderer; none changes drawing yet (gap).
pub(crate) fn store_hint(lua: &Lua, owner: String, name: &str, value: &str) {
    lua.app_data_mut::<Model>()
        .expect("model app_data")
        .xml_hints
        .entry(owner)
        .or_default()
        .push((name.to_string(), value.to_string()));
}

/// The key of a frame or region wrapper's hints.
pub(crate) fn hint_owner(this: &Table) -> Option<String> {
    decode_id(this).ok().map(|id| format!("id:{id}"))
}

impl super::UiScript {
    /// The value of the layout hint `name` that the loader kept for `owner` (see [`store_hint`]).
    pub fn xml_hint(&self, owner: &str, name: &str) -> Option<String> {
        self.model_ref()
            .xml_hints
            .get(owner)?
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    }
}

/// `protected="true"` from the XML: the flag `IsProtected` answers.
pub(crate) fn set_protected(lua: &Lua, this: &Table, on: bool) -> mlua::Result<()> {
    let h = frame_handle_of(lua, this)?;
    let mut model = lua.app_data_mut::<Model>().expect("model app_data");
    if let Some(f) = model.arena.frame_mut(h) {
        f.protected = on;
    }
    Ok(())
}

fn install_attributes(lua: &Lua) -> mlua::Result<()> {
    lua.set_named_registry_value(REG_ATTRS, lua.create_table()?)?;
    let m: Table = lua.named_registry_value(super::REG_FRAME_METHODS)?;
    m.set(
        "SetAttribute",
        lua.create_function(|lua, (this, name, value): (Table, Value, Value)| {
            let Value::String(name) = name else {
                let who = who(lua, &this);
                return Err(mlua::Error::runtime(format!(
                    "Usage: {who}:SetAttribute(\"name\", value)"
                )));
            };
            set_attribute(lua, &this, &name.to_str()?, value)
        })?,
    )?;
    // `GetAttribute("name")`, or the secure templates' `GetAttribute(prefix, "name", suffix)`.
    m.set(
        "GetAttribute",
        lua.create_function(|lua, (this, args): (Table, Variadic<Value>)| {
            let text = |v: Option<&Value>| match v {
                Some(Value::String(s)) => s.to_str().ok().map(|s| s.to_string()),
                Some(Value::Integer(i)) => Some(i.to_string()),
                Some(Value::Number(n)) => Some(n.to_string()),
                _ => None,
            };
            let usage = || {
                let who = who(lua, &this);
                mlua::Error::runtime(format!("Usage: {who}:GetAttribute(\"name\")"))
            };
            let (prefix, name, suffix) = if args.len() >= 3 {
                let name = text(args.get(1)).ok_or_else(usage)?;
                (
                    text(args.first()).unwrap_or_default(),
                    name,
                    text(args.get(2)).unwrap_or_default(),
                )
            } else {
                (
                    String::new(),
                    text(args.first()).ok_or_else(usage)?,
                    String::new(),
                )
            };
            frame_handle_of(lua, &this)?;
            let id = decode_id(&this)?;
            let store: Table = lua.named_registry_value(REG_ATTRS)?;
            let Value::Table(per) = store.get::<Value>(id)? else {
                return Ok(Value::Nil);
            };
            if args.len() >= 3 {
                lookup_attribute(&per, &prefix, &name, &suffix)
            } else {
                per.get(name)
            }
        })?,
    )?;
    // `IsProtected()` answers `protected, explicit`: the frame's own flag twice; nothing inherits
    // or enforces protection yet, so no frame is protected that was not marked.
    m.set(
        "IsProtected",
        lua.create_function(|lua, this: Table| {
            let h = frame_handle_of(lua, &this)?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let on = model.arena.frame(h).is_some_and(|f| f.protected);
            Ok((on, on))
        })?,
    )?;
    // `AllowAttributeChanges()`: the secure code lets a child take attribute writes in combat;
    // there is no combat lockdown to lift yet.
    m.set(
        "AllowAttributeChanges",
        lua.create_function(|lua, this: Table| {
            frame_handle_of(lua, &this)?;
            Ok(())
        })?,
    )
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

    const HINT_XML: &str = r#"<Ui>
        <Font name="HintFont" monochrome="true" font="x.ttf"/>
        <Button name="HintBtn">
            <PushedTextOffset><AbsDimension x="2" y="-3"/></PushedTextOffset>
            <ButtonText name="HintBtnText" nonspacewrap="true"/>
            <Layers><Layer><FontString name="HintFS" bytes="64" maxLines="2" indented="true"
                nonspacewrap="true"><FontHeight><AbsValue val="20"/></FontHeight></FontString></Layer></Layers>
        </Button>
        <ScrollingMessageFrame name="HintMsg" insertMode="TOP"/></Ui>"#;

    #[test]
    fn the_layout_attributes_of_2_4_3_are_read_and_kept_for_the_renderer() {
        let s = s51();
        let report = load(&s, &crate::framexml::parse(HINT_XML).unwrap(), &|_| None);
        assert!(
            report.unknown_attributes.is_empty() && report.unknown_elements.is_empty(),
            "{report:?}"
        );
        let owner = |name: &str| {
            let t: Table = s.lua().globals().get(name).unwrap();
            hint_owner(&t).unwrap()
        };
        assert_eq!(
            s.xml_hint(&owner("HintBtn"), "pushedTextOffset").as_deref(),
            Some("2,-3")
        );
        assert_eq!(
            s.xml_hint(&owner("HintBtnText"), "nonspacewrap").as_deref(),
            Some("true")
        );
        for (attr, want) in [("bytes", "64"), ("maxLines", "2"), ("indented", "true")] {
            assert_eq!(s.xml_hint(&owner("HintFS"), attr).as_deref(), Some(want));
        }
        assert_eq!(
            s.xml_hint(&owner("HintMsg"), "insertMode").as_deref(),
            Some("TOP")
        );
        assert_eq!(
            s.xml_hint("font:HintFont", "monochrome").as_deref(),
            Some("true")
        );
    }

    #[test]
    fn the_same_attributes_stay_unread_and_reported_on_1_12_1() {
        let v = UiScript::new().unwrap();
        let report = load(&v, &crate::framexml::parse(HINT_XML).unwrap(), &|_| None);
        for want in [
            "<FontString nonspacewrap>",
            "<FontString bytes>",
            "<Font monochrome>",
        ] {
            assert!(
                report.unknown_attributes.contains(&want.to_string()),
                "{want}: {report:?}"
            );
        }
        assert!(report
            .unknown_elements
            .contains(&"<Button><PushedTextOffset>".to_string()));
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

    const LISTED: &str = r#"<Ui>
        <Frame name="TplA" virtual="true" frameLevel="7"/>
        <Button name="TplB" virtual="true" id="9"/>
        <Button name="Both" inherits="TplA, TplB"/>
    </Ui>"#;

    #[test]
    fn a_comma_list_of_templates_applies_each_in_order_on_2_4_3() {
        let s = s51();
        let doc = crate::framexml::parse(LISTED).unwrap();
        let report = load(&s, &doc, &|_| None);
        assert!(
            report.warnings.is_empty() && report.errors.is_empty(),
            "{report:?}"
        );
        assert_eq!(s.eval::<i64>("return Both:GetFrameLevel()").unwrap(), 7);
        assert_eq!(s.eval::<i64>("return Both:GetID()").unwrap(), 9);
        s.run("CreateFrame('Frame', 'Made', nil, 'TplA, TplB')")
            .unwrap();
        let e = s
            .run("CreateFrame('Frame', 'Nope', nil, 'TplA, Missing')")
            .unwrap_err()
            .to_string();
        assert!(e.contains("Couldn't find inherited node"), "{e}");
    }

    #[test]
    fn vanilla_reads_the_whole_value_as_one_template_name() {
        let s = UiScript::new().unwrap();
        let doc = crate::framexml::parse(LISTED).unwrap();
        let report = load(&s, &doc, &|_| None);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("unknown template 'TplA, TplB'")),
            "{report:?}"
        );
    }

    fn num(s: &UiScript, code: &str) -> i64 {
        s.eval::<i64>(code).unwrap()
    }

    #[test]
    fn attributes_store_by_name_and_answer_the_wildcard_forms_in_order() {
        let s = s51();
        s.run("f = CreateFrame('Frame', 'AF')").unwrap();
        assert!(s.eval::<bool>("return f:GetAttribute('x') == nil").unwrap());
        s.run("f:SetAttribute('type', 'plain') f:SetAttribute('*type2', 'anyMod') f:SetAttribute('shift-type*', 'shiftAnyBtn') f:SetAttribute('shift-type1', '')")
            .unwrap();
        assert_eq!(
            s.eval::<String>("return f:GetAttribute('type')").unwrap(),
            "plain"
        );
        // Exact beats every wildcard; `""` is a value, found, and ends the search.
        assert_eq!(
            s.eval::<String>("return f:GetAttribute('shift-', 'type', '1')")
                .unwrap(),
            ""
        );
        // Then `*name suffix`, then `prefix name*`, then plain.
        assert_eq!(
            s.eval::<String>("return f:GetAttribute('shift-', 'type', '2')")
                .unwrap(),
            "anyMod"
        );
        assert_eq!(
            s.eval::<String>("return f:GetAttribute('shift-', 'type', '3')")
                .unwrap(),
            "shiftAnyBtn"
        );
        assert_eq!(
            s.eval::<String>("return f:GetAttribute('ctrl-', 'type', '3')")
                .unwrap(),
            "plain"
        );
        s.run("f:SetAttribute('type', nil)").unwrap();
        assert!(s
            .eval::<bool>("return f:GetAttribute('ctrl-', 'type', '3') == nil")
            .unwrap());
        // Values of any type, per frame.
        s.run("g = CreateFrame('Frame', 'AG') f:SetAttribute('other', g) g:SetAttribute('n', 4)")
            .unwrap();
        assert_eq!(
            num(&s, "return f:GetAttribute('other'):GetAttribute('n')"),
            4
        );
        assert!(s.eval::<bool>("return f:GetAttribute('n') == nil").unwrap());
        let e = s.run("f:SetAttribute(3, 1)").unwrap_err().to_string();
        assert!(e.contains("Usage: AF:SetAttribute(\"name\", value)"), "{e}");
        let e = s.run("f:GetAttribute()").unwrap_err().to_string();
        assert!(e.contains("Usage: AF:GetAttribute(\"name\")"), "{e}");
    }

    #[test]
    fn on_attribute_changed_fires_with_self_name_value_when_the_value_changes() {
        let s = s51();
        s.run(
            "log = {} f = CreateFrame('Frame', 'AH') \
             f:SetScript('OnAttributeChanged', function(self, name, value) \
                 table.insert(log, tostring(self == f) .. ':' .. name .. '=' .. tostring(value)) end)",
        )
        .unwrap();
        s.run("f:SetAttribute('a', 1) f:SetAttribute('a', 1) f:SetAttribute('a', 2) f:SetAttribute('a', nil)")
            .unwrap();
        assert_eq!(
            s.eval::<Vec<String>>("return log").unwrap(),
            ["true:a=1", "true:a=2", "true:a=nil"]
        );
    }

    #[test]
    fn the_attributes_element_and_protected_flag_load_and_a_bad_row_is_named() {
        let s = s51();
        let doc = crate::framexml::parse(
            r#"<Ui><Frame name="XA" protected="true"><Attributes>
                <Attribute name="n" type="number" value="7"/>
                <Attribute name="b" type="boolean" value="true"/>
                <Attribute name="s" value="text"/>
                <Attribute name="gone" type="nil"/>
                <Attribute type="number" value="1"/>
                <Attribute name="novalue"/>
                <Other/>
            </Attributes></Frame><Frame name="XB"/></Ui>"#,
        )
        .unwrap();
        let report = load(&s, &doc, &|_| None);
        assert!(report.errors.is_empty(), "{report:?}");
        assert!(report.unknown_attributes.is_empty(), "{report:?}");
        assert_eq!(report.unknown_elements, ["<Attributes><Other>"]);
        assert_eq!(num(&s, "return XA:GetAttribute('n')"), 7);
        assert!(s
            .eval::<bool>("return XA:GetAttribute('b') == true")
            .unwrap());
        assert_eq!(
            s.eval::<String>("return XA:GetAttribute('s')").unwrap(),
            "text"
        );
        assert!(s
            .eval::<bool>("return XA:GetAttribute('gone') == nil")
            .unwrap());
        assert!(s
            .eval::<bool>(
                "return select(2, XA:IsProtected()) == true and XB:IsProtected() == false"
            )
            .unwrap());
        assert_eq!(report.warnings.len(), 3, "{:?}", report.warnings);
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("unnamed attribute element")));
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("attribute element named novalue missing value")));
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("Unknown attributes element Other")));
    }

    /// The cast events of the engine reach a 2.4.3 listener under their `UNIT_SPELLCAST_*` names,
    /// the unit first, and a 1.12.1 listener under the old ones with the old arguments.
    #[test]
    fn the_cast_events_are_signalled_under_the_names_each_build_registers() {
        use crate::script::ScriptValue::{Int, Str};
        let mut s = s51();
        s.run(
            "seen = {} f = CreateFrame('Frame') \
             for _, e in ipairs({'UNIT_SPELLCAST_START', 'UNIT_SPELLCAST_CHANNEL_START', \
                 'UNIT_SPELLCAST_STOP', 'UNIT_SPELLCAST_DELAYED', 'SPELLCAST_START'}) do f:RegisterEvent(e) end \
             f:SetScript('OnEvent', function(self, event, ...) \
                 table.insert(seen, event .. ':' .. table.concat({tostring((...)), tostring((select(2, ...))), tostring((select(3, ...)))}, ',')) end)",
        )
        .unwrap();
        s.fire_event("SPELLCAST_START", vec![Str("Fireball".into()), Int(3000)]);
        s.fire_event(
            "SPELLCAST_CHANNEL_START",
            vec![Int(8000), Str("Blizzard".into())],
        );
        s.fire_event("SPELLCAST_STOP", vec![]);
        s.fire_event("SPELLCAST_DELAYED", vec![Int(500)]);
        assert_eq!(
            s.eval::<Vec<String>>("return seen").unwrap(),
            [
                "UNIT_SPELLCAST_START:player,Fireball,",
                "UNIT_SPELLCAST_CHANNEL_START:player,Blizzard,",
                "UNIT_SPELLCAST_STOP:player,nil,nil",
                "UNIT_SPELLCAST_DELAYED:player,nil,nil",
            ]
        );
        let mut old = UiScript::new().unwrap();
        old.run(
            "seen = {} f = CreateFrame('Frame') f:RegisterEvent('SPELLCAST_START') \
             f:SetScript('OnEvent', function() table.insert(seen, event .. ':' .. arg1 .. ':' .. arg2) end)",
        )
        .unwrap();
        old.fire_event("SPELLCAST_START", vec![Str("Fireball".into()), Int(3000)]);
        assert_eq!(
            old.eval::<Vec<String>>("return seen").unwrap(),
            ["SPELLCAST_START:Fireball:3000"]
        );
    }

    #[test]
    fn handlers_of_2_4_3_register_on_its_dialect_only_and_a_click_runs_pre_on_post() {
        let s = s51();
        for k in [
            "PreClick",
            "PostClick",
            "OnAttributeChanged",
            "OnTooltipSetItem",
            "OnTooltipSetUnit",
            "OnCharComposition",
            "OnInputLanguageChanged",
        ] {
            assert!(
                s.eval::<bool>(&format!(
                    "return CreateFrame('Frame'):HasScript('{k}') == 1"
                ))
                .unwrap(),
                "{k}"
            );
        }
        s.run(
            "order = {} b = CreateFrame('Button', 'AB') \
             b:SetScript('PreClick', function(self, btn, down) table.insert(order, 'pre:' .. btn .. ':' .. tostring(down)) end) \
             b:SetScript('OnClick', function(self, btn) table.insert(order, 'on:' .. btn) end) \
             b:SetScript('PostClick', function(self, btn) table.insert(order, 'post:' .. btn) end) \
             b:Click('RightButton')",
        )
        .unwrap();
        assert_eq!(
            s.eval::<Vec<String>>("return order").unwrap(),
            [
                "pre:RightButton:false",
                "on:RightButton",
                "post:RightButton"
            ]
        );
        let v = UiScript::new().unwrap();
        for k in ["PreClick", "OnAttributeChanged", "OnCharComposition"] {
            let e = v
                .run(&format!(
                    "CreateFrame('Frame'):SetScript('{k}', function() end)"
                ))
                .unwrap_err()
                .to_string();
            assert!(e.contains(&format!("unsupported script '{k}'")), "{e}");
        }
        assert!(v.eval::<bool>("return CreateFrame('Frame').SetAttribute == nil and CreateFrame('Frame').IsProtected == nil").unwrap());
    }

    #[test]
    fn the_text_and_caret_methods_are_thin_over_the_widget_and_only_on_2_4_3() {
        let s = s51();
        s.run(
            "f = CreateFrame('Frame', 'TF') fs = f:CreateFontString('TFS') \
             fs:SetFormattedText('%s has %d', 'Bob', 3) \
             b = CreateFrame('Button', 'TB') b:SetFormattedText('%03d', 7) \
             e = CreateFrame('EditBox', 'TE') e:SetText('héllo') e:SetCursorPosition(2)",
        )
        .unwrap();
        assert_eq!(
            s.eval::<String>("return fs:GetText()").unwrap(),
            "Bob has 3"
        );
        assert_eq!(s.eval::<String>("return b:GetText()").unwrap(), "007");
        s.run("e:Insert('X')").unwrap();
        assert_eq!(s.eval::<String>("return e:GetText()").unwrap(), "héXllo");
        s.run("e:SetCursorPosition(99) e:Insert('!')").unwrap();
        assert_eq!(s.eval::<String>("return e:GetText()").unwrap(), "héXllo!");
        assert!(s
            .eval::<bool>("return e:IsInIMECompositionMode() == false")
            .unwrap());
        let v = UiScript::new().unwrap();
        assert!(v
            .eval::<bool>(
                "local e = CreateFrame('EditBox') local b = CreateFrame('Button') \
                 return CreateFrame('Frame'):CreateFontString().SetFormattedText == nil \
                    and b.SetFormattedText == nil and e.SetCursorPosition == nil \
                    and e.IsInIMECompositionMode == nil"
            )
            .unwrap());
    }
}
