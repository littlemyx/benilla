//! `ItemDisplayInfo.dbc`: a display id to a held item's or worn equipment's visual identity. 23
//! fields and 92-byte records in 5875, both checked by the reference loader (`0x547590`).
//!
//! A row's model and texture are independently resolved basenames in the same
//! `Item\ObjectComponents\<dir>\` folder; never derive one from the other. The Worn Wooden Shield
//! pairs model `Shield_Round_A_01.mdx` with texture `Buckler_Damaged_A_01Purple`.

use std::collections::HashMap;

use crate::{Chain, DbcLayout};
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, RecordSet, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at, unread};
use crate::models::model_path;

const ITEM_DISPLAY_INFO: &str = "DBFilesClient\\ItemDisplayInfo.dbc";

/// One `ItemDisplayInfo` row: the visual identity of a held item or worn equipment. Basenames carry
/// no directory: the `Item\ObjectComponents\<dir>\` join belongs to the app, since `<dir>` follows
/// the inventory type, which is not a column here.
#[derive(Debug, Clone, Default)]
pub struct ItemDisplay {
    /// Left and right model basenames, normalized by [`crate::models::model_path`].
    pub model: [Option<String>; 2],
    /// Left and right texture basenames, raw and extensionless; never derived from [`Self::model`].
    pub model_texture: [Option<String>; 2],
    /// Worn-equipment geoset selectors, 0 where unauthored.
    pub geoset_groups: [u32; 3],
    /// Body-region textures, in ArmUpper, ArmLower, Hand, TorsoUpper, TorsoLower, LegUpper,
    /// LegLower, Foot order.
    pub region_textures: [Option<String>; 8],
    /// A helm's `HelmetGeosetVisData` rows, `[male, female]`, 0 for none (`0x4799a0` reads
    /// `+0x30 + sex * 4`).
    pub helmet_vis: [u32; 2],
    /// The inventory icon as an extensionless `Interface\Icons\…` path, ready to load.
    pub icon: Option<String>,
    /// The `ItemGroupSounds.dbc` pickup and place sound group (`0x458008`); 0 is silent, as the
    /// reference's bounds check returns (`0x45800b`).
    pub group_sounds: u32,
    /// The `SpellVisual.dbc` id a ranged-attribute spell borrows from the equipped ranged weapon,
    /// field by field where its own visual leaves a 0 (`VisualStages::merged_over_weapon`,
    /// `0x60d450`, read at `0x60d493`).
    pub spell_visual: u32,
    /// The flags word, the reference's `+0x24`. Only bit 0 is read, [`Self::takes_guild_emblem`];
    /// bit 1, set on four chest and robe rows, has no reader.
    pub flags: u32,
    /// The `ItemVisuals.dbc` id of the display's intrinsic weapon glow (`0x47a200`). Signed, as the
    /// reference's `jle` gate reads it (`0x4798c0`): 0 and `-1` both mean none.
    pub item_visual: i32,
}

impl ItemDisplay {
    /// The `HelmetGeosetVisData` pair this display hides hair and ears with, only when it names a
    /// left model: the reference's head-slot handler (`0x4799a0`) tests `ModelName[0]` for an empty
    /// string (`0x4799c1`) and skips the whole geoset-vis tail (`0x479b33`), never reading the
    /// right slot or the model's load result. Twelve model-less rows carry a mask that 126 NPC head
    /// displays point at, and the reference draws those NPCs' hair in full.
    pub fn worn_helm_vis(&self) -> Option<[u32; 2]> {
        self.model[0].is_some().then_some(self.helmet_vis)
    }

    /// Whether the wearer's guild emblem replaces this tabard's torso art (`flags & 1`, tested at
    /// `0x472ca4` on the tabard slot). Seven shipped rows set it, all tabards.
    pub fn takes_guild_emblem(&self) -> bool {
        self.flags & FLAG_GUILD_EMBLEM_TABARD != 0
    }
}

const FLAG_GUILD_EMBLEM_TABARD: u32 = 0x1;

/// `ItemDisplayInfo.dbc` by display id, the id `ItemDisplayInfoID` and
/// `UNIT_VIRTUAL_ITEM_SLOT_DISPLAY` name.
pub struct ItemDisplayCatalog {
    displays: HashMap<u32, ItemDisplay>,
}

impl ItemDisplayCatalog {
    /// A catalog from an explicit row map, for tests and fixtures.
    pub fn from_displays(displays: HashMap<u32, ItemDisplay>) -> Self {
        ItemDisplayCatalog { displays }
    }

    pub fn get(&self, display_id: u32) -> Option<&ItemDisplay> {
        self.displays.get(&display_id)
    }

    pub fn len(&self) -> usize {
        self.displays.len()
    }

    pub fn is_empty(&self) -> bool {
        self.displays.is_empty()
    }

    /// Every `(displayId, row)`, unordered.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &ItemDisplay)> {
        self.displays.iter().map(|(&id, d)| (id, d))
    }
}

/// The 1.12.1 table (the synthetic-record tests'): 23 columns, the unread ones included, so the field-count check against the
/// header is exact.
#[cfg(test)]
pub(crate) fn item_display_info_schema() -> Schema {
    item_display_info_schema_for(DbcLayout::VANILLA_1_12_1)
}

/// The columns for a layout: 2.4.3 has 25, inserting a second inventory icon after the first and
/// appending a particle colour, so every column from the geoset groups on sits one slot later
/// (measured: all 21 read columns match their shifted slot on the 29604 shared rows; the
/// definitions project's column list agrees).
pub(crate) fn item_display_info_schema_for(layout: DbcLayout) -> Schema {
    let mut s = layout.schema("ItemDisplayInfo");
    for (name, ty) in [
        ("ID", FieldType::UInt32),
        ("ModelNameLeft", FieldType::String),
        ("ModelNameRight", FieldType::String),
        ("ModelTextureLeft", FieldType::String),
        ("ModelTextureRight", FieldType::String),
        ("Icon", FieldType::String),
    ] {
        s.add_field(SchemaField::new(name, ty));
    }
    if layout.is_tbc() {
        s.add_field(SchemaField::new("IconSecond", FieldType::String));
    }
    for (name, ty) in [
        ("GeosetGroup0", FieldType::UInt32),
        ("GeosetGroup1", FieldType::UInt32),
        ("GeosetGroup2", FieldType::UInt32),
    ] {
        s.add_field(SchemaField::new(name, ty));
    }
    for name in [
        "Flags",
        "SpellVisualID",
        "ItemGroupSoundsID",
        "HelmVisMale",
        "HelmVisFemale",
    ] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    for name in [
        "ArmUpperTexture",
        "ArmLowerTexture",
        "HandTexture",
        "TorsoUpperTexture",
        "TorsoLowerTexture",
        "LegUpperTexture",
        "LegLowerTexture",
        "FootTexture",
    ] {
        s.add_field(SchemaField::new(name, FieldType::String));
    }
    s.add_field(SchemaField::new("ItemVisualID", FieldType::UInt32));
    if layout.is_tbc() {
        unread(&mut s, "ParticleColor", 1);
    }
    s
}

/// The catalog from a parsed record set, testable without a chain.
#[cfg(test)]
fn catalog_from_records(rs: RecordSet) -> ItemDisplayCatalog {
    catalog_from_records_in(rs, &item_display_info_schema())
}

/// The catalog from a record set parsed under `schema`, each column read at its slot there.
fn catalog_from_records_in(rs: RecordSet, schema: &Schema) -> ItemDisplayCatalog {
    let at = |name: &str| {
        schema
            .slot_of(name)
            .unwrap_or_else(|| panic!("ItemDisplayInfo schema has no column {name}"))
    };
    let (geoset, flags_slot, visual_slot, sounds_slot, helm_slot, region_slot, item_visual_slot) = (
        at("GeosetGroup0"),
        at("Flags"),
        at("SpellVisualID"),
        at("ItemGroupSoundsID"),
        at("HelmVisMale"),
        at("ArmUpperTexture"),
        at("ItemVisualID"),
    );
    let mut displays = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let model = [
            str_at(&rs, r, 1).map(|s| model_path(&s)),
            str_at(&rs, r, 2).map(|s| model_path(&s)),
        ];
        let model_texture = [str_at(&rs, r, 3), str_at(&rs, r, 4)];
        let geoset_groups = [
            u32_at(r, geoset).unwrap_or(0),
            u32_at(r, geoset + 1).unwrap_or(0),
            u32_at(r, geoset + 2).unwrap_or(0),
        ];
        let region_textures = std::array::from_fn(|i| str_at(&rs, r, region_slot + i));
        let helmet_vis = [
            u32_at(r, helm_slot).unwrap_or(0),
            u32_at(r, helm_slot + 1).unwrap_or(0),
        ];
        let icon = str_at(&rs, r, 5).map(|i| format!("Interface\\Icons\\{i}"));
        let group_sounds = u32_at(r, sounds_slot).unwrap_or(0);
        let spell_visual = u32_at(r, visual_slot).unwrap_or(0);
        let flags = u32_at(r, flags_slot).unwrap_or(0);
        let item_visual = u32_at(r, item_visual_slot).unwrap_or(0) as i32;
        displays.insert(
            id,
            ItemDisplay {
                model,
                model_texture,
                geoset_groups,
                region_textures,
                helmet_vis,
                icon,
                group_sounds,
                spell_visual,
                flags,
                item_visual,
            },
        );
    }
    ItemDisplayCatalog { displays }
}

/// Load `ItemDisplayInfo.dbc` off the patch chain into an [`ItemDisplayCatalog`].
pub fn load_item_display_catalog(chain: &mut Chain) -> Result<ItemDisplayCatalog> {
    let bytes = chain
        .read_file(ITEM_DISPLAY_INFO)
        .with_context(|| format!("reading {ITEM_DISPLAY_INFO}"))?;
    let schema = item_display_info_schema_for(chain.dbc_layout());
    let rs = parse(&bytes, schema.clone(), "ItemDisplayInfo")?;
    Ok(catalog_from_records_in(rs, &schema))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal synthetic WDBC: 20-byte header, fixed-width records, string block.
    fn build_wdbc(
        record_count: u32,
        field_count: u32,
        record_size: u32,
        records: &[u8],
        strings: &[u8],
    ) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"WDBC");
        b.extend_from_slice(&record_count.to_le_bytes());
        b.extend_from_slice(&field_count.to_le_bytes());
        b.extend_from_slice(&record_size.to_le_bytes());
        b.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        b.extend_from_slice(records);
        b.extend_from_slice(strings);
        b
    }

    const FIELD_COUNT: u32 = 23;
    const RECORD_SIZE: u32 = FIELD_COUNT * 4;
    const OFF_EMPTY: u32 = 0;

    fn u32le(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }

    /// A string block whose offset 0 is `""`, where an absent column points.
    #[derive(Default)]
    struct StringBlock(Vec<u8>);
    impl StringBlock {
        fn new() -> Self {
            Self(vec![0u8])
        }
        fn push(&mut self, s: &str) -> u32 {
            let off = self.0.len() as u32;
            self.0.extend_from_slice(s.as_bytes());
            self.0.push(0);
            off
        }
    }

    /// A projectile display carries its flight model in the right slot (`Ammo\`), a thrown weapon
    /// in the left (`Weapon\`). The missile spawner keys its folder on that shape; the reference
    /// keys the same fork on the ammo's `InventoryType` (`0x19` thrown, `0x60ba30`), with the same
    /// result on every shipped row.
    #[test]
    fn real_ammo_displays_carry_flight_models_right_thrown_left() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_item_display_catalog(&mut chain).expect("load ItemDisplayInfo");

        // Rough Arrow: flight model and skin in the right slot, the left empty.
        let arrow = cat.get(5996).expect("arrow display");
        assert_eq!(arrow.model, [None, Some("arrowflight_01.m2".into())]);
        assert_eq!(arrow.model_texture[1].as_deref(), Some("Arrow_A_01Brown"));
        // Light Shot: the bullet pair.
        let shot = cat.get(5998).expect("bullet display");
        assert_eq!(shot.model[1].as_deref(), Some("bulletflight_01.m2"));

        // Balanced Throwing Dagger: the weapon model, from `Item\ObjectComponents\Weapon\`, in the
        // left slot.
        let thrown = cat.get(16752).expect("thrown display");
        assert_eq!(thrown.model[0].as_deref(), Some("thrown_1h_dagger_a_01.m2"));
        assert_eq!(
            thrown.model_texture[0].as_deref(),
            Some("Thrown_1H_Dagger_A_01Copper")
        );
    }

    #[test]
    fn parses_model_and_texture_independently_and_skips_empty_columns() {
        let mut strings = StringBlock::new();
        let off_model = strings.push("Shield_Round_A_01.mdx");
        let off_texture = strings.push("Buckler_Damaged_A_01Purple");

        let mut rec = Vec::with_capacity(RECORD_SIZE as usize);
        rec.extend(u32le(18730)); // ID: the Worn Wooden Shield's displayId
        rec.extend(u32le(off_model)); // ModelNameLeft
        rec.extend(u32le(OFF_EMPTY)); // ModelNameRight: absent
        rec.extend(u32le(off_texture)); // ModelTextureLeft
        rec.extend(u32le(OFF_EMPTY)); // ModelTextureRight: absent
        rec.extend(u32le(OFF_EMPTY)); // Icon: absent
        rec.extend(u32le(1)); // GeosetGroup0
        rec.extend(u32le(0)); // GeosetGroup1
        rec.extend(u32le(0)); // GeosetGroup2
        rec.extend(u32le(0)); // Flags
        rec.extend(u32le(0)); // SpellVisualID
        rec.extend(u32le(21)); // ItemGroupSoundsID
        rec.extend(u32le(0)); // HelmVisMale
        rec.extend(u32le(0)); // HelmVisFemale
        for _ in 0..8 {
            rec.extend(u32le(OFF_EMPTY)); // region textures: none
        }
        rec.extend(u32le(0)); // ItemVisualID
        assert_eq!(rec.len(), RECORD_SIZE as usize);

        let bytes = build_wdbc(1, FIELD_COUNT, RECORD_SIZE, &rec, &strings.0);
        let rs = parse(&bytes, item_display_info_schema(), "test").expect("synthetic DBC parses");
        let catalog = catalog_from_records(rs);

        assert_eq!(catalog.len(), 1);
        let display = catalog.get(18730).expect("displayId 18730 present");

        // The model normalizer applied: lowercased, `.mdx` to `.m2`.
        assert_eq!(
            display.model[0].as_deref(),
            Some("shield_round_a_01.m2"),
            "model name normalized like any other model path"
        );
        assert_eq!(display.model[1], None, "empty ModelNameRight column ⇒ None");

        // The texture is resolved independently, never derived from the model name.
        assert_eq!(
            display.model_texture[0].as_deref(),
            Some("Buckler_Damaged_A_01Purple"),
            "model texture is unrelated to the model's own basename"
        );
        assert_eq!(display.model_texture[1], None);

        assert_eq!(display.geoset_groups, [1, 0, 0]);
        assert!(
            display.region_textures.iter().all(Option::is_none),
            "a held-item row paints no body regions"
        );
        assert_eq!(
            display.group_sounds, 21,
            "field 11 is the ItemGroupSounds id"
        );
    }

    #[test]
    fn unknown_display_id_misses() {
        let rec = vec![0u8; RECORD_SIZE as usize]; // ID 0, everything else empty or zero
        let bytes = build_wdbc(1, FIELD_COUNT, RECORD_SIZE, &rec, &StringBlock::new().0);
        let rs = parse(&bytes, item_display_info_schema(), "test").expect("synthetic DBC parses");
        let catalog = catalog_from_records(rs);
        assert_eq!(catalog.len(), 1);
        assert!(catalog.get(999).is_none());
    }

    /// 2.4.3's ItemDisplayInfo, a second icon column inserted: every read column on rows the build
    /// added and on rows both builds ship.
    #[test]
    fn real_2_4_3_item_displays_read_through_the_shifted_columns() {
        let data = crate::wow_data_tbc_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cat = load_item_display_catalog(&mut chain).expect("load ItemDisplayInfo");
        assert_eq!(cat.len(), 42208);

        // The buckler of the 1.12.1 test: the same row through the shifted columns.
        let buckler = cat.get(18730).expect("display 18730");
        assert_eq!(buckler.model[0].as_deref(), Some("shield_round_a_01.m2"));
        assert_eq!(
            buckler.model_texture[0].as_deref(),
            Some("Buckler_Damaged_A_01Purple")
        );
        assert_eq!(
            buckler.icon.as_deref(),
            Some("Interface\\Icons\\INV_Shield_09")
        );
        assert_eq!(buckler.group_sounds, 9);
        // A helm: the hide-mask pair, the sound group.
        let helm = cat.get(20255).expect("display 20255");
        assert_eq!(helm.helmet_vis, [248, 306]);
        assert_eq!(helm.worn_helm_vis(), Some([248, 306]));
        assert_eq!(helm.group_sounds, 10);
        // A rifle names a spell visual; a cloak a geoset group and an item visual; a tabard the
        // emblem flag; a bow its glow; one weapon the "none" glow -1.
        assert_eq!(cat.get(20504).expect("rifle").spell_visual, 224);
        let cloak = cat.get(23942).expect("cloak");
        assert_eq!((cloak.geoset_groups, cloak.item_visual), ([1, 0, 0], 42));
        assert!(cat.get(20621).expect("tabard").takes_guild_emblem());
        assert_eq!(cat.get(24929).expect("bow").item_visual, 104);
        assert_eq!(cat.get(20031).expect("horde sword").item_visual, -1);
        // A 2.x row: an Outland mail glove painted on the second region texture.
        let glove = cat.get(45000).expect("display 45000");
        assert_eq!(
            glove.region_textures[1].as_deref(),
            Some("Mail_RaidShaman_B_01OutlandGreen_Glove_AL")
        );
        assert!(glove.region_textures[0].is_none());
        assert_eq!(
            cat.get(1542).expect("shortsword").icon.as_deref(),
            Some("Interface\\Icons\\INV_Sword_04")
        );
    }
}
