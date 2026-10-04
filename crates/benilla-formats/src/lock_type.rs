//! `LockType.dbc`, a lock's interaction kind (Herbalism, Mining, Pick Lock, Fishing): its name and,
//! for three kinds, the cursor shown over a GameObject wearing it (`0x5f3070`). The GO cursor
//! resolves by data: template `lockId` → [`crate::LockCatalog`] row → its first slot's `LockType`
//! index → `CursorName`, which names the cursor BLP, or when empty falls through to the Interact
//! gear. `LockType` 1 (Pick Lock) is also the classifier's never-grayed case.

use std::collections::HashMap;

use crate::Chain;
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, slots, str_at, u32_at};
use crate::DbcLayout;

const LOCK_TYPE: &str = "DBFilesClient\\LockType.dbc";
/// `LockType.Id` → `CursorName` for the three rows with one, plus every row's `Name`.
pub struct LockTypeCatalog {
    cursors: HashMap<u32, String>,
    names: HashMap<u32, String>,
}

impl LockTypeCatalog {
    /// The `Interface\Cursor\<name>.blp` stem for a `LockType` index, `None` for the Interact
    /// gear. The index is the lock's raw first slot, so a key item or an empty `0` simply misses.
    pub fn cursor_name(&self, lock_type_id: u32) -> Option<&str> {
        self.cursors.get(&lock_type_id).map(String::as_str)
    }

    /// A lock kind's `Name` ("Herbalism"), filled into "Requires %s" (`0x5f34f9`); the caller
    /// supplies the reference's `"UNKNOWN"` for a missing row.
    pub fn name(&self, lock_type_id: u32) -> Option<&str> {
        self.names.get(&lock_type_id).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.cursors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cursors.is_empty()
    }
}

pub(crate) fn schema(layout: DbcLayout) -> Schema {
    let mut s = layout.schema("LockType");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Name", FieldType::LocString));
    s.add_field(SchemaField::new("ResourceName", FieldType::LocString));
    s.add_field(SchemaField::new("Verb", FieldType::LocString));
    // `CursorName`, the `[row+0x70]` in 5875 that `0x5f3070` reads.
    s.add_field(SchemaField::new("CursorName", FieldType::String));
    s
}

/// Load `LockType.dbc` from the patch chain into a [`LockTypeCatalog`].
pub fn load_lock_type_catalog(chain: &mut Chain) -> Result<LockTypeCatalog> {
    let bytes = chain
        .read_file(LOCK_TYPE)
        .with_context(|| format!("reading {LOCK_TYPE}"))?;
    let schema = schema(chain.dbc_layout());
    let [name_slot, cursor_slot] = slots(&schema, ["Name", "CursorName"])?;
    let rs = parse(&bytes, schema, "LockType.dbc")?;
    let mut cursors = HashMap::new();
    let mut names = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        if let Some(name) = str_at(&rs, r, cursor_slot) {
            cursors.insert(id, name);
        }
        if let Some(name) = str_at(&rs, r, name_slot) {
            names.insert(id, name);
        }
    }
    Ok(LockTypeCatalog { cursors, names })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 5875 rows: a column slip lands on another column and fails.
    #[test]
    fn real_lock_types_name_their_cursors() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_lock_type_catalog(&mut chain).expect("load LockType.dbc");

        assert_eq!(cat.cursor_name(1), Some("PickLock"));
        assert_eq!(cat.cursor_name(2), Some("GatherHerbs"));
        assert_eq!(cat.cursor_name(3), Some("Mine"));
        assert_eq!(cat.cursor_name(5), None); // Open; 13 is the keyless chest
        assert_eq!(cat.cursor_name(13), None);
        assert_eq!(cat.cursor_name(19), None); // Fishing
        assert_eq!(cat.cursor_name(0), None); // an empty lock slot's index
        assert_eq!(
            cat.len(),
            3,
            "only three LockType rows carry a CursorName in 5875"
        );

        // A slip to `ResourceName` (field 10) would print "Requires Herbs".
        assert_eq!(cat.name(1), Some("Pick Lock"));
        assert_eq!(cat.name(2), Some("Herbalism"));
        assert_eq!(cat.name(3), Some("Mining"));
        assert_eq!(cat.name(19), Some("Fishing"));
        assert_eq!(cat.name(0), None);
    }

    /// 2.4.3's table: 20 types, the new REUSEME, Lockpicking renamed, the cursor name read after
    /// three wide strings.
    #[test]
    fn the_2_4_3_types_read_their_names_and_cursors() {
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_lock_type_catalog(&mut chain).expect("LockType.dbc");
        assert_eq!(cat.len(), 3, "the rows with a cursor: Pick, Gather, Mine");
        assert_eq!(cat.name(1), Some("Lockpicking"), "5875: Pick Lock");
        assert_eq!(cat.cursor_name(1), Some("PickLock"));
        assert_eq!(cat.name(2), Some("Herbalism"));
        assert_eq!(cat.cursor_name(2), Some("GatherHerbs"));
        assert_eq!(cat.name(20), Some("REUSEME"));
        assert_eq!(cat.cursor_name(20), None);
    }
}
