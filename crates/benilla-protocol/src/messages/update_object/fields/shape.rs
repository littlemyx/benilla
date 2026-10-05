//! The structure of the update-field groups a build lays out in its own way: counts, strides and
//! packing, which sit next to the indices in the [`FieldTable`](super::FieldTable) so that no
//! accessor computes from a literal. The values of 2.4.3 are single-source (cmangos-tbc, digest
//! `tbc-login-wire.md` section 11) and were checked live against a 2.4.3 server.

use super::table::FieldTable;

/// One byte of a packed dword field: the field's index and the byte, 0 the low one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteAt {
    pub field: u16,
    pub byte: u8,
}

impl ByteAt {
    /// A sub-field the build dropped: its accessor reads `None`, never a neighbour's byte.
    pub const NONE: Self = Self {
        field: FieldTable::ABSENT,
        byte: 0,
    };
    pub(super) const fn new(field: u16, byte: u8) -> Self {
        Self { field, byte }
    }
}

/// A byte of one of the two dwords of a virtual item's info.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DwordByte {
    pub dword: u8,
    pub byte: u8,
}

/// Where the buff / debuff boundary of a unit's aura slots comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuraSplit {
    /// A constant: the slots below it are buffs.
    Fixed(u8),
    /// A byte the server sends per unit (2.4.3: `UNIT_FIELD_BYTES_2` byte 1, 40 for a player or
    /// pet, 16 for a creature), with the constant it falls back to when the byte is not carried.
    Byte(ByteAt, u8),
}

/// Where a quest-log slot keeps its state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestState {
    /// Byte 3 of the counts dword (1.12.1).
    CountsByte3,
    /// A dword of its own, at this offset in the slot (2.4.3), low byte read.
    Dword(u8),
}

/// The shape of a virtual item's two info dwords.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualItemShape {
    pub class: DwordByte,
    pub subclass: DwordByte,
    pub material: DwordByte,
    pub inventory_type: DwordByte,
    pub sheath: DwordByte,
}

/// Counts, strides and packing of one build's update fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldShape {
    /// Aura slots per unit; the four `UNIT_FIELD_AURA*` arrays hold this many.
    pub aura_slots: u8,
    /// Bits of one slot in `UNIT_FIELD_AURAFLAGS` (1.12.1 a nibble, 2.4.3 a byte); the level and
    /// application arrays are a byte per slot in both.
    pub aura_flag_bits: u8,
    /// The effect bits of an aura flag byte: a live aura has one of them set.
    pub aura_live_mask: u8,
    /// The cancelable bit of an aura flag byte.
    pub aura_cancelable: u8,
    pub aura_positive: AuraSplit,
    pub stand_state: ByteAt,
    pub loyalty: ByteAt,
    pub vis_flags: ByteAt,
    pub shapeshift_form: ByteAt,
    pub virtual_item: VirtualItemShape,
    pub quest_slots: u8,
    /// Fields per quest-log slot; the quest id is the first.
    pub quest_stride: u8,
    pub quest_counts: u8,
    pub quest_timer: u8,
    pub quest_state: QuestState,
    /// Bits of one of the four kill, cast and interact counters.
    pub quest_counter_bits: u8,
    /// Fields per `PLAYER_VISIBLE_ITEM` slot; the creator (2 fields), then the entry.
    pub visible_stride: u16,
    pub visible_enchant_slots: u8,
    /// Offset of the properties dword in a visible-item slot.
    pub visible_properties: u16,
    pub item_enchant_slots: u8,
    pub bank_slots: u8,
    pub bank_bag_slots: u8,
    pub explored_zone_words: u16,
    /// `PLAYER_FIELD_BYTES` byte 0, the flags the accessors test bits of.
    pub player_flags_byte: ByteAt,
    pub combo_points: ByteAt,
    pub action_bar_toggles: ByteAt,
    pub honor_rank: ByteAt,
    pub drunk: ByteAt,
    pub pvp_rank: ByteAt,
    pub pvp_medal: ByteAt,
    pub honor_rank_bar: ByteAt,
}

const fn dw(dword: u8, byte: u8) -> DwordByte {
    DwordByte { dword, byte }
}

/// 1.12.1 (5875): the layout the accessors were written against.
pub const SHAPE_5875: FieldShape = FieldShape {
    aura_slots: 48,
    aura_flag_bits: 4,
    aura_live_mask: 0x0E,
    aura_cancelable: 0x01,
    aura_positive: AuraSplit::Fixed(32),
    // UNIT_FIELD_BYTES_1 = 138
    stand_state: ByteAt::new(138, 0),
    loyalty: ByteAt::new(138, 1),
    vis_flags: ByteAt::new(138, 3),
    shapeshift_form: ByteAt::new(138, 2),
    virtual_item: VirtualItemShape {
        class: dw(0, 0),
        subclass: dw(0, 1),
        material: dw(0, 2),
        inventory_type: dw(0, 3),
        sheath: dw(1, 0),
    },
    quest_slots: 20,
    quest_stride: 3,
    quest_counts: 1,
    quest_timer: 2,
    quest_state: QuestState::CountsByte3,
    quest_counter_bits: 6,
    visible_stride: 12,
    visible_enchant_slots: 7,
    visible_properties: 10,
    item_enchant_slots: 7,
    bank_slots: 24,
    bank_bag_slots: 6,
    explored_zone_words: 64,
    // PLAYER_FIELD_BYTES = 1222
    player_flags_byte: ByteAt::new(1222, 0),
    combo_points: ByteAt::new(1222, 1),
    action_bar_toggles: ByteAt::new(1222, 2),
    honor_rank: ByteAt::new(1222, 3),
    // PLAYER_BYTES_3 = 195
    drunk: ByteAt::new(195, 1),
    pvp_rank: ByteAt::new(195, 3),
    pvp_medal: ByteAt::new(195, 2),
    // PLAYER_FIELD_BYTES2 = 1260
    honor_rank_bar: ByteAt::new(1260, 0),
};

/// 2.4.3 (8606), cmangos-tbc `UpdateFields.h` and the code that writes each field (digest
/// section 11); a sub-field the build dropped is [`ByteAt::NONE`].
pub const SHAPE_8606: FieldShape = FieldShape {
    // 56 slots (`MAX_AURAS`), a byte of flags per slot (`SetAuraFlag`): effect bits 0x01..0x04,
    // cancelable 0x10, not cancelable 0x20 (`AuraFlags`)
    aura_slots: 56,
    aura_flag_bits: 8,
    aura_live_mask: 0x07,
    aura_cancelable: 0x10,
    // the boundary the server scans from is `UNIT_FIELD_BYTES_2` (209) byte 1
    // (`UNIT_BYTES_2_OFFSET_DEBUFF_LIMIT`); `MAX_POSITIVE_AURAS` 40 when it is not carried
    aura_positive: AuraSplit::Byte(ByteAt::new(209, 1), 40),
    // UNIT_FIELD_BYTES_1 = 159: stand state, pet loyalty, vis flags, misc flags
    stand_state: ByteAt::new(159, 0),
    loyalty: ByteAt::new(159, 1),
    vis_flags: ByteAt::new(159, 2),
    // UNIT_FIELD_BYTES_2 byte 3 (`UNIT_BYTES_2_OFFSET_SHAPESHIFT`)
    shapeshift_form: ByteAt::new(209, 3),
    // `VirtualItemInfo` offsets: dword 0 class, subclass, unknown, material; dword 1 inventory
    // type, sheath
    virtual_item: VirtualItemShape {
        class: dw(0, 0),
        subclass: dw(0, 1),
        material: dw(0, 3),
        inventory_type: dw(1, 0),
        sheath: dw(1, 1),
    },
    // `MAX_QUEST_LOG_SIZE` 25 slots of `MAX_QUEST_OFFSET` 4: id, state, counts, time
    quest_slots: 25,
    quest_stride: 4,
    quest_counts: 2,
    quest_timer: 3,
    quest_state: QuestState::Dword(1),
    quest_counter_bits: 8,
    // `MAX_VISIBLE_ITEM_OFFSET` 16: creator (2), entry and 11 enchant slots, properties, pad
    visible_stride: 16,
    visible_enchant_slots: 11,
    visible_properties: 14,
    // `MAX_ENCHANTMENT_SLOT` 11
    item_enchant_slots: 11,
    bank_slots: 28,
    bank_bag_slots: 7,
    explored_zone_words: 128,
    // PLAYER_FIELD_BYTES = 1486: flags, refer-a-friend level (the combo points are gone), action
    // bar toggles, lifetime max PvP rank
    player_flags_byte: ByteAt::new(1486, 0),
    combo_points: ByteAt::NONE,
    action_bar_toggles: ByteAt::new(1486, 2),
    honor_rank: ByteAt::new(1486, 3),
    // PLAYER_BYTES_3 = 241: byte 1 still carries the drunk value; byte 2 is unused and byte 3 is
    // the arena faction, so the 1.12.1 rank and medal bytes have no counterpart
    drunk: ByteAt::new(241, 1),
    pvp_rank: ByteAt::NONE,
    pvp_medal: ByteAt::NONE,
    // PLAYER_FIELD_BYTES2 = 1518 no longer has the honor rank bar in byte 0
    honor_rank_bar: ByteAt::NONE,
};
