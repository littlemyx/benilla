//! `SpellDispelType.dbc`: the dispel class name the aura tooltip (`0x52f880`) shows and
//! `UnitDebuff` returns as its `dispelType`, which FrameXML's `DebuffTypeColor` keys the
//! debuff border on. A row is named only when column 10 (`+0x28`) is nonzero (`0x52f906`): on 5875
//! that is ids 1-4, so Stealth and Invisibility carry names that never print. Column 11 repeats
//! the name on those four rows; the tooltip does not read it.

use std::collections::HashMap;

use crate::{Chain, DbcLayout};
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, slots, str_at, u32_at, unread};

/// The named dispel classes by `Spell.dbc` `Dispel` id; a row the gate withholds is absent.
#[derive(Default)]
pub struct SpellDispelTypes {
    names: HashMap<u32, String>,
}

impl SpellDispelTypes {
    /// The class name; `None` for no dispel class (0) or one the gate withholds.
    pub fn name(&self, dispel: u32) -> Option<&str> {
        self.names.get(&dispel).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// 12 fields in 1.12.1; 2.4.3 has 21: the name is 17 slots wide and a mask word is inserted before
/// the gate. The gate's slot is measured on all 11 shared rows (it equals 2.4.3's column 19 on every
/// one, the four named rows and the seven others; the mask is 0 on the four named rows), so the
/// definitions project's label for 1.12.1's column 10 does not hold for the gate.
pub(crate) fn spell_dispel_type_schema(layout: DbcLayout) -> Schema {
    let mut schema = layout.schema("SpellDispelType");
    schema.add_field(SchemaField::new("ID", FieldType::UInt32));
    schema.add_field(SchemaField::new("Name", FieldType::LocString));
    if layout.is_tbc() {
        unread(&mut schema, "Mask", 1);
    }
    schema.add_field(SchemaField::new("Named", FieldType::UInt32));
    schema.add_field(SchemaField::new("InternalName", FieldType::String));
    schema
}

/// Load `SpellDispelType.dbc` off the patch chain, keeping only the rows the gate names.
pub fn load_spell_dispel_types(chain: &mut Chain) -> Result<SpellDispelTypes> {
    let bytes = chain
        .read_file("DBFilesClient\\SpellDispelType.dbc")
        .context("reading SpellDispelType.dbc")?;
    let schema = spell_dispel_type_schema(chain.dbc_layout());
    let [name_slot, gate_slot] = slots(&schema, ["Name", "Named"])?;
    let set = parse(&bytes, schema, "SpellDispelType.dbc")?;
    let mut names = HashMap::new();
    for r in set.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        // The gate first: a 0 here withholds the name however good the string is (Stealth, id 5).
        if u32_at(r, gate_slot).unwrap_or(0) == 0 {
            continue;
        }
        if let Some(name) = str_at(&set, r, name_slot).filter(|n| !n.is_empty()) {
            names.insert(id, name);
        }
    }
    Ok(SpellDispelTypes { names })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_dispel_types_name_only_what_the_gate_allows() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let types = load_spell_dispel_types(&mut chain).expect("load SpellDispelType");
        assert_eq!(types.name(1), Some("Magic"));
        assert_eq!(types.name(2), Some("Curse"));
        assert_eq!(types.name(3), Some("Disease"));
        assert_eq!(types.name(4), Some("Poison"));
        // Named in the file, withheld by the gate.
        assert_eq!(types.name(5), None, "Stealth");
        assert_eq!(types.name(6), None, "Invisibility");
        assert_eq!(types.name(9), None, "Frenzy");
        assert_eq!(types.name(0), None, "no dispel class");
        assert_eq!(types.len(), 4, "exactly the four the gate allows");
    }

    /// 2.4.3: the same four named classes, read through the 17-slot name and the gate that follows
    /// the new mask column.
    #[test]
    fn the_2_4_3_dispel_types_name_only_what_the_gate_allows() {
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let types = load_spell_dispel_types(&mut chain).expect("load SpellDispelType");
        assert_eq!(types.name(1), Some("Magic"));
        assert_eq!(types.name(4), Some("Poison"));
        assert_eq!(types.name(5), None, "Stealth");
        assert_eq!(
            types.name(7),
            None,
            "the all-types row carries a mask, not the gate"
        );
        assert_eq!(types.len(), 4);
    }
}
