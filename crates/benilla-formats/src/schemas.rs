//! The registry of the client database tables benilla reads: every table name with the schema (or,
//! for the two hand-parsed ones, the header shape) its loader checks the file against. The dump
//! (`dbc_to_csv`) and the walk test (`tests::every_table_fits_its_install`) read it, so a table a
//! loader reads cannot go unchecked against an install. A table with a `LocString` column takes
//! the build's [`DbcLayout`]; the rest are one schema for every build, and the walk test says which
//! of those a build's file still fits.

use benilla_dbc::Schema;

use crate::dbc::DbcLayout;
use crate::{
    anim_data, area_poi, area_sound, area_table, area_trigger, auction_house, bank_bag_slot_prices,
    camera_shakes, cfg_categories, characters, chat_channels, chr_classes, cinematics,
    creature_families, creature_sound, creature_types, creatures, dbc, death_thud, durability,
    elevators, emote_text, emotes, environmental_damage, exhaustion, factions, footsteps,
    game_tips, gameobjects, gm_ticket_category, ground_effects, item_random_properties,
    item_sounds, item_visuals, itembagfamily, itemclass, items, itemsets, itemsubclass, languages,
    light, loading_screen, lock, lock_type, maps, material, npc_greeting, packages,
    page_text_material, pet_stats, server_messages, sheathe, skill_lines, sound_entries,
    sound_provider, sound_water, spell_focus, spell_mechanic, spell_visual, spells,
    stable_slot_prices, stationery, talents, taxi, taxi_nodes, taxi_path, text_filter_lists,
    unit_blood, vocal_ui_sounds, weapon_impact, weapon_swing, wmo_area, world_map_area,
    world_map_continent, world_map_overlay, world_state_ui,
};

/// How a table's layout is checked against its file's header.
pub(crate) enum Shape {
    /// A schema built for a layout; its expanded length is the header's field count.
    Schema(fn(DbcLayout) -> Schema),
    /// Parsed by hand off the record block, with the header checked to this shape (sub-dword
    /// fields: the record size is not four bytes a field). The walk test reads the shape.
    #[cfg_attr(not(test), allow(dead_code))]
    Hand { field_count: u32, record_size: u32 },
}

/// One table, as one loader reads it. A table several loaders read has one entry per distinct
/// schema, so each is checked.
pub(crate) struct Table {
    pub name: &'static str,
    pub shape: Shape,
}

/// A table whose schema does not depend on the build's layout.
macro_rules! plain {
    ($name:literal, $schema:expr) => {
        Table {
            name: $name,
            shape: Shape::Schema(|_| $schema),
        }
    };
}

/// A table whose schema takes the build's layout (it has a `LocString` column).
macro_rules! wide {
    ($name:literal, $schema:expr) => {
        Table {
            name: $name,
            shape: Shape::Schema($schema),
        }
    };
}

pub(crate) static TABLES: &[Table] = &[
    plain!("AnimationData", anim_data::schema()),
    plain!("AreaPOI", area_poi::schema()),
    plain!("AreaTable", area_table::schema()),
    plain!("AreaTable", area_sound::area_schema()),
    Table {
        name: "AreaTable",
        shape: Shape::Schema(|l| dbc::id_name_schema("AreaTable", l, 11, 25)),
    },
    plain!("AreaTrigger", area_trigger::area_trigger_schema()),
    plain!("AuctionHouse", auction_house::auction_house_schema()),
    plain!("BankBagSlotPrices", bank_bag_slot_prices::schema()),
    plain!("CameraShakes", camera_shakes::camera_shakes_schema()),
    plain!("Cfg_Categories", cfg_categories::cfg_categories_schema()),
    plain!(
        "CharacterFacialHairStyles",
        characters::char_facial_hair_schema()
    ),
    // `load_combos` refuses any header but 2 fields of 2 bytes.
    Table {
        name: "CharBaseInfo",
        shape: Shape::Hand {
            field_count: 2,
            record_size: 2,
        },
    },
    plain!("CharHairGeosets", characters::char_hair_geosets_schema()),
    plain!("CharSections", characters::char_sections_schema()),
    // `load_start_outfits` refuses any header but 41 fields of 152 bytes.
    Table {
        name: "CharStartOutfit",
        shape: Shape::Hand {
            field_count: 41,
            record_size: 152,
        },
    },
    wide!("ChatChannels", chat_channels::schema),
    plain!("ChatProfanity", text_filter_lists::schema("ChatProfanity")),
    plain!("ChrClasses", chr_classes::schema()),
    plain!("ChrRaces", languages::chr_races_schema()),
    plain!("ChrRaces", factions::chr_races_schema()),
    plain!("CinematicCamera", cinematics::cameras_schema()),
    plain!("CinematicSequences", cinematics::sequences_schema()),
    plain!(
        "CreatureDisplayInfo",
        creatures::creature_display_info_schema()
    ),
    plain!(
        "CreatureDisplayInfoExtra",
        creatures::creature_display_info_extra_schema()
    ),
    plain!("CreatureFamily", creature_families::family_schema()),
    plain!("CreatureModelData", creatures::creature_model_data_schema()),
    plain!("CreatureSoundData", creature_sound::csd_schema()),
    wide!("CreatureType", creature_types::creature_type_schema),
    plain!(
        "DeathThudLookups",
        death_thud::n_u32_schema("DeathThudLookups", 5)
    ),
    plain!("DurabilityCosts", durability::costs_schema()),
    plain!("DurabilityQuality", durability::qualities_schema()),
    plain!("Emotes", emotes::schema("Emotes", 7, &[1])),
    plain!("EmotesText", emote_text::emotes_text_schema()),
    plain!("EmotesTextData", emote_text::emotes_text_data_schema()),
    plain!("EmotesTextSound", emotes::schema("EmotesTextSound", 5, &[])),
    plain!("EnvironmentalDamage", environmental_damage::schema()),
    plain!("Exhaustion", exhaustion::exhaustion_schema()),
    plain!("Faction", factions::faction_schema()),
    plain!("FactionGroup", factions::faction_group_schema()),
    plain!("FactionTemplate", factions::faction_template_schema()),
    plain!(
        "FootprintTextures",
        footsteps::n_u32_schema("FootprintTextures", 2, &[1])
    ),
    plain!(
        "FootstepTerrainLookup",
        footsteps::n_u32_schema("FootstepTerrainLookup", 5, &[])
    ),
    plain!("GameObjectDisplayInfo", gameobjects::schema()),
    plain!("GameTips", game_tips::schema()),
    plain!(
        "GMTicketCategory",
        gm_ticket_category::gm_ticket_category_schema()
    ),
    plain!("GroundEffectDoodad", ground_effects::doodad_schema()),
    plain!("GroundEffectTexture", ground_effects::texture_schema()),
    plain!("HelmetGeosetVisData", characters::helmet_vis_schema()),
    plain!("ItemBagFamily", itembagfamily::item_bag_family_schema()),
    plain!("ItemClass", itemclass::item_class_schema()),
    plain!("ItemDisplayInfo", items::item_display_info_schema()),
    plain!("ItemGroupSounds", item_sounds::item_group_sounds_schema()),
    plain!("ItemPetFood", creature_families::food_schema()),
    plain!(
        "ItemRandomProperties",
        item_random_properties::item_random_properties_schema()
    ),
    plain!("ItemSet", itemsets::item_set_schema()),
    plain!("ItemSubClass", itemsubclass::item_sub_class_schema()),
    plain!(
        "ItemSubClassMask",
        itemsubclass::item_sub_class_mask_schema()
    ),
    plain!(
        "ItemVisualEffects",
        item_visuals::item_visual_effects_schema()
    ),
    plain!("ItemVisuals", item_visuals::item_visuals_schema()),
    plain!("Languages", languages::languages_schema()),
    plain!("LanguageWords", languages::language_words_schema()),
    plain!("Light", light::light_schema()),
    plain!("LightFloatBand", light::float_band_schema()),
    plain!("LightIntBand", light::int_band_schema()),
    plain!("LightParams", light::light_params_schema()),
    plain!("LightSkybox", light::light_skybox_schema()),
    plain!("LoadingScreens", loading_screen::schema()),
    plain!("Lock", lock::schema()),
    plain!("LockType", lock_type::schema()),
    plain!("Map", maps::map_schema()),
    plain!("Material", material::schema()),
    plain!("NPCSounds", npc_greeting::npcsounds_schema()),
    plain!("Package", packages::package_schema()),
    plain!(
        "PageTextMaterial",
        page_text_material::page_text_material_schema()
    ),
    plain!("PetLoyalty", pet_stats::loyalty_schema()),
    plain!("PetPersonality", pet_stats::personality_schema()),
    Table {
        name: "QuestInfo",
        shape: Shape::Schema(|l| dbc::id_name_schema("QuestInfo", l, 1, 10)),
    },
    Table {
        name: "QuestSort",
        shape: Shape::Schema(|l| dbc::id_name_schema("QuestSort", l, 1, 10)),
    },
    plain!("ServerMessages", server_messages::schema()),
    plain!("SheatheSoundLookups", sheathe::schema()),
    plain!("SkillLine", skill_lines::skill_line_schema()),
    plain!("SkillLineAbility", skill_lines::skill_line_ability_schema()),
    plain!(
        "SkillLineCategory",
        skill_lines::skill_line_category_schema()
    ),
    plain!(
        "SkillRaceClassInfo",
        skill_lines::skill_race_class_info_schema()
    ),
    plain!("SoundAmbience", area_sound::ambience_schema()),
    plain!("SoundEntries", sound_entries::sound_entries_schema()),
    plain!("SoundProviderPreferences", sound_provider::schema()),
    plain!("SoundWaterType", sound_water::schema()),
    plain!("SpamMessages", text_filter_lists::schema("SpamMessages")),
    plain!("Spell", spells::spell_schema()),
    plain!("SpellCastTimes", spells::spell_cast_times_schema()),
    plain!("SpellCategory", spells::spell_category_schema()),
    plain!(
        "SpellChainEffects",
        spell_visual::chain_effects::chain_effects_schema()
    ),
    plain!("SpellDispelType", spells::spell_dispel_type_schema()),
    plain!("SpellDuration", spells::spell_duration_schema()),
    plain!(
        "SpellEffectCameraShakes",
        camera_shakes::spell_effect_camera_shakes_schema()
    ),
    plain!("SpellFocusObject", spell_focus::schema()),
    plain!("SpellIcon", dbc::spell_icon_schema()),
    plain!(
        "SpellItemEnchantment",
        item_visuals::spell_item_enchantment_schema()
    ),
    plain!("SpellMechanic", spell_mechanic::schema()),
    plain!("SpellRadius", spells::spell_radius_schema()),
    plain!("SpellRange", spells::spell_range_schema()),
    plain!("SpellShapeshiftForm", spells::shapeshift_form_schema()),
    plain!("SpellVisual", spell_visual::spell_visual_schema()),
    plain!("SpellVisualEffectName", spell_visual::effect_name_schema()),
    plain!("SpellVisualKit", spell_visual::kit_schema()),
    plain!("StableSlotPrices", stable_slot_prices::schema()),
    plain!("Stationery", stationery::stationery_schema()),
    plain!("Talent", talents::talent_schema()),
    wide!("TalentTab", talents::talent_tab_schema),
    wide!("TaxiNodes", taxi_nodes::schema),
    plain!("TaxiPath", taxi_path::schema()),
    plain!("TaxiPathNode", taxi::schema()),
    plain!(
        "TerrainType",
        footsteps::n_u32_schema("TerrainType", 6, &[1])
    ),
    plain!(
        "TerrainTypeSounds",
        death_thud::n_u32_schema("TerrainTypeSounds", 1)
    ),
    plain!("TransportAnimation", elevators::schema()),
    plain!("UnitBlood", unit_blood::unit_blood_schema()),
    plain!("UnitBloodLevels", unit_blood::unit_blood_levels_schema()),
    plain!("VocalUISounds", vocal_ui_sounds::vocal_ui_sounds_schema()),
    plain!("WeaponImpactSounds", weapon_impact::schema()),
    plain!("WeaponSwingSounds2", weapon_swing::schema()),
    plain!("WMOAreaTable", wmo_area::schema()),
    plain!("WorldMapArea", world_map_area::schema()),
    plain!("WorldMapContinent", world_map_continent::schema()),
    plain!("WorldMapOverlay", world_map_overlay::schema()),
    plain!("WorldStateUI", world_state_ui::schema()),
    plain!("ZoneIntroMusicTable", area_sound::intro_schema()),
    plain!("ZoneMusic", area_sound::zone_music_schema()),
];

/// The schema of the first entry for `table` (a base name, `.dbc` or not, any case), for `layout`.
pub(crate) fn schema(table: &str, layout: DbcLayout) -> Option<Schema> {
    let table = table.strip_suffix(".dbc").unwrap_or(table);
    TABLES
        .iter()
        .filter(|t| t.name.eq_ignore_ascii_case(table))
        .find_map(|t| match t.shape {
            Shape::Schema(build) => Some(build(layout)),
            Shape::Hand { .. } => None,
        })
}

/// Every registered table name, once each, in registry order.
pub(crate) fn names() -> impl Iterator<Item = &'static str> {
    TABLES
        .iter()
        .enumerate()
        .filter(|(i, t)| !TABLES[..*i].iter().any(|u| u.name == t.name))
        .map(|(_, t)| t.name)
}
