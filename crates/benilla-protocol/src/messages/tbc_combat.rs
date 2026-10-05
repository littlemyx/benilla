//! The combat readout on 2.4.3: the combat log packets whose bytes or whose values differ from
//! 1.12.1. Every layout is cmangos-tbc's (`Unit.cpp`, `SpellEffects.cpp`) and agrees with
//! wow_messages' 2.4.3 definition, except where a comment says a part is single-source.

use std::io::{self, Read};

use crate::wire::{capacity_hint, read_packed_guid, read_u32_le, read_u64_le, read_u8};

use super::combat_log::{DamageShield, SpellDispelLog, SpellHealLog, SpellInstaKillLog};

/// The school index (0 physical, 1 holy, 2 fire, 3 nature, 4 frost, 5 shadow, 6 arcane) of the
/// first school in a 2.4.3 school mask (cmangos-tbc sends the mask where cmangos-classic sends
/// `GetFirstSchoolInMask`); 0 for an empty mask.
pub(super) fn school_from_mask(mask: u32) -> u32 {
    if mask == 0 {
        0
    } else {
        mask.trailing_zeros()
    }
}

/// `SMSG_SPELLDAMAGESHIELD` (2.4.3): victim guid, attacker guid, `u32` spell (dropped, 1.12.1 has
/// none), damage, school mask.
pub(super) fn read_damage_shield_tbc(r: &mut impl Read) -> io::Result<DamageShield> {
    let victim = read_u64_le(r)?;
    let attacker = read_u64_le(r)?;
    let _spell = read_u32_le(r)?;
    let damage = read_u32_le(r)?;
    let school = school_from_mask(read_u32_le(r)?);
    Ok(DamageShield {
        victim,
        attacker,
        damage,
        school,
    })
}

/// `SMSG_SPELLHEALLOG` (2.4.3): the 1.12.1 body and a trailing `u8` (unused by the client).
pub(super) fn read_spell_heal_log_tbc(r: &mut impl Read) -> io::Result<SpellHealLog> {
    let log = super::combat_log::read_spell_heal_log(r)?;
    let _unused = read_u8(r)?;
    Ok(log)
}

/// `SMSG_SPELLINSTAKILLLOG` (2.4.3): the caster's raw guid (dropped), the victim's, the spell.
pub(super) fn read_spell_insta_kill_log_tbc(r: &mut impl Read) -> io::Result<SpellInstaKillLog> {
    let _caster = read_u64_le(r)?;
    super::combat_log::read_spell_insta_kill_log(r)
}

/// `SMSG_SPELLDISPELLOG` (2.4.3): packed victim and caster, the dispelling spell and a `u8`
/// (both dropped), a count of `{u32 spell, u8 method}` (the method, dispelled or stolen, dropped).
pub(super) fn read_spell_dispel_log_tbc(r: &mut impl Read) -> io::Result<SpellDispelLog> {
    let victim = read_packed_guid(r)?;
    let caster = read_packed_guid(r)?;
    let _dispel_spell = read_u32_le(r)?;
    let _unused = read_u8(r)?;
    let count = read_u32_le(r)?;
    let mut spell_ids = Vec::with_capacity(capacity_hint(count, 64));
    for _ in 0..count {
        spell_ids.push(read_u32_le(r)?);
        let _method = read_u8(r)?;
    }
    Ok(SpellDispelLog {
        victim,
        caster,
        spell_ids,
    })
}

/// `SMSG_LOG_XPGAIN` (2.4.3): the 1.12.1 body, then a `u8` refer-a-friend flag (dropped) when
/// present. cmangos-tbc `SendLogXPGain` appends it (cmangos-classic does not); wow_messages' 2.4.3
/// definition shares 1.12.1's and ends without it, so the byte is read when the body carries it and
/// the other form still reads (single-source for the byte).
pub(super) fn read_xp_gain_tbc(r: &mut &[u8]) -> io::Result<super::progression::XpGain> {
    let gain = super::progression::read_xp_gain(r)?;
    if !r.is_empty() {
        let _refer_a_friend = read_u8(r)?;
    }
    Ok(gain)
}
