//! Creature voice: `CreatureDisplayInfo.SoundID` to `CreatureSoundData.dbc`, 30 `u32` columns of
//! per-display voice kits whose names are the community's, since a DBC carries none. Column 22
//! (`NPCSoundID`) is empty on every row and unread. A display whose own `SoundID` is 0, nearly
//! every one, resolves through `CreatureModelData.SoundID` (col 13), as the client does.

use std::collections::HashMap;

use crate::{Chain, DbcLayout};
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, slots, u32_at, unread};

/// One `CreatureSoundData` row; every field is a `SoundEntries` kit id (0 for none) unless noted.
pub struct CreatureVoice {
    /// Attack grunt `[normal, critical]`.
    pub exertion: [u32; 2],
    /// Wound vocal `[normal, critical, crushing]`.
    pub injury: [u32; 3],
    pub death: u32,
    pub stun: u32,
    /// Fired by the `$FDX` anim event.
    pub stand: u32,
    /// The footstep class (`FootstepTerrainLookup.CreatureFootstepID`), not a kit id.
    pub footstep_class: u32,
    pub aggro: u32,
    /// Fired by `$WNG` / `$WGG`.
    pub wing_flap: u32,
    pub wing_glide: u32,
    pub alert: u32,
    /// Fired by `$FD1`..`$FD4`.
    pub fidget: [u32; 4],
    /// Fired by `$AH0`..`$AH3`.
    pub custom_attack: [u32; 4],
    /// A looping body sound (SoundEntries type 27).
    pub loop_sound: u32,
    /// Melee impact material (`WeaponImpactSounds`): 0 flesh, 1 stone, 2 wood, 3 ethereal.
    pub impact_type: u32,
    pub jump_start: u32,
    pub jump_end: u32,
    /// Column 27, the bark on an attack order: `SMSG_PET_ACTION_SOUND`'s `PET_TALK_ATTACK`, bark
    /// state 2 of `0x623a40` (`0x623ad3` reads `[row+0x6c]`).
    pub pet_attack: u32,
    /// Column 28, the bark acknowledging an ordered spell: `PET_TALK_SPECIAL_SPELL`, bark state 1
    /// (`0x623ac8` reads `[row+0x70]`).
    pub pet_order: u32,
    /// Column 29, `SMSG_PET_DISMISS_SOUND`'s parting line: `0x604140` reads `[row+0x74]`
    /// (`0x6041c4`, `0x6041dc`) and plays it at a bare world position, since the pet is gone.
    pub pet_dismiss: u32,
}

/// Voice rows by display id through `CreatureDisplayInfo.SoundID`, and by model id through
/// `CreatureModelData.SoundID` for the dismiss sound.
pub struct CreatureVoiceCatalog {
    display_to_sound: HashMap<u32, u32>,
    model_to_sound: HashMap<u32, u32>,
    rows: HashMap<u32, CreatureVoice>,
}

impl CreatureVoiceCatalog {
    /// The voice set for a creature display id (`UNIT_FIELD_DISPLAYID`).
    pub fn for_display(&self, display_id: u32) -> Option<&CreatureVoice> {
        self.rows.get(self.display_to_sound.get(&display_id)?)
    }

    /// The voice set for a `CreatureModelData` id through its own `SoundID` (col 13), with no
    /// display step and no fallback: `SMSG_PET_DISMISS_SOUND` names a model, and `0x604140`
    /// resolves it so (`[[0xc0de68] + id*4] + 0x34`, then `[[0xc0de54] + sound*4]`).
    pub fn for_model(&self, model_id: u32) -> Option<&CreatureVoice> {
        self.rows.get(self.model_to_sound.get(&model_id)?)
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// `CreatureSoundData.dbc`: 30 fields in 1.12.1, 37 in 2.4.3, which inserts a fifth fidget kit
/// after the four and appends six columns (a fidget delay pair, birth, directed-cast and two
/// submerge sounds). Everything from the custom attacks on sits one slot later (measured against
/// the 402 shared rows: each moved column matches at 0.985 to 1.0 on its non-zero rows; columns
/// that are zero on every row, 5, 22, 25 and 26 of 1.12.1, sit by the same shift, as the
/// definitions project's column list has them). A few kits were retuned, which the lower matches show.
pub(crate) fn csd_schema(layout: DbcLayout) -> Schema {
    let mut s = layout.schema("CreatureSoundData");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new_array("Exertion", FieldType::UInt32, 2));
    s.add_field(SchemaField::new_array("Injury", FieldType::UInt32, 3));
    for name in [
        "Death",
        "Stun",
        "Stand",
        "Footstep",
        "Aggro",
        "WingFlap",
        "WingGlide",
        "Alert",
    ] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    s.add_field(SchemaField::new_array("Fidget", FieldType::UInt32, 4));
    if layout.is_tbc() {
        unread(&mut s, "FifthFidget", 1);
    }
    s.add_field(SchemaField::new_array("CustomAttack", FieldType::UInt32, 4));
    unread(&mut s, "NpcSound", 1);
    for name in ["LoopSound", "ImpactType", "JumpStart", "JumpEnd"] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    s.add_field(SchemaField::new_array("Pet", FieldType::UInt32, 3));
    if layout.is_tbc() {
        unread(&mut s, "Appended", 6);
    }
    s
}

/// Read the three tables off the patch chain into the joined catalog.
pub fn load_creature_voice_catalog(chain: &mut Chain) -> Result<CreatureVoiceCatalog> {
    let bytes = chain
        .read_file("DBFilesClient\\CreatureSoundData.dbc")
        .context("reading CreatureSoundData.dbc")?;
    let schema = csd_schema(chain.dbc_layout());
    let [exertion, injury, death, fidget, custom, loop_sound, jump, pet] = slots(
        &schema,
        [
            "Exertion",
            "Injury",
            "Death",
            "Fidget",
            "CustomAttack",
            "LoopSound",
            "JumpStart",
            "Pet",
        ],
    )?;
    let rs = parse(&bytes, schema, "CreatureSoundData")?;
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let g = |i: usize| u32_at(r, i).unwrap_or(0);
        rows.insert(
            id,
            CreatureVoice {
                exertion: [g(exertion), g(exertion + 1)],
                injury: [g(injury), g(injury + 1), g(injury + 2)],
                death: g(death),
                stun: g(death + 1),
                stand: g(death + 2),
                footstep_class: g(death + 3),
                aggro: g(death + 4),
                wing_flap: g(death + 5),
                wing_glide: g(death + 6),
                alert: g(death + 7),
                fidget: [g(fidget), g(fidget + 1), g(fidget + 2), g(fidget + 3)],
                custom_attack: [g(custom), g(custom + 1), g(custom + 2), g(custom + 3)],
                loop_sound: g(loop_sound),
                impact_type: g(loop_sound + 1),
                jump_start: g(jump),
                jump_end: g(jump + 1),
                pet_attack: g(pet),
                pet_order: g(pet + 1),
                pet_dismiss: g(pet + 2),
            },
        );
    }

    let bytes = chain
        .read_file("DBFilesClient\\CreatureModelData.dbc")
        .context("reading CreatureModelData.dbc")?;
    let rs = parse(
        &bytes,
        crate::creatures::creature_model_data_schema(chain.dbc_layout()),
        "CreatureModelData",
    )?;
    let mut model_to_sound = HashMap::new();
    for r in rs.records() {
        if let (Some(id), Some(sound)) = (u32_at(r, 0), u32_at(r, 13)) {
            if sound != 0 {
                model_to_sound.insert(id, sound);
            }
        }
    }

    let bytes = chain
        .read_file("DBFilesClient\\CreatureDisplayInfo.dbc")
        .context("reading CreatureDisplayInfo.dbc")?;
    let rs = parse(
        &bytes,
        crate::creatures::creature_display_info_schema(chain.dbc_layout()),
        "CreatureDisplayInfo",
    )?;
    let mut display_to_sound = HashMap::new();
    for r in rs.records() {
        let (Some(id), Some(sound), Some(model)) = (u32_at(r, 0), u32_at(r, 2), u32_at(r, 1))
        else {
            continue;
        };
        // The display's own `SoundID` wins; 0 falls back to the model's.
        let sound = if sound != 0 {
            Some(sound)
        } else {
            model_to_sound.get(&model).copied()
        };
        if let Some(sound) = sound {
            display_to_sound.insert(id, sound);
        }
    }
    Ok(CreatureVoiceCatalog {
        display_to_sound,
        model_to_sound,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_creature_voice_resolves() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_creature_voice_catalog(&mut chain).expect("load creature voices");
        assert_eq!(cat.len(), 406, "all CreatureSoundData rows load");

        let v = cat.for_display(26).expect("display 26 has a voice");
        assert_eq!(v.death, 314);
        assert_eq!(v.exertion, [312, 313]);
        assert_eq!(v.footstep_class, 8);
        assert_eq!(v.aggro, 694);

        // The model fallback: the Elwynn wolf (903) and a human male (49) have display `SoundID` 0.
        let wolf = cat.for_display(903).expect("wolf resolves via the model");
        assert_eq!(wolf.footstep_class, 8);
        let human = cat
            .for_display(49)
            .expect("human male resolves via the model");
        assert_eq!(human.footstep_class, 7);
    }
    /// Only four rows carry columns 27-29, and their kits are named `_KILL`, `_ORDER` and
    /// `_DISMISS` in column order; a hunter pet's are 0, so it is silent, as in the reference.
    #[test]
    fn only_the_four_demon_voices_carry_pet_barks() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_creature_voice_catalog(&mut chain).expect("load creature voices");
        let kits = crate::load_sound_kit_catalog(&mut chain).expect("load SoundEntries");

        let named = |id: u32| kits.get(id).map(|k| k.name.clone()).unwrap_or_default();
        let mut carrying: Vec<(u32, String, String, String)> = cat
            .rows
            .iter()
            .filter(|(_, v)| v.pet_attack != 0 || v.pet_order != 0 || v.pet_dismiss != 0)
            .map(|(id, v)| {
                (
                    *id,
                    named(v.pet_attack),
                    named(v.pet_order),
                    named(v.pet_dismiss),
                )
            })
            .collect();
        carrying.sort_unstable();
        // Casing is the file's own; the kit lookup is case-insensitive.
        assert_eq!(
            carrying,
            vec![
                (
                    11,
                    "A_IMP_KILL".into(),
                    "A_IMP_ORDER".into(),
                    "A_Imp_Dismiss".into()
                ),
                (
                    37,
                    "A_SUCCUBUS_KILL".into(),
                    "A_SUCCUBUS_ORDER".into(),
                    "A_SUCCUBUS_DISMISS".into()
                ),
                (
                    68,
                    "A_DOOMGUARD_KILL".into(),
                    "A_DOOMGUARD_ORDER".into(),
                    "A_DOOMGUARD_DISMISS01".into()
                ),
                (
                    162,
                    "A_VOIDWALKER_KILL".into(),
                    "A_VOIDWALKER_ORDER".into(),
                    "A_VOIDWALKER_DISMISS".into()
                ),
            ],
        );

        let imp = cat.for_display(904).expect("an imp display resolves");
        assert_eq!(
            (imp.pet_attack, imp.pet_order, imp.pet_dismiss),
            (9097, 9098, 9096)
        );
        let wolf = cat.for_display(903).expect("the Elwynn wolf resolves");
        assert_eq!(
            (wolf.pet_attack, wolf.pet_order, wolf.pet_dismiss),
            (0, 0, 0)
        );

        // The dismiss sound's model join (`0x604140`) reaches the imp's row too.
        let by_model = cat
            .for_model(
                *cat.model_to_sound
                    .iter()
                    .find(|(_, s)| **s == 11)
                    .expect("a model resolves the imp voice")
                    .0,
            )
            .expect("model join");
        assert_eq!(by_model.pet_dismiss, 9096);
    }

    /// 2.4.3's voice kits: a fifth fidget slot is inserted, so the custom attacks, loop sound and
    /// pet barks sit one slot later; rows both builds ship, a 2.x race and the demon barks.
    #[test]
    fn the_2_4_3_creature_voices_read_through_the_inserted_fidget() {
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_creature_voice_catalog(&mut chain).expect("load creature voices");
        assert_eq!(cat.len(), 799);

        let v = cat.for_display(26).expect("display 26 has a voice");
        assert_eq!(
            (v.death, v.exertion, v.footstep_class, v.aggro),
            (314, [312, 0], 8, 694)
        );
        let wolf = cat.for_display(903).expect("the Elwynn wolf");
        assert_eq!((wolf.exertion[0], wolf.fidget[0]), (391, 1018));
        let human = cat.for_display(49).expect("a human male");
        assert_eq!(
            (human.exertion, human.death, human.footstep_class),
            ([2941, 186], 2944, 7)
        );
        // New in 2.x: the Blood Elf and Draenei males, voiced through their models.
        let belf = cat.for_display(15476).expect("a Blood Elf male");
        assert_eq!((belf.exertion[0], belf.death), (8996, 8999));
        assert_eq!(cat.for_display(16125).map(|v| v.death), Some(8987));
        // The slots after the inserted fidget.
        assert_eq!(cat.rows[&1].custom_attack, [0, 7374, 0, 0]);
        assert_eq!(cat.rows[&43].custom_attack, [3176; 4]);
        assert_eq!(
            (cat.rows[&24].loop_sound, cat.rows[&69].loop_sound),
            (1454, 5634)
        );
        assert_eq!(cat.rows[&37].fidget, [0, 0, 1121, 0]);
        let imp = cat.for_display(904).expect("an imp");
        assert_eq!(
            (imp.pet_attack, imp.pet_order, imp.pet_dismiss),
            (9097, 9098, 9096)
        );
        let barking: std::collections::BTreeSet<u32> = cat
            .rows
            .iter()
            .filter(|(_, v)| v.pet_attack != 0 || v.pet_order != 0 || v.pet_dismiss != 0)
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(
            barking,
            [11, 37, 68, 162, 348, 488, 2257].into_iter().collect()
        );
    }
}
