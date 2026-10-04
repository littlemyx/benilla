use std::io::{self, Read};

use crate::wire::{capacity_hint, read_u32_le, read_u8, Vector3d};

use super::movement::ObjectType;

// The 1.12.1 table's indices this module's tests read by name.
#[cfg(test)]
const FIELD_UNIT_HEALTH: u16 = FIELDS_5875.unit_health;
#[cfg(test)]
const FIELD_UNIT_POWER1: u16 = FIELDS_5875.unit_power1;
#[cfg(test)]
const FIELD_UNIT_MAXHEALTH: u16 = FIELDS_5875.unit_maxhealth;
#[cfg(test)]
const FIELD_UNIT_MAXPOWER1: u16 = FIELDS_5875.unit_maxpower1;
#[cfg(test)]
const FIELD_UNIT_BYTES_1: u16 = FIELDS_5875.unit_bytes_1;
#[cfg(test)]
const FIELD_UNIT_AURA: u16 = FIELDS_5875.unit_aura;
#[cfg(test)]
const FIELD_UNIT_AURAFLAGS: u16 = FIELDS_5875.unit_auraflags;
#[cfg(test)]
const FIELD_UNIT_AURALEVELS: u16 = FIELDS_5875.unit_auralevels;
#[cfg(test)]
const FIELD_UNIT_AURAAPPLICATIONS: u16 = FIELDS_5875.unit_auraapplications;
#[cfg(test)]
const FIELD_UNIT_DYNAMIC_FLAGS: u16 = FIELDS_5875.unit_dynamic_flags;
#[cfg(test)]
const FIELD_UNIT_BASEATTACKTIME: u16 = FIELDS_5875.unit_baseattacktime;
#[cfg(test)]
const FIELD_UNIT_RANGEDATTACKTIME: u16 = FIELDS_5875.unit_rangedattacktime;
#[cfg(test)]
const FIELD_UNIT_MINDAMAGE: u16 = FIELDS_5875.unit_mindamage;
#[cfg(test)]
const FIELD_UNIT_MAXDAMAGE: u16 = FIELDS_5875.unit_maxdamage;
#[cfg(test)]
const FIELD_UNIT_MINOFFHANDDAMAGE: u16 = FIELDS_5875.unit_minoffhanddamage;
#[cfg(test)]
const FIELD_UNIT_MAXOFFHANDDAMAGE: u16 = FIELDS_5875.unit_maxoffhanddamage;
#[cfg(test)]
const FIELD_UNIT_STAT0: u16 = FIELDS_5875.unit_stat0;
#[cfg(test)]
const FIELD_UNIT_RESISTANCES: u16 = FIELDS_5875.unit_resistances;
#[cfg(test)]
const FIELD_UNIT_ATTACK_POWER: u16 = FIELDS_5875.unit_attack_power;
#[cfg(test)]
const FIELD_UNIT_ATTACK_POWER_MODS: u16 = FIELDS_5875.unit_attack_power_mods;
#[cfg(test)]
const FIELD_UNIT_ATTACK_POWER_MULTIPLIER: u16 = FIELDS_5875.unit_attack_power_multiplier;
#[cfg(test)]
const FIELD_UNIT_RANGED_ATTACK_POWER: u16 = FIELDS_5875.unit_ranged_attack_power;
#[cfg(test)]
const FIELD_UNIT_RANGED_ATTACK_POWER_MODS: u16 = FIELDS_5875.unit_ranged_attack_power_mods;
#[cfg(test)]
const FIELD_UNIT_RANGED_ATTACK_POWER_MULTIPLIER: u16 =
    FIELDS_5875.unit_ranged_attack_power_multiplier;
#[cfg(test)]
const FIELD_UNIT_MINRANGEDDAMAGE: u16 = FIELDS_5875.unit_minrangeddamage;
#[cfg(test)]
const FIELD_UNIT_MAXRANGEDDAMAGE: u16 = FIELDS_5875.unit_maxrangeddamage;

/// Aura slots per unit (vmangos `MAX_AURAS`, `SpellAuraDefines.h:25`).
pub const UNIT_AURA_SLOTS: u8 = 48;
/// Slots below this hold buffs, the rest debuffs (`SpellAuraDefines.h:26`). Passives get no slot,
/// so the client renders the array unfiltered (`SpellAuras.cpp:6715-6736`).
pub const UNIT_AURA_POSITIVE_SLOTS: u8 = 32;

/// `AFLAG_CANCELABLE`: set on a positive aura without `SPELL_ATTR_NO_AURA_CANCEL`, the same test
/// the `CMSG_CANCEL_AURA` handler makes (`SpellAuras.cpp:7467`).
pub const AURA_FLAG_CANCELABLE: u8 = 0x01;
/// `AFLAG_EFF_INDEX_0|1|2`, the client's aura liveness test: a cleared slot can keep a stale spell
/// id, so a live aura is one with any of these bits set.
pub const AURA_FLAG_EFF_INDEX_MASK: u8 = 0x0E;

/// One occupied aura slot, from the four `UNIT_FIELD_AURA*` arrays; duration and caster are not
/// on the descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitAuraSlot {
    /// Also the buff or debuff class, and the key `SMSG_UPDATE_AURA_DURATION` uses.
    pub slot: u8,
    pub spell_id: u32,
    /// The raw `UNIT_FIELD_AURAFLAGS` nibble.
    pub flags: u8,
    /// The caster's level at apply time, the only trace of the caster.
    pub level: u8,
    /// At least 1; the wire byte holds `stack - 1`.
    pub stacks: u8,
}

impl UnitAuraSlot {
    /// A buff: the slot is in the positive half.
    pub fn is_helpful(&self) -> bool {
        self.slot < UNIT_AURA_POSITIVE_SLOTS
    }
    /// Whether the server will honour a `CMSG_CANCEL_AURA` for this aura.
    pub fn is_cancelable(&self) -> bool {
        self.flags & AURA_FLAG_CANCELABLE != 0
    }
}

#[cfg(test)]
const FIELD_PLAYER_BYTES_3: u16 = FIELDS_5875.player_bytes_3;
pub const FIELD_PLAYER_QUEST_LOG_1_1: u16 = FIELDS_5875.player_quest_log_1_1;

/// What a field watch names that is not a table index: the dead bit of a dynamic-flags edge, and
/// the quest-log base the probes read.
pub mod field {
    /// The bit a watcher tests on a raw `UNIT_DYNAMIC_FLAGS` edge (feign death).
    pub use super::unit::UNIT_DYNFLAG_DEAD;
    pub use super::FIELD_PLAYER_QUEST_LOG_1_1;
}

/// `MAX_QUEST_LOG_SIZE` (`QuestDef.h:34`).
pub const PLAYER_QUEST_LOG_SLOTS: u8 = 20;

/// One `PLAYER_QUEST_LOG` slot, the durable quest state; `SMSG_QUESTUPDATE_*` are only toasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuestLogSlot {
    /// 0 for an empty slot.
    pub quest_id: u32,
    /// Four 6-bit kill, cast and interact counters at bits `6i..6i+6` (`Player.h:1100-1106`); the
    /// client counts item objectives from the bags itself.
    pub counters: [u8; 4],
    /// Byte 3 of the counter field (`Player.h:1107`): a [`quest_slot_state`] bit, 0 in progress.
    pub state: u8,
    /// The absolute end time of a timed quest, else 0.
    pub timer: u32,
}

/// `QUEST_STATE_*` bits of [`QuestLogSlot::state`] (`Player.h:447-451`).
pub mod quest_slot_state {
    pub const COMPLETE: u8 = 0x01;
    pub const FAIL: u8 = 0x02;
}

#[cfg(test)]
const FIELD_PLAYER_INV_SLOT_HEAD: u16 = FIELDS_5875.player_inv_slot_head;
#[cfg(test)]
const FIELD_PLAYER_KEYRING_SLOT_1: u16 = FIELDS_5875.player_keyring_slot_1;
#[cfg(test)]
const FIELD_PLAYER_FARSIGHT: u16 = FIELDS_5875.player_farsight;
#[cfg(test)]
const FIELD_PLAYER_BUYBACK_PRICE_1: u16 = FIELDS_5875.player_buyback_price_1;
#[cfg(test)]
const FIELD_PLAYER_BUYBACK_TIMESTAMP_1: u16 = FIELDS_5875.player_buyback_timestamp_1;
#[cfg(test)]
const FIELD_PLAYER_FIELD_COINAGE: u16 = FIELDS_5875.player_field_coinage;
#[cfg(test)]
const FIELD_PLAYER_XP: u16 = FIELDS_5875.player_xp;
#[cfg(test)]
const FIELD_PLAYER_NEXT_LEVEL_XP: u16 = FIELDS_5875.player_next_level_xp;
#[cfg(test)]
const FIELD_PLAYER_WATCHED_FACTION_INDEX: u16 = FIELDS_5875.player_watched_faction_index;
#[cfg(test)]
const FIELD_PLAYER_REST_STATE_EXPERIENCE: u16 = FIELDS_5875.player_rest_state_experience;
#[cfg(test)]
const FIELD_PLAYER_EXPLORED_ZONES_1: u16 = FIELDS_5875.player_explored_zones_1;
/// The bitset's slot count (`Size: 64` in the server enum).
pub const PLAYER_EXPLORED_ZONES_SLOTS: u16 = 64;
#[cfg(test)]
const FIELD_PLAYER_POSSTAT0: u16 = FIELDS_5875.player_posstat0;
#[cfg(test)]
const FIELD_PLAYER_NEGSTAT0: u16 = FIELDS_5875.player_negstat0;
#[cfg(test)]
const FIELD_PLAYER_RESISTANCEBUFFMODSPOSITIVE: u16 = FIELDS_5875.player_resistancebuffmodspositive;
#[cfg(test)]
const FIELD_PLAYER_RESISTANCEBUFFMODSNEGATIVE: u16 = FIELDS_5875.player_resistancebuffmodsnegative;
#[cfg(test)]
const FIELD_PLAYER_MOD_DAMAGE_DONE_POS: u16 = FIELDS_5875.player_mod_damage_done_pos;
#[cfg(test)]
const FIELD_PLAYER_MOD_DAMAGE_DONE_NEG: u16 = FIELDS_5875.player_mod_damage_done_neg;
#[cfg(test)]
const FIELD_PLAYER_MOD_DAMAGE_DONE_PCT: u16 = FIELDS_5875.player_mod_damage_done_pct;
// (`Player.cpp:3336`, `Player.cpp:7274`)
#[cfg(test)]
const FIELD_PLAYER_SKILL_INFO_1_1: u16 = FIELDS_5875.player_skill_info_1_1;

#[cfg(test)]
const FIELD_PLAYER_CHARACTER_POINTS1: u16 = FIELDS_5875.player_character_points1;
#[cfg(test)]
const FIELD_PLAYER_CHARACTER_POINTS2: u16 = FIELDS_5875.player_character_points2;
#[cfg(test)]
const FIELD_PLAYER_TRACK_CREATURES: u16 = FIELDS_5875.player_track_creatures;
#[cfg(test)]
const FIELD_PLAYER_TRACK_RESOURCES: u16 = FIELDS_5875.player_track_resources;
#[cfg(test)]
const FIELD_PLAYER_AMMO_ID: u16 = FIELDS_5875.player_ammo_id;
#[cfg(test)]
const FIELD_PLAYER_FLAGS: u16 = FIELDS_5875.player_flags;
#[cfg(test)]
const FIELD_PLAYER_FIELD_COMBO_TARGET: u16 = FIELDS_5875.player_field_combo_target;
#[cfg(test)]
const FIELD_PLAYER_FIELD_BYTES: u16 = FIELDS_5875.player_field_bytes;
// highest honor rank; not the appearance PLAYER_BYTES

#[cfg(test)]
const FIELD_PLAYER_FIELD_SESSION_KILLS: u16 = FIELDS_5875.player_field_session_kills;
#[cfg(test)]
const FIELD_PLAYER_FIELD_YESTERDAY_KILLS: u16 = FIELDS_5875.player_field_yesterday_kills;
#[cfg(test)]
const FIELD_PLAYER_FIELD_LAST_WEEK_KILLS: u16 = FIELDS_5875.player_field_last_week_kills;
#[cfg(test)]
const FIELD_PLAYER_FIELD_THIS_WEEK_KILLS: u16 = FIELDS_5875.player_field_this_week_kills;
#[cfg(test)]
const FIELD_PLAYER_FIELD_THIS_WEEK_CONTRIBUTION: u16 =
    FIELDS_5875.player_field_this_week_contribution;
#[cfg(test)]
const FIELD_PLAYER_FIELD_LIFETIME_HONORABLE_KILLS: u16 =
    FIELDS_5875.player_field_lifetime_honorable_kills;
#[cfg(test)]
const FIELD_PLAYER_FIELD_LIFETIME_DISHONORABLE_KILLS: u16 =
    FIELDS_5875.player_field_lifetime_dishonorable_kills;
#[cfg(test)]
const FIELD_PLAYER_FIELD_YESTERDAY_CONTRIBUTION: u16 =
    FIELDS_5875.player_field_yesterday_contribution;
#[cfg(test)]
const FIELD_PLAYER_FIELD_LAST_WEEK_CONTRIBUTION: u16 =
    FIELDS_5875.player_field_last_week_contribution;
#[cfg(test)]
const FIELD_PLAYER_FIELD_LAST_WEEK_RANK: u16 = FIELDS_5875.player_field_last_week_rank;
#[cfg(test)]
const FIELD_PLAYER_FIELD_BYTES2: u16 = FIELDS_5875.player_field_bytes2;

/// `PLAYER_SKILL_INFO` slots on the wire, 3 fields each; vmangos fills only 127 (`Player.h:69`).
pub const PLAYER_SKILL_SLOTS: u8 = 128;

/// One `PLAYER_SKILL_INFO` slot, three dwords (`Player.cpp:90-99`): `id | step << 16`,
/// `value | max << 16`, `temp_bonus | perm_bonus << 16` with signed bonuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerSkillSlot {
    /// A `SkillLine.dbc` id, 0 for an unused slot.
    pub skill_id: u16,
    /// The tier step, 0 unless a tier-raising effect is live.
    pub step: u16,
    pub value: u16,
    pub max: u16,
    /// From auras, consumables and enchants; a malus is negative.
    pub temp_bonus: i16,
    /// From talents.
    pub perm_bonus: i16,
}

/// A corpse's own appearance, snapshotted at death: the seven `CORPSE_FIELD_BYTES_1`/`_2` bytes
/// the client's dress `0x5d6260` loads, not the owner's live `PLAYER_BYTES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorpseLook {
    pub race: u8,
    pub sex: u8,
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
}

/// A sparse update-field set: the wire's block mask and a dense value array. A CREATE carries only
/// nonzero fields (`Object::_SetCreateBits`) into the client's zeroed buffer, so an absent field
/// within the created type's descriptor reads `Some(0)`; in a `Values` delta, or past the
/// descriptor's end, it reads `None`.
#[derive(Clone)]
pub struct ObjectFields {
    /// The wire's block mask, verbatim (`index = word * 32 + bit`).
    present: Vec<u32>,
    /// Always `present.len() * 32` long; an unset slot holds 0 but must be read through the mask.
    values: Vec<u32>,
    /// The created type's [`FieldTable::descriptor_len`], below which absent reads 0; 0 for a
    /// delta or fixture.
    descriptor_end: u16,
    /// The build's field indices, which every accessor reads.
    table: &'static FieldTable,
}

impl Default for ObjectFields {
    fn default() -> Self {
        Self::empty(&FIELDS_5875)
    }
}

impl std::fmt::Debug for ObjectFields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectFields")
            .field("present", &self.present)
            .field("values", &self.values)
            .field("descriptor_end", &self.descriptor_end)
            .finish()
    }
}

impl ObjectFields {
    /// No fields, indexed by `table`.
    fn empty(table: &'static FieldTable) -> Self {
        Self {
            present: Vec::new(),
            values: Vec::new(),
            descriptor_end: 0,
            table,
        }
    }

    pub(super) fn read(r: &mut impl Read, table: &'static FieldTable) -> io::Result<Self> {
        let amount_of_blocks = read_u8(r)?;
        // Mask words of the widest descriptor; no object needs more.
        let max_mask_words = usize::from(table.player_end).div_ceil(32);
        let mut present = Vec::with_capacity(capacity_hint(amount_of_blocks, max_mask_words));
        for _ in 0..amount_of_blocks {
            present.push(read_u32_le(r)?);
        }
        let mut values = vec![0u32; present.len() * 32];
        for (word, block) in present.iter().enumerate() {
            for bit in 0..32 {
                if block & (1u32 << bit) != 0 {
                    values[word * 32 + bit] = read_u32_le(r)?;
                }
            }
        }
        Ok(Self {
            present,
            values,
            descriptor_end: 0,
            table,
        })
    }

    /// Marks a CREATE snapshot of `object_type`, so absent fields in its descriptor read 0.
    pub fn into_created(mut self, object_type: ObjectType) -> Self {
        self.descriptor_end = self.table.descriptor_len(object_type);
        self
    }

    /// A fixture from `(index, value)` pairs; [`Self::into_created`] makes it a create.
    pub fn from_pairs(pairs: &[(u16, u32)]) -> Self {
        Self::from_pairs_in(&FIELDS_5875, pairs)
    }

    /// [`Self::from_pairs`] indexed by `table`.
    pub fn from_pairs_in(table: &'static FieldTable, pairs: &[(u16, u32)]) -> Self {
        let mut this = Self::empty(table);
        for &(index, value) in pairs {
            this.insert(index, value);
        }
        this
    }

    /// `CORPSE_FIELD_OWNER` (`UpdateFields_1_12_1.h:339`): the dead player, only on a corpse.
    pub fn corpse_owner(&self) -> Option<u64> {
        self.get_guid(self.table.corpse_owner).filter(|&g| g != 0)
    }

    /// The body's `CreatureDisplayInfo` id, the owner's native display (`Player.cpp:4809`). A bone
    /// pile keeps it, but `0x5d6700` ignores it there and builds the skeleton from race and sex.
    pub fn corpse_display_id(&self) -> Option<u32> {
        self.get_u32(self.table.corpse_display_id)
            .filter(|&d| d != 0)
    }

    /// The piece in equipment slot `slot` (0..18) as `(ItemDisplayInfo id, InventoryType)`, packed
    /// `display | type << 24` (`Player.cpp:4822`); unlike `PLAYER_VISIBLE_ITEM`, not an item entry.
    pub fn corpse_item(&self, slot: u8) -> Option<(u32, u8)> {
        let raw = self.get_u32(self.table.corpse_item + u16::from(slot))?;
        let display = raw & 0x00ff_ffff;
        (display != 0).then_some((display, (raw >> 24) as u8))
    }

    /// The owner's guild id at death, 0 for none; the client builds the corpse's tabard crest from
    /// it (`0x5d6ec0`).
    pub fn corpse_guild(&self) -> u32 {
        self.get_u32(self.table.corpse_guild).unwrap_or(0)
    }

    /// Race, gender and skin in bytes 1..3; byte 0 is unused (`Corpse.cpp:228`).
    fn corpse_bytes_1(&self) -> Option<u32> {
        self.get_u32(self.table.corpse_bytes_1)
    }
    /// Face, hair style, hair colour and facial hair (`Corpse.cpp:229`).
    fn corpse_bytes_2(&self) -> Option<u32> {
        self.get_u32(self.table.corpse_bytes_2)
    }
    /// The dead player's race, `CORPSE_FIELD_BYTES_1` byte 1, the one corpse field the reaction
    /// gate reads (`0x5d7120`); 0, which no `ChrRaces` row names, when absent.
    pub fn corpse_race(&self) -> u8 {
        self.corpse_bytes_1().map_or(0, |b1| (b1 >> 8) as u8)
    }
    /// The corpse's appearance, `None` in a delta that lacks either BYTES word.
    pub fn corpse_look(&self) -> Option<CorpseLook> {
        let (b1, b2) = (self.corpse_bytes_1()?, self.corpse_bytes_2()?);
        Some(CorpseLook {
            race: (b1 >> 8) as u8,
            sex: (b1 >> 16) as u8,
            skin: (b1 >> 24) as u8,
            face: b2 as u8,
            hair_style: (b2 >> 8) as u8,
            hair_color: (b2 >> 16) as u8,
            facial_hair: (b2 >> 24) as u8,
        })
    }

    /// `CORPSE_FIELD_FLAGS`, 0 when absent (client `[[corpse+0x110]+0x74]`).
    pub fn corpse_flags(&self) -> u32 {
        self.get_u32(self.table.corpse_flags).unwrap_or(0)
    }
    /// [`Self::corpse_flags`] keeping absence: untouched flags in a delta are `None`, not all
    /// clear. A reader acting on a change must use this one.
    pub fn corpse_flags_present(&self) -> Option<u32> {
        self.get_u32(self.table.corpse_flags)
    }
    /// `CORPSE_FLAG_BONES`: a bone pile, which `0x5d6260` tests first; it wears nothing and takes
    /// its model from race and sex.
    pub fn corpse_is_bones(&self) -> bool {
        self.corpse_flags() & 0x01 != 0
    }
    /// `CORPSE_FLAG_HIDE_HELM`, the owner's setting at death; the client skips slot 0 (`0x5d6465`).
    pub fn corpse_hides_helm(&self) -> bool {
        self.corpse_flags() & 0x08 != 0
    }
    /// `CORPSE_FLAG_HIDE_CLOAK`: the same for slot 14 (`0x5d6470`).
    pub fn corpse_hides_cloak(&self) -> bool {
        self.corpse_flags() & 0x10 != 0
    }

    /// `CORPSE_DYNFLAG_LOOTABLE` (`Map.cpp:3655`): the bone pile has insignia; `0x5d6e20` gates the
    /// loot highlight and the `CMSG_LOOT` click on it.
    pub fn corpse_lootable(&self) -> bool {
        self.get_u32(self.table.corpse_dynamic_flags).unwrap_or(0) & 0x01 != 0
    }

    /// `CORPSE_FLAG_LOOTABLE`: the battleground insignia is takeable, by spell 22027 "Remove
    /// Insignia". The cursor `0x482740` and right-click `0x5d6bf0` test it; the corpse's PvP flag
    /// is bit 2 (`UnitIsPVP`, `0x516460`).
    pub fn corpse_pvp_insignia(&self) -> bool {
        self.corpse_flags() & 0x20 != 0
    }

    /// The ground caster (`UpdateFields_1_12_1.h:325`), at the same index as a corpse's owner.
    pub fn dynamicobject_caster(&self) -> Option<u64> {
        self.get_guid(self.table.dynamicobject_caster)
            .filter(|&g| g != 0)
    }
    /// The type byte; vmangos sends 1 (area spell) for every persistent-area cast.
    pub fn dynamicobject_bytes(&self) -> Option<u32> {
        self.get_u32(self.table.dynamicobject_bytes)
    }
    /// The anchoring spell, the root of the ground-targeted visual chain.
    pub fn dynamicobject_spell_id(&self) -> Option<u32> {
        self.get_u32(self.table.dynamicobject_spell_id)
            .filter(|&s| s != 0)
    }
    /// The area's radius in yards, which the server resolves from `SpellRadius.dbc`.
    pub fn dynamicobject_radius(&self) -> Option<f32> {
        self.get_f32(self.table.dynamicobject_radius)
    }
    /// The anchored point and facing in raw WoW coordinates, equal to the create's position.
    pub fn dynamicobject_position(&self) -> Option<([f32; 3], f32)> {
        Some((
            [
                self.get_f32(self.table.dynamicobject_pos_x)?,
                self.get_f32(self.table.dynamicobject_pos_x + 1)?,
                self.get_f32(self.table.dynamicobject_pos_x + 2)?,
            ],
            self.get_f32(self.table.dynamicobject_facing).unwrap_or(0.0),
        ))
    }

    /// Whether the mask carries the field, ignoring the created store's absent-is-zero.
    fn contains(&self, index: u16) -> bool {
        self.present
            .get(usize::from(index / 32))
            .is_some_and(|w| w & (1u32 << (index % 32)) != 0)
    }

    fn get_raw(&self, index: u16) -> Option<u32> {
        self.contains(index)
            .then(|| self.values[usize::from(index)])
    }

    fn insert(&mut self, index: u16, value: u32) {
        // A `u16` index bounds the store at 64 Ki values, whatever the wire says.
        let word = usize::from(index / 32);
        if word >= self.present.len() {
            self.present.resize(word + 1, 0);
            self.values.resize(self.present.len() * 32, 0);
        }
        self.present[word] |= 1u32 << (index % 32);
        self.values[usize::from(index)] = value;
    }

    fn get_guid(&self, index: u16) -> Option<u64> {
        // Raw on purpose: a guid is present iff its low half is, even on a created store.
        let lo = self.get_raw(index)?;
        let hi = self.get_u32(index + 1).unwrap_or(0);
        Some(u64::from(lo) | (u64::from(hi) << 32))
    }

    fn get_u32(&self, index: u16) -> Option<u32> {
        self.get_raw(index)
            .or((index < self.descriptor_end).then_some(0))
    }
    fn get_f32(&self, index: u16) -> Option<f32> {
        self.get_u32(index).map(f32::from_bits)
    }
    fn get_i32(&self, index: u16) -> Option<i32> {
        self.get_u32(index).map(|v| v as i32)
    }
    /// A `TWO_SHORT` field as its (low, high) halves (vmangos `shared/Common.h:119-121`).
    fn get_u16_pair(&self, index: u16) -> Option<(u16, u16)> {
        self.get_u32(index).map(|v| (v as u16, (v >> 16) as u16))
    }

    /// One slot's `UNIT_FIELD_AURAFLAGS` nibble; an absent word reads 0, as in the client.
    fn get_aura_nibble(&self, slot: u8) -> u8 {
        let word = self
            .get_u32(self.table.unit_auraflags + u16::from(slot >> 3))
            .unwrap_or(0);
        ((word >> ((slot & 7) * 4)) & 0x0F) as u8
    }

    /// One slot's byte of `UNIT_FIELD_AURALEVELS` or `UNIT_FIELD_AURAAPPLICATIONS`; an absent word
    /// reads 0.
    fn get_aura_byte(&self, base: u16, slot: u8) -> u8 {
        let word = self.get_u32(base + u16::from(slot >> 2)).unwrap_or(0);
        (word >> ((slot & 3) * 8)) as u8
    }

    /// Folds in an update: a `Values` delta overlays, since a field going to 0 is sent as 0; a
    /// re-CREATE replaces, as overlaying it would keep fields that have since dropped to 0.
    pub fn merge(&mut self, delta: ObjectFields) {
        self.merge_diff(delta, |_, _, _| {});
    }

    /// [`Self::merge`], calling `changed(index, old, new)` once per changed dword in ascending
    /// order, as the client's `CMirrorHandler` pass does (`0x465330`): a resend of the same value
    /// reports nothing, and an absent old value reads 0. A first create is seeded, never merged.
    pub fn merge_diff(&mut self, delta: ObjectFields, mut changed: impl FnMut(u16, u32, u32)) {
        if delta.descriptor_end != 0 {
            let words = self.present.len().max(delta.present.len());
            for word in 0..words {
                let mask = self.present.get(word).copied().unwrap_or(0)
                    | delta.present.get(word).copied().unwrap_or(0);
                if mask == 0 {
                    continue;
                }
                for bit in 0..32u16 {
                    if mask & (1u32 << bit) == 0 {
                        continue;
                    }
                    let index = word as u16 * 32 + bit;
                    let (old, new) = (
                        self.get_raw(index).unwrap_or(0),
                        delta.get_raw(index).unwrap_or(0),
                    );
                    if old != new {
                        changed(index, old, new);
                    }
                }
            }
            *self = delta;
        } else {
            for (index, value) in delta.raw_fields() {
                let old = self.get_raw(index).unwrap_or(0);
                if old != value {
                    changed(index, old, value);
                }
                self.insert(index, value);
            }
        }
    }

    /// The field table these indices are read against.
    pub fn table(&self) -> &'static FieldTable {
        self.table
    }

    /// The type this was created as, from its descriptor length (the eight lengths are distinct).
    /// Unlike [`Self::object_type`] it tells a corpse from an item, as index 36 needs.
    pub fn created_as(&self) -> Option<ObjectType> {
        const ALL: [ObjectType; 8] = [
            ObjectType::Object,
            ObjectType::Item,
            ObjectType::Container,
            ObjectType::Unit,
            ObjectType::Player,
            ObjectType::GameObject,
            ObjectType::DynamicObject,
            ObjectType::Corpse,
        ];
        (self.descriptor_end != 0)
            .then(|| {
                ALL.into_iter()
                    .find(|&t| self.table.descriptor_len(t) == self.descriptor_end)
            })
            .flatten()
    }

    /// No fields carried; the codec skips an empty `Values` delta.
    pub fn is_empty(&self) -> bool {
        self.present.iter().all(|&w| w == 0)
    }
    /// Every carried `(index, value)` pair in ascending order, for debugging and probes.
    pub fn raw_fields(&self) -> impl Iterator<Item = (u16, u32)> + '_ {
        self.present.iter().enumerate().flat_map(move |(word, &w)| {
            (0..32u16)
                .filter(move |bit| w & (1u32 << bit) != 0)
                .map(move |bit| {
                    let index = word as u16 * 32 + bit;
                    (index, self.values[usize::from(index)])
                })
        })
    }
}

mod player;
mod table;
mod unit;

pub use table::{field_table, FieldTable, FIELDS_5875};
pub use unit::{power_display_scale, OwnerFallback};

#[cfg(test)]
mod tests;
