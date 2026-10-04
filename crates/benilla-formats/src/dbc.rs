//! Shared helpers for the typed DBC loaders. A DBC carries no column types, so each loader supplies
//! a [`Schema`] whose field count must match the file header, and reads fields by index.

use std::collections::HashMap;
use std::io::Cursor;

use anyhow::{anyhow, Context, Result};
use benilla_build::{ClientBuild, Expansion};
use benilla_dbc::{
    DbcParser, FieldType, Record, RecordSet, Schema, SchemaField, StringRef, Value,
    LOC_SLOTS_1_12_1, LOC_SLOTS_2_4_3,
};

use crate::Chain;

/// How a build lays its tables out: the width of a localized string, and the expansion whose
/// column lists a table follows where 2.4.3 inserted or appended columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DbcLayout {
    /// Slots of one localized string: the locale offsets and the flags word.
    pub loc_slots: usize,
    /// The expansion whose column lists the tables follow.
    pub expansion: Expansion,
}

impl DbcLayout {
    /// 1.12.1: 8 locale slots and a flags word.
    pub const VANILLA_1_12_1: DbcLayout = DbcLayout {
        loc_slots: LOC_SLOTS_1_12_1,
        expansion: Expansion::Vanilla,
    };
    /// 2.4.3: 16 locale slots and a flags word.
    pub const TBC_2_4_3: DbcLayout = DbcLayout {
        loc_slots: LOC_SLOTS_2_4_3,
        expansion: Expansion::Tbc,
    };

    /// Whether this is the 2.4.3 layout, whose tables carry columns 1.12.1's lack.
    pub(crate) fn is_tbc(self) -> bool {
        self.expansion == Expansion::Tbc
    }

    /// An empty schema named `name` whose localized strings are this layout's width.
    pub(crate) fn schema(self, name: &str) -> Schema {
        Schema::new(name).with_loc_slots(self.loc_slots)
    }

    /// The layout of a build's tables, or `None` for an expansion with none measured yet.
    pub fn of(build: &ClientBuild) -> Option<Self> {
        match build.expansion {
            Expansion::Vanilla => Some(Self::VANILLA_1_12_1),
            Expansion::Tbc => Some(Self::TBC_2_4_3),
            _ => None,
        }
    }
}

/// Parse a DBC with `schema`; `what` names the file in errors.
pub(crate) fn parse(bytes: &[u8], schema: Schema, what: &str) -> Result<RecordSet> {
    let parser = DbcParser::parse(&mut Cursor::new(bytes))
        .map_err(|e| anyhow!("parsing {what} header: {e}"))?;
    let parser = parser
        .with_schema(schema)
        .map_err(|e| anyhow!("applying {what} schema (field-count mismatch?): {e}"))?;
    parser
        .parse_records()
        .map_err(|e| anyhow!("parsing {what} records: {e}"))
}

/// `n` columns this loader does not read, one field named `name` (an array when `n > 1`): a gap
/// that keeps the slots after it where the file has them.
pub(crate) fn unread(s: &mut Schema, name: &str, n: usize) {
    if n > 0 {
        s.add_field(SchemaField::new_array(name, FieldType::UInt32, n));
    }
}

/// The expanded slot of each named column, resolved once per load and not per row. A name the
/// schema lacks is the loader's own mistake, so it errors by name.
pub(crate) fn slots<const N: usize>(schema: &Schema, names: [&str; N]) -> Result<[usize; N]> {
    let mut out = [0; N];
    for (slot, name) in out.iter_mut().zip(names) {
        *slot = schema
            .slot_of(name)
            .ok_or_else(|| anyhow!("{} schema has no column {name}", schema.name))?;
    }
    Ok(out)
}

/// An unsigned field, from either int variant: the schema tag only picks the decoding.
pub(crate) fn u32_at(r: &Record, i: usize) -> Option<u32> {
    match r.get_value(i)? {
        Value::UInt32(v) => Some(*v),
        Value::Int32(v) => Some(*v as u32),
        _ => None,
    }
}

pub(crate) fn f32_at(r: &Record, i: usize) -> Option<f32> {
    match r.get_value(i)? {
        Value::Float32(v) => Some(*v),
        _ => None,
    }
}

/// A signed field; either int variant reads, bit for bit.
pub(crate) fn i32_at(r: &Record, i: usize) -> Option<i32> {
    match r.get_value(i)? {
        Value::Int32(v) => Some(*v),
        Value::UInt32(v) => Some(*v as i32),
        _ => None,
    }
}

/// A string field from the string block; an empty string reads as `None`.
pub(crate) fn str_at(rs: &RecordSet, r: &Record, i: usize) -> Option<String> {
    match r.get_value(i)? {
        Value::StringRef(StringRef(off)) => {
            let s = rs.get_string(StringRef(*off)).ok()?;
            (!s.is_empty()).then(|| s.to_string())
        }
        _ => None,
    }
}

pub(crate) fn spell_icon_schema() -> Schema {
    let mut schema = Schema::new("SpellIcon");
    schema.add_field(SchemaField::new("ID", FieldType::UInt32));
    schema.add_field(SchemaField::new("TextureFilename", FieldType::String));
    schema
}

/// `SpellIcon.dbc`: id → extensionless texture path (`Interface\Icons\…`).
pub(crate) fn load_spell_icon_map(chain: &mut Chain) -> Result<HashMap<u32, String>> {
    let bytes = chain
        .read_file("DBFilesClient\\SpellIcon.dbc")
        .context("reading SpellIcon.dbc")?;
    let set = parse(&bytes, spell_icon_schema(), "SpellIcon.dbc")?;
    let mut icons = HashMap::new();
    for r in set.records() {
        if let (Some(id), Some(path)) = (u32_at(r, 0), str_at(&set, r, 1)) {
            icons.insert(id, path);
        }
    }
    Ok(icons)
}

/// An id → name schema: `name_col` plain columns, the localized `Name`, then plain columns up to
/// `cols`, the table's width in 1.12.1 slots (a localized string counted as 9); the other locale
/// slots and the flags word are the `LocString`'s own, whatever the build's width.
pub(crate) fn id_name_schema(
    what: &str,
    layout: DbcLayout,
    name_col: usize,
    cols: usize,
) -> Schema {
    let mut s = layout.schema(what);
    for i in 0..name_col {
        s.add_field(SchemaField::new(format!("c{i}"), FieldType::UInt32));
    }
    s.add_field(SchemaField::new("Name", FieldType::LocString));
    for i in name_col + LOC_SLOTS_1_12_1..cols {
        s.add_field(SchemaField::new(format!("c{i}"), FieldType::UInt32));
    }
    s
}

/// Read an id → name DBC into a map, skipping rows whose name is empty (a hole, not a name).
pub(crate) fn load_id_name_table(
    chain: &mut Chain,
    file: &str,
    name_col: usize,
    cols: usize,
    what: &str,
) -> Result<HashMap<u32, String>> {
    let bytes = chain
        .read_file(file)
        .with_context(|| format!("reading {file}"))?;
    let schema = id_name_schema(what, chain.dbc_layout(), name_col, cols);
    let [name_slot] = slots(&schema, ["Name"])?;
    let rs = parse(&bytes, schema, what)?;
    let mut out = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        if let (Some(id), Some(name)) = (u32_at(r, 0), str_at(&rs, r, name_slot)) {
            out.insert(id, name);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shared id/name helper on QuestSort, the table that reaches it without AreaTable: 34
    /// rows in 5875, 35 in 8606, the name read through a 9- and a 17-slot `LocString`.
    #[test]
    fn the_id_name_table_reads_quest_sorts_at_both_widths() {
        let file = "DBFilesClient\\QuestSort.dbc";
        if let Some(data) = crate::wow_data() {
            let mut chain = crate::open_chain(&data).expect("open chain");
            let sorts = load_id_name_table(&mut chain, file, 1, 10, "QuestSort").expect("5875");
            assert_eq!(sorts.len(), 34);
            assert_eq!(sorts.get(&61).map(String::as_str), Some("Warlock"));
        }
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let sorts = load_id_name_table(&mut chain, file, 1, 10, "QuestSort").expect("8606");
        assert_eq!(sorts.len(), 35);
        assert_eq!(sorts.get(&61).map(String::as_str), Some("Warlock"));
        assert_eq!(sorts.get(&25).map(String::as_str), Some("Battlegrounds"));
    }

    #[test]
    fn the_id_name_schema_keeps_its_name_slot_and_widens_after_it() {
        let narrow = id_name_schema("AreaTable", DbcLayout::VANILLA_1_12_1, 11, 25);
        let wide = id_name_schema("AreaTable", DbcLayout::TBC_2_4_3, 11, 25);
        assert_eq!(narrow.expanded_len(), 25);
        assert_eq!(wide.expanded_len(), 33);
        assert_eq!(narrow.slot_of("Name"), Some(11));
        assert_eq!(wide.slot_of("Name"), Some(11));
    }
}
