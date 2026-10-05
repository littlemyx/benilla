//! The stock 2.4.3 interface after its load, asked what it does with the engine's state: the buff
//! bar over the aura verbs. Each test reads the player's own 2.4.3 install and skips without it.

use benilla_ui::script::{AuraState, UiScript, UnitGuids};

use super::stock_load::load_stock_with_script;
use crate::local_state::test_env::ENV_LOCK;

fn aura(spell_id: u32, name: &str, icon: &str, helpful: bool, count: u8) -> AuraState {
    AuraState {
        spell_id,
        name: Some(name.into()),
        icon: Some(icon.into()),
        count,
        debuff_type: None,
        duration: 0.0,
        expiration_time: 0.0,
        helpful,
        cancelable: helpful,
        until_cancelled: true,
        channeled: false,
    }
}

/// What the buff bar shows: `(buff buttons shown, debuff buttons shown, first buff's icon)`.
fn bar(s: &UiScript) -> (i64, i64, Option<String>) {
    s.eval::<(i64, i64, Option<String>)>(
        r#"local function shown(prefix, max)
               local n = 0
               for i = 1, max do
                   local b = getglobal(prefix .. i)
                   if b and b:IsShown() then n = n + 1 end
               end
               return n
           end
           local icon = getglobal("BuffButton1Icon")
           return shown("BuffButton", 32), shown("DebuffButton", 16), icon and icon:GetTexture()"#,
    )
    .unwrap()
}

fn push(s: &mut UiScript, auras: Vec<AuraState>) {
    s.set_player_auras(auras);
    s.fire_event("PLAYER_AURAS_CHANGED", vec![]);
}

/// The stock bar asks `GetPlayerBuff(i, filter)` from 1 and takes 0 for none: with a player who has
/// no aura it shows no button, and with one buff it shows exactly that buff's icon.
#[test]
fn the_2_4_3_buff_bar_shows_one_button_for_one_buff_and_none_for_none() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let _l = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_report, mut s, _laid) = load_stock_with_script(&data, "buff-243");
    let icon = "Interface\\Icons\\Ability_Warrior_OffensiveStance";

    push(&mut s, vec![]);
    assert_eq!(bar(&s), (0, 0, None), "no aura, no button");

    push(&mut s, vec![aura(2457, "Battle Stance", icon, true, 1)]);
    assert_eq!(bar(&s), (1, 0, Some(icon.to_string())));

    // A debuff lands on the debuff row; the buff row keeps its one.
    push(
        &mut s,
        vec![
            aura(2457, "Battle Stance", icon, true, 1),
            aura(
                702,
                "Curse of Weakness",
                "Interface\\Icons\\Spell_Shadow_CurseOfMannoroth",
                false,
                2,
            ),
        ],
    );
    let (buffs, debuffs, _) = bar(&s);
    assert_eq!((buffs, debuffs), (1, 1));
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

/// The 2.4.3 verbs answer the 1-based buffIndex and the longer `UnitBuff` / `UnitDebuff` lists.
#[test]
fn the_2_4_3_aura_verbs_answer_the_lists_the_stock_files_read() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let _l = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_report, mut s, _laid) = load_stock_with_script(&data, "aura-verbs-243");
    s.set_unit_guids(&UnitGuids {
        player: 0x10,
        ..Default::default()
    });
    let mut curse = aura(
        702,
        "Curse of Weakness",
        "Interface\\Icons\\Curse",
        false,
        2,
    );
    curse.debuff_type = Some("Curse".into());
    curse.duration = 120.0;
    curse.expiration_time = s.now() + 100.0;
    curse.until_cancelled = false;
    push(
        &mut s,
        vec![
            aura(2457, "Battle Stance", "Interface\\Icons\\Stance", true, 1),
            curse,
        ],
    );
    // buffIndex: 1-based cache position, 0 for none, the filter counted from 1.
    assert_eq!(
        s.eval::<(i64, i64)>(r#"return GetPlayerBuff(1, "HELPFUL")"#)
            .unwrap(),
        (1, 1)
    );
    assert_eq!(
        s.eval::<(i64, i64)>(r#"return GetPlayerBuff(1, "HARMFUL")"#)
            .unwrap(),
        (2, 0)
    );
    assert_eq!(
        s.eval::<(i64, i64)>(r#"return GetPlayerBuff(2, "HELPFUL")"#)
            .unwrap(),
        (0, 0)
    );
    assert_eq!(
        s.eval::<Option<String>>("return GetPlayerBuffTexture(2)")
            .unwrap()
            .as_deref(),
        Some("Interface\\Icons\\Curse")
    );
    assert_eq!(
        s.eval::<Option<String>>("return GetPlayerBuffTexture(0)")
            .unwrap(),
        None
    );
    assert_eq!(
        s.eval::<i64>("return GetPlayerBuffApplications(2)")
            .unwrap(),
        2
    );
    assert_eq!(
        s.eval::<Option<String>>("return GetPlayerBuffDispelType(2)")
            .unwrap()
            .as_deref(),
        Some("Curse")
    );
    // UnitBuff: name, rank, icon, count, duration, timeLeft.
    let (name, rank, icon, count, duration, left): (
        String,
        String,
        String,
        i64,
        Option<f64>,
        Option<f64>,
    ) = s.eval(r#"return UnitBuff("player", 1)"#).unwrap();
    assert_eq!(
        (name.as_str(), rank.as_str(), icon.as_str(), count),
        ("Battle Stance", "", "Interface\\Icons\\Stance", 1)
    );
    assert_eq!(
        (duration, left),
        (None, None),
        "a permanent aura carries no time"
    );
    // UnitDebuff: name, rank, icon, count, debuffType, duration, timeLeft.
    let (name, _, _, count, kind, duration, left): (String, String, String, i64, String, f64, f64) =
        s.eval(r#"return UnitDebuff("player", 1)"#).unwrap();
    assert_eq!(
        (name.as_str(), count, kind.as_str()),
        ("Curse of Weakness", 2, "Curse")
    );
    assert!(
        duration == 120.0 && (left - 100.0).abs() < 1.0,
        "{duration} {left}"
    );
    assert_eq!(
        s.arity(r#"UnitBuff("player", 2)"#).unwrap(),
        0,
        "past the end: nothing"
    );
}

/// `PLAYER_LOGIN` makes the stock interface ask for `Blizzard_CombatLog` on demand: the addon is a
/// registry row of the 2.4.3 chain, loads from it, and the failure popup's table stays empty.
#[test]
fn the_combat_log_addon_loads_on_demand_at_login() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let _l = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_report, mut s, _laid) = load_stock_with_script(&data, "combatlog-243");
    assert!(
        s.eval::<bool>(r#"return IsAddOnLoadOnDemand("Blizzard_CombatLog") ~= nil"#)
            .unwrap(),
        "the addon is a registry row"
    );
    assert!(!s
        .eval::<bool>(r#"return IsAddOnLoaded("Blizzard_CombatLog") ~= nil"#)
        .unwrap());
    let before = s.errors().len();
    s.fire_event("PLAYER_LOGIN", vec![]);
    assert!(
        s.eval::<bool>(r#"return IsAddOnLoaded("Blizzard_CombatLog") ~= nil"#)
            .unwrap(),
        "the addon loaded"
    );
    assert!(
        !s.eval::<bool>(r#"return StaticPopup1 ~= nil and StaticPopup1:IsShown() == 1"#)
            .unwrap(),
        "no failure popup is up"
    );
    let errors = s.errors().split_off(before);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(s.eval::<bool>("return COMBATLOG ~= nil").unwrap());
}
