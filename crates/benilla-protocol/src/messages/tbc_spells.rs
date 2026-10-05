//! Spells on 2.4.3: the cast failure with its renumbered result table, the cooldown list and the
//! channel packets. Each layout is cmangos-tbc's (`Spell.cpp`, `Player.cpp`) and agrees with
//! wow_messages' 2.4.3 definition, except where a comment says a part is single-source.

use std::io;

use crate::wire::{read_packed_guid, read_u32_le, read_u64_le, read_u8};

use super::{CastOutcome, ServerPacket, TbcPacket};

/// `SpellCastResult` of 2.4.3 -> 1.12.1, by entry name: `(2.4.3 value, 1.12.1 value)`, sorted by
/// the 2.4.3 value. A name is on the table only where cmangos-tbc and wow_messages' 2.4.3 enum give
/// it the same value and cmangos-classic and wow_messages' 1.12 enum do too; a result with no 1.12.1
/// entry (35 of the 168 both 2.4.3 sources name) is not on it and reads as [`TbcPacket::CastFailed`].
pub const CAST_RESULT_TBC_TO_112: &[(u8, u8)] = &[
    (0x00, 0x00), // AFFECTING_COMBAT
    (0x01, 0x01), // ALREADY_AT_FULL_HEALTH
    (0x04, 0x03), // ALREADY_BEING_TAMED
    (0x05, 0x04), // ALREADY_HAVE_CHARM
    (0x06, 0x05), // ALREADY_HAVE_SUMMON
    (0x07, 0x06), // ALREADY_OPEN
    (0x0a, 0x09), // BAD_IMPLICIT_TARGETS
    (0x0b, 0x0a), // BAD_TARGETS
    (0x0c, 0x0b), // CANT_BE_CHARMED
    (0x0d, 0x0c), // CANT_BE_DISENCHANTED
    (0x0f, 0x0d), // CANT_BE_PROSPECTED
    (0x10, 0x0e), // CANT_CAST_ON_TAPPED
    (0x11, 0x0f), // CANT_DUEL_WHILE_INVISIBLE
    (0x12, 0x10), // CANT_DUEL_WHILE_STEALTHED
    (0x15, 0x13), // CASTER_DEAD
    (0x16, 0x14), // CHARMED
    (0x17, 0x15), // CHEST_IN_USE
    (0x18, 0x16), // CONFUSED
    (0x19, 0x17), // DONT_REPORT
    (0x1a, 0x18), // EQUIPPED_ITEM
    (0x1b, 0x19), // EQUIPPED_ITEM_CLASS
    (0x1c, 0x1a), // EQUIPPED_ITEM_CLASS_MAINHAND
    (0x1d, 0x1b), // EQUIPPED_ITEM_CLASS_OFFHAND
    (0x1e, 0x1c), // ERROR
    (0x1f, 0x1d), // FIZZLE
    (0x20, 0x1e), // FLEEING
    (0x21, 0x1f), // FOOD_LOWLEVEL
    (0x22, 0x20), // HIGHLEVEL
    (0x24, 0x22), // IMMUNE
    (0x25, 0x23), // INTERRUPTED
    (0x26, 0x24), // INTERRUPTED_COMBAT
    (0x27, 0x25), // ITEM_ALREADY_ENCHANTED
    (0x28, 0x26), // ITEM_GONE
    (0x2a, 0x28), // ITEM_NOT_READY
    (0x2b, 0x29), // LEVEL_REQUIREMENT
    (0x2c, 0x2a), // LINE_OF_SIGHT
    (0x2d, 0x2b), // LOWLEVEL
    (0x2f, 0x2d), // MAINHAND_EMPTY
    (0x30, 0x2e), // MOVING
    (0x31, 0x2f), // NEED_AMMO
    (0x33, 0x31), // NEED_EXOTIC_AMMO
    (0x34, 0x32), // NOPATH
    (0x35, 0x33), // NOT_BEHIND
    (0x36, 0x34), // NOT_FISHABLE
    (0x38, 0x35), // NOT_HERE
    (0x39, 0x36), // NOT_INFRONT
    (0x3a, 0x37), // NOT_IN_CONTROL
    (0x3b, 0x38), // NOT_KNOWN
    (0x3c, 0x39), // NOT_MOUNTED
    (0x3d, 0x3a), // NOT_ON_TAXI
    (0x3e, 0x3b), // NOT_ON_TRANSPORT
    (0x3f, 0x3c), // NOT_READY
    (0x40, 0x3d), // NOT_SHAPESHIFT
    (0x41, 0x3e), // NOT_STANDING
    (0x42, 0x3f), // NOT_TRADEABLE
    (0x43, 0x40), // NOT_TRADING
    (0x44, 0x41), // NOT_UNSHEATHED
    (0x45, 0x42), // NOT_WHILE_GHOST
    (0x46, 0x43), // NO_AMMO
    (0x47, 0x44), // NO_CHARGES_REMAIN
    (0x48, 0x45), // NO_CHAMPION
    (0x49, 0x46), // NO_COMBO_POINTS
    (0x4a, 0x47), // NO_DUELING
    (0x4b, 0x48), // NO_ENDURANCE
    (0x4c, 0x49), // NO_FISH
    (0x4d, 0x4a), // NO_ITEMS_WHILE_SHAPESHIFTED
    (0x4e, 0x4b), // NO_MOUNTS_ALLOWED
    (0x4f, 0x4c), // NO_PET
    (0x50, 0x4d), // NO_POWER
    (0x51, 0x4e), // NOTHING_TO_DISPEL
    (0x52, 0x4f), // NOTHING_TO_STEAL
    (0x53, 0x50), // ONLY_ABOVEWATER
    (0x54, 0x51), // ONLY_DAYTIME
    (0x55, 0x52), // ONLY_INDOORS
    (0x56, 0x53), // ONLY_MOUNTED
    (0x57, 0x54), // ONLY_NIGHTTIME
    (0x58, 0x55), // ONLY_OUTDOORS
    (0x59, 0x56), // ONLY_SHAPESHIFT
    (0x5a, 0x57), // ONLY_STEALTHED
    (0x5b, 0x58), // ONLY_UNDERWATER
    (0x5c, 0x59), // OUT_OF_RANGE
    (0x5d, 0x5a), // PACIFIED
    (0x5e, 0x5b), // POSSESSED
    (0x60, 0x5d), // REQUIRES_AREA
    (0x61, 0x5e), // REQUIRES_SPELL_FOCUS
    (0x62, 0x5f), // ROOTED
    (0x63, 0x60), // SILENCED
    (0x64, 0x61), // SPELL_IN_PROGRESS
    (0x65, 0x62), // SPELL_LEARNED
    (0x66, 0x63), // SPELL_UNAVAILABLE
    (0x67, 0x64), // STUNNED
    (0x68, 0x65), // TARGETS_DEAD
    (0x69, 0x66), // TARGET_AFFECTING_COMBAT
    (0x6a, 0x67), // TARGET_AURASTATE
    (0x6b, 0x68), // TARGET_DUELING
    (0x6c, 0x69), // TARGET_ENEMY
    (0x6d, 0x6a), // TARGET_ENRAGED
    (0x6e, 0x6b), // TARGET_FRIENDLY
    (0x6f, 0x6c), // TARGET_IN_COMBAT
    (0x70, 0x6d), // TARGET_IS_PLAYER
    (0x72, 0x6e), // TARGET_NOT_DEAD
    (0x73, 0x6f), // TARGET_NOT_IN_PARTY
    (0x74, 0x70), // TARGET_NOT_LOOTED
    (0x75, 0x71), // TARGET_NOT_PLAYER
    (0x76, 0x72), // TARGET_NO_POCKETS
    (0x77, 0x73), // TARGET_NO_WEAPONS
    (0x78, 0x74), // TARGET_UNSKINNABLE
    (0x79, 0x75), // THIRST_SATIATED
    (0x7a, 0x76), // TOO_CLOSE
    (0x7b, 0x77), // TOO_MANY_OF_ITEM
    (0x7e, 0x79), // TRAINING_POINTS
    (0x7f, 0x7a), // TRY_AGAIN
    (0x80, 0x7b), // UNIT_NOT_BEHIND
    (0x81, 0x7c), // UNIT_NOT_INFRONT
    (0x82, 0x7d), // WRONG_PET_FOOD
    (0x83, 0x7e), // NOT_WHILE_FATIGUED
    (0x84, 0x7f), // TARGET_NOT_IN_INSTANCE
    (0x85, 0x80), // NOT_WHILE_TRADING
    (0x86, 0x81), // TARGET_NOT_IN_RAID
    (0x87, 0x82), // DISENCHANT_WHILE_LOOTING
    (0x88, 0x83), // PROSPECT_WHILE_LOOTING
    (0x8a, 0x85), // TARGET_FREEFORALL
    (0x8b, 0x86), // NO_EDIBLE_CORPSES
    (0x8c, 0x87), // ONLY_BATTLEGROUNDS
    (0x8d, 0x88), // TARGET_NOT_GHOST
    (0x8e, 0x89), // TOO_MANY_SKILLS
    (0x90, 0x8b), // WRONG_WEATHER
    (0x91, 0x8c), // DAMAGE_IMMUNE
    (0x92, 0x8d), // PREVENTED_BY_MECHANIC
    (0x93, 0x8e), // PLAY_TIME
    (0x94, 0x8f), // REPUTATION
    (0x95, 0x90), // MIN_SKILL
    (0xa8, 0x91), // UNKNOWN
];

/// The 1.12.1 `SpellCastResult` for a 2.4.3 one, `None` when 1.12.1 has no entry of that name.
pub fn cast_result_from_tbc(result: u8) -> Option<u8> {
    CAST_RESULT_TBC_TO_112
        .binary_search_by_key(&result, |&(tbc, _)| tbc)
        .ok()
        .map(|i| CAST_RESULT_TBC_TO_112[i].1)
}

/// 2.4.3 results whose first argument word every source agrees on: the spell focus, the area and
/// the equipped item class (cmangos-tbc `SendCastResult`, wow_messages `SMSG_CAST_FAILED`).
const TBC_REQUIRES_AREA: u8 = 0x60;
const TBC_REQUIRES_SPELL_FOCUS: u8 = 0x61;
const TBC_EQUIPPED_ITEM_CLASS: [u8; 3] = [0x1b, 0x1c, 0x1d];

/// Read `SMSG_CAST_RESULT` (0x130; wow_messages calls it `SMSG_CAST_FAILED`): `u32` spell, `u8` result, `u8` cast
/// count, then arguments by result. The result is mapped to 1.12.1's where it has one
/// ([`ServerPacket::CastResult`]); the others read as [`TbcPacket::CastFailed`]. Only the first
/// argument word of the three results above is read: the later words and the other results'
/// arguments differ between the sources (the equipped-item arm is 2 words in cmangos-tbc and 3 in
/// wow_messages) and are dropped.
pub(super) fn read_cast_failed(r: &mut &[u8]) -> io::Result<ServerPacket> {
    let spell_id = read_u32_le(r)?;
    let result = read_u8(r)?;
    let cast_count = read_u8(r)?;
    let arg = match result {
        TBC_REQUIRES_AREA | TBC_REQUIRES_SPELL_FOCUS => Some(read_u32_le(r)?),
        r_ if TBC_EQUIPPED_ITEM_CLASS.contains(&r_) => Some(read_u32_le(r)?),
        _ => None,
    };
    // Arguments past the first word, whose widths the sources dispute.
    *r = &r[r.len()..];
    Ok(match cast_result_from_tbc(result) {
        Some(reason) => ServerPacket::CastResult {
            spell_id,
            outcome: CastOutcome::Failed { reason, arg },
        },
        None => ServerPacket::Tbc(TbcPacket::CastFailed {
            spell_id,
            result,
            cast_count,
            arg,
        }),
    })
}

/// Read `SMSG_SPELL_COOLDOWN` (2.4.3): a raw guid, a `u8` of flags (bit 0 starts the global
/// cooldown too, dropped), then `(spell, ms)` pairs to the end.
pub(super) fn read_spell_cooldown(r: &mut &[u8]) -> io::Result<(u64, Vec<(u32, u32)>)> {
    let caster = read_u64_le(r)?;
    let _flags = read_u8(r)?;
    let mut cooldowns = Vec::new();
    while !r.is_empty() {
        cooldowns.push((read_u32_le(r)?, read_u32_le(r)?));
    }
    Ok((caster, cooldowns))
}

/// Read `MSG_CHANNEL_START` (2.4.3, sent to everyone in range): packed caster, spell, duration ms.
pub(super) fn read_channel_start(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::ChannelStart {
        caster: read_packed_guid(r)?,
        spell_id: read_u32_le(r)?,
        duration_ms: read_u32_le(r)?,
    })
}

/// Read `MSG_CHANNEL_UPDATE` (2.4.3): packed caster, ms left (0 ends the channel).
pub(super) fn read_channel_update(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::ChannelUpdate {
        caster: read_packed_guid(r)?,
        remaining_ms: read_u32_le(r)?,
    })
}
