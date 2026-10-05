//! The global verbs the 2.4.3 interface calls that 1.12.1 has no name for, installed only for the
//! 5.1 dialect. A verb whose subsystem this client does not hold yet answers what the reference
//! answers with nothing in it (no totem, no voice session, no ready check); each says so.

use mlua::{Lua, MultiValue, Table, Value, Variadic};

use super::binding_abi::number_arg;
use super::object::{decode_id, frame_handle_of, frame_wrapper};
use super::Model;
use crate::widget::KindState;

/// The exe's own bound for `ChangeActionBarPage` (`NUM_ACTIONBAR_PAGES` in FrameXML is the same 6).
const ACTION_BAR_PAGES: i32 = 6;

impl super::UiScript {
    /// Push the account's expansion level, the byte of the 2.4.3 `SMSG_AUTH_RESPONSE` that admitted
    /// the session (`0` classic, `1` Burning Crusade), which `GetAccountExpansionLevel` answers.
    pub fn set_account_expansion(&mut self, level: u8) {
        self.model_mut().account_expansion = level;
    }

    /// Push the action ids whose item template stacks past one, which `IsStackableAction` answers.
    pub fn set_stackable_actions(&mut self, actions: std::collections::HashSet<u32>) {
        self.model_mut().stackable_actions = actions;
    }

    /// Seat the `<ModifiedClick>` rows of the stock `Bindings.xml`: each action's default key, which
    /// `GetModifiedClick` answers until the player sets another.
    pub fn register_modified_clicks(&self, rows: &[(String, String)]) {
        let mut model = self.model_mut();
        for (action, key) in rows {
            model
                .modified_clicks
                .insert(action.to_ascii_uppercase(), key.clone());
        }
    }

    /// How many times the secure-execution verbs ran: the taint model is pending, so each answered
    /// "secure" (`issecure`) or just called through (`securecall`).
    pub fn secure_model_calls(&self) -> std::collections::BTreeMap<&'static str, u64> {
        self.model_ref().secure_model_calls.clone()
    }
}

fn count_secure(lua: &Lua, verb: &'static str) {
    *lua.app_data_mut::<Model>()
        .expect("model app_data")
        .secure_model_calls
        .entry(verb)
        .or_default() += 1;
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    install_secure(lua)?;
    install_action_bar(lua)?;
    install_account(lua)?;
    install_empty_state(lua)?;
    install_tooltip_owner(lua)
}

/// `GameTooltip:GetOwner()`: the frame `SetOwner` was given, nil while unowned (1.12.1 answers
/// only `IsOwned(frame)`).
fn install_tooltip_owner(lua: &Lua) -> mlua::Result<()> {
    let m: Table = lua.named_registry_value(super::tooltip::REG_TOOLTIP_METHODS)?;
    m.set(
        "GetOwner",
        lua.create_function(|lua, this: Table| {
            let h = frame_handle_of(lua, &this)?;
            let id = {
                let mut model = lua.app_data_mut::<Model>().expect("model app_data");
                let owner = match model.arena.frame(h).map(|f| &f.kind_state) {
                    Some(KindState::Tooltip(t)) => t.owner,
                    _ => return Err(mlua::Error::runtime("not a GameTooltip")),
                };
                owner.map(|o| model.frame_id(o))
            };
            match id {
                Some(id) => Ok(Value::Table(frame_wrapper(lua, id)?)),
                None => Ok(Value::Nil),
            }
        })?,
    )
}

/// The secure-execution names. Taint model pending: nothing is ever tainted, so execution is
/// always secure and `securecall` is a plain call.
fn install_secure(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "issecure",
        lua.create_function(|lua, ()| {
            count_secure(lua, "issecure");
            Ok(true)
        })?,
    )?;
    // `securecall(function-or-global-name, ...)` returns what the callee returns.
    g.set(
        "securecall",
        lua.create_function(|lua, (callee, args): (Value, Variadic<Value>)| {
            count_secure(lua, "securecall");
            let func = match callee {
                Value::Function(f) => f,
                Value::String(name) => match lua.globals().get::<Value>(name.to_str()?)? {
                    Value::Function(f) => f,
                    _ => {
                        return Err(mlua::Error::runtime(
                            "securecall(): the name is not a global function",
                        ))
                    }
                },
                _ => {
                    return Err(mlua::Error::runtime(
                        "Usage: securecall(function or \"name\" [, ...])",
                    ))
                }
            };
            func.call::<MultiValue>(args)
        })?,
    )
}

/// The action bar page is the engine's in 2.4.3: `ChangeActionBarPage(n)` sets it (1.12.1 leaves
/// the page to FrameXML's `CURRENT_ACTIONBAR_PAGE`) and `GetActionBarPage` reads it.
fn install_action_bar(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "GetActionBarPage",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(i64::from(model.action_bar_page))
        })?,
    )?;
    g.set(
        "ChangeActionBarPage",
        lua.create_function(|lua, page: Value| {
            let usage = "ChangeActionBarPage() needs a page in the range 1 to 6";
            let page = number_arg(lua, page, usage)?;
            if !(1..=ACTION_BAR_PAGES).contains(&page) {
                return Err(mlua::Error::runtime(usage));
            }
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .action_bar_page = page as u8;
            super::tick::fire_event_into(lua, "ACTIONBAR_PAGE_CHANGED", Vec::new());
            Ok(())
        })?,
    )?;
    // The slot's item template stacks, pushed with the slot; the Count gate beside
    // `IsConsumableAction`.
    g.set(
        "IsStackableAction",
        lua.create_function(|lua, slot: Value| {
            let slot = number_arg(lua, slot, "Usage: IsStackableAction(slot)")?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(
                match u32::try_from(slot).is_ok_and(|s| model.stackable_actions.contains(&s)) {
                    true => Value::Integer(1),
                    false => Value::Nil,
                },
            )
        })?,
    )
}

/// What the session and the locale already know.
fn install_account(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "GetAccountExpansionLevel",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(i64::from(model.account_expansion))
        })?,
    )?;
    // `GetModifiedClick("action")`: the key setting of a click modifier, the stock file's default
    // until another is set; nothing for an action the file does not name.
    g.set(
        "GetModifiedClick",
        lua.create_function(|lua, action: Value| {
            let action = super::binding_abi::string_arg(
                lua,
                action,
                r#"Usage: GetModifiedClick("action")"#,
            )?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model
                .modified_clicks
                .get(&action.to_ascii_uppercase())
                .cloned())
        })?,
    )?;
    // The locales this client has: the one it runs, `GetLocale`'s.
    g.set(
        "GetExistingLocales",
        lua.create_function(|lua, ()| {
            let locale: mlua::Function = lua.globals().get("GetLocale")?;
            locale.call::<MultiValue>(())
        })?,
    )?;
    // In a battleground when a queue slot is active; only the player's own state is held, so any
    // other unit answers nil.
    g.set(
        "UnitInBattleground",
        lua.create_function(|lua, unit: Value| {
            let is_player =
                matches!(&unit, Value::String(s) if s.as_bytes().eq_ignore_ascii_case(b"player"));
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let active = model.battlefield_slots.iter().any(|s| s.status == 3);
            Ok(if is_player && active {
                Value::Integer(1)
            } else {
                Value::Nil
            })
        })?,
    )?;
    // The frame the world map's pings are drawn over; the ping itself is not drawn yet.
    g.set(
        "InitWorldMapPing",
        lua.create_function(|lua, parent: Value| {
            let Value::Table(parent) = parent else {
                return Err(mlua::Error::runtime("Usage: InitWorldMapPing(parent)"));
            };
            // The same two checks, in the same order, as `CreateWorldMapArrowFrame`'s.
            let Ok(id) = decode_id(&parent) else {
                return Err(mlua::Error::runtime(
                    "InitWorldMapPing(): Couldn't find 'this' in parent object",
                ));
            };
            if frame_handle_of(lua, &parent).is_err() {
                return Err(mlua::Error::runtime(
                    "InitWorldMapPing(): Wrong object type, expected frame",
                ));
            }
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .world_map_ping_host = Some(id);
            Ok(())
        })?,
    )?;
    // The names `RegisterForSave` records, for a variable the reference keeps per character; this
    // client has one saved-variables file, so both kinds persist there.
    g.set(
        "RegisterForSavePerCharacter",
        lua.create_function(|lua, name: Value| {
            let usage = r#"Usage: RegisterForSavePerCharacter("variable")"#;
            let name = super::binding_abi::string_arg(lua, name, usage)?;
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            if !model.saved_names.contains(&name) {
                model.saved_names.push(name);
            }
            Ok(())
        })?,
    )
}

/// The subsystems this client does not hold yet answer as a client with nothing in them does.
fn install_empty_state(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // `GetLFGTypeEntries(type)`: the entry names of an LFG type, none while no types exist.
    g.set(
        "GetLFGTypeEntries",
        lua.create_function(|lua, ty: Value| {
            number_arg(lua, ty, "Usage: GetLFGTypeEntries(type)")?;
            Ok(MultiValue::new())
        })?,
    )?;
    // `GetPossessInfo(index)`: texture and name of a possess-bar button, none without a possession.
    g.set(
        "GetPossessInfo",
        lua.create_function(|lua, index: Value| {
            number_arg(lua, index, "Usage: GetPossessInfo(index)")?;
            Ok(MultiValue::new())
        })?,
    )?;
    // `GetChannelDisplayInfo(index)`: a channel-pane row; none exist, so every index (0 included)
    // names no row and answers no values.
    g.set(
        "GetChannelDisplayInfo",
        lua.create_function(|lua, index: Value| {
            number_arg(lua, index, "Usage: GetChannelDisplayInfo(index)")?;
            Ok(MultiValue::new())
        })?,
    )?;
    // No title chosen is -1 (FrameXML reads 0 as "select a title", -1 as "None").
    g.set(
        "GetCurrentTitle",
        lua.create_function(|_, _: MultiValue| Ok(-1))?,
    )?;
    // The dungeon difficulty a character that never changed it has: 1, normal (2 is heroic).
    g.set(
        "GetCurrentDungeonDifficulty",
        lua.create_function(|_, _: MultiValue| Ok(1))?,
    )?;
    // No totems: `haveTotem` false with an empty name, icon and zero times.
    g.set(
        "GetTotemInfo",
        lua.create_function(|lua, slot: Value| {
            number_arg(lua, slot, "Usage: GetTotemInfo(slot)")?;
            Ok((false, "", 0, 0, ""))
        })?,
    )?;
    for (name, answer) in [
        // No voice chat: no sessions, no capture or playback drivers.
        ("GetNumVoiceSessions", 0),
        ("Sound_ChatSystem_GetNumInputDrivers", 0),
        // Output drivers are not enumerated.
        ("Sound_GameSystem_GetNumOutputDrivers", 0),
        // No tracking spells are read from the spellbook yet.
        ("GetNumTrackingTypes", 0),
        ("Sound_ChatSystem_GetNumOutputDrivers", 0),
        // No channel-pane rows and no titles are held.
        ("GetNumDisplayChannels", 0),
        ("GetNumTitles", 0),
    ] {
        g.set(
            name,
            lua.create_function(move |_, _: MultiValue| Ok(answer))?,
        )?;
    }
    // Nothing to answer: no ready check, no selected channel row, no possession, no LFG types,
    // no voice device at any index.
    for name in [
        "GetReadyCheckStatus",
        "GetSelectedDisplayChannel",
        "UnitIsPossessed",
        "GetActiveVoiceChannel",
        "GetVoiceCurrentSessionID",
        "IsVoiceChatEnabled",
        // Not opted out of loot; no refer-a-friend link.
        "GetOptOutOfLoot",
        "IsReferAFriendLinked",
    ] {
        g.set(
            name,
            lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    for name in [
        "GetLFGTypes",
        "VoiceEnumerateOutputDevices",
        "VoiceEnumerateCaptureDevices",
        "Sound_GameSystem_GetOutputDriverNameByIndex",
    ] {
        g.set(
            name,
            lua.create_function(|_, _: MultiValue| Ok(MultiValue::new()))?,
        )?;
    }
    g.set(
        "IsPossessBarVisible",
        lua.create_function(|_, _: MultiValue| Ok(false))?,
    )
}

#[cfg(test)]
mod tests {
    use crate::script::{ScriptDialect, UiScript};

    fn s51() -> UiScript {
        UiScript::with_dialect(ScriptDialect::Lua51).unwrap()
    }

    #[test]
    fn the_secure_model_answers_secure_and_counts_every_call() {
        let s = s51();
        assert!(s.eval::<bool>("return issecure()").unwrap());
        let n: f64 = s
            .eval("return select('#', securecall(function(a, b) return a, b, 3 end, 1, 2))")
            .unwrap();
        assert_eq!(n, 3.0);
        s.run("function Named(x) return x * 2 end").unwrap();
        assert_eq!(s.eval::<i64>("return securecall('Named', 21)").unwrap(), 42);
        assert!(s.run("securecall(5)").is_err());
        assert_eq!(s.secure_model_calls().values().sum::<u64>(), 4);
        assert_eq!(s.secure_model_calls()["issecure"], 1);
    }

    #[test]
    fn none_of_them_exist_on_the_1_12_1_surface() {
        let s = UiScript::new().unwrap();
        for name in [
            "issecure",
            "securecall",
            "GetActionBarPage",
            "IsStackableAction",
            "GetAccountExpansionLevel",
            "UnitInBattleground",
            "GetTotemInfo",
            "RegisterForSavePerCharacter",
            "InitWorldMapPing",
        ] {
            assert!(
                s.eval::<bool>(&format!("return {name} == nil")).unwrap(),
                "{name} is 2.4.3's"
            );
        }
    }

    #[test]
    fn the_action_bar_page_is_the_engine_s_and_changes_only_inside_its_range() {
        let s = s51();
        s.run(
            "SEEN = 0 local f = CreateFrame('Frame') f:RegisterEvent('ACTIONBAR_PAGE_CHANGED') \
             f:SetScript('OnEvent', function() SEEN = SEEN + 1 end)",
        )
        .unwrap();
        assert_eq!(s.eval::<i64>("return GetActionBarPage()").unwrap(), 1);
        s.run("ChangeActionBarPage(4)").unwrap();
        assert_eq!(s.eval::<i64>("return GetActionBarPage()").unwrap(), 4);
        assert_eq!(s.eval::<i64>("return SEEN").unwrap(), 1);
        for bad in ["0", "7", "nil", "'x'"] {
            let e = s.run(&format!("ChangeActionBarPage({bad})")).unwrap_err();
            assert!(e.to_string().contains("range 1 to 6"), "{bad}: {e}");
        }
        assert_eq!(s.eval::<i64>("return GetActionBarPage()").unwrap(), 4);
        assert_eq!(s.eval::<i64>("return SEEN").unwrap(), 1);
    }

    #[test]
    fn the_account_level_and_the_locale_come_from_what_the_client_holds() {
        let mut s = s51();
        assert_eq!(
            s.eval::<i64>("return GetAccountExpansionLevel()").unwrap(),
            0
        );
        s.set_account_expansion(1);
        assert_eq!(
            s.eval::<i64>("return GetAccountExpansionLevel()").unwrap(),
            1
        );
        assert_eq!(
            s.eval::<String>("return table.concat({GetExistingLocales()}, ',')")
                .unwrap(),
            s.eval::<String>("return GetLocale()").unwrap()
        );
    }

    #[test]
    fn a_click_modifier_answers_the_default_the_bindings_file_states() {
        let s = s51();
        let rows = crate::bindings_xml::parse_modified_clicks(
            r#"<Bindings><ModifiedClick action="PROBEPICK" default="SHIFT-BUTTON1"/>
               <ModifiedClick default="X"/><Binding name="N">F()</Binding></Bindings>"#,
        );
        assert_eq!(
            rows,
            vec![("PROBEPICK".to_string(), "SHIFT-BUTTON1".to_string())]
        );
        s.register_modified_clicks(&rows);
        assert_eq!(
            s.eval::<String>("return GetModifiedClick('ProbePick')")
                .unwrap(),
            "SHIFT-BUTTON1"
        );
        assert_eq!(
            s.eval::<String>("return tostring(GetModifiedClick('OTHER'))")
                .unwrap(),
            "nil"
        );
        assert!(s.run("GetModifiedClick()").is_err());
    }

    #[test]
    fn a_tooltip_names_the_frame_that_owns_it() {
        let s = s51();
        s.run("T = CreateFrame('GameTooltip', 'ProbeTip') O = CreateFrame('Frame', 'ProbeOwner')")
            .unwrap();
        assert_eq!(
            s.eval::<String>("return tostring(T:GetOwner())").unwrap(),
            "nil"
        );
        s.run("T:SetOwner(O, 'ANCHOR_RIGHT')").unwrap();
        assert!(s.eval::<bool>("return T:GetOwner() == O").unwrap());
    }

    #[test]
    fn the_world_map_arrow_is_named_and_has_its_effect_frame_only_in_2_4_3() {
        let code = "P = CreateFrame('Frame', 'ProbeMap') CreateWorldMapArrowFrame(P) \
                    CreateWorldMapArrowFrame(P) return PlayerArrowFrame ~= nil, PlayerArrowEffectFrame ~= nil";
        let s = s51();
        let (arrow, effect): (bool, bool) = s.eval(code).unwrap();
        assert!(arrow && effect);
        assert_eq!(
            s.eval::<String>("return PlayerArrowEffectFrame:GetObjectType()")
                .unwrap(),
            "Model"
        );
        let old = UiScript::new().unwrap();
        let (arrow, effect): (bool, bool) = old.eval(code).unwrap();
        assert!(!arrow && !effect, "1.12.1 builds the one unnamed arrow");
    }

    #[test]
    fn the_world_map_ping_host_must_be_a_frame() {
        let s = s51();
        s.run("InitWorldMapPing(CreateFrame('Frame'))").unwrap();
        let e = s.run("InitWorldMapPing()").unwrap_err().to_string();
        assert!(e.contains("Usage: InitWorldMapPing(parent)"), "{e}");
        let e = s.run("InitWorldMapPing({})").unwrap_err().to_string();
        assert!(e.contains("Couldn't find 'this' in parent object"), "{e}");
        let e = s
            .run("InitWorldMapPing(CreateFrame('Frame'):CreateTexture())")
            .unwrap_err()
            .to_string();
        assert!(e.contains("Wrong object type, expected frame"), "{e}");
    }

    #[test]
    fn per_character_names_join_the_saved_set_and_a_non_name_is_refused() {
        let s = s51();
        s.run("RegisterForSavePerCharacter('A') RegisterForSavePerCharacter('A')")
            .unwrap();
        assert_eq!(s.saved_variable_names(), vec!["A".to_string()]);
        assert!(s.run("RegisterForSavePerCharacter()").is_err());
    }

    #[test]
    fn a_client_with_none_of_a_subsystem_answers_its_empty_state() {
        let s = s51();
        let one = |code: &str| {
            s.eval::<String>(&format!("return tostring({code})"))
                .unwrap()
        };
        assert_eq!(one("GetNumVoiceSessions()"), "0");
        assert_eq!(one("GetNumTrackingTypes()"), "0");
        assert_eq!(one("Sound_ChatSystem_GetNumInputDrivers()"), "0");
        assert_eq!(one("Sound_GameSystem_GetNumOutputDrivers()"), "0");
        assert_eq!(one("GetReadyCheckStatus('player')"), "nil");
        assert_eq!(one("GetSelectedDisplayChannel()"), "nil");
        assert_eq!(one("IsPossessBarVisible()"), "false");
        assert_eq!(one("UnitIsPossessed('pet')"), "nil");
        assert_eq!(one("UnitInBattleground('player')"), "nil");
        assert_eq!(
            s.eval::<i64>("return select('#', GetTotemInfo(1))")
                .unwrap(),
            5
        );
        assert_eq!(one("(GetTotemInfo(1))"), "false");
        for call in ["GetLFGTypes()", "VoiceEnumerateOutputDevices(0)"] {
            assert_eq!(
                s.eval::<i64>(&format!("return select('#', {call})"))
                    .unwrap(),
                0
            );
        }
        assert!(s.run("GetTotemInfo()").is_err());
    }

    #[test]
    fn an_action_stacks_when_its_pushed_slot_says_so() {
        let mut s = s51();
        s.set_stackable_actions([7].into());
        assert_eq!(s.eval::<i64>("return IsStackableAction(7)").unwrap(), 1);
        assert_eq!(
            s.eval::<String>("return tostring(IsStackableAction(8))")
                .unwrap(),
            "nil"
        );
        assert!(s.run("IsStackableAction()").is_err());
    }
}
