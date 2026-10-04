//! The update-field indices of one client build. Indices differ completely between builds, so
//! every accessor on `ObjectFields` reads its index from the build's table, never as a constant.
//! An array field stores its base index only; the slot arithmetic stays at the use site.

use benilla_build::ClientBuild;

use super::super::movement::ObjectType;

const ABSENT: u16 = FieldTable::ABSENT;

macro_rules! field_table {
    ($($(#[$doc:meta])* $name:ident,)*) => {
        /// The descriptor indices of one build, `u16` each.
        #[derive(Debug, PartialEq, Eq)]
        pub struct FieldTable {
            $($(#[$doc])* pub $name: u16,)*
        }

        impl FieldTable {
            /// The members this build has no checked counterpart of, by name.
            pub fn absent_members(&self) -> Vec<&'static str> {
                let mut absent = Vec::new();
                $(if self.$name == Self::ABSENT {
                    absent.push(stringify!($name));
                })*
                absent
            }

            /// Every member as `(name, index)`, for the table's own tests.
            #[cfg(test)]
            pub(super) fn entries(&self) -> Vec<(&'static str, u16)> {
                vec![$((stringify!($name), self.$name),)*]
            }
        }
    };
}

field_table! {
    /// Descriptor field indices for build 5875.
    object_type,
    object_scale_x,
    /// The creator of a spell-spawned object (bobber, ritual portal); 0 for a world spawn.
    gameobject_created_by,
    gameobject_displayid,
    gameobject_flags,
    /// The spawn's rotation quaternion (x, y, z, w), four floats.
    gameobject_rotation,
    gameobject_state,
    gameobject_pos_x,
    gameobject_pos_y,
    gameobject_pos_z,
    gameobject_facing,
    /// The client reads it at GameObject block byte 0x34. vmangos `UpdateFields_1_5_1.h` adds a
    /// `GAMEOBJECT_TIMESTAMP` that would make it 20, but that is not the 5875 layout.
    gameobject_dyn_flags,
    /// The client reads it at `[go+0x110]+0x38`, between DYN_FLAGS and TYPE_ID.
    gameobject_faction,
    gameobject_type_id,
    /// OBJECT_END + 0x10 (vmangos `UpdateFields_1_12_1.h:317`).
    gameobject_level,
    /// `CORPSE_FIELD_DYNAMIC_FLAGS`; bit 0 is lootable insignia. On a unit, index 36 is
    /// `UNIT_FIELD_BYTES_0`, so a field edge carries its object class.
    corpse_dynamic_flags,
    /// UNIT fields. The client reads FLAGS, COMBATREACH, DYNAMIC_FLAGS and NPC_FLAGS at unit block
    /// bytes 0xa0, 0x1f0, 0x224 and 0x234.
    /// The unit's target; the client turns an idle unit to face it, and no packet carries that facing.
    unit_target,
    /// The unit this one charms (`UpdateFields_1_12_1.h:41`), descriptor byte 0.
    unit_charm,
    /// The unit this one summoned (`UpdateFields_1_12_1.h:42`); on us, the `"pet"` unit. The pet bar
    /// reads its guid off `SMSG_PET_SPELLS`, so the two can disagree briefly around a summon.
    unit_summon,
    /// The charmer, 0 for none. The attack-start check `0x612df0` refuses a swing when it is set and
    /// is not us (`ERR_ATTACK_CHARMED`); `0x5ee5a0` and `0x5ff580` prefer it as the owner.
    unit_charmedby,
    /// The summoner of a pet, guardian or totem; with CREATEDBY, the "owned by me" test of `0x5efea0`.
    unit_summonedby,
    unit_createdby,
    /// The channel's target, possibly the caster (`UpdateFields_1_12_1.h:48`). Public, so another
    /// unit's channel renders from it; `MSG_CHANNEL_START`/`UPDATE` reach only the caster.
    unit_channel_object,
    unit_health,
    /// Five power slots: mana, rage, focus, energy, happiness; `MAXPOWER1..5` follow `MAXHEALTH`.
    unit_power1,
    /// `UNIT_FIELD_POWER2` (rage), the second of the five power slots.
    unit_power2,
    /// `UNIT_FIELD_POWER5` (happiness), the fifth power slot.
    unit_power5,
    unit_maxhealth,
    unit_maxpower1,
    /// `UNIT_FIELD_MAXPOWER2`, the maximum of the second power slot.
    unit_maxpower2,
    unit_level,
    unit_factiontemplate,
    unit_bytes_0,
    unit_bytes_1,
    unit_flags,
    /// The aura block: four public parallel arrays (`UpdateFields_1_12_1.h:67-70`), which the client
    /// reads at unit block bytes 0xa4 and 0x164 onward. Duration reaches only the aura's own target,
    /// over `SMSG_UPDATE_AURA_DURATION`, and no packet carries the caster.
    unit_aura,
    /// Nibble-packed, 8 slots per `u32` (`SpellAuraHolder::SetAuraFlag`, `SpellAuras.cpp:7456-7462`).
    unit_auraflags,
    /// Byte-packed, 4 slots per `u32`: the caster's level (`SetAuraLevel`, `SpellAuras.cpp:7484`).
    unit_auralevels,
    /// Byte-packed, 4 slots per `u32`, holding `stack - 1` (`SpellAuras.cpp:7500-7507`).
    unit_auraapplications,
    /// Aura-state bits, tested as `1 << (state - 1)` by the usable check (client `[unit+0x110]+0x1dc`).
    unit_aurastate,
    /// Horizontal bounding radius, in yards.
    unit_boundingradius,
    unit_combatreach,
    unit_base_mana,
    unit_base_health,
    unit_displayid,
    /// The unshifted appearance, untouched by forms, morphs and polymorph (`UpdateFields_1_12_1.h:77`).
    /// The client sizes the mover collision box from it (`0x60b270`), so a shapeshift keeps the box.
    unit_nativedisplayid,
    /// The ridden mount's `CreatureDisplayInfo` id, 0 when unmounted; this field, not the aura, is the
    /// mounted state (`Unit::IsMounted`; client `[unit+0x110]+0x1fc`).
    unit_mountdisplayid,
    /// Nonzero on a pet or charm; the rank getter `0x605620` then forces rank 0, so an enslaved mob
    /// shows no elite dragon, tooltip rank word or boss skull.
    unit_petnumber,
    /// Unix time of the pet's last rename (`Pet.cpp:285`). The name itself comes from
    /// `CMSG_PET_NAME_QUERY`, cached by pet number, so a change here invalidates that cache.
    unit_pet_name_timestamp,
    /// With the next field, the `(currXP, nextXP)` pair `GetPetExperience` (`0x4be840`) returns; the
    /// client reads both unsigned.
    unit_petexperience,
    unit_petnextlevelexp,
    /// Two `u16`s: `GetPetTrainingPoints` (`0x4be790`) returns the high word first, which
    /// `PetPaperDollFrame.lua` names `totalPoints`, then the low word as `spent`.
    unit_training_points,
    unit_dynamic_flags,
    /// The spell being channeled, 0 for none (`UpdateFields_1_12_1.h:89`).
    unit_channel_spell,
    /// The summoning spell, 0 for none; the first gate of the client's feed-pet path `0x6ea1e0`.
    unit_created_by_spell,
    unit_npc_flags,
    /// The looping state emote, an `Emotes.dbc` id or 0; on a player, the server's echo of a
    /// state-class emote (`ChatHandler.cpp:738`).
    unit_npc_emotestate,
    /// A creature's weapon display ids (main hand, off hand, ranged), with no item behind them
    /// (`Creature.cpp:4158-4181`).
    unit_virtual_item_slot_display,
    /// Two dwords per weapon slot: class, subclass, material and inventory type bytes, then the
    /// sheath byte (`CreatureDefines.h:621-628`).
    unit_virtual_item_info,
    /// Byte 0 is the sheath state (`UnitDefines.h:93`); a player's comes only from `CMSG_SETSHEATHED`.
    /// Not `FIELD_PLAYER_BYTES_2`.
    unit_bytes_2,
    /// UNIT combat and stat block: offset from OBJECT_END, then the wire type.
    /// OBJECT_END+0x78; 2 slots [main, offhand], ms, INT
    unit_baseattacktime,
    /// +0x7A, INT
    unit_rangedattacktime,
    /// +0x80, FLOAT
    unit_mindamage,
    /// +0x81, FLOAT
    unit_maxdamage,
    /// +0x82, FLOAT
    unit_minoffhanddamage,
    /// +0x83, FLOAT
    unit_maxoffhanddamage,
    /// +0x90 ×5, INT
    unit_stat0,
    /// +0x95 ×7, INT; [0] = armor
    unit_resistances,
    /// +0x9F, INT
    unit_attack_power,
    /// +0xA0, `TWO_SHORT`: signed halves, the low positive, the high already negative or zero
    /// (`StatSystem.cpp:335-336`).
    unit_attack_power_mods,
    /// +0xA1, FLOAT: multiplier − 1.0
    unit_attack_power_multiplier,
    /// +0xA2, INT
    unit_ranged_attack_power,
    /// +0xA3, TWO_SHORT as above
    unit_ranged_attack_power_mods,
    /// +0xA4, FLOAT
    unit_ranged_attack_power_multiplier,
    /// +0xA5, FLOAT
    unit_minrangeddamage,
    /// +0xA6, FLOAT
    unit_maxrangeddamage,
    /// PLAYER fields start at UNIT_END (188); the client decodes appearance bytes at `0x5fb200`.
    player_bytes,
    player_bytes_2,
    /// The low u16 is `gender | (drunk & 0xFFFE)`, so byte 1 is the drunk level the client reads at
    /// `[[unit+0xe68]+0x1d]`; byte 2 is the city-protector race, byte 3 the current honor rank
    /// (`Player.h:351-356`). Public: the one honor value that streams for every visible player.
    player_bytes_3,
    /// 20 slots of 3 fields (`Player.h:439-444`): the quest id (group-only), then the counters and
    /// state byte, and the timer (both private).
    player_quest_log_1_1,
    /// Inventory fields. The PLAYER slot arrays are private, and each slot is a 2-field guid.
    /// OBJECT_END + 0x8
    item_stack_count,
    /// OBJECT_END + 0x10; 7 slots × 3 (id, duration, charges)
    item_enchantment,
    /// ITEM_END = 6 + 0x2A
    container_num_slots,
    /// ITEM_END + 0x2; 36 slots × 2
    container_slot_1,
    /// From the CONTAINER block on, the hex comments in vmangos `UpdateFields_1_12_1.h` run 6 low. The
    /// indices here follow its enum arithmetic, which the server compiles: INV_SLOT_HEAD = 188 + 0x12A.
    /// UNIT_END + 0x46; 12 fields per slot
    player_visible_item_1_creator,
    /// 23 slots × 2 (equipment 0–18, bags 19–22)
    player_inv_slot_head,
    /// 16 slots × 2 (the backpack)
    player_pack_slot_1,
    /// 24 slots × 2
    player_bank_slot_1,
    /// 564 + 24×2; 6 bag slots × 2 (item guids)
    player_bank_bag_slot_1,
    /// 12 slots × 2 (item guids)
    player_vendorbuyback_slot_1,
    /// 32 slots × 2 (item guids), wire slots 81–112
    player_keyring_slot_1,
    /// our view's anchor (Mind Vision, Sentry Totem), or 0
    player_farsight,
    /// 12 × u32 copper, indexed slot−69
    player_buyback_price_1,
    /// 12 × u32, the client's sort key only
    player_buyback_timestamp_1,
    /// copper
    player_field_coinage,
    player_xp,
    player_next_level_xp,
    /// The watched reputation slot, signed: slot 0 is a real faction, so only -1 means none.
    player_watched_faction_index,
    /// The rested pool in base kill-XP units: a kill drains it 1:1 while granting +100%
    /// (`Player::GetXPRestBonus`), so the doubled span on the XP bar is twice this value.
    player_rest_state_experience,
    /// EXPLORED_ZONES_1 is the discovery bitset, 2048 bits indexed by `AreaTable.dbc` exploreFlag. Just
    /// below it sit block, dodge, parry and crit chance (FLOAT, UNIT_END + 0x396..0x399).
    player_block_percentage,
    player_dodge_percentage,
    player_parry_percentage,
    player_crit_percentage,
    player_explored_zones_1,
    /// PLAYER stat block. POSSTAT, NEGSTAT and the resistance buff mods are floats in vmangos but go out
    /// as INT (`Object::BuildValuesUpdate` narrows them). Read them signed: the server's cast of a
    /// negative float wraps on x86 and saturates to 0 on aarch64.
    /// UNIT_END+0x3DD ×5, INT
    player_posstat0,
    /// ×5, INT; negative-or-zero where the wire can carry it
    player_negstat0,
    /// ×7, INT
    player_resistancebuffmodspositive,
    /// ×7, INT; negative-or-zero
    player_resistancebuffmodsnegative,
    /// ×7 schools ([0] physical), INT
    player_mod_damage_done_pos,
    /// ×7, INT; negative-or-zero
    player_mod_damage_done_neg,
    /// ×7, FLOAT (header says INT), default 1.0
    player_mod_damage_done_pct,
    /// ×384: 128 skills × 3 dwords
    player_skill_info_1_1,
    /// Unspent talent points and free primary professions, `UnitCharacterPoints("player")`.
    player_character_points1,
    player_character_points2,
    /// Tracking masks the minimap tests: bit `1 << (n - 1)` for creature type or `LockType.dbc` id `n`,
    /// one bit per active tracking aura.
    player_track_creatures,
    player_track_resources,
    /// UNIT_END+0x40B, INT: the equipped ammo item id
    player_ammo_id,
    player_self_res_spell,
    /// Bit 0x10 is PLAYER_FLAGS_GHOST (`Player.h:319`), held by the ghost aura 8326 until resurrection.
    player_flags,
    /// The client's player block base `[player+0xe68]` is field 188: the arbiter at +0x0, PLAYER_FLAGS
    /// at +0x8 and the duel team at +0x20, the three reads of `UnitReaction`'s duel leg (`0x6061e0`).
    player_duel_arbiter,
    player_duel_team,
    /// Public, so `GetGuildInfo(unit)` (`0x4c9330`, reading block +0xc and +0x10) answers for any
    /// visible player; a guild id of 0 is guildless.
    player_guildid,
    player_guildrank,
    /// `GetComboPoints` (`0x51a190`) reads the target at block +0x838 and the count byte at +0x1029.
    player_field_combo_target,
    /// UNIT_END+0x40A: flags, combo points, action bars,
    player_field_bytes,
    /// The honor block (`UpdateFields_1_12_1.h:288-298`) is private: another player's honor needs
    /// `MSG_INSPECT_HONOR_STATS`. The four kill counters are TWO_SHORT (honorable, dishonorable), but
    /// vmangos writes all except SESSION_KILLS as a whole dword. LAST_WEEK_RANK is the weekly standing,
    /// not a rank. BYTES2 byte 0 is the rank bar (`Player.h:366-372`).
    player_field_session_kills,
    player_field_yesterday_kills,
    player_field_last_week_kills,
    player_field_this_week_kills,
    player_field_this_week_contribution,
    player_field_lifetime_honorable_kills,
    player_field_lifetime_dishonorable_kills,
    player_field_yesterday_contribution,
    player_field_last_week_contribution,
    player_field_last_week_rank,
    player_field_bytes2,
    /// `OBJECT_FIELD_ENTRY`: the template entry.
    object_entry,
    /// `ITEM_FIELD_SPELL_CHARGES`; 5 slots.
    item_spell_charges,
    /// `ITEM_FIELD_CREATOR`.
    item_creator,
    /// `ITEM_FIELD_FLAGS`.
    item_flags,
    /// `ITEM_FIELD_RANDOM_PROPERTIES_ID`.
    item_random_properties_id,
    /// `ITEM_FIELD_ITEM_TEXT_ID`.
    item_text_id,
    /// `ITEM_FIELD_DURABILITY`.
    item_durability,
    /// `ITEM_FIELD_MAXDURABILITY`.
    item_max_durability,
    /// `CORPSE_FIELD_OWNER`, a guid.
    corpse_owner,
    /// `CORPSE_FIELD_DISPLAY_ID`.
    corpse_display_id,
    /// `CORPSE_FIELD_ITEM`; 19 slots.
    corpse_item,
    /// `CORPSE_FIELD_BYTES_1`.
    corpse_bytes_1,
    /// `CORPSE_FIELD_BYTES_2`.
    corpse_bytes_2,
    /// `CORPSE_FIELD_GUILD`.
    corpse_guild,
    /// `CORPSE_FIELD_FLAGS`.
    corpse_flags,
    /// `DYNAMICOBJECT_CASTER`, a guid.
    dynamicobject_caster,
    /// `DYNAMICOBJECT_BYTES`.
    dynamicobject_bytes,
    /// `DYNAMICOBJECT_SPELLID`.
    dynamicobject_spell_id,
    /// `DYNAMICOBJECT_RADIUS`, a float.
    dynamicobject_radius,
    /// `DYNAMICOBJECT_POS_X`; Y and Z follow.
    dynamicobject_pos_x,
    /// `DYNAMICOBJECT_FACING`.
    dynamicobject_facing,
    /// `OBJECT_END`: the descriptor length of an object.
    object_end,
    /// `ITEM_END`.
    item_end,
    /// `CONTAINER_END`.
    container_end,
    /// `UNIT_END`.
    unit_end,
    /// `PLAYER_END`, the widest descriptor.
    player_end,
    /// `GAMEOBJECT_END`.
    gameobject_end,
    /// `DYNAMICOBJECT_END`.
    dynamicobject_end,
    /// `CORPSE_END`.
    corpse_end,
}

/// 1.12.1 (5875).
pub const FIELDS_5875: FieldTable = FieldTable {
    object_type: 2,
    object_scale_x: 4,
    gameobject_created_by: 6,
    gameobject_displayid: 8,
    gameobject_flags: 9,
    gameobject_rotation: 10,
    gameobject_state: 14,
    gameobject_pos_x: 15,
    gameobject_pos_y: 16,
    gameobject_pos_z: 17,
    gameobject_facing: 18,
    gameobject_dyn_flags: 19,
    gameobject_faction: 20,
    gameobject_type_id: 21,
    gameobject_level: 22,
    corpse_dynamic_flags: 36,
    unit_target: 16,
    unit_charm: 6,
    unit_summon: 8,
    unit_charmedby: 10,
    unit_summonedby: 12,
    unit_createdby: 14,
    unit_channel_object: 20,
    unit_health: 22,
    unit_power1: 23,
    unit_power2: 24,
    unit_power5: 27,
    unit_maxhealth: 28,
    unit_maxpower1: 29,
    unit_maxpower2: 30,
    unit_level: 34,
    unit_factiontemplate: 35,
    unit_bytes_0: 36,
    unit_bytes_1: 138,
    unit_flags: 46,
    unit_aura: 47,
    unit_auraflags: 95,
    unit_auralevels: 101,
    unit_auraapplications: 113,
    unit_aurastate: 125,
    unit_boundingradius: 129,
    unit_combatreach: 130,
    unit_base_mana: 162,
    unit_base_health: 163,
    unit_displayid: 131,
    unit_nativedisplayid: 132,
    unit_mountdisplayid: 133,
    unit_petnumber: 139,
    unit_pet_name_timestamp: 140,
    unit_petexperience: 141,
    unit_petnextlevelexp: 142,
    unit_training_points: 149,
    unit_dynamic_flags: 143,
    unit_channel_spell: 144,
    unit_created_by_spell: 146,
    unit_npc_flags: 147,
    unit_npc_emotestate: 148,
    unit_virtual_item_slot_display: 37,
    unit_virtual_item_info: 40,
    unit_bytes_2: 164,
    unit_baseattacktime: 126,
    unit_rangedattacktime: 128,
    unit_mindamage: 134,
    unit_maxdamage: 135,
    unit_minoffhanddamage: 136,
    unit_maxoffhanddamage: 137,
    unit_stat0: 150,
    unit_resistances: 155,
    unit_attack_power: 165,
    unit_attack_power_mods: 166,
    unit_attack_power_multiplier: 167,
    unit_ranged_attack_power: 168,
    unit_ranged_attack_power_mods: 169,
    unit_ranged_attack_power_multiplier: 170,
    unit_minrangeddamage: 171,
    unit_maxrangeddamage: 172,
    player_bytes: 193,
    player_bytes_2: 194,
    player_bytes_3: 195,
    player_quest_log_1_1: 198,
    item_stack_count: 14,
    item_enchantment: 22,
    container_num_slots: 48,
    container_slot_1: 50,
    player_visible_item_1_creator: 258,
    player_inv_slot_head: 486,
    player_pack_slot_1: 532,
    player_bank_slot_1: 564,
    player_bank_bag_slot_1: 612,
    player_vendorbuyback_slot_1: 624,
    player_keyring_slot_1: 648,
    player_farsight: 712,
    player_buyback_price_1: 1226,
    player_buyback_timestamp_1: 1238,
    player_field_coinage: 1176,
    player_xp: 716,
    player_next_level_xp: 717,
    player_watched_faction_index: 1261,
    player_rest_state_experience: 1175,
    player_block_percentage: 1106,
    player_dodge_percentage: 1107,
    player_parry_percentage: 1108,
    player_crit_percentage: 1109,
    player_explored_zones_1: 1111,
    player_posstat0: 1177,
    player_negstat0: 1182,
    player_resistancebuffmodspositive: 1187,
    player_resistancebuffmodsnegative: 1194,
    player_mod_damage_done_pos: 1201,
    player_mod_damage_done_neg: 1208,
    player_mod_damage_done_pct: 1215,
    player_skill_info_1_1: 718,
    player_character_points1: 1102,
    player_character_points2: 1103,
    player_track_creatures: 1104,
    player_track_resources: 1105,
    player_ammo_id: 1223,
    player_self_res_spell: 1224,
    player_flags: 190,
    player_duel_arbiter: 188,
    player_duel_team: 196,
    player_guildid: 191,
    player_guildrank: 192,
    player_field_combo_target: 714,
    player_field_bytes: 1222,
    player_field_session_kills: 1250,
    player_field_yesterday_kills: 1251,
    player_field_last_week_kills: 1252,
    player_field_this_week_kills: 1253,
    player_field_this_week_contribution: 1254,
    player_field_lifetime_honorable_kills: 1255,
    player_field_lifetime_dishonorable_kills: 1256,
    player_field_yesterday_contribution: 1257,
    player_field_last_week_contribution: 1258,
    player_field_last_week_rank: 1259,
    player_field_bytes2: 1260,
    object_entry: 3,
    item_spell_charges: 16,
    item_creator: 10,
    item_flags: 21,
    item_random_properties_id: 44,
    item_text_id: 45,
    item_durability: 46,
    item_max_durability: 47,
    corpse_owner: 6,
    corpse_display_id: 12,
    corpse_item: 13,
    corpse_bytes_1: 32,
    corpse_bytes_2: 33,
    corpse_guild: 34,
    corpse_flags: 35,
    dynamicobject_caster: 6,
    dynamicobject_bytes: 8,
    dynamicobject_spell_id: 9,
    dynamicobject_radius: 10,
    dynamicobject_pos_x: 11,
    dynamicobject_facing: 14,
    object_end: 6,
    item_end: 48,
    container_end: 122,
    unit_end: 188,
    player_end: 1282,
    gameobject_end: 26,
    dynamicobject_end: 16,
    corpse_end: 38,
};

/// 2.4.3 (8606), from cmangos-tbc's `UpdateFields.h` for that build (one source; each packing and
/// length named below was read in its code). A member is filled only when its accessors read the
/// right answer on 2.4.3 as written: same meaning, packing, array length and stride. Every other
/// member is [`FieldTable::ABSENT`], with the difference named above it.
pub const FIELDS_8606: FieldTable = FieldTable {
    object_type: 2,
    object_scale_x: 4,
    gameobject_created_by: 6,
    gameobject_displayid: 8,
    gameobject_flags: 9,
    gameobject_rotation: 10,
    gameobject_state: 14,
    gameobject_pos_x: 15,
    gameobject_pos_y: 16,
    gameobject_pos_z: 17,
    gameobject_facing: 18,
    gameobject_dyn_flags: 19,
    gameobject_faction: 20,
    gameobject_type_id: 21,
    gameobject_level: 22,
    corpse_dynamic_flags: 38,
    unit_target: 16,
    unit_charm: 6,
    unit_summon: 8,
    unit_charmedby: 10,
    unit_summonedby: 12,
    unit_createdby: 14,
    unit_channel_object: 20,
    unit_health: 22,
    unit_power1: 23,
    unit_power2: 24,
    unit_power5: 27,
    unit_maxhealth: 28,
    unit_maxpower1: 29,
    unit_maxpower2: 30,
    unit_level: 34,
    unit_factiontemplate: 35,
    unit_bytes_0: 36,
    // byte 2 is the vis flags and byte 3 the misc flags, where the accessors read stealth, ghost
    // and untrackable from byte 3 and the shapeshift form from byte 2 (the 2.4.3 form is BYTES_2
    // byte 3)
    unit_bytes_1: ABSENT,
    unit_flags: 46,
    // 56 slots against the 48 the accessors assume
    unit_aura: ABSENT,
    // byte-packed over 14 dwords against 1.12.1's 8-slot nibbles over 6
    unit_auraflags: ABSENT,
    // 56 byte-packed slots over 14 dwords against 48 over 12
    unit_auralevels: ABSENT,
    // 56 byte-packed slots over 14 dwords against 48 over 12
    unit_auraapplications: ABSENT,
    unit_aurastate: 146,
    unit_boundingradius: 150,
    unit_combatreach: 151,
    unit_base_mana: 207,
    unit_base_health: 208,
    unit_displayid: 152,
    unit_nativedisplayid: 153,
    unit_mountdisplayid: 154,
    unit_petnumber: 160,
    unit_pet_name_timestamp: 161,
    unit_petexperience: 162,
    unit_petnextlevelexp: 163,
    // the packing is not checked: the server stores a signed display value (`GetDispTP`) in a
    // TWO_SHORT field
    unit_training_points: ABSENT,
    unit_dynamic_flags: 164,
    unit_channel_spell: 165,
    unit_created_by_spell: 167,
    unit_npc_flags: 168,
    unit_npc_emotestate: 169,
    unit_virtual_item_slot_display: 37,
    // byte layout differs: class, subclass, unknown, material in the first dword, then inventory
    // type and sheath in the second (1.12.1: class, subclass, material, inventory type, then
    // sheath)
    unit_virtual_item_info: ABSENT,
    unit_bytes_2: 209,
    unit_baseattacktime: 147,
    unit_rangedattacktime: 149,
    unit_mindamage: 155,
    unit_maxdamage: 156,
    unit_minoffhanddamage: 157,
    unit_maxoffhanddamage: 158,
    unit_stat0: 171,
    unit_resistances: 186,
    unit_attack_power: 210,
    unit_attack_power_mods: 211,
    unit_attack_power_multiplier: 212,
    unit_ranged_attack_power: 213,
    unit_ranged_attack_power_mods: 214,
    unit_ranged_attack_power_multiplier: 215,
    unit_minrangeddamage: 216,
    unit_maxrangeddamage: 217,
    player_bytes: 239,
    player_bytes_2: 240,
    // byte 3 is the arena faction and byte 2 is unused, where the accessors read the honor rank
    // and the city-protector medal
    player_bytes_3: ABSENT,
    // 25 slots of 4 fields against 20 of 3
    player_quest_log_1_1: ABSENT,
    item_stack_count: 14,
    // 11 slots of 3 against 7
    item_enchantment: ABSENT,
    container_num_slots: 60,
    container_slot_1: 62,
    // 16 fields per slot against 12
    player_visible_item_1_creator: ABSENT,
    player_inv_slot_head: 650,
    player_pack_slot_1: 696,
    // 28 slots against 24
    player_bank_slot_1: ABSENT,
    // 7 slots against 6
    player_bank_bag_slot_1: ABSENT,
    player_vendorbuyback_slot_1: 798,
    player_keyring_slot_1: 822,
    player_farsight: 922,
    player_buyback_price_1: 1490,
    player_buyback_timestamp_1: 1502,
    player_field_coinage: 1461,
    player_xp: 926,
    player_next_level_xp: 927,
    player_watched_faction_index: 1519,
    player_rest_state_experience: 1460,
    player_block_percentage: 1316,
    player_dodge_percentage: 1317,
    player_parry_percentage: 1318,
    player_crit_percentage: 1321,
    // 128 slots against 64
    player_explored_zones_1: ABSENT,
    player_posstat0: 176,
    player_negstat0: 181,
    player_resistancebuffmodspositive: 193,
    player_resistancebuffmodsnegative: 200,
    player_mod_damage_done_pos: 1462,
    player_mod_damage_done_neg: 1469,
    player_mod_damage_done_pct: 1476,
    player_skill_info_1_1: 928,
    player_character_points1: 1312,
    player_character_points2: 1313,
    player_track_creatures: 1314,
    player_track_resources: 1315,
    player_ammo_id: 1487,
    player_self_res_spell: 1488,
    player_flags: 236,
    player_duel_arbiter: 234,
    player_duel_team: 242,
    player_guildid: 237,
    player_guildrank: 238,
    // no such field: combo points and their target are sent by `SMSG_UPDATE_COMBO_POINTS`
    player_field_combo_target: ABSENT,
    // byte 1 is the refer-a-friend grantable level and byte 3 the lifetime top PvP rank, where the
    // accessors read combo points from byte 1
    player_field_bytes: ABSENT,
    // honor block replaced: one `PLAYER_FIELD_KILLS` pair, today and yesterday contribution, honor
    // and arena currency, no weekly or standing fields
    player_field_session_kills: ABSENT,
    // honor block replaced (see session kills)
    player_field_yesterday_kills: ABSENT,
    // honor block replaced (see session kills)
    player_field_last_week_kills: ABSENT,
    // honor block replaced (see session kills)
    player_field_this_week_kills: ABSENT,
    // honor block replaced (see session kills)
    player_field_this_week_contribution: ABSENT,
    // honor block replaced (see session kills)
    player_field_lifetime_honorable_kills: ABSENT,
    // honor block replaced: no lifetime dishonorable counter
    player_field_lifetime_dishonorable_kills: ABSENT,
    // honor block replaced (see session kills)
    player_field_yesterday_contribution: ABSENT,
    // honor block replaced: no last-week contribution
    player_field_last_week_contribution: ABSENT,
    // honor block replaced: no weekly standing
    player_field_last_week_rank: ABSENT,
    // byte 0 is no longer the honor rank bar; the 2.4.3 byte flags (stealth, invisibility glow)
    // sit in byte 1
    player_field_bytes2: ABSENT,
    object_entry: 3,
    item_spell_charges: 16,
    item_creator: 10,
    item_flags: 21,
    item_random_properties_id: 56,
    item_text_id: 57,
    item_durability: 58,
    item_max_durability: 59,
    corpse_owner: 6,
    corpse_display_id: 14,
    corpse_item: 15,
    corpse_bytes_1: 34,
    corpse_bytes_2: 35,
    corpse_guild: 36,
    corpse_flags: 37,
    dynamicobject_caster: 6,
    dynamicobject_bytes: 8,
    dynamicobject_spell_id: 9,
    dynamicobject_radius: 10,
    dynamicobject_pos_x: 11,
    dynamicobject_facing: 14,
    object_end: 6,
    item_end: 60,
    container_end: 134,
    unit_end: 234,
    player_end: 1592,
    gameobject_end: 26,
    dynamicobject_end: 16,
    corpse_end: 40,
};

/// The table of `build`; a build without one is a programming error, not a runtime condition.
pub fn field_table(build: &ClientBuild) -> &'static FieldTable {
    match build_field_table(build) {
        Some(table) => table,
        None => panic!("no update-field table for build {}", build.build),
    }
}

/// The table of `build`, `None` for a known build that has none; a build number that names no
/// known build is a programming error and panics.
pub fn build_field_table(build: &ClientBuild) -> Option<&'static FieldTable> {
    match build.build {
        5875 => Some(&FIELDS_5875),
        8606 => Some(&FIELDS_8606),
        other if ClientBuild::from_build(other).is_some() => None,
        other => panic!("no update-field table for build {other}"),
    }
}

/// The widest descriptor of any build's table, in dwords, which bounds a values block's mask.
pub(super) const MAX_PLAYER_END: u16 = if FIELDS_5875.player_end > FIELDS_8606.player_end {
    FIELDS_5875.player_end
} else {
    FIELDS_8606.player_end
};

impl FieldTable {
    /// A member the build has no counterpart of, or whose layout there is not checked yet; every
    /// reader answers `None` for it.
    pub const ABSENT: u16 = u16::MAX;

    /// An object's descriptor length in dwords: the `*_END` of its innermost block, so a Player
    /// spans OBJECT, UNIT and PLAYER.
    pub(super) fn descriptor_len(&self, object_type: ObjectType) -> u16 {
        match object_type {
            ObjectType::Object => self.object_end,
            ObjectType::Item => self.item_end,
            ObjectType::Container => self.container_end,
            ObjectType::Unit => self.unit_end,
            ObjectType::Player => self.player_end,
            ObjectType::GameObject => self.gameobject_end,
            ObjectType::DynamicObject => self.dynamicobject_end,
            ObjectType::Corpse => self.corpse_end,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use benilla_build::VANILLA_1_12_1;

    use super::super::{at, ObjectFields};
    use super::*;

    #[test]
    fn the_vanilla_build_reads_the_5875_table() {
        assert_eq!(field_table(&VANILLA_1_12_1), &FIELDS_5875);
    }

    #[test]
    #[should_panic(expected = "no update-field table for build 12340")]
    fn a_build_without_a_table_is_refused() {
        let wotlk = ClientBuild {
            build: 12340,
            ..VANILLA_1_12_1
        };
        field_table(&wotlk);
    }

    // Two fields of one object block never share an index; across blocks they do (a corpse's
    // owner is a dynamic object's caster), so each block is checked on its own.
    #[test]
    fn no_two_fields_of_a_block_share_an_index() {
        let mut seen: HashMap<(&str, u16), &str> = HashMap::new();
        for (name, index) in FIELDS_5875.entries() {
            let block = name.split('_').next().unwrap();
            if let Some(other) = seen.insert((block, index), name) {
                panic!("{other} and {name} share index {index}");
            }
        }
    }

    #[test]
    fn descriptor_lengths_are_distinct() {
        let t = &FIELDS_5875;
        let mut lens = [
            ObjectType::Object,
            ObjectType::Item,
            ObjectType::Container,
            ObjectType::Unit,
            ObjectType::Player,
            ObjectType::GameObject,
            ObjectType::DynamicObject,
            ObjectType::Corpse,
        ]
        .map(|ty| t.descriptor_len(ty));
        lens.sort_unstable();
        assert!(lens.windows(2).all(|w| w[0] != w[1]));
        assert_eq!(t.descriptor_len(ObjectType::Player), 1282);
    }

    // ---- 2.4.3 (8606) ----

    /// The block a member's name prefix names; a member is checked against that block's end.
    fn block_of(name: &str) -> (&'static str, fn(&FieldTable) -> u16) {
        match name.split('_').next().unwrap() {
            "object" => ("object", |t| t.object_end),
            "item" => ("item", |t| t.item_end),
            "container" => ("container", |t| t.container_end),
            "unit" => ("unit", |t| t.unit_end),
            "player" => ("player", |t| t.player_end),
            "gameobject" => ("gameobject", |t| t.gameobject_end),
            "dynamicobject" => ("dynamicobject", |t| t.dynamicobject_end),
            "corpse" => ("corpse", |t| t.corpse_end),
            other => panic!("member {name} belongs to no block ({other})"),
        }
    }

    #[test]
    fn the_tbc_build_reads_the_8606_table() {
        assert_eq!(field_table(&benilla_build::TBC_2_4_3), &FIELDS_8606);
        assert_eq!(
            build_field_table(&benilla_build::TBC_2_4_3),
            Some(&FIELDS_8606)
        );
    }

    // No two filled members of one block share an index; an absent member makes no claim.
    #[test]
    fn no_two_filled_members_of_an_8606_block_share_an_index() {
        let mut seen: HashMap<(&str, u16), &str> = HashMap::new();
        for (name, index) in FIELDS_8606.entries() {
            if index == FieldTable::ABSENT || name.ends_with("_end") {
                continue;
            }
            let (block, _) = block_of(name);
            if let Some(other) = seen.insert((block, index), name) {
                panic!("{other} and {name} share index {index}");
            }
        }
    }

    // A filled index lies inside its own block's descriptor.
    #[test]
    fn every_filled_8606_index_is_inside_its_blocks_descriptor() {
        for (name, index) in FIELDS_8606.entries() {
            if index == FieldTable::ABSENT || name.ends_with("_end") {
                continue;
            }
            let (_, end) = block_of(name);
            assert!(
                index < end(&FIELDS_8606),
                "{name} at {index} is past its block's end {}",
                end(&FIELDS_8606)
            );
        }
    }

    // A filled array member fits whole inside its block, at the length its accessors assume.
    #[test]
    fn filled_8606_arrays_fit_their_blocks() {
        let t = &FIELDS_8606;
        let arrays: [(&str, u16, u16, u16); 20] = [
            ("unit_power1", t.unit_power1, 5, t.unit_end),
            ("unit_maxpower1", t.unit_maxpower1, 5, t.unit_end),
            ("unit_stat0", t.unit_stat0, 5, t.unit_end),
            ("unit_resistances", t.unit_resistances, 7, t.unit_end),
            ("unit_baseattacktime", t.unit_baseattacktime, 2, t.unit_end),
            ("player_posstat0", t.player_posstat0, 5, t.unit_end),
            ("player_negstat0", t.player_negstat0, 5, t.unit_end),
            (
                "buff mods +",
                t.player_resistancebuffmodspositive,
                7,
                t.unit_end,
            ),
            (
                "buff mods -",
                t.player_resistancebuffmodsnegative,
                7,
                t.unit_end,
            ),
            ("container_slot_1", t.container_slot_1, 72, t.container_end),
            ("item_spell_charges", t.item_spell_charges, 5, t.item_end),
            ("corpse_item", t.corpse_item, 19, t.corpse_end),
            ("inv_slot_head", t.player_inv_slot_head, 46, t.player_end),
            ("pack_slot_1", t.player_pack_slot_1, 32, t.player_end),
            (
                "buyback_slot",
                t.player_vendorbuyback_slot_1,
                24,
                t.player_end,
            ),
            ("keyring_slot_1", t.player_keyring_slot_1, 64, t.player_end),
            ("buyback_price", t.player_buyback_price_1, 12, t.player_end),
            (
                "buyback_time",
                t.player_buyback_timestamp_1,
                12,
                t.player_end,
            ),
            ("skill_info", t.player_skill_info_1_1, 384, t.player_end),
            (
                "mod_damage_pct",
                t.player_mod_damage_done_pct,
                7,
                t.player_end,
            ),
        ];
        for (name, base, len, end) in arrays {
            assert_ne!(base, FieldTable::ABSENT, "{name} is absent");
            assert!(base + len <= end, "{name}: {base} + {len} > {end}");
        }
    }

    #[test]
    fn the_8606_descriptor_lengths_are_the_2_4_3_totals() {
        let t = &FIELDS_8606;
        let lens = [
            (ObjectType::Object, 6),
            (ObjectType::Item, 60),
            (ObjectType::Container, 134),
            (ObjectType::Unit, 234),
            (ObjectType::Player, 1592),
            (ObjectType::GameObject, 26),
            (ObjectType::DynamicObject, 16),
            (ObjectType::Corpse, 40),
        ];
        for (ty, len) in lens {
            assert_eq!(t.descriptor_len(ty), len, "{ty:?}");
        }
        let mut sorted: Vec<u16> = lens.iter().map(|&(_, l)| l).collect();
        sorted.sort_unstable();
        assert!(sorted.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn the_mask_bound_covers_the_widest_descriptor_of_any_table() {
        assert_eq!(
            MAX_PLAYER_END,
            FIELDS_5875.player_end.max(FIELDS_8606.player_end)
        );
        assert_eq!(MAX_PLAYER_END, 1592);
    }

    // The 2.4.3 positions the live probe leans on, by 2.4.3 name.
    #[test]
    fn the_8606_unit_and_player_positions_are_the_2_4_3_ones() {
        let t = &FIELDS_8606;
        assert_eq!(
            [
                t.unit_health,
                t.unit_power1,
                t.unit_maxhealth,
                t.unit_maxpower1
            ],
            [22, 23, 28, 29]
        );
        assert_eq!(
            [
                t.unit_level,
                t.unit_factiontemplate,
                t.unit_bytes_0,
                t.unit_flags
            ],
            [34, 35, 36, 46]
        );
        assert_eq!([t.unit_displayid, t.unit_nativedisplayid], [152, 153]);
        assert_eq!(
            [t.unit_aurastate, t.unit_base_mana, t.unit_bytes_2],
            [146, 207, 209]
        );
        assert_eq!(
            [t.player_flags, t.player_bytes, t.player_xp],
            [236, 239, 926]
        );
        assert_eq!(
            [t.player_field_coinage, t.player_skill_info_1_1],
            [1461, 928]
        );
    }

    // The members left absent are the structural differences, and 1.12.1 has none.
    #[test]
    fn the_absent_members_of_8606_are_the_checked_structural_differences() {
        let absent: Vec<&str> = FIELDS_8606
            .entries()
            .into_iter()
            .filter(|&(_, i)| i == FieldTable::ABSENT)
            .map(|(n, _)| n)
            .collect();
        assert_eq!(absent.len(), 27, "{absent:?}");
        for name in [
            "unit_aura",
            "unit_auraflags",
            "unit_bytes_1",
            "player_quest_log_1_1",
            "player_visible_item_1_creator",
            "item_enchantment",
            "player_explored_zones_1",
            "player_field_combo_target",
        ] {
            assert!(absent.contains(&name), "{name} should be absent");
        }
        assert!(FIELDS_5875
            .entries()
            .iter()
            .all(|&(_, i)| i != FieldTable::ABSENT));
    }

    #[test]
    fn slot_arithmetic_on_an_absent_base_stays_absent_and_never_overflows() {
        let absent = FieldTable::ABSENT;
        assert_eq!(at(absent, 0), absent);
        assert_eq!(at(absent, 7), absent);
        assert_eq!(at(absent, u16::MAX), absent);
        assert_eq!(at(u16::MAX - 1, 1), absent);
        assert_eq!(at(u16::MAX - 1, 2), absent);
        assert_eq!(at(1, u16::MAX), absent);
        assert_eq!(at(0, u16::MAX), absent);
        assert_eq!(at(10, 5), 15);
        assert_eq!(at(0, 0), 0);
    }

    #[test]
    fn an_absent_member_reads_none_whatever_the_store_holds() {
        // Every index set, so only the ABSENT guard can make a read miss.
        let all: Vec<(u16, u32)> = (0..1592).map(|i| (i, 1)).collect();
        let f = ObjectFields::from_pairs_in(&FIELDS_8606, &all).into_created(ObjectType::Player);
        assert_eq!(f.player_quest_log(0), None);
        assert_eq!(f.unit_aura(0), None);
        assert_eq!(f.player_visible_item_entry(0), None);
        assert_eq!(f.player_visible_item_enchant(0, 0), None);
        assert_eq!(f.player_bank_slot(0), None);
        assert_eq!(f.player_bank_bag_slot(0), None);
        assert_eq!(f.item_enchant(0), None);
        assert_eq!(f.player_combo_points(), None);
        assert_eq!(f.player_pvp_rank(), None);
        assert_eq!(f.player_session_kills(), None);
        assert_eq!(f.unit_virtual_item_info(0), None);
        // A reader that defaults an absent value gives its default, not a neighbour's number.
        assert_eq!(f.unit_aura_ids().max(), Some(0));
        assert_eq!(f.player_explored_zone_slot(0), 0);
        assert_eq!(f.player_combo_target(), 0);
        assert!(!f.unit_is_stealthed());
        assert_eq!(f.unit_shapeshift_form(), 0);
        // A filled member of the same store still reads.
        assert_eq!(f.unit_health(), Some(1));
        assert_eq!(f.player_xp(), Some(1));
    }

    #[test]
    fn an_index_of_u16_max_is_never_a_carried_field() {
        let f = ObjectFields::from_pairs_in(&FIELDS_8606, &[(u16::MAX, 9)]);
        assert_eq!(f.get_raw(FieldTable::ABSENT), None);
        assert!(!f.contains(FieldTable::ABSENT));
        assert_eq!(f.get_u32(FieldTable::ABSENT), None);
        assert_eq!(f.get_guid(FieldTable::ABSENT), None);
    }
}
