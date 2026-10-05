//! The combat log's engine side that `Blizzard_CombatLog` calls, installed for the 5.1 dialect only:
//! the entry store with its cursor, the filter list, and the object-flag test. The store holds what
//! the client received as `COMBAT_LOG_EVENT` rows; nothing feeds it in this client yet, so a log
//! answers what the reference answers with nothing in it: no entries, no current entry, no advance.
//!
//! Contracts, from the addon's own use (`Blizzard_CombatLog.lua`):
//! - `CombatLogGetNumEntries()` -> the stored count; the refilter loop bounds itself with it.
//! - `CombatLogSetCurrentEntry(n)` -> nothing; the refilter rewinds with 0 (the newest entry).
//! - `CombatLogGetCurrentEntry()` -> the current entry's values (timestamp, event, ...), or false
//!   when there is none (`local valid = CombatLogGetCurrentEntry()`).
//! - `CombatLogAdvanceEntry(n)` -> whether an entry is current after moving `n` toward older
//!   (negative) or newer (positive) entries; the refilter walks with -1 until false.
//! - `CombatLogResetFilter()` / `CombatLogAddFilter(eventList, sourceFlags, destFlags)` -> nothing;
//!   each argument may be nil, `eventList` is a comma-separated string, a flags argument a number
//!   or, for a GUID filter, a string.
//! - `CombatLogClearEntries()` -> nothing.
//! - `CombatLog_Object_IsA(flags, mask)` -> boolean: the unit flags hit every category of `mask`.

use std::collections::VecDeque;

use mlua::{Lua, MultiValue, Value};

use super::Model;
use super::ScriptValue;

/// The retained history, the addon's own `COMBATLOG_MESSAGE_LIMIT` order of magnitude.
const MAX_ENTRIES: usize = 4000;

/// One filter row as `CombatLogAddFilter` took it.
#[derive(Clone, Debug, PartialEq)]
pub struct CombatLogFilter {
    /// The event names, comma separated; `None` for every event.
    pub events: Option<String>,
    pub source: FilterUnits,
    pub dest: FilterUnits,
}

/// What a filter asks of the source or destination unit.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterUnits {
    Any,
    /// A `COMBATLOG_OBJECT_*` flag mask.
    Flags(u32),
    /// One unit's GUID.
    Guid(String),
}

/// The entry store, its cursor and the filter list.
#[derive(Default)]
pub(crate) struct CombatLogState {
    /// Oldest first.
    entries: VecDeque<Vec<ScriptValue>>,
    /// The current entry counted from the newest, 0 the newest.
    cursor: usize,
    filters: Vec<CombatLogFilter>,
}

/// The four category groups of `COMBATLOG_OBJECT_*` (affiliation, reaction, control, type) and the
/// special-case group (target, focus, raid targets, none).
const GROUPS: [u32; 5] = [
    0x0000_000F,
    0x0000_00F0,
    0x0000_0300,
    0x0000_FC00,
    0xFFFF_0000,
];

/// Whether `flags` hit every group `mask` names: `CombatLog_Object_IsA`.
fn object_is_a(flags: u32, mask: u32) -> bool {
    let named: Vec<u32> = GROUPS.into_iter().filter(|g| mask & g != 0).collect();
    !named.is_empty() && named.iter().all(|g| flags & mask & g != 0)
}

impl super::UiScript {
    /// Store one `COMBAT_LOG_EVENT` row as the newest entry, dropping the oldest past the limit.
    pub fn combat_log_push(&mut self, entry: Vec<ScriptValue>) {
        let mut model = self.model_mut();
        let log = &mut model.combat_log;
        if log.entries.len() == MAX_ENTRIES {
            log.entries.pop_front();
        }
        log.entries.push_back(entry);
    }

    /// The filters the addon last set, in the order it added them.
    pub fn combat_log_filters(&self) -> Vec<CombatLogFilter> {
        self.model_ref().combat_log.filters.clone()
    }
}

fn units_arg(v: &Value) -> FilterUnits {
    match v {
        Value::Integer(i) => FilterUnits::Flags(*i as u32),
        Value::Number(n) => FilterUnits::Flags(*n as u32),
        Value::String(s) => FilterUnits::Guid(s.to_string_lossy().to_string()),
        _ => FilterUnits::Any,
    }
}

fn flags_arg(v: &Value) -> Option<u32> {
    match v {
        Value::Integer(i) => Some(*i as u32),
        Value::Number(n) => Some(*n as u32),
        _ => None,
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "CombatLogGetNumEntries",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(model.combat_log.entries.len() as i64)
        })?,
    )?;
    g.set(
        "CombatLogSetCurrentEntry",
        lua.create_function(|lua, n: Value| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            model.combat_log.cursor = flags_arg(&n).unwrap_or(0) as usize;
            Ok(())
        })?,
    )?;
    g.set(
        "CombatLogGetCurrentEntry",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let log = &model.combat_log;
            let Some(entry) = log
                .entries
                .len()
                .checked_sub(log.cursor + 1)
                .and_then(|i| log.entries.get(i))
            else {
                return Ok(MultiValue::from_vec(vec![Value::Boolean(false)]));
            };
            let mut out = Vec::with_capacity(entry.len());
            for v in entry {
                out.push(v.clone().into_lua(lua)?);
            }
            Ok(MultiValue::from_vec(out))
        })?,
    )?;
    g.set(
        "CombatLogAdvanceEntry",
        lua.create_function(|lua, n: Value| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let log = &mut model.combat_log;
            // Negative walks toward older entries, which sit further from the newest.
            let step = match n {
                Value::Integer(i) => i,
                Value::Number(f) => f as i64,
                _ => 0,
            };
            let to = log.cursor as i64 - step;
            if to < 0 || to >= log.entries.len() as i64 {
                return Ok(false);
            }
            log.cursor = to as usize;
            Ok(true)
        })?,
    )?;
    g.set(
        "CombatLogResetFilter",
        lua.create_function(|lua, ()| {
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .combat_log
                .filters
                .clear();
            Ok(())
        })?,
    )?;
    g.set(
        "CombatLogAddFilter",
        lua.create_function(|lua, (events, source, dest): (Value, Value, Value)| {
            let events = match &events {
                Value::String(s) => Some(s.to_string_lossy().to_string()),
                _ => None,
            };
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .combat_log
                .filters
                .push(CombatLogFilter {
                    events,
                    source: units_arg(&source),
                    dest: units_arg(&dest),
                });
            Ok(())
        })?,
    )?;
    g.set(
        "CombatLogClearEntries",
        lua.create_function(|lua, ()| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            model.combat_log.entries.clear();
            model.combat_log.cursor = 0;
            Ok(())
        })?,
    )?;
    g.set(
        "CombatLog_Object_IsA",
        lua.create_function(|_, (flags, mask): (Value, Value)| {
            Ok(match (flags_arg(&flags), flags_arg(&mask)) {
                (Some(f), Some(m)) => object_is_a(f, m),
                _ => false,
            })
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::UiScript;

    fn script() -> UiScript {
        UiScript::with_dialect(crate::script::ScriptDialect::Lua51).unwrap()
    }

    #[test]
    fn an_empty_log_answers_zero_entries_no_current_entry_and_no_advance() {
        let s = script();
        assert_eq!(s.eval::<i64>("return CombatLogGetNumEntries()").unwrap(), 0);
        s.run("CombatLogSetCurrentEntry(0)").unwrap();
        assert!(!s.eval::<bool>("return CombatLogGetCurrentEntry()").unwrap());
        assert!(!s.eval::<bool>("return CombatLogAdvanceEntry(-1)").unwrap());
        s.run("CombatLogClearEntries()").unwrap();
    }

    #[test]
    fn filters_are_stored_in_order_and_reset_clears_them() {
        let s = script();
        s.run(
            r#"CombatLogResetFilter()
               CombatLogAddFilter("SPELL_DAMAGE,SWING_DAMAGE", 0x511, nil)
               CombatLogAddFilter(nil, "0xF130000000000001", 4)"#,
        )
        .unwrap();
        assert_eq!(
            s.combat_log_filters(),
            vec![
                CombatLogFilter {
                    events: Some("SPELL_DAMAGE,SWING_DAMAGE".into()),
                    source: FilterUnits::Flags(0x511),
                    dest: FilterUnits::Any,
                },
                CombatLogFilter {
                    events: None,
                    source: FilterUnits::Guid("0xF130000000000001".into()),
                    dest: FilterUnits::Flags(4),
                },
            ]
        );
        s.run("CombatLogResetFilter()").unwrap();
        assert!(s.combat_log_filters().is_empty());
    }

    #[test]
    fn a_pushed_entry_is_current_and_the_walk_ends_past_the_oldest() {
        let mut s = script();
        s.combat_log_push(vec![
            ScriptValue::Number(1.0),
            ScriptValue::Str("OLD".into()),
        ]);
        s.combat_log_push(vec![
            ScriptValue::Number(2.0),
            ScriptValue::Str("NEW".into()),
        ]);
        assert_eq!(s.eval::<i64>("return CombatLogGetNumEntries()").unwrap(), 2);
        s.run("CombatLogSetCurrentEntry(0)").unwrap();
        assert_eq!(
            s.eval::<(f64, String)>("return CombatLogGetCurrentEntry()")
                .unwrap(),
            (2.0, "NEW".into())
        );
        assert!(s.eval::<bool>("return CombatLogAdvanceEntry(-1)").unwrap());
        assert_eq!(
            s.eval::<(f64, String)>("return CombatLogGetCurrentEntry()")
                .unwrap(),
            (1.0, "OLD".into())
        );
        assert!(!s.eval::<bool>("return CombatLogAdvanceEntry(-1)").unwrap());
    }

    /// The two uses the addon makes: a unit with no unit-flags is `NONE`; mine hits every category.
    #[test]
    fn the_object_test_hits_every_named_category() {
        const MINE: u32 = 0x1;
        const FRIENDLY: u32 = 0x10;
        const PLAYER_CONTROL: u32 = 0x100;
        const PLAYER: u32 = 0x400;
        let me = MINE | FRIENDLY | PLAYER_CONTROL | PLAYER;
        let filter_mine = MINE | FRIENDLY | PLAYER_CONTROL | PLAYER | 0x1000;
        assert!(object_is_a(me, filter_mine));
        // A hostile NPC hits the type group of the filter (the NPC bit) but not the others.
        assert!(!object_is_a(0x40 | 0x200 | 0x800 | 0x8, filter_mine));
        assert!(object_is_a(0x8000_0000, 0x8000_0000));
        assert!(!object_is_a(me, 0x8000_0000));
        assert!(!object_is_a(me, 0));
    }
}
