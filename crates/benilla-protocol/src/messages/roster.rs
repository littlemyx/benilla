//! Character select: the `SMSG_CHAR_ENUM` [`Character`] entry (vmangos `Player::BuildEnumData`
//! order), the create/delete result codes and the create request.

use std::io::{self, Read};

use crate::wire::{read_cstring, read_u32_le, read_u64_le, read_u8, Vector3d};

/// `WorldResult::CharCreateSuccess` / `CharCreateNameInUse` (`SMSG_CHAR_CREATE`).
pub const CHAR_CREATE_SUCCESS: u8 = 0x2E;
pub const CHAR_CREATE_NAME_IN_USE: u8 = 0x31;
/// `WorldResult::CharCreateServerLimit`: the account already holds `CharactersPerRealm`
/// characters (vmangos `HandleCharCreateOpcode`; 0x2E + 6 in `SharedDefines.h` ResponseCodes).
pub const CHAR_CREATE_SERVER_LIMIT: u8 = 0x34;
/// `WorldResult::CharDeleteSuccess` (`SMSG_CHAR_DELETE`; vmangos `SharedDefines.h` ResponseCodes).
pub const CHAR_DELETE_SUCCESS: u8 = 0x39;
/// Race/Class/Gender values benilla's char-create sends.
pub const RACE_HUMAN: u8 = 0x1;
pub const CLASS_WARRIOR: u8 = 0x1;
pub const GENDER_MALE: u8 = 0x0;

/// A `CMSG_CHAR_CREATE` request for [`super::char_create`]. The outfit id is always 0 on the wire:
/// the server ignores it and picks the start gear (vmangos `CharacterHandler.cpp:310`). The
/// appearance bytes must be valid `CharSections` indices or `Player::ValidateAppearance` refuses.
#[derive(Debug, Clone)]
pub struct CharCreateReq {
    pub name: String,
    /// `ChrRaces.dbc` id (1 Human … 8 Troll).
    pub race: u8,
    /// `ChrClasses.dbc` id (1 Warrior … 11 Druid).
    pub class: u8,
    /// 0 male, 1 female.
    pub gender: u8,
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
}

/// The vmangos `CHARACTER_FLAG_*` bits of [`Character::flags`] that the select screen renders.
pub const CHARACTER_FLAG_HIDE_HELM: u32 = 0x0400;
pub const CHARACTER_FLAG_HIDE_CLOAK: u32 = 0x0800;
pub const CHARACTER_FLAG_GHOST: u32 = 0x2000;
pub const CHARACTER_FLAG_RENAME: u32 = 0x4000;

/// One visible-equipment slot: an `ItemDisplayInfo.dbc` id (not an item id, 0 for empty) and the
/// item's `InventoryType`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CharEnumItem {
    pub display_id: u32,
    pub inventory_type: u8,
}

/// One `SMSG_CHAR_ENUM` roster entry.
#[derive(Debug, Clone)]
pub struct Character {
    pub guid: u64,
    pub name: String,
    /// `ChrRaces.dbc` id (1 Human … 8 Troll).
    pub race: u8,
    /// `ChrClasses.dbc` id (1 Warrior … 11 Druid).
    pub class: u8,
    /// 0 male, 1 female.
    pub gender: u8,
    /// First of the five appearance bytes, all `CharSections` indices.
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
    pub level: u8,
    /// `AreaTable.dbc` id of the character's zone.
    pub zone: u32,
    pub map: u32,
    pub position: Vector3d,
    /// `CHARACTER_FLAG_*` bits.
    pub flags: u32,
    /// Equipment order: head, neck, shoulders, shirt, chest, waist, legs, feet, wrists, hands,
    /// finger x2, trinket x2, back, main hand, off hand, ranged, tabard.
    pub equipment: [CharEnumItem; 19],
    /// The pet's `CreatureDisplayInfo.dbc` id, 0 for none. The server already zeroes it for all but
    /// a living hunter or warlock (`Player::BuildEnumData`), so the client needs no class check.
    pub pet_display_id: u32,
    /// The pet's level; `pet_family` is its `CreatureFamily.dbc` id.
    pub pet_level: u32,
    pub pet_family: u32,
}

/// One equipment slot as the wire carries it: 1.12.1 has no enchant field, 2.4.3 adds a `u32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharRecordSlot {
    pub display_id: u32,
    pub inventory_type: u8,
    /// `SpellItemEnchantment.dbc` aura id of the visible enchant (2.4.3 only; 0 for none).
    pub enchant_aura_id: Option<u32>,
}

/// A roster entry with the fields [`Character`] does not keep, for probes that print the record.
#[derive(Debug, Clone)]
pub struct CharRecord {
    pub character: Character,
    pub guild_id: u32,
    pub first_login: u8,
    /// The 19 equipment slots then the first bag: 20 entries in both builds' records.
    pub slots: Vec<CharRecordSlot>,
}

impl Character {
    pub(super) fn read(r: &mut impl Read) -> io::Result<Self> {
        Ok(Self::read_with(r, false)?.character)
    }

    /// The 2.4.3 entry: 20 equipment slots of `u32` display id, `u8` inventory type and `u32`
    /// enchant aura id (two sources), the 20th being the first bag, which 1.12.1 writes after
    /// the 19 slots. The enchant aura and the bag have no field and are dropped.
    pub(super) fn read_tbc(r: &mut impl Read) -> io::Result<Self> {
        Ok(Self::read_with(r, true)?.character)
    }

    /// One entry with every field the wire carries, the ones [`Character`] drops included.
    pub(super) fn read_record(r: &mut impl Read, tbc: bool) -> io::Result<CharRecord> {
        Self::read_with(r, tbc)
    }

    fn read_with(r: &mut impl Read, tbc: bool) -> io::Result<CharRecord> {
        let guid = read_u64_le(r)?;
        let name = read_cstring(r)?;
        let race = read_u8(r)?;
        let class = read_u8(r)?;
        let gender = read_u8(r)?;
        let skin = read_u8(r)?;
        let face = read_u8(r)?;
        let hair_style = read_u8(r)?;
        let hair_color = read_u8(r)?;
        let facial_hair = read_u8(r)?;
        let level = read_u8(r)?;
        let zone = read_u32_le(r)?;
        let map = read_u32_le(r)?;
        let position = Vector3d::read(r)?;
        let guild_id = read_u32_le(r)?;
        let flags = read_u32_le(r)?;
        let first_login = read_u8(r)?;
        let pet_display_id = read_u32_le(r)?;
        let pet_level = read_u32_le(r)?;
        let pet_family = read_u32_le(r)?;
        let mut equipment = [CharEnumItem::default(); 19];
        let mut slots = Vec::with_capacity(20);
        for slot in &mut equipment {
            slot.display_id = read_u32_le(r)?;
            slot.inventory_type = read_u8(r)?;
            let enchant_aura_id = if tbc { Some(read_u32_le(r)?) } else { None };
            slots.push(CharRecordSlot {
                display_id: slot.display_id,
                inventory_type: slot.inventory_type,
                enchant_aura_id,
            });
        }
        let first_bag_display_id = read_u32_le(r)?;
        let first_bag_inventory_type = read_u8(r)?;
        let first_bag_enchant = if tbc { Some(read_u32_le(r)?) } else { None };
        slots.push(CharRecordSlot {
            display_id: first_bag_display_id,
            inventory_type: first_bag_inventory_type,
            enchant_aura_id: first_bag_enchant,
        });
        let character = Self {
            guid,
            name,
            race,
            class,
            gender,
            skin,
            face,
            hair_style,
            hair_color,
            facial_hair,
            level,
            zone,
            map,
            position,
            flags,
            equipment,
            pet_display_id,
            pet_level,
            pet_family,
        };
        Ok(CharRecord {
            character,
            guild_id,
            first_login,
            slots,
        })
    }
}
