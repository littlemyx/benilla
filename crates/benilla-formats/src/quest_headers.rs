//! Quest log header names. As in the 1.12 client, a quest's `ZoneOrSort` is an `AreaTable.dbc` id
//! when positive and a negated `QuestSort.dbc` id (class, profession, seasonal) when negative.

use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::chain::Chain;
use crate::dbc::{load_id_name_table, parse, slots, str_at, u32_at};

/// `ZoneOrSort` → header name.
#[derive(Debug, Default)]
pub struct QuestHeaderNames {
    zones: HashMap<u32, String>,
    sorts: HashMap<u32, String>,
}

impl QuestHeaderNames {
    /// The header title; `None` for 0 or an unknown id, where the caller picks its own bucket.
    pub fn resolve(&self, zone_or_sort: i32) -> Option<&str> {
        if zone_or_sort > 0 {
            self.zones.get(&(zone_or_sort as u32)).map(String::as_str)
        } else if zone_or_sort < 0 {
            self.sorts
                .get(&(zone_or_sort.unsigned_abs()))
                .map(String::as_str)
        } else {
            None
        }
    }
}

/// `AreaTable.dbc` id to area name, by the table's own schema.
fn load_zone_names(chain: &mut Chain) -> Result<HashMap<u32, String>> {
    let bytes = chain
        .read_file("DBFilesClient\\AreaTable.dbc")
        .context("reading AreaTable.dbc")?;
    let schema = crate::area_table::schema(chain.dbc_layout());
    let [name_slot] = slots(&schema, ["AreaName"])?;
    let rs = parse(&bytes, schema, "AreaTable")?;
    Ok(rs
        .records()
        .iter()
        .filter_map(|r| Some((u32_at(r, 0)?, str_at(&rs, r, name_slot)?)))
        .collect())
}

/// Load both name tables through the patch chain.
pub fn load_quest_header_names(chain: &mut Chain) -> Result<QuestHeaderNames> {
    Ok(QuestHeaderNames {
        zones: load_zone_names(chain)?,
        sorts: load_id_name_table(chain, "DBFilesClient\\QuestSort.dbc", 1, 10, "QuestSort")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2.4.3: a quest's zone header names an Outland zone, a negated sort a class.
    #[test]
    fn real_2_4_3_headers_name_outland_zones() {
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let names = load_quest_header_names(&mut chain).expect("load headers");
        assert_eq!(names.resolve(3483), Some("Hellfire Peninsula"));
        assert_eq!(names.resolve(12), Some("Elwynn Forest"));
        assert_eq!(names.resolve(-61), Some("Warlock"));
    }
}
