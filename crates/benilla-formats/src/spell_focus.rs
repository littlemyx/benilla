//! `SpellFocusObject.dbc`: the object a spell must be cast near (Anvil, Forge, …), named by the
//! crafting book's "Requires:" line from `Spell.dbc` `RequiresSpellFocus`. The server does the
//! proximity check (`Spell::CheckCast`); the client only names it.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, slots, str_at, u32_at};
use crate::{Chain, DbcLayout};

const SPELL_FOCUS_OBJECT: &str = "DBFilesClient\\SpellFocusObject.dbc";

/// `SpellFocusObject` id → its enUS name.
pub struct SpellFocusCatalog {
    names: HashMap<u32, String>,
}

impl SpellFocusCatalog {
    /// The name for a `RequiresSpellFocus` id; `None` for 0, which draws no requirement line.
    pub fn name(&self, focus_id: u32) -> Option<&str> {
        self.names.get(&focus_id).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

pub(crate) fn schema(layout: DbcLayout) -> Schema {
    let mut s = layout.schema("SpellFocusObject");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Name", FieldType::LocString));
    s
}

/// Load SpellFocusObject.dbc from the patch chain into a [`SpellFocusCatalog`].
pub fn load_spell_focus_catalog(chain: &mut Chain) -> Result<SpellFocusCatalog> {
    let bytes = chain
        .read_file(SPELL_FOCUS_OBJECT)
        .with_context(|| format!("reading {SPELL_FOCUS_OBJECT}"))?;
    let schema = schema(chain.dbc_layout());
    let [name_slot] = slots(&schema, ["Name"])?;
    let rs = parse(&bytes, schema, "SpellFocusObject.dbc")?;
    let mut names = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        if let Some(name) = str_at(&rs, r, name_slot) {
            names.insert(id, name);
        }
    }
    Ok(SpellFocusCatalog { names })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_spell_focus_names_the_profession_objects() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_spell_focus_catalog(&mut chain).expect("load SpellFocusObject.dbc");

        assert_eq!(cat.name(1), Some("Anvil"));
        assert_eq!(cat.name(3), Some("Forge"));
        assert_eq!(cat.name(4), Some("Cooking Fire"));
        assert_eq!(cat.name(0), None, "0 = no requirement");
        assert_eq!(cat.len(), 138, "the 5875 file's full row count");
    }

    /// 2.4.3's table: 246 foci, the new Eversong Runestone and Altar of Aggonar among them.
    #[test]
    fn the_2_4_3_table_names_the_new_foci() {
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_spell_focus_catalog(&mut chain).expect("load SpellFocusObject");
        assert_eq!(cat.len(), 246);
        assert_eq!(cat.name(1), Some("Anvil"));
        assert_eq!(cat.name(1363), Some("Eversong Runestone"));
        assert_eq!(cat.name(1367), Some("Altar of Aggonar"));
    }
}
