//! `TaxiNodes.dbc`, every flight-master node: its map and position, name, and the taxi mount each
//! team rides from it. The layout is vmangos's `TaxiNodesEntry` (`DBCStructure.h:789-799`): `ID`,
//! `MapID`, `X`, `Y`, `Z`, the `Name` loc-string (5-13), then `MountCreatureID[2]`, Horde first.
//! The mounts are `creature_template` entries, not display ids, resolved like any NPC's.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, slots, str_at, u32_at};
use crate::{Chain, DbcLayout};

const TAXI_NODES: &str = "DBFilesClient\\TaxiNodes.dbc";

/// One `TaxiNodes.dbc` row, a flight-master node.
#[derive(Clone, Debug)]
pub struct TaxiNode {
    /// The id `SMSG_SHOWTAXINODES`'s known-mask and `CMSG_ACTIVATETAXI`'s nodes name.
    pub id: u32,
    /// The map this node's `pos` is expressed in (`Map.dbc` id).
    pub map_id: u32,
    /// World position `(x, y, z)` on `map_id`.
    pub pos: [f32; 3],
    /// The enUS name (locale slot 0), the taxi map's node label.
    pub name: String,
    /// The `creature_template` entry of a Horde rider's taxi mount here, `0` for no Horde service.
    pub mount_horde: u32,
    /// The `creature_template` entry of an Alliance rider's taxi mount here, `0` for none.
    pub mount_alliance: u32,
}

/// `TaxiNodes.dbc` rows keyed by `ID`.
pub struct TaxiNodes {
    rows: HashMap<u32, TaxiNode>,
}

impl TaxiNodes {
    pub fn get(&self, id: u32) -> Option<&TaxiNode> {
        self.rows.get(&id)
    }

    /// Every row, in no particular order.
    pub fn rows(&self) -> impl Iterator<Item = &TaxiNode> {
        self.rows.values()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

pub(crate) fn schema(layout: DbcLayout) -> Schema {
    let mut s = layout.schema("TaxiNodes");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("MapID", FieldType::UInt32));
    for name in ["X", "Y", "Z"] {
        s.add_field(SchemaField::new(name, FieldType::Float32));
    }
    s.add_field(SchemaField::new("Name", FieldType::LocString)); // enUS (locale 0)
    s.add_field(SchemaField::new("MountIdHorde", FieldType::UInt32));
    s.add_field(SchemaField::new("MountIdAlliance", FieldType::UInt32));
    s
}

/// Read `TaxiNodes.dbc` off the patch chain into a [`TaxiNodes`] catalog.
pub fn load_taxi_nodes(chain: &mut Chain) -> Result<TaxiNodes> {
    let bytes = chain
        .read_file(TAXI_NODES)
        .context("reading TaxiNodes.dbc")?;
    let schema = schema(chain.dbc_layout());
    let [map_slot, x_slot, y_slot, z_slot, name_slot, horde_slot, alliance_slot] = slots(
        &schema,
        [
            "MapID",
            "X",
            "Y",
            "Z",
            "Name",
            "MountIdHorde",
            "MountIdAlliance",
        ],
    )?;
    let rs = parse(&bytes, schema, "TaxiNodes")?;
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let (Some(x), Some(y), Some(z)) = (f32_at(r, x_slot), f32_at(r, y_slot), f32_at(r, z_slot))
        else {
            continue;
        };
        let node = TaxiNode {
            id,
            map_id: u32_at(r, map_slot).unwrap_or(0),
            pos: [x, y, z],
            name: str_at(&rs, r, name_slot).unwrap_or_default(),
            mount_horde: u32_at(r, horde_slot).unwrap_or(0),
            mount_alliance: u32_at(r, alliance_slot).unwrap_or(0),
        };
        rows.insert(id, node);
    }
    Ok(TaxiNodes { rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 5875 layout on Stormwind's node, which serves the Alliance only.
    #[test]
    fn real_taxi_nodes_layout_sanity() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_taxi_nodes(&mut chain).expect("load TaxiNodes");
        assert_eq!(cat.len(), 85, "1.12 ships 85 taxi nodes");

        let stormwind = cat.get(2).expect("node 2 exists");
        assert_eq!(stormwind.name, "Stormwind, Elwynn");
        assert_eq!(stormwind.map_id, 0, "Eastern Kingdoms");
        assert_eq!(stormwind.mount_horde, 0, "no Horde service from Stormwind");
        assert_eq!(
            stormwind.mount_alliance, 541,
            "Stormwind's Alliance gryphon mount template"
        );

        // The other end of the hop the `TaxiPath` test pins.
        let sentinel_hill = cat.get(4).expect("node 4 exists");
        assert_eq!(sentinel_hill.name, "Sentinel Hill, Westfall");
    }

    /// The 2.4.3 table: 189 nodes, Outland's on map 530 with each team's mount.
    #[test]
    fn real_2_4_3_taxi_nodes_reach_outland() {
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_taxi_nodes(&mut chain).expect("load TaxiNodes");
        assert_eq!(cat.len(), 189, "2.4.3 ships 189 taxi nodes");

        let thrallmar = cat.get(99).expect("node 99 exists");
        assert_eq!(thrallmar.name, "Thrallmar, Hellfire Peninsula");
        assert_eq!(thrallmar.map_id, 530, "Outland");
        assert_eq!(
            (thrallmar.mount_horde, thrallmar.mount_alliance),
            (2224, 0),
            "the Horde wyvern mount"
        );
        assert!(thrallmar.pos.iter().all(|c| c.is_finite() && *c != 0.0));
        let honor_hold = cat.get(100).expect("node 100 exists");
        assert_eq!(honor_hold.name, "Honor Hold, Hellfire Peninsula");
        assert_eq!(
            (honor_hold.mount_horde, honor_hold.mount_alliance),
            (0, 541),
            "the Alliance gryphon mount"
        );
        // A node both builds ship, read the same through the wider name.
        assert_eq!(cat.get(2).expect("node 2").name, "Stormwind, Elwynn");
    }
}
