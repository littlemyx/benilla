//! `ChrRaces.dbc` column 3: the sound the client plays on `SMSG_EXPLORATION_EXPERIENCE`, by the
//! player's race (`0x5e41d2`: row `[0xc0dee0][race]`, offset `0xc`, played at `0x458850`).

use std::collections::HashMap;

use crate::Chain;
use anyhow::{Context, Result};

use crate::dbc::{parse, slots, u32_at};

const CHR_RACES: &str = "DBFilesClient\\ChrRaces.dbc";

/// Race (the `UNIT_FIELD_BYTES_0` race byte) → exploration `SoundEntries` id; a 0 is absent.
pub struct ExplorationSoundCatalog {
    by_race: HashMap<u32, u32>,
}

impl ExplorationSoundCatalog {
    pub fn kit(&self, race: u32) -> Option<u32> {
        self.by_race.get(&race).copied()
    }

    pub fn len(&self) -> usize {
        self.by_race.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_race.is_empty()
    }
}

/// Load the race → exploration-sound map off the patch chain.
pub fn load_exploration_sound_catalog(chain: &mut Chain) -> Result<ExplorationSoundCatalog> {
    let bytes = chain
        .read_file(CHR_RACES)
        .with_context(|| format!("reading {CHR_RACES}"))?;
    let schema = crate::factions::chr_races_schema(chain.dbc_layout());
    let [kit_slot] = slots(&schema, ["ExploreSound"])?;
    let rs = parse(&bytes, schema, "ChrRaces")?;
    let mut by_race = HashMap::new();
    for r in rs.records() {
        let (Some(id), Some(kit)) = (u32_at(r, 0), u32_at(r, kit_slot)) else {
            continue;
        };
        if kit != 0 {
            by_race.insert(id, kit);
        }
    }
    Ok(ExplorationSoundCatalog { by_race })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_chr_races_exploration_kits() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_exploration_sound_catalog(&mut chain).expect("load ChrRaces");
        for race in 1..=8u32 {
            let kit = cat.kit(race);
            eprintln!("race {race}: exploration kit {kit:?}");
            assert!(kit.is_some(), "race {race} has no exploration sound");
        }
        // The shipped kits run 4140-4147; Human and Orc guard the column position.
        assert_eq!(cat.kit(1), Some(4140));
        assert_eq!(cat.kit(2), Some(4141));
    }

    /// 2.4.3: ten races carry a kit, the Blood Elf with the Undead's and the Draenei the Human's.
    #[test]
    fn real_2_4_3_exploration_kits_cover_the_new_races() {
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_exploration_sound_catalog(&mut chain).expect("load ChrRaces");
        assert_eq!(cat.len(), 10);
        assert_eq!(cat.kit(1), Some(4140));
        assert_eq!(cat.kit(10), Some(4142));
        assert_eq!(cat.kit(11), Some(4140));
        assert_eq!(cat.kit(9), None, "Goblin has none");
    }
}
