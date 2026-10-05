//! `ChrClasses.dbc`, the per-class table, narrowed to the three columns the reference reads off
//! its class-indexed record table (`0xc0def4`, max id `0xc0def8`, loader `0x542360`).
//!
//! Field 2, the damage bonus stat: `GetDamageBonusStat` (`0x48b520`) reads it at `rec + 8` for the
//! active player's class byte and answers it plus one, a 1-based `UnitStat` index. Strength (0)
//! for every row but the Hunter's and the Rogue's Agility (1).
//!
//! Field 4, the pet name token: the second return of `HasPetSpells` (`0x4b4410`), `"PET"` on every
//! row but the Warlock's `"DEMON"`. FrameXML resolves it with `getglobal("PET_TYPE_"..token)`
//! (`SpellBookFrame.lua:173`), so the token is a key, never display text, and is not localized.
//!
//! Field 15, the class spell family: a talent modifier applies to a spell only when its
//! `SpellFamilyName` (`Spell.dbc` column 160) equals the local player's (`GetSpellModifiers`,
//! `0x6e6b30`), cached in `[0xcecaac]` by its one non-zeroing writer, `0x6e6ca0` (called from
//! `0x5debcc`). The shipped values are vmangos `SpellFamilyNames`.
//!
//! Field 16, the relic-slot flag, read by `UnitHasRelicSlot` (`0x519e50`) and nothing else: no
//! class id is compared, the table is the data. It is set for Paladin, Shaman and Druid, and
//! `IsValidForSlot` (`0x5da1d0`) enforces it: INVSLOT 17 takes a relic (`InventoryType` 28) for
//! those three and a ranged weapon for everyone else.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::chain::Chain;
use crate::dbc::{parse, slots, str_at, u32_at, unread};
use crate::DbcLayout;

const CHR_CLASSES: &str = "DBFilesClient\\ChrClasses.dbc";

/// The literal the reference pushes when the player does not resolve (`0x846a40`). Every class but
/// the Warlock carries it too, so an unloaded table degrades invisibly.
pub const PET_NAME_TOKEN_FALLBACK: &str = "PET";

#[derive(Debug, Clone)]
struct ChrClass {
    damage_bonus_stat: u32,
    pet_name_token: Option<String>,
    has_relic_slot: bool,
    spell_family: u32,
}

/// `ChrClasses.dbc`'s read columns by class id.
#[derive(Debug, Default, Clone)]
pub struct ChrClasses(HashMap<u32, ChrClass>);

impl ChrClasses {
    /// `HasPetSpells`' second return: always a string, never nil, and the literal
    /// [`PET_NAME_TOKEN_FALLBACK`] for a class with no row (`0x4b44a6`).
    pub fn pet_name_token(&self, class: u32) -> &str {
        self.0
            .get(&class)
            .and_then(|c| c.pet_name_token.as_deref())
            .unwrap_or(PET_NAME_TOKEN_FALLBACK)
    }

    /// The class's damage bonus stat, 0-based (Strength 0, Agility 1); `None` for a class with
    /// no row, where `GetDamageBonusStat` answers 0 (`0x48b58a`).
    pub fn damage_bonus_stat(&self, class: u32) -> Option<u32> {
        self.0.get(&class).map(|c| c.damage_bonus_stat)
    }

    /// Whether the class's INVSLOT 17 is a relic slot, `UnitHasRelicSlot` whole. A class with no
    /// row answers false, as the reference's bound check against `0xc0def8` falls to its nil leg.
    pub fn has_relic_slot(&self, class: u32) -> bool {
        self.0.get(&class).is_some_and(|c| c.has_relic_slot)
    }

    /// The class spell family, which the reference caches in `[0xcecaac]` and matches against a
    /// spell's `SpellFamilyName` before a talent modifier applies. A class with no row answers 0,
    /// the global's value from world-enter (`0x6e7150`) until the player resolves, which the
    /// gate's `SpellFamilyName != 0` conjunct makes match nothing.
    pub fn spell_family(&self, class: u32) -> u32 {
        self.0.get(&class).map_or(0, |c| c.spell_family)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The table's columns for a layout, the read ones named. 1.12.1 is 17 fields (the patch copy's
/// count, which `benilla-dbc` enforces as the reference loader does, `0x54240e`, `0x542446`: the
/// base archive's 16-column copy lacks the relic flag and is refused). 2.4.3 is 58: the class
/// index word at column 1 is gone, the name is 17 slots wide with a female and a male name after
/// it, and the last column is a flag word whose relic bit is not 1.12.1's boolean.
pub(crate) fn schema(layout: DbcLayout) -> Schema {
    let mut s = layout.schema("ChrClasses");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    if layout.is_tbc() {
        // Slots measured against the 1.12.1 rows (each read column matches on all nine shared
        // ids) and confirmed by the emulator's format and the definitions project's column list.
        s.add_field(SchemaField::new("DamageBonusStat", FieldType::UInt32));
        unread(&mut s, "DisplayPower", 1);
        s.add_field(SchemaField::new("PetNameToken", FieldType::String));
        s.add_field(SchemaField::new("Name", FieldType::LocString));
        s.add_field(SchemaField::new("NameFemale", FieldType::LocString));
        s.add_field(SchemaField::new("NameMale", FieldType::LocString));
        unread(&mut s, "FileName", 1);
        s.add_field(SchemaField::new("SpellFamily", FieldType::UInt32));
        unread(&mut s, "ClassFlags", 1);
    } else {
        unread(&mut s, "ClassIndex", 1);
        s.add_field(SchemaField::new("DamageBonusStat", FieldType::UInt32));
        unread(&mut s, "DisplayPower", 1);
        s.add_field(SchemaField::new("PetNameToken", FieldType::String));
        s.add_field(SchemaField::new("Name", FieldType::LocString));
        unread(&mut s, "FileName", 1);
        s.add_field(SchemaField::new("SpellFamily", FieldType::UInt32));
        s.add_field(SchemaField::new("RelicSlot", FieldType::UInt32));
    }
    s
}

/// Load `ChrClasses.dbc`'s read columns from the patch chain.
pub fn load_chr_classes(chain: &mut Chain) -> Result<ChrClasses> {
    let bytes = chain
        .read_file(CHR_CLASSES)
        .with_context(|| format!("reading {CHR_CLASSES}"))?;
    let schema = schema(chain.dbc_layout());
    let [damage_slot, token_slot, family_slot] =
        slots(&schema, ["DamageBonusStat", "PetNameToken", "SpellFamily"])?;
    // 2.4.3 packs the relic flag into a flag word with other bits (11 on Paladin, Shaman and
    // Druid, 2 to 7 on the rest), not 1.12.1's boolean: the flag stays unset there.
    let relic_slot = schema.slot_of("RelicSlot");
    let rs = parse(&bytes, schema, "ChrClasses.dbc")?;
    let mut by_id = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        by_id.insert(
            id,
            ChrClass {
                damage_bonus_stat: u32_at(r, damage_slot).unwrap_or(0),
                pet_name_token: str_at(&rs, r, token_slot),
                has_relic_slot: relic_slot
                    .and_then(|slot| u32_at(r, slot))
                    .is_some_and(|v| v != 0),
                spell_family: u32_at(r, family_slot).unwrap_or(0),
            },
        );
    }
    Ok(ChrClasses(by_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> Option<crate::chain::Chain> {
        let data = crate::wow_data_or_skip!(None);
        Some(crate::open_chain(&data).expect("open chain"))
    }

    /// Field 5 holds the class name, so a one-column slip would blank the spellbook tab silently.
    #[test]
    fn warlocks_pet_is_a_demon_and_everyone_elses_is_a_pet() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        assert!(!t.is_empty());
        assert_eq!(t.pet_name_token(9), "DEMON", "Warlock");
        for (class, who) in [(1, "Warrior"), (3, "Hunter"), (11, "Druid")] {
            assert_eq!(t.pet_name_token(class), "PET", "{who}");
        }
        // 6 and 10 have no row in 1.12; the reference's out-of-range arm answers the literal.
        assert_eq!(t.pet_name_token(6), PET_NAME_TOKEN_FALLBACK);
        assert_eq!(t.pet_name_token(0), PET_NAME_TOKEN_FALLBACK);
    }

    #[test]
    fn libram_totem_and_idol_are_the_three_relic_classes() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        for (class, who) in [(2, "Paladin"), (7, "Shaman"), (11, "Druid")] {
            assert!(t.has_relic_slot(class), "{who} carries a relic");
        }
        for (class, who) in [
            (1, "Warrior"),
            (3, "Hunter"),
            (4, "Rogue"),
            (5, "Priest"),
            (8, "Mage"),
            (9, "Warlock"),
        ] {
            assert!(!t.has_relic_slot(class), "{who} wields a ranged weapon");
        }
        // No row: the reference's nil leg.
        assert!(!t.has_relic_slot(6));
        assert!(!t.has_relic_slot(0));
    }

    /// Hunters and Rogues scale their damage off Agility, everyone else off Strength.
    #[test]
    fn hunters_and_rogues_take_their_damage_bonus_from_agility() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        for (class, who) in [(3, "Hunter"), (4, "Rogue")] {
            assert_eq!(t.damage_bonus_stat(class), Some(1), "{who}");
        }
        for (class, who) in [
            (1, "Warrior"),
            (2, "Paladin"),
            (5, "Priest"),
            (7, "Shaman"),
            (8, "Mage"),
            (9, "Warlock"),
            (11, "Druid"),
        ] {
            assert_eq!(t.damage_bonus_stat(class), Some(0), "{who}");
        }
        assert_eq!(t.damage_bonus_stat(6), None);
    }

    /// The whole column pins field 15: nine distinct values matching vmangos `SpellFamilyNames`
    /// are a shape no neighbour has (field 14 is the class's file name, field 16 the relic flag).
    #[test]
    fn every_class_row_carries_its_vmangos_spell_family() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        for (class, family, who) in [
            (1u32, 4u32, "Warrior"),
            (2, 10, "Paladin"),
            (3, 9, "Hunter"),
            (4, 8, "Rogue"),
            (5, 6, "Priest"),
            (7, 11, "Shaman"),
            (8, 3, "Mage"),
            (9, 5, "Warlock"),
            (11, 7, "Druid"),
        ] {
            assert_eq!(t.spell_family(class), family, "{who}");
        }
        // No row: the reference's zeroed global, which the gate's first conjunct refuses.
        assert_eq!(t.spell_family(6), 0);
        assert_eq!(t.spell_family(0), 0);
    }

    /// 2.4.3's class table, whose columns sit 1, 1 and 41 slots from 1.12.1's: the same nine
    /// rows' damage stat, pet token and spell family read through the new layout.
    #[test]
    fn the_2_4_3_class_table_reads_through_its_own_layout() {
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        assert_eq!(t.pet_name_token(9), "DEMON", "Warlock");
        assert_eq!(t.pet_name_token(3), "PET", "Hunter");
        for (class, stat) in [(3, 1), (4, 1), (1, 0), (11, 0)] {
            assert_eq!(t.damage_bonus_stat(class), Some(stat), "class {class}");
        }
        for (class, family) in [
            (1, 4),
            (2, 10),
            (3, 9),
            (4, 8),
            (5, 6),
            (7, 11),
            (8, 3),
            (9, 5),
            (11, 7),
        ] {
            assert_eq!(t.spell_family(class), family, "class {class}");
        }
        assert_eq!(t.damage_bonus_stat(6), None, "no Death Knight row in 2.4.3");
        // The relic flag is packed into a flag word in 2.4.3: not read, so never set.
        assert!(!t.has_relic_slot(2));
    }
}
