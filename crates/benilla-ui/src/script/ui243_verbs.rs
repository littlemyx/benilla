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

/// One running mirror timer, as `MIRROR_TIMER_START` carried it (times in ms, `scale` the signed
/// rate: -1 drains, 1 fills).
#[derive(Clone, Debug)]
pub(crate) struct MirrorTimerState {
    name: String,
    /// The value at `set_at`.
    value: f64,
    max: f64,
    scale: i64,
    paused: bool,
    label: String,
    /// `GetTime` seconds when `value` was true.
    set_at: f64,
}

impl MirrorTimerState {
    /// The value now: moving at `scale` per second of ms while running, held while paused.
    fn value_at(&self, now: f64) -> f64 {
        if self.paused {
            return self.value;
        }
        (self.value + self.scale as f64 * (now - self.set_at) * 1000.0).clamp(0.0, self.max)
    }
}

/// The player's channel: what the cast bar shows, on the `GetTime` clock in ms.
#[derive(Clone, Debug)]
pub(crate) struct ChannelState {
    name: String,
    text: String,
    texture: Option<String>,
    start_ms: f64,
    end_ms: f64,
}

impl super::UiScript {
    /// `MIRROR_TIMER_START`: the timer `name` runs from `value_ms` of `max_ms` at `scale`.
    pub fn mirror_timer_start(
        &mut self,
        name: &str,
        value_ms: i64,
        max_ms: i64,
        scale: i64,
        paused: bool,
        label: &str,
    ) {
        let now = super::clock::now(self.lua());
        let mut model = self.model_mut();
        model.mirror_timers.retain(|t| t.name != name);
        model.mirror_timers.push(MirrorTimerState {
            name: name.to_owned(),
            value: value_ms as f64,
            max: max_ms as f64,
            scale,
            paused,
            label: label.to_owned(),
            set_at: now,
        });
    }

    /// `MIRROR_TIMER_PAUSE`: freeze or resume `name` at the value it has now.
    pub fn mirror_timer_pause(&mut self, name: &str, paused: bool) {
        let now = super::clock::now(self.lua());
        if let Some(t) = self
            .model_mut()
            .mirror_timers
            .iter_mut()
            .find(|t| t.name == name)
        {
            t.value = t.value_at(now);
            t.set_at = now;
            t.paused = paused;
        }
    }

    /// `MIRROR_TIMER_STOP`.
    pub fn mirror_timer_stop(&mut self, name: &str) {
        self.model_mut().mirror_timers.retain(|t| t.name != name);
    }

    /// `SPELLCAST_START`: the player casts `name` for `duration_ms`, the bar reading `text`.
    pub fn cast_start(
        &mut self,
        name: &str,
        text: &str,
        texture: Option<String>,
        duration_ms: i64,
    ) {
        let start_ms = super::clock::now(self.lua()) * 1000.0;
        self.model_mut().cast = Some(ChannelState {
            name: name.to_owned(),
            text: text.to_owned(),
            texture,
            start_ms,
            end_ms: start_ms + duration_ms as f64,
        });
    }

    /// `SPELLCAST_DELAYED`: the running cast ends `delay_ms` later.
    pub fn cast_delay(&mut self, delay_ms: i64) {
        if let Some(c) = self.model_mut().cast.as_mut() {
            c.end_ms += delay_ms as f64;
        }
    }

    /// `SPELLCAST_STOP`, `_FAILED` and `_INTERRUPTED`: no cast runs.
    pub fn cast_stop(&mut self) {
        self.model_mut().cast = None;
    }

    /// Push the dungeon difficulty the server states (0 normal, 1 heroic on the wire), which
    /// `GetCurrentDungeonDifficulty` answers as 1 or 2.
    pub fn set_dungeon_difficulty(&mut self, wire: u32) {
        self.model_mut().dungeon_difficulty = wire.min(1) as u8 + 1;
    }

    /// Push whether the server allows voice chat (`SMSG_FEATURE_SYSTEM_STATUS`), which
    /// `IsVoiceChatAllowedByServer` answers.
    pub fn set_voice_chat_allowed(&mut self, allowed: bool) {
        self.model_mut().voice_chat_allowed = allowed;
    }

    /// `SPELLCAST_CHANNEL_START`: the player channels `name` for `duration_ms`, the bar reading
    /// `text` and `texture`.
    pub fn channel_start(
        &mut self,
        name: &str,
        text: &str,
        texture: Option<String>,
        duration_ms: i64,
    ) {
        let start_ms = super::clock::now(self.lua()) * 1000.0;
        self.model_mut().channel = Some(ChannelState {
            name: name.to_owned(),
            text: text.to_owned(),
            texture,
            start_ms,
            end_ms: start_ms + duration_ms as f64,
        });
    }

    /// `SPELLCAST_CHANNEL_UPDATE`: the channel now ends `remaining_ms` from now.
    pub fn channel_retime(&mut self, remaining_ms: i64) {
        let now_ms = super::clock::now(self.lua()) * 1000.0;
        if let Some(c) = self.model_mut().channel.as_mut() {
            c.end_ms = now_ms + remaining_ms as f64;
        }
    }

    /// `SPELLCAST_CHANNEL_STOP`.
    pub fn channel_stop(&mut self) {
        self.model_mut().channel = None;
    }

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
    install_engine_state(lua)?;
    install_tooltip_owner(lua)
}

/// Verbs that read what the engine holds. A bag's family is not tracked (every bag is a general
/// one), a channel's icon is not resolved, and `SetAutoLootDefault` keeps its value for no reader yet.
fn install_engine_state(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    // `GetContainerNumFreeSlots(bag)` -> free slots, bag family (0 = a general bag, the only kind
    // held); nothing for a bag that is not equipped. The stock backpack count sums the family-0 bags.
    g.set(
        "GetContainerNumFreeSlots",
        lua.create_function(|lua, bag: Value| {
            let bag = number_arg(lua, bag, "Usage: GetContainerNumFreeSlots(bag)")?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(match model.containers.get(&i64::from(bag)) {
                Some(c) => {
                    let used = u32::try_from(c.slots.len()).unwrap_or(u32::MAX);
                    MultiValue::from_vec(vec![
                        Value::Integer(i64::from(c.num_slots.saturating_sub(used))),
                        Value::Integer(0),
                    ])
                }
                None => MultiValue::new(),
            })
        })?,
    )?;
    // `GetNumLanguages()`: the languages the character knows, the list `GetLanguageByIndex` walks
    // (1.12.1 registers the same count as `GetNumLaguages`).
    g.set(
        "GetNumLanguages",
        lua.create_function(|lua, _: MultiValue| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.known_languages.len() as i64)
        })?,
    )?;
    // `GetMirrorTimerInfo(index)` -> name, value, maxvalue, scale, paused, label of the index-th
    // running timer; `"UNKNOWN"` with zeros for a slot with none (`MirrorTimer.lua` hides on it).
    g.set(
        "GetMirrorTimerInfo",
        lua.create_function(|lua, index: Value| {
            let index = number_arg(lua, index, r#"Usage: GetMirrorTimerInfo("timer")"#)?;
            let now = super::clock::now(lua);
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let timer = usize::try_from(index)
                .ok()
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| model.mirror_timers.get(i));
            Ok(match timer {
                Some(t) => (
                    t.name.clone(),
                    t.value_at(now).round() as i64,
                    t.max as i64,
                    t.scale,
                    i64::from(t.paused),
                    t.label.clone(),
                ),
                None => ("UNKNOWN".to_owned(), 0, 0, 0, 0, String::new()),
            })
        })?,
    )?;
    // The auction sort order per list (`"list"`, `"bidder"`, `"owner"`), as the browse, bid and
    // auction tabs set it before a header click: `SortAuctionClearSort(type)` empties it,
    // `SortAuctionSetSort(type, column, reverse)` appends, `GetAuctionSort(type, n)` -> column,
    // reverse of the n-th. The order reaches the app's lists through the 1.12.1 `SortAuctionItems`
    // feed only, so `SortAuctionApplySort` keeps the order and re-sorts nothing yet.
    g.set(
        "SortAuctionClearSort",
        lua.create_function(|lua, ty: String| {
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .auction_sort_order
                .remove(&ty.to_ascii_lowercase());
            Ok(())
        })?,
    )?;
    g.set(
        "SortAuctionSetSort",
        lua.create_function(|lua, (ty, column, reverse): (String, String, Value)| {
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .auction_sort_order
                .entry(ty.to_ascii_lowercase())
                .or_default()
                .push((
                    column,
                    !matches!(reverse, Value::Nil | Value::Boolean(false)),
                ));
            Ok(())
        })?,
    )?;
    g.set(
        "SortAuctionApplySort",
        lua.create_function(|_, _: String| Ok(()))?,
    )?;
    g.set(
        "GetAuctionSort",
        lua.create_function(|lua, (ty, n): (String, Value)| {
            let n = number_arg(lua, n, r#"Usage: GetAuctionSort("type", index)"#)?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(
                match usize::try_from(n)
                    .ok()
                    .and_then(|n| n.checked_sub(1))
                    .and_then(|i| {
                        model
                            .auction_sort_order
                            .get(&ty.to_ascii_lowercase())?
                            .get(i)
                    }) {
                    Some((column, reverse)) => MultiValue::from_vec(vec![
                        Value::String(lua.create_string(column)?),
                        Value::Boolean(*reverse),
                    ]),
                    None => MultiValue::new(),
                },
            )
        })?,
    )?;
    // `UnitCastingInfo("unit")` -> name, subtext, text, texture, startTime, endTime, isTradeSkill
    // while the unit casts (`CastingBarFrame.lua:54`); only the player's cast is held.
    g.set(
        "UnitCastingInfo",
        lua.create_function(|lua, unit: Value| {
            let unit =
                super::binding_abi::string_arg(lua, unit, r#"Usage: UnitCastingInfo("unit")"#)?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let cast = model
                .cast
                .as_ref()
                .filter(|_| unit.eq_ignore_ascii_case("player"));
            Ok(match cast {
                Some(c) => MultiValue::from_vec(vec![
                    Value::String(lua.create_string(&c.name)?),
                    Value::String(lua.create_string("")?),
                    Value::String(lua.create_string(&c.text)?),
                    match &c.texture {
                        Some(t) => Value::String(lua.create_string(t)?),
                        None => Value::Nil,
                    },
                    Value::Number(c.start_ms),
                    Value::Number(c.end_ms),
                    Value::Boolean(false),
                ]),
                None => MultiValue::new(),
            })
        })?,
    )?;
    // `IsVoiceChatAllowedByServer()`: the server's voice-chat flag from its feature status.
    g.set(
        "IsVoiceChatAllowedByServer",
        lua.create_function(|lua, _: MultiValue| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.voice_chat_allowed)
        })?,
    )?;
    // `GetBuildInfo()` -> version, build, date: the 2.4.3 exe's own three strings.
    g.set(
        "GetBuildInfo",
        lua.create_function(|_, ()| Ok(("2.4.3", "8606", "Jul 10 2008")))?,
    )?;
    // `UnitChannelInfo("unit")` -> name, subtext, text, texture, startTime, endTime, isTradeSkill
    // while the unit channels; only the player's channel is held, so any other unit answers nothing.
    g.set(
        "UnitChannelInfo",
        lua.create_function(|lua, unit: Value| {
            let unit =
                super::binding_abi::string_arg(lua, unit, r#"Usage: UnitChannelInfo("unit")"#)?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let channel = model
                .channel
                .as_ref()
                .filter(|_| unit.eq_ignore_ascii_case("player"));
            Ok(match channel {
                Some(c) => MultiValue::from_vec(vec![
                    Value::String(lua.create_string(&c.name)?),
                    Value::String(lua.create_string("")?),
                    Value::String(lua.create_string(&c.text)?),
                    match &c.texture {
                        Some(t) => Value::String(lua.create_string(t)?),
                        None => Value::Nil,
                    },
                    Value::Number(c.start_ms),
                    Value::Number(c.end_ms),
                    Value::Boolean(false),
                ]),
                None => MultiValue::new(),
            })
        })?,
    )?;
    // `SetAutoLootDefault(bool)`: the Auto Loot option's engine side; nothing reads it yet.
    g.set(
        "SetAutoLootDefault",
        lua.create_function(|lua, on: Value| {
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .auto_loot_default = !matches!(on, Value::Nil | Value::Boolean(false));
            Ok(())
        })?,
    )?;
    // `frame:IsEventRegistered("EVENT")`: whether this frame registered it (1, else nil).
    let frame_methods: Table = lua.named_registry_value(super::REG_FRAME_METHODS)?;
    frame_methods.set(
        "IsEventRegistered",
        lua.create_function(|lua, (this, event): (Table, Value)| {
            let h = frame_handle_of(lua, &this)?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let Value::String(event) = event else {
                let who = model
                    .arena
                    .frame(h)
                    .and_then(|f| f.name.clone())
                    .unwrap_or_else(|| "<unnamed>".to_string());
                return Err(mlua::Error::runtime(format!(
                    "Usage: {who}:IsEventRegistered(\"event\")"
                )));
            };
            let event = event.to_str()?;
            let registered = model
                .frame_events
                .get(&h)
                .is_some_and(|set| set.contains(&*event));
            Ok(if registered {
                Value::Integer(1)
            } else {
                Value::Nil
            })
        })?,
    )
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
    // The dungeon difficulty the server last stated; 1, normal, until it states one (2 is heroic).
    g.set(
        "GetCurrentDungeonDifficulty",
        lua.create_function(|lua, _: MultiValue| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(i64::from(model.dungeon_difficulty))
        })?,
    )?;
    // No totems: `haveTotem` false with an empty name, icon and zero times.
    g.set(
        "GetTotemInfo",
        lua.create_function(|lua, slot: Value| {
            number_arg(lua, slot, "Usage: GetTotemInfo(slot)")?;
            Ok((false, "", 0, 0, ""))
        })?,
    )?;
    // No refer-a-friend summon has been used: no cooldown, `start, duration` both 0.
    g.set(
        "GetSummonFriendCooldown",
        lua.create_function(|_, _: MultiValue| Ok((0, 0)))?,
    )?;
    // No arena teams are held: `GetArenaTeam(index)` answers no team at any index.
    g.set(
        "GetArenaTeam",
        lua.create_function(|lua, index: Value| {
            number_arg(lua, index, "Usage: GetArenaTeam(index)")?;
            Ok(MultiValue::new())
        })?,
    )?;
    // No voice sessions: `GetVoiceSessionInfo(id)` names none.
    g.set(
        "GetVoiceSessionInfo",
        lua.create_function(|lua, id: Value| {
            number_arg(lua, id, "Usage: GetVoiceSessionInfo(id)")?;
            Ok(MultiValue::new())
        })?,
    )?;
    // No LFG autojoin or LFM autofill flag is held, so clearing one changes nothing.
    for name in ["ClearLFGAutojoin", "ClearLFMAutofill"] {
        g.set(name, lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    }
    // No daily quest is counted: the quest log shows no daily line at 0.
    g.set(
        "GetDailyQuestsCompleted",
        lua.create_function(|_, _: MultiValue| Ok(0))?,
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
        // No guild bank is open: no tabs, no money, and nothing a member may withdraw.
        ("GetNumGuildBankTabs", 0),
        ("GetGuildBankMoney", 0),
        ("GetGuildBankWithdrawMoney", 0),
        // The first tab, the one `GetNumGuildBankTabs() + 1` offers to buy.
        ("GetCurrentGuildBankTab", 1),
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
        // No voice chat: nobody talks.
        "UnitIsTalking",
        "GetSelectedDisplayChannel",
        "UnitIsPossessed",
        "GetActiveVoiceChannel",
        "GetVoiceCurrentSessionID",
        "IsVoiceChatEnabled",
        // Not opted out of loot; no refer-a-friend link.
        "GetOptOutOfLoot",
        "IsReferAFriendLinked",
        // No refer-a-friend link, so no friend can be summoned; no voice session has members.
        "CanSummonFriend",
        "GetNumVoiceSessionMembersBySessionID",
        // No craft slot filter is set and no guild bank is open, so none is repairable.
        "GetCraftFilter",
        "CanGuildBankRepair",
        "CanWithdrawGuildBankMoney",
    ] {
        g.set(
            name,
            lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?,
        )?;
    }
    for name in [
        "GetLFGTypes",
        // No craft window is open, so no inventory slots to filter by; no guild bank tab exists.
        "GetCraftSlots",
        "GetGuildBankTabInfo",
        // The guild's tabard is not held, so it names no image files.
        "GetGuildTabardFileNames",
        "VoiceEnumerateOutputDevices",
        "VoiceEnumerateCaptureDevices",
        "Sound_GameSystem_GetOutputDriverNameByIndex",
    ] {
        g.set(
            name,
            lua.create_function(|_, _: MultiValue| Ok(MultiValue::new()))?,
        )?;
    }
    // No honor currency is read from the player's fields yet.
    g.set(
        "GetHonorCurrency",
        lua.create_function(|_, _: MultiValue| Ok(0))?,
    )?;
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
    fn unit_casting_info_answers_the_players_running_cast_and_nothing_else() {
        let mut s = s51();
        assert_eq!(s.arity(r#"UnitCastingInfo("player")"#).unwrap(), 0);
        s.cast_start(
            "Fireball",
            "Fireball",
            Some("Interface\\Icons\\Spell_Fire".into()),
            3000,
        );
        let (name, sub, text, tex, start, end, trade): (
            String,
            String,
            String,
            String,
            f64,
            f64,
            bool,
        ) = s.eval(r#"return UnitCastingInfo("player")"#).unwrap();
        assert_eq!(
            (name.as_str(), sub.as_str(), text.as_str()),
            ("Fireball", "", "Fireball")
        );
        assert_eq!(tex, "Interface\\Icons\\Spell_Fire");
        assert_eq!(end - start, 3000.0);
        assert!(!trade);
        s.cast_delay(500);
        let end2: f64 = s
            .eval(r#"return select(6, UnitCastingInfo("player"))"#)
            .unwrap();
        assert_eq!(end2 - end, 500.0);
        assert_eq!(s.arity(r#"UnitCastingInfo("target")"#).unwrap(), 0);
        s.cast_stop();
        assert_eq!(s.arity(r#"UnitCastingInfo("player")"#).unwrap(), 0);
    }

    #[test]
    fn the_build_voice_guild_bank_and_honor_verbs_answer_what_the_files_read() {
        let mut s = s51();
        assert_eq!(
            s.eval::<(String, String, String)>("return GetBuildInfo()")
                .unwrap(),
            ("2.4.3".into(), "8606".into(), "Jul 10 2008".into())
        );
        assert!(!s
            .eval::<bool>("return IsVoiceChatAllowedByServer()")
            .unwrap());
        s.set_voice_chat_allowed(true);
        assert!(s
            .eval::<bool>("return IsVoiceChatAllowedByServer()")
            .unwrap());
        assert_eq!(
            s.eval::<(i64, i64, i64, i64)>(
                "return GetNumGuildBankTabs(), GetGuildBankMoney(), GetGuildBankWithdrawMoney(), GetHonorCurrency()"
            )
            .unwrap(),
            (0, 0, 0, 0)
        );
        assert!(!s
            .eval::<bool>("return CanSummonFriend('Someone') and true or false")
            .unwrap());
        assert_eq!(s.arity("GetCraftSlots()").unwrap(), 0);
        assert_eq!(
            s.arity("GetNumVoiceSessionMembersBySessionID(1)").unwrap(),
            1
        );
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

    /// The 14 verbs the first 2.4.3 run found missing at run time are 2.4.3's alone.
    #[test]
    fn the_run_time_verbs_are_2_4_3s_alone() {
        let old = UiScript::new().unwrap();
        let new = s51();
        for name in [
            "GetSummonFriendCooldown",
            "GetMirrorTimerInfo",
            "UnitChannelInfo",
            "GetContainerNumFreeSlots",
            "GetArenaTeam",
            "GetNumLanguages",
            "GetVoiceSessionInfo",
            "ClearLFGAutojoin",
            "ClearLFMAutofill",
            "GetDailyQuestsCompleted",
            "SetAutoLootDefault",
            "UnitIsTalking",
        ] {
            let code = format!("return {name} == nil");
            assert!(old.eval::<bool>(&code).unwrap(), "{name} is not 1.12.1's");
            assert!(!new.eval::<bool>(&code).unwrap(), "{name} is missing");
        }
        let code = "return CreateFrame('Frame').IsEventRegistered == nil";
        assert!(old.eval::<bool>(code).unwrap());
        assert!(!new.eval::<bool>(code).unwrap());
    }

    #[test]
    fn free_slots_are_the_bag_less_what_it_holds_and_the_language_count_is_the_list() {
        use crate::script::{ContainerSlot, ContainerState};
        let mut s = s51();
        let one = |s: &UiScript, code: &str| s.eval::<String>(&format!("return {code}")).unwrap();
        assert_eq!(
            one(&s, "select('#', GetContainerNumFreeSlots(0))"),
            "0",
            "no bag, no answer"
        );
        let slots = (1..=3u32)
            .map(|i| (i, ContainerSlot::default()))
            .collect::<std::collections::HashMap<_, _>>();
        s.set_container(
            0,
            Some(ContainerState {
                name: Some("Backpack".into()),
                num_slots: 16,
                slots,
            }),
        );
        assert_eq!(
            one(&s, "table.concat({GetContainerNumFreeSlots(0)}, ',')"),
            "13,0"
        );
        assert!(s.run("GetContainerNumFreeSlots()").is_err());
        assert_eq!(one(&s, "GetNumLanguages()"), "0");
        s.set_known_languages(vec!["Common".into(), "Orcish".into()]);
        assert_eq!(one(&s, "GetNumLanguages()"), "2");
        assert_eq!(one(&s, "GetLanguageByIndex(2)"), "Orcish");
    }

    #[test]
    fn a_mirror_timer_reads_back_as_it_runs_and_an_empty_slot_is_unknown() {
        let mut s = s51();
        let row = |s: &UiScript, i: u32| {
            s.eval::<String>(&format!(
                "return table.concat({{GetMirrorTimerInfo({i})}}, ',')"
            ))
            .unwrap()
        };
        assert_eq!(row(&s, 1), "UNKNOWN,0,0,0,0,");
        s.set_now(100.0);
        s.mirror_timer_start("BREATH", 60_000, 60_000, -1, false, "Breath");
        assert_eq!(row(&s, 1), "BREATH,60000,60000,-1,0,Breath");
        assert_eq!(row(&s, 2), "UNKNOWN,0,0,0,0,");
        // Ten seconds later at scale -1 it has drained 10000 ms.
        s.set_now(110.0);
        assert_eq!(row(&s, 1), "BREATH,50000,60000,-1,0,Breath");
        // Paused, it holds the value it had.
        s.mirror_timer_pause("BREATH", true);
        s.set_now(130.0);
        assert_eq!(row(&s, 1), "BREATH,50000,60000,-1,1,Breath");
        s.mirror_timer_stop("BREATH");
        assert_eq!(row(&s, 1), "UNKNOWN,0,0,0,0,");
        assert!(s.run("GetMirrorTimerInfo('x')").is_err());
    }

    #[test]
    fn the_players_channel_reads_back_while_it_runs() {
        let mut s = s51();
        let info = |s: &UiScript, unit: &str| {
            s.eval::<String>(&format!(
                "local n, sub, text, tex, st, en, ts = UnitChannelInfo('{unit}') \
                 return tostring(n)..'|'..tostring(text)..'|'..tostring(tex)..'|'..tostring(st)..'|'..tostring(en)..'|'..tostring(ts)"
            ))
            .unwrap()
        };
        assert_eq!(info(&s, "player"), "nil|nil|nil|nil|nil|nil");
        s.set_now(10.0);
        s.channel_start("Drain Life", "Channeling", None, 5_000);
        assert_eq!(
            info(&s, "player"),
            "Drain Life|Channeling|nil|10000|15000|false"
        );
        assert_eq!(
            info(&s, "target"),
            "nil|nil|nil|nil|nil|nil",
            "only the player's is held"
        );
        s.set_now(11.0);
        s.channel_retime(2_000);
        assert!(info(&s, "player").ends_with("|13000|false"));
        s.channel_stop();
        assert_eq!(info(&s, "player"), "nil|nil|nil|nil|nil|nil");
        assert!(s.run("UnitChannelInfo()").is_err());
    }

    #[test]
    fn a_frame_knows_the_events_it_registered() {
        let s = s51();
        s.run("F = CreateFrame('Frame') F:RegisterEvent('PLAYER_LOGIN')")
            .unwrap();
        assert_eq!(
            s.eval::<i64>("return F:IsEventRegistered('PLAYER_LOGIN')")
                .unwrap(),
            1
        );
        assert_eq!(
            s.eval::<String>("return tostring(F:IsEventRegistered('PLAYER_LOGOUT'))")
                .unwrap(),
            "nil"
        );
        s.run("F:UnregisterEvent('PLAYER_LOGIN')").unwrap();
        assert_eq!(
            s.eval::<String>("return tostring(F:IsEventRegistered('PLAYER_LOGIN'))")
                .unwrap(),
            "nil"
        );
        assert!(s.run("F:IsEventRegistered()").is_err());
    }

    #[test]
    fn the_subsystems_the_client_lacks_answer_as_one_with_none_does() {
        let s = s51();
        let one = |code: &str| {
            s.eval::<String>(&format!("return tostring({code})"))
                .unwrap()
        };
        assert_eq!(one("table.concat({GetSummonFriendCooldown()}, ',')"), "0,0");
        assert_eq!(one("select('#', GetArenaTeam(1))"), "0");
        assert_eq!(one("select('#', GetVoiceSessionInfo(1))"), "0");
        assert_eq!(one("GetDailyQuestsCompleted()"), "0");
        assert_eq!(one("UnitIsTalking('Someone')"), "nil");
        s.run("ClearLFGAutojoin() ClearLFMAutofill() SetAutoLootDefault(true)")
            .unwrap();
        assert!(s.run("GetArenaTeam()").is_err());
    }
}
