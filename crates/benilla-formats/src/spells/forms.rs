//! `SpellShapeshiftForm.dbc`: the per-form rows the action bar, the form gate, the spell tooltip
//! and tracking read.

use std::collections::HashMap;

use crate::{Chain, DbcLayout};
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{i32_at, parse, slots, str_at, u32_at, unread};

const SHAPESHIFT_FORM: &str = "DBFilesClient\\SpellShapeshiftForm.dbc";

/// One `SpellShapeshiftForm.dbc` row's consumed fields.
#[derive(Clone, Debug, Default)]
pub struct ShapeshiftForm {
    /// `BonusActionBar` (column 1): the page the action bar flips to, 0 for none.
    pub bonus_bar: u32,
    /// The enUS name (column 2), the tooltip's `SPELL_REQUIRED_FORM` "Requires %s"
    /// (`0x52f10a`-`0x52f2ae`).
    pub name: String,
    /// `flags1` (column 11): bit 0 marks a stance, bit 1 blocks cancelling the active form.
    pub flags: u32,
    /// `creatureType` (column 12): the form's creature-type override, which the resolver
    /// (`0x605570`) reads before the creature template or race, so a cat-form druid tracks as a
    /// Beast; `<= 0` is no override, and the template or race answers.
    pub creature_type: i32,
    /// `AttackIconID` (column 13) through `SpellIcon.dbc`: the Attack action shows the current
    /// form's icon before the main-hand weapon's (`0x4e6870`); `None` falls through to the weapon.
    pub attack_icon: Option<String>,
}

impl ShapeshiftForm {
    /// A stance (vmangos `SHAPESHIFT_FLAG_STANCE`: warrior stances, Stealth), which the form gate
    /// ([`crate::spells::SpellDisplay::usable_in_form`]) does not count as shapeshifted.
    pub fn is_stance(&self) -> bool {
        self.flags & 1 != 0
    }

    /// Clicking the active form's button cancels its aura (`CMSG_CANCEL_AURA`) unless bit `0x2`
    /// makes it a silent no-op (`CastShapeshiftForm` `0x4b4810`, guard at `0x4b4963`); the
    /// warrior stances (`0x7`) set it on 5875.
    pub fn cancelable(&self) -> bool {
        self.flags & 0x2 == 0
    }
}

/// 14 fields in 1.12.1; 2.4.3 has 35: the name is 17 slots wide, so the flags, creature type and
/// attack icon sit eight slots later, and 13 columns follow the icon (a round time, four display
/// ids and eight preset spells, not read). Position by the emulator's format and the definitions
/// project's column list; the measured match is high for the bonus bar and icon and lower for
/// the flags and creature type, whose values 2.4.3 changed (a stance bit on Tree of Life, a new
/// flag bit on the druid forms).
pub(crate) fn shapeshift_form_schema(layout: DbcLayout) -> Schema {
    let mut schema = layout.schema("SpellShapeshiftForm");
    schema.add_field(SchemaField::new("ID", FieldType::UInt32));
    schema.add_field(SchemaField::new("BonusBar", FieldType::UInt32));
    schema.add_field(SchemaField::new("Name", FieldType::LocString));
    schema.add_field(SchemaField::new("Flags", FieldType::UInt32));
    schema.add_field(SchemaField::new("CreatureType", FieldType::UInt32));
    schema.add_field(SchemaField::new("AttackIcon", FieldType::UInt32));
    if layout.is_tbc() {
        unread(&mut schema, "Appended", 13);
    }
    schema
}

/// Load `SpellShapeshiftForm.dbc` by form id. The bonus bar is data, not a stance switch:
/// `GetBonusBarOffset` (`0x4e7620`) returns what the `UPDATE_BONUS_ACTIONBAR` handler (`0x4e4fc0`)
/// read from column 1 for the player's form. `flags1` feeds the form gate (`0x612480`).
pub fn load_shapeshift_forms(chain: &mut Chain) -> Result<HashMap<u32, ShapeshiftForm>> {
    let bytes = chain
        .read_file(SHAPESHIFT_FORM)
        .context("reading SpellShapeshiftForm.dbc")?;
    let schema = shapeshift_form_schema(chain.dbc_layout());
    let [bar_slot, name_slot, flags_slot, type_slot, icon_slot] = slots(
        &schema,
        ["BonusBar", "Name", "Flags", "CreatureType", "AttackIcon"],
    )?;
    let set = parse(&bytes, schema, "SpellShapeshiftForm.dbc")?;
    // AttackIconID resolves through SpellIcon.dbc like a spell's own icon (`0x4e68af`-`0x4e68da`).
    let icons = crate::dbc::load_spell_icon_map(chain)?;
    let mut map = HashMap::new();
    for r in set.records() {
        if let Some(id) = u32_at(r, 0) {
            map.insert(
                id,
                ShapeshiftForm {
                    bonus_bar: u32_at(r, bar_slot).unwrap_or(0),
                    name: str_at(&set, r, name_slot).unwrap_or_default(),
                    flags: u32_at(r, flags_slot).unwrap_or(0),
                    creature_type: i32_at(r, type_slot).unwrap_or(0),
                    attack_icon: u32_at(r, icon_slot)
                        .filter(|&i| i != 0)
                        .and_then(|i| icons.get(&i).cloned()),
                },
            );
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2.4.3: the flags, creature type and icon sit eight slots later; Tree of Life is a stance
    /// there, and the flight forms are new.
    #[test]
    fn the_2_4_3_forms_read_through_the_shifted_columns() {
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let forms = load_shapeshift_forms(&mut chain).expect("load SpellShapeshiftForm");
        assert_eq!(forms.len(), 32);

        let cat = &forms[&1];
        assert_eq!(
            (
                cat.name.as_str(),
                cat.bonus_bar,
                cat.flags,
                cat.creature_type
            ),
            ("Cat Form", 1, 120, 1)
        );
        assert!(
            cat.attack_icon.is_some(),
            "the Attack action shows the cat's icon"
        );
        let tree = &forms[&2];
        assert_eq!(
            (
                tree.name.as_str(),
                tree.bonus_bar,
                tree.flags,
                tree.creature_type
            ),
            ("Tree of Life Form", 2, 81, 4)
        );
        assert!(tree.is_stance(), "Tree of Life is a stance in 2.4.3");
        let flight = &forms[&29];
        assert_eq!(
            (flight.name.as_str(), flight.flags, flight.creature_type),
            ("Flight Form", 8, 1)
        );
        assert_eq!(forms[&27].name, "Flight Form, Epic");
        let moonkin = &forms[&31];
        assert_eq!(
            (moonkin.bonus_bar, moonkin.flags, moonkin.creature_type),
            (4, 65, -1)
        );
        // A 1.12.1 stance through the same columns: stance, not cancelable.
        let battle = &forms[&17];
        assert_eq!(
            (battle.bonus_bar, battle.flags, battle.creature_type),
            (1, 7, -1)
        );
        assert!(battle.is_stance() && !battle.cancelable());
    }
}
