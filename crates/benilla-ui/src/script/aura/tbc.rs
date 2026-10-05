//! The 2.4.3 spelling of the aura verbs, installed over the 1.12 ones for that build only. The stock
//! 2.4.3 `BuffFrame` loops `GetPlayerBuff(i, "HELPFUL")` from 1 and hides a button when the answer
//! is 0, so a buffIndex is the 1-based cache position and 0 is none (1.12 answers 0-based, -1 for
//! none, and would fill every button); the siblings take that buffIndex. `UnitBuff` returns
//! `name, rank, icon, count, duration, timeLeft` and `UnitDebuff`
//! `name, rank, icon, count, debuffType, duration, timeLeft`, as `TargetFrame.lua` reads them; the
//! two times are nil for an aura whose duration is not known (every unit but the player).

use mlua::{Function, Lua, MultiValue, Table, Value};

use super::player_buff::{
    buff_index_arg, enumerate_player_buff, player_buff_record, PlayerBuffFilter,
};
use super::{auras_of, cancel_authorized, AuraState, Model};

/// The cache position a 2.4.3 buffIndex names; negative for 0 and below, which names none.
fn position(index: i64) -> i64 {
    index - 1
}

fn str_or_nil(lua: &Lua, s: Option<&str>) -> mlua::Result<Value> {
    Ok(match s {
        Some(s) => Value::String(lua.create_string(s)?),
        None => Value::Nil,
    })
}

/// One aura as `UnitBuff` / `UnitDebuff` return it in 2.4.3.
fn unit_returns(lua: &Lua, a: &AuraState, debuff: bool, own: bool) -> mlua::Result<MultiValue> {
    let now = crate::script::clock::now(lua);
    let mut out = vec![
        str_or_nil(lua, a.name.as_deref())?,
        // The spell's rank text is not carried on an aura.
        Value::String(lua.create_string("")?),
        str_or_nil(lua, a.icon.as_deref())?,
        Value::Integer(i64::from(a.count)),
    ];
    if debuff {
        out.push(str_or_nil(lua, a.debuff_type.as_deref())?);
    }
    let timed = own && a.duration > 0.0;
    out.push(if timed {
        Value::Number(a.duration)
    } else {
        Value::Nil
    });
    out.push(if timed {
        Value::Number((a.expiration_time - now).max(0.0))
    } else {
        Value::Nil
    });
    Ok(MultiValue::from_vec(out))
}

fn nth_unit_aura(
    lua: &Lua,
    token: &Option<String>,
    index: i64,
    helpful: bool,
) -> mlua::Result<MultiValue> {
    let hit = {
        let model = lua.app_data_ref::<Model>().expect("model app_data");
        match token {
            Some(t) => {
                let own = model
                    .unit_guids
                    .guid_of(t)?
                    .is_some_and(|g| g == model.unit_guids.player);
                auras_of(&model, t)?
                    .filter(|_| index >= 1)
                    .and_then(|list| {
                        list.iter()
                            .filter(|a| a.helpful == helpful)
                            .nth((index - 1) as usize)
                            .cloned()
                    })
                    .map(|a| (a, own))
            }
            None => None,
        }
    };
    match hit {
        Some((a, own)) => unit_returns(lua, &a, !helpful, own),
        None => Ok(MultiValue::new()),
    }
}

pub(in crate::script) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    g.set(
        "GetPlayerBuff",
        lua.create_function(|lua, (index, filter): (Value, Option<String>)| {
            let index = buff_index_arg(lua, index, r#"Usage: GetPlayerBuff(index [, "filter"])"#)?;
            let filter = PlayerBuffFilter::parse(filter.as_deref());
            Ok(match enumerate_player_buff(lua, position(index), filter) {
                Some((pos, a)) => (pos as i64 + 1, i64::from(a.until_cancelled)),
                None => (0, 0),
            })
        })?,
    )?;
    g.set(
        "GetPlayerBuffTexture",
        lua.create_function(|lua, index: Value| {
            let i = buff_index_arg(lua, index, "Usage: GetPlayerBuffTexture(buffIndex)")?;
            Ok(player_buff_record(lua, position(i)).and_then(|a| a.icon))
        })?,
    )?;
    g.set(
        "GetPlayerBuffDispelType",
        lua.create_function(|lua, index: Value| {
            let i = buff_index_arg(lua, index, "Usage: GetPlayerBuffDispelType(buffIndex)")?;
            Ok(player_buff_record(lua, position(i)).and_then(|a| a.debuff_type))
        })?,
    )?;
    g.set(
        "GetPlayerBuffApplications",
        lua.create_function(|lua, index: Value| {
            let i = buff_index_arg(lua, index, "Usage: GetPlayerBuffApplications(buffIndex)")?;
            Ok(player_buff_record(lua, position(i)).map_or(1, |a| i64::from(a.count)))
        })?,
    )?;
    g.set(
        "GetPlayerBuffTimeLeft",
        lua.create_function(|lua, index: Value| {
            let i = buff_index_arg(lua, index, "Usage: GetPlayerBuffTimeLeft(buffIndex)")?;
            let now = crate::script::clock::now(lua);
            Ok(player_buff_record(lua, position(i))
                .map_or(0.0, |a| (a.expiration_time - now).max(0.0)))
        })?,
    )?;
    g.set(
        "CancelPlayerBuff",
        lua.create_function(|lua, index: Value| {
            let i = buff_index_arg(lua, index, "Usage: CancelPlayerBuff(buffIndex)")?;
            if let Some(a) = player_buff_record(lua, position(i)).filter(cancel_authorized) {
                let mut model = lua.app_data_mut::<Model>().expect("model app_data");
                model.cancel_aura_requests.push(a.spell_id);
            }
            Ok(())
        })?,
    )?;
    g.set(
        "UnitBuff",
        lua.create_function(
            |lua, (token, index, _castable): (Option<String>, i64, Option<Value>)| {
                nth_unit_aura(lua, &token, index, true)
            },
        )?,
    )?;
    g.set(
        "UnitDebuff",
        lua.create_function(
            |lua, (token, index, _dispellable): (Option<String>, i64, Option<Value>)| {
                nth_unit_aura(lua, &token, index, false)
            },
        )?,
    )?;

    // `GameTooltip:SetPlayerBuff(buffIndex)` takes the same 1-based buffIndex; 1.12's takes the
    // 0-based position, so the 2.4.3 method hands it the position.
    let methods: Table = lua.named_registry_value(crate::script::tooltip::REG_TOOLTIP_METHODS)?;
    let original: Function = methods.get("SetPlayerBuff")?;
    methods.set(
        "SetPlayerBuff",
        lua.create_function(move |_, (this, index): (Table, i64)| {
            original.call::<()>((this, position(index)))
        })?,
    )?;
    Ok(())
}
