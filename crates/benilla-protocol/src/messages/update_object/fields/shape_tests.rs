//! The groups the two builds lay out differently: the same logical content, written by hand into
//! each build's own structure, reads the same through the accessors.

use benilla_build::{TBC_2_4_3, VANILLA_1_12_1};

use super::*;

fn vanilla(object: ObjectType, pairs: &[(u16, u32)]) -> ObjectFields {
    assert_eq!(field_table(&VANILLA_1_12_1), &FIELDS_5875);
    ObjectFields::from_pairs_in(&FIELDS_5875, pairs).into_created(object)
}

fn tbc(object: ObjectType, pairs: &[(u16, u32)]) -> ObjectFields {
    assert_eq!(field_table(&TBC_2_4_3), &FIELDS_8606);
    ObjectFields::from_pairs_in(&FIELDS_8606, pairs).into_created(object)
}

// ---- auras ----

// Slot 0: spell 2457, level 1, one effect, cancelable; slot 5: spell 100, level 60, three
// stacks, effect 0, not cancelable; slot 40 (a debuff in both): spell 700, level 20, effect 0.
// 1.12.1: 48 slots, a nibble of flags (cancelable 0x1, effects 0x2/0x4/0x8). 2.4.3: 56 slots, a
// byte of flags (effects 0x1/0x2/0x4, cancelable 0x10, not cancelable 0x20).
fn auras_5875() -> ObjectFields {
    vanilla(
        ObjectType::Unit,
        &[
            (47, 2457),
            (52, 100),
            (87, 700),
            // nibble of slot s is bit 4*(s%8) of word 95 + s/8
            (95, 0x3 | (0x2 << 20)),
            (100, 0x2),
            (101, 1),
            (102, 60 << 8),
            (111, 20),
            (114, 2 << 8),
        ],
    )
}

fn auras_8606() -> ObjectFields {
    tbc(
        ObjectType::Unit,
        &[
            (48, 2457),
            (53, 100),
            (88, 700),
            // byte of slot s is bits 8*(s%4) of word 104 + s/4
            (104, 0x11),
            (105, 0x21 << 8),
            (114, 0x01),
            (118, 1),
            (119, 60 << 8),
            (128, 20),
            (133, 2 << 8),
            // `UNIT_FIELD_BYTES_2` byte 1: the debuff limit of a player
            (209, 40 << 8),
        ],
    )
}

#[test]
fn the_same_three_auras_read_alike_in_both_builds() {
    for f in [auras_5875(), auras_8606()] {
        let got: Vec<_> = f.unit_auras().collect();
        assert_eq!(
            got.iter().map(|a| a.slot).collect::<Vec<_>>(),
            [0, 5, 40],
            "{:?}",
            f.table().shape.aura_slots
        );
        assert_eq!(
            got.iter().map(|a| a.spell_id).collect::<Vec<_>>(),
            [2457, 100, 700]
        );
        assert_eq!(got.iter().map(|a| a.level).collect::<Vec<_>>(), [1, 60, 20]);
        assert_eq!(got.iter().map(|a| a.stacks).collect::<Vec<_>>(), [1, 3, 1]);
        assert_eq!(
            got.iter()
                .map(|a| f.unit_aura_is_cancelable(a))
                .collect::<Vec<_>>(),
            [true, false, false]
        );
        assert_eq!(
            got.iter()
                .map(|a| f.unit_aura_is_helpful(a))
                .collect::<Vec<_>>(),
            [true, true, false]
        );
    }
}

#[test]
fn an_aura_block_has_its_builds_slot_count() {
    let v = auras_5875();
    let t = auras_8606();
    assert_eq!(
        (v.unit_aura_slot_count(), t.unit_aura_slot_count()),
        (48, 56)
    );
    assert_eq!(v.unit_aura_ids().count(), 48);
    assert_eq!(t.unit_aura_ids().count(), 56);
    assert_eq!(v.unit_aura(48), None);
    assert_eq!(t.unit_aura(56), None);
    // The last slot of each build reads: slot 47 (nibble 7 of word 100) and slot 55 (byte 3 of
    // word 117).
    let v = vanilla(ObjectType::Unit, &[(47 + 47, 9), (100, 0x2 << 28)]);
    assert_eq!(v.unit_aura(47).map(|a| a.spell_id), Some(9));
    let t = tbc(ObjectType::Unit, &[(48 + 55, 9), (117, 0x01 << 24)]);
    assert_eq!(t.unit_aura(55).map(|a| a.spell_id), Some(9));
    // Slots 48..55 do not exist in 1.12.1: their index is another field's.
    assert_eq!(v.unit_aura(50), None);
}

#[test]
fn a_cleared_slot_with_a_stale_spell_id_is_not_live_in_either_build() {
    let v = vanilla(ObjectType::Unit, &[(47, 77)]);
    let t = tbc(ObjectType::Unit, &[(48, 77)]);
    assert_eq!(v.unit_aura(0), None);
    assert_eq!(t.unit_aura(0), None);
    // A flag byte with only the cancel bits is no effect bit in 2.4.3.
    let t = tbc(ObjectType::Unit, &[(48, 77), (104, 0x30)]);
    assert_eq!(t.unit_aura(0), None);
}

#[test]
fn the_2_4_3_aura_flag_byte_is_not_a_nibble() {
    // Slot 1's byte is bits 8..16 of word 104; a nibble reader would take bits 4..8.
    let t = tbc(ObjectType::Unit, &[(49, 5), (104, 0x05 << 8)]);
    let a = t.unit_aura(1).expect("slot 1");
    assert_eq!((a.spell_id, a.flags), (5, 0x05));
    assert_eq!(t.unit_aura(0), None);
}

#[test]
fn the_2_4_3_debuff_boundary_is_the_units_own_byte() {
    // A creature's limit is 16: slot 20 is a debuff on it and a buff on a player.
    let slot20 = |limit: Option<u32>| {
        let mut pairs = vec![(48 + 20, 5), (104 + 5, 0x01)];
        if let Some(l) = limit {
            pairs.push((209, l << 8));
        }
        let f = tbc(ObjectType::Unit, &pairs);
        let a = f.unit_aura(20).expect("slot 20");
        (f.unit_aura_positive_limit(), f.unit_aura_is_helpful(&a))
    };
    assert_eq!(slot20(Some(16)), (Some(16), false));
    assert_eq!(slot20(Some(40)), (Some(40), true));
    // Not carried (a delta without it): the constant `MAX_POSITIVE_AURAS`, never 0.
    assert_eq!(slot20(None), (None, true));
    assert_eq!(auras_5875().unit_aura_positive_limit(), Some(32));
}

// ---- bytes fields ----

#[test]
fn stand_loyalty_form_and_vis_flags_read_alike_in_both_builds() {
    // stand 1, loyalty 4, form 17, vis flags ghost | creep.
    let v = vanilla(
        ObjectType::Unit,
        &[(138, 1 | (4 << 8) | (17 << 16) | (0x3 << 24))],
    );
    // 2.4.3: BYTES_1 holds stand, loyalty, vis flags, misc flags (always-stand 0x1); the form is
    // BYTES_2 byte 3, with a sheath of 1 in byte 0.
    let t = tbc(
        ObjectType::Unit,
        &[
            (159, 1 | (4 << 8) | (0x3 << 16) | (0x1 << 24)),
            (209, 1 | (17 << 24)),
        ],
    );
    for f in [&v, &t] {
        assert_eq!(f.unit_stand_state(), 1);
        assert_eq!(f.unit_loyalty_level(), 4);
        assert_eq!(f.unit_shapeshift_form(), 17);
        assert!(f.unit_is_stealthed());
        assert!(f.unit_is_ghost_visual());
        assert!(!f.unit_is_untrackable());
    }
    assert_eq!(t.unit_sheath_state(), Some(1));
}

#[test]
fn a_2_4_3_misc_flag_is_never_read_as_a_vis_flag() {
    // Misc byte 0x07 (byte 3) with no vis flags (byte 2): a 1.12.1 reader would see all three.
    let t = tbc(ObjectType::Unit, &[(159, 0x07 << 24)]);
    assert!(!t.unit_is_stealthed());
    assert!(!t.unit_is_ghost_visual());
    assert!(!t.unit_is_untrackable());
    let t = tbc(ObjectType::Unit, &[(159, 0x04 << 16)]);
    assert!(t.unit_is_untrackable());
    // The form is not BYTES_1 byte 2 on 2.4.3.
    let t = tbc(ObjectType::Unit, &[(159, 0x11 << 16)]);
    assert_eq!(t.unit_shapeshift_form(), 0);
}

#[test]
fn player_bytes_3_keeps_the_drunk_byte_and_drops_the_honor_bytes() {
    // low half: gender 1 | drunk 0x6400; byte 2 = 2; byte 3 = 7.
    let word = 1 | (0x64 << 8) | (2 << 16) | (7 << 24);
    let v = vanilla(ObjectType::Player, &[(195, word)]);
    let t = tbc(ObjectType::Player, &[(241, word)]);
    assert_eq!(
        (v.player_drunk_byte(), t.player_drunk_byte()),
        (Some(0x64), Some(0x64))
    );
    assert_eq!(
        (v.player_pvp_rank(), v.player_pvp_medal()),
        (Some(7), Some(2))
    );
    // 2.4.3: byte 3 is the arena faction and byte 2 is unused.
    assert_eq!((t.player_pvp_rank(), t.player_pvp_medal()), (None, None));
}

#[test]
fn player_field_bytes_moves_combo_points_out_and_keeps_the_rest() {
    // flags track-stealthed | release timer, a byte 1 of 3, action bars 5, rank 9.
    let word = 0x0A | (3 << 8) | (5 << 16) | (9 << 24);
    let v = vanilla(ObjectType::Player, &[(1222, word)]);
    let t = tbc(ObjectType::Player, &[(1486, word)]);
    for f in [&v, &t] {
        assert!(f.player_track_stealthed());
        assert!(f.player_release_timer_running());
        assert_eq!(f.player_action_bar_toggles(), Some(5));
        assert_eq!(f.player_honor_rank(), Some(9));
    }
    assert_eq!(v.player_combo_points(), Some(3));
    // On 2.4.3 byte 1 is the refer-a-friend level, not combo points.
    assert_eq!(t.player_combo_points(), None);
}

#[test]
fn the_honor_block_keeps_what_has_the_same_meaning() {
    let v = vanilla(
        ObjectType::Player,
        &[(1257, 123), (1255, 77), (1250, 5 | (2 << 16)), (1260, 0x42)],
    );
    let t = tbc(
        ObjectType::Player,
        &[
            (1516, 123),
            (1517, 77),
            (1514, 5 | (2 << 16)),
            (1518, 0x42 << 8),
        ],
    );
    for f in [&v, &t] {
        assert_eq!(f.player_yesterday_contribution(), Some(123));
        assert_eq!(f.player_lifetime_honorable_kills(), Some(77));
    }
    assert_eq!(v.player_session_kills(), Some((5, 2)));
    assert_eq!(v.player_honor_rank_bar(), Some(0x42));
    // 2.4.3 has no dishonorable half, no weekly block and no rank bar.
    assert_eq!(t.player_session_kills(), None);
    assert_eq!(t.player_yesterday_kills(), None);
    assert_eq!(t.player_this_week_kills(), None);
    assert_eq!(t.player_this_week_contribution(), None);
    assert_eq!(t.player_last_week_contribution(), None);
    assert_eq!(t.player_last_week_rank(), None);
    assert_eq!(t.player_lifetime_dishonorable_kills(), None);
    assert_eq!(t.player_honor_rank_bar(), None);
}

#[test]
fn the_defaulting_readers_say_when_a_build_has_no_such_field() {
    let v = vanilla(ObjectType::Unit, &[(149, (6 << 16) | 2)]);
    assert_eq!(v.unit_training_points(), (6, 2));
    assert_eq!(v.unit_training_points_carried(), Some((6, 2)));
    // 2.4.3: one whole signed display value, not the pair, whatever the store holds at 170.
    let t = tbc(ObjectType::Unit, &[(170, 0xFFFF_FFFE)]);
    assert_eq!(t.unit_training_points(), (0, 0));
    assert_eq!(t.unit_training_points_carried(), None);
    let v = vanilla(ObjectType::Player, &[(714, 9), (715, 1)]);
    assert_eq!(v.player_combo_target_carried(), Some(9 | (1 << 32)));
    let t = tbc(ObjectType::Player, &[(1000, 9)]);
    assert_eq!(t.player_combo_target(), 0);
    assert_eq!(t.player_combo_target_carried(), None);
}

#[test]
fn virtual_item_info_reads_the_same_item_from_each_builds_byte_order() {
    // class 2 (weapon), subclass 7, material 3, inventory type 21, sheath 3, in slot 1.
    let v = vanilla(
        ObjectType::Unit,
        &[(42, 2 | (7 << 8) | (3 << 16) | (21 << 24)), (43, 3)],
    );
    let t = tbc(
        ObjectType::Unit,
        &[
            (42, 2 | (7 << 8) | (0xFF << 16) | (3 << 24)),
            (43, 21 | (3 << 8)),
        ],
    );
    for f in [&v, &t] {
        assert_eq!(f.unit_virtual_item_info(1), Some((2, 7, 3, 21)));
        assert_eq!(f.unit_virtual_item_sheath(1), Some(3));
        assert_eq!(f.unit_virtual_item_info(3), None);
    }
    // A delta carrying only the first dword is incomplete on 2.4.3, where the inventory type is
    // in the second.
    let t = ObjectFields::from_pairs_in(&FIELDS_8606, &[(42, 2 | (3 << 24))]);
    assert_eq!(t.unit_virtual_item_info(1), None);
}

// ---- quest log ----

// Slot 0 empty; slot 1: quest 4321, counters 3, 0, 12, 5, complete, timer 999.
#[test]
fn a_quest_log_slot_reads_alike_in_both_builds() {
    let v = vanilla(
        ObjectType::Player,
        &[
            (198 + 3, 4321),
            (198 + 4, 3 | (12 << 12) | (5 << 18) | (1 << 24)),
            (198 + 5, 999),
        ],
    );
    let t = tbc(
        ObjectType::Player,
        &[
            (244 + 4, 4321),
            (244 + 5, 1),
            (244 + 6, 3 | (12 << 16) | (5 << 24)),
            (244 + 7, 999),
        ],
    );
    let want = QuestLogSlot {
        quest_id: 4321,
        counters: [3, 0, 12, 5],
        state: quest_slot_state::COMPLETE,
        timer: 999,
    };
    assert_eq!(v.player_quest_log(1), Some(want));
    assert_eq!(t.player_quest_log(1), Some(want));
    let empty = QuestLogSlot {
        quest_id: 0,
        counters: [0; 4],
        state: 0,
        timer: 0,
    };
    assert_eq!(v.player_quest_log(0), Some(empty));
    assert_eq!(t.player_quest_log(0), Some(empty));
}

#[test]
fn the_quest_log_has_its_builds_slot_count_and_counter_width() {
    let v = vanilla(ObjectType::Player, &[]);
    let t = tbc(ObjectType::Player, &[]);
    assert_eq!(
        (
            v.player_quest_log_slot_count(),
            t.player_quest_log_slot_count()
        ),
        (20, 25)
    );
    assert!(v.player_quest_log(19).is_some() && v.player_quest_log(20).is_none());
    assert!(t.player_quest_log(24).is_some() && t.player_quest_log(25).is_none());
    // 2.4.3 counters are whole bytes: 200 reads 200, where 6 bits would cut it to 8.
    let t = tbc(
        ObjectType::Player,
        &[(244 + 24 * 4, 9), (244 + 24 * 4 + 2, 200)],
    );
    assert_eq!(t.player_quest_log(24).map(|q| q.counters[0]), Some(200));
    // The 2.4.3 state is its own dword: a fail bit with no counters.
    let t = tbc(ObjectType::Player, &[(248, 5), (249, 2)]);
    assert_eq!(
        t.player_quest_log(1).map(|q| q.state),
        Some(quest_slot_state::FAIL)
    );
}

// ---- visible items and item enchantments ----

// Equipment slot 3: entry 38, enchants 2564 and 7 in its first two slots, suffix 0x1234.
#[test]
fn a_visible_item_reads_alike_in_both_builds() {
    let v = vanilla(
        ObjectType::Player,
        &[
            // slot 3 starts at 258 + 3*12; creator 2, entry, enchants, properties at +10
            (258 + 36 + 2, 38),
            (258 + 36 + 3, 2564),
            (258 + 36 + 4, 7),
            (258 + 36 + 10, 0xABCD_1234),
        ],
    );
    let t = tbc(
        ObjectType::Player,
        &[
            // slot 3 starts at 344 + 3*16; properties at +14
            (344 + 48 + 2, 38),
            (344 + 48 + 3, 2564),
            (344 + 48 + 4, 7),
            (344 + 48 + 14, 0xABCD_1234),
        ],
    );
    for f in [&v, &t] {
        assert_eq!(f.player_visible_item_entry(3), Some(38));
        assert_eq!(f.player_visible_item_enchant(3, 0), Some(2564));
        assert_eq!(f.player_visible_item_enchant(3, 1), Some(7));
        assert_eq!(f.player_visible_item_enchant(3, 2), None);
        assert_eq!(f.player_visible_item_properties(3), 0x1234);
        assert_eq!(f.player_visible_item_entry(2), None);
        assert_eq!(f.player_visible_item_entry(4), None);
    }
}

#[test]
fn visible_items_have_their_builds_stride_and_enchant_count() {
    let v = vanilla(ObjectType::Player, &[(258 + 18 * 12 + 2, 99)]);
    let t = tbc(ObjectType::Player, &[(344 + 18 * 16 + 2, 99)]);
    assert_eq!(
        (
            v.player_visible_item_entry(18),
            t.player_visible_item_entry(18)
        ),
        (Some(99), Some(99))
    );
    assert_eq!(v.player_visible_item_entry(19), None);
    assert_eq!(
        (
            v.player_visible_item_enchant_slot_count(),
            t.player_visible_item_enchant_slot_count()
        ),
        (7, 11)
    );
    // The eleventh enchant field of a 2.4.3 slot is the last before the properties.
    let t = tbc(ObjectType::Player, &[(344 + 3 + 10, 55)]);
    assert_eq!(t.player_visible_item_enchant(0, 10), Some(55));
    assert_eq!(t.player_visible_item_enchant(0, 11), None);
    let v = vanilla(ObjectType::Player, &[(258 + 3 + 6, 55)]);
    assert_eq!(v.player_visible_item_enchant(0, 6), Some(55));
    assert_eq!(v.player_visible_item_enchant(0, 7), None);
}

#[test]
fn item_enchantments_have_their_builds_slot_count() {
    // Permanent id 2564 (slot 0), charges 3 in slot 1, and the last slot of each build.
    let v = vanilla(
        ObjectType::Item,
        &[(22, 2564), (22 + 3 + 2, 3), (22 + 18, 77)],
    );
    let t = tbc(
        ObjectType::Item,
        &[(22, 2564), (22 + 3 + 2, 3), (22 + 30, 77)],
    );
    for f in [&v, &t] {
        assert_eq!(f.item_enchant(0), Some(2564));
        assert_eq!(f.item_enchant(1), None);
        assert_eq!(f.item_enchant_charges(1), 3);
    }
    assert_eq!(
        (v.item_enchant_slot_count(), t.item_enchant_slot_count()),
        (7, 11)
    );
    assert_eq!(v.item_enchant(6), Some(77));
    assert_eq!(v.item_enchant(7), None);
    assert_eq!(t.item_enchant(10), Some(77));
    assert_eq!(t.item_enchant(11), None);
}

// ---- inventory, bank, explored zones ----

#[test]
fn the_bank_has_its_builds_slot_counts() {
    let guid = |lo: u16| [(lo, 0xAA), (lo + 1, 0x4000)];
    let g = 0xAA | (0x4000u64 << 32);
    let mut v: Vec<(u16, u32)> = guid(564 + 46).to_vec(); // slot 23
    v.extend(guid(612 + 10)); // bag slot 5
    let v = vanilla(ObjectType::Player, &v);
    let mut t: Vec<(u16, u32)> = guid(728 + 54).to_vec(); // slot 27
    t.extend(guid(784 + 12)); // bag slot 6
    let t = tbc(ObjectType::Player, &t);
    assert_eq!(
        (v.player_bank_slot_count(), t.player_bank_slot_count()),
        (24, 28)
    );
    assert_eq!(
        (
            v.player_bank_bag_slot_count(),
            t.player_bank_bag_slot_count()
        ),
        (6, 7)
    );
    assert_eq!(v.player_bank_slot(23), Some(g));
    assert_eq!(v.player_bank_slot(24), None);
    assert_eq!(t.player_bank_slot(27), Some(g));
    assert_eq!(t.player_bank_slot(28), None);
    assert_eq!(v.player_bank_bag_slot(5), Some(g));
    assert_eq!(v.player_bank_bag_slot(6), None);
    assert_eq!(t.player_bank_bag_slot(6), Some(g));
    assert_eq!(t.player_bank_bag_slot(7), None);
}

#[test]
fn equipment_and_pack_slots_keep_their_counts_in_both_builds() {
    // Not reshaped: 23 equipment and bag slots, a 16-slot backpack, 12 buyback slots.
    let v = vanilla(ObjectType::Player, &[(486 + 6, 5), (532 + 2, 6)]);
    let t = tbc(ObjectType::Player, &[(650 + 6, 5), (696 + 2, 6)]);
    for f in [&v, &t] {
        assert_eq!(f.player_inv_slot(3), Some(5));
        assert_eq!(f.player_inv_slot(23), None);
        assert_eq!(f.player_pack_slot(1), Some(6));
        assert_eq!(f.player_pack_slot(16), None);
        assert_eq!(f.player_buyback_slot(12), None);
    }
}

#[test]
fn explored_zones_have_their_builds_word_count() {
    // The last word of each build.
    let v = vanilla(ObjectType::Player, &[(1111 + 63, 0x8000_0001)]);
    let t = tbc(
        ObjectType::Player,
        &[(1332 + 127, 0x8000_0001), (1332 + 100, 4)],
    );
    assert_eq!(
        (
            v.player_explored_zone_count(),
            t.player_explored_zone_count()
        ),
        (64, 128)
    );
    assert_eq!(v.player_explored_zone_slot(63), 0x8000_0001);
    assert_eq!(v.player_explored_zone_slot(64), 0);
    assert_eq!(t.player_explored_zone_slot(127), 0x8000_0001);
    assert_eq!(t.player_explored_zone_slot(100), 4);
    assert_eq!(t.player_explored_zone_slot(128), 0);
}

// ---- table sanity ----

/// `(name, first index, length)` of the large arrays of a build's unit and player blocks.
type Blocks = Vec<(&'static str, u16, u16)>;

fn arrays(t: &FieldTable) -> (Blocks, Blocks) {
    let s = &t.shape;
    let unit = vec![
        ("aura", t.unit_aura, u16::from(s.aura_slots)),
        (
            "auraflags",
            t.unit_auraflags,
            u16::from(s.aura_slots) * u16::from(s.aura_flag_bits) / 32,
        ),
        ("auralevels", t.unit_auralevels, u16::from(s.aura_slots) / 4),
        (
            "auraapps",
            t.unit_auraapplications,
            u16::from(s.aura_slots) / 4,
        ),
    ];
    let player = vec![
        (
            "quest log",
            t.player_quest_log_1_1,
            u16::from(s.quest_slots) * u16::from(s.quest_stride),
        ),
        (
            "visible items",
            t.player_visible_item_1_creator,
            19 * s.visible_stride,
        ),
        ("equipment", t.player_inv_slot_head, 46),
        ("pack", t.player_pack_slot_1, 32),
        ("bank", t.player_bank_slot_1, 2 * u16::from(s.bank_slots)),
        (
            "bank bags",
            t.player_bank_bag_slot_1,
            2 * u16::from(s.bank_bag_slots),
        ),
        ("buyback", t.player_vendorbuyback_slot_1, 24),
        ("keyring", t.player_keyring_slot_1, 64),
        ("skills", t.player_skill_info_1_1, 384),
        ("explored", t.player_explored_zones_1, s.explored_zone_words),
    ];
    (unit, player)
}

#[test]
fn the_shaped_blocks_end_inside_their_descriptor_and_do_not_overlap() {
    for t in [&FIELDS_5875, &FIELDS_8606] {
        let (unit, player) = arrays(t);
        for (blocks, end) in [(unit, t.unit_end), (player, t.player_end)] {
            for &(name, start, len) in &blocks {
                assert_ne!(start, FieldTable::ABSENT, "{name}");
                assert!(start + len <= end, "{name}: {start} + {len} > {end}");
            }
            for (i, &(a, a0, al)) in blocks.iter().enumerate() {
                for &(b, b0, bl) in &blocks[i + 1..] {
                    assert!(a0 + al <= b0 || b0 + bl <= a0, "{a} and {b} overlap");
                }
            }
        }
        // The item enchantments end before the item's own tail.
        assert!(
            t.item_enchantment + 3 * u16::from(t.shape.item_enchant_slots)
                <= t.item_random_properties_id
        );
        // A visible item's enchants end before its properties word, which ends inside the stride.
        assert_eq!(
            3 + u16::from(t.shape.visible_enchant_slots),
            t.shape.visible_properties
        );
        assert!(t.shape.visible_properties + 2 <= t.shape.visible_stride);
        // The quest-log counters, timer and state all lie inside a slot.
        let s = &t.shape;
        assert!(s.quest_counts < s.quest_stride && s.quest_timer < s.quest_stride);
        assert!(u16::from(s.quest_counter_bits) * 4 <= 32);
    }
}

#[test]
fn every_byte_triple_names_a_field_of_its_group() {
    for t in [&FIELDS_5875, &FIELDS_8606] {
        let s = &t.shape;
        for (name, loc, field) in [
            ("stand", s.stand_state, t.unit_bytes_1),
            ("loyalty", s.loyalty, t.unit_bytes_1),
            ("vis", s.vis_flags, t.unit_bytes_1),
            ("flags byte", s.player_flags_byte, t.player_field_bytes),
            ("action bars", s.action_bar_toggles, t.player_field_bytes),
            ("honor rank", s.honor_rank, t.player_field_bytes),
            ("drunk", s.drunk, t.player_bytes_3),
        ] {
            assert_eq!(loc.field, field, "{name}");
            assert!(loc.byte < 4, "{name}");
        }
        // Dropped sub-fields are NONE, never half-absent.
        for loc in [s.combo_points, s.pvp_rank, s.pvp_medal, s.honor_rank_bar] {
            assert!(loc == ByteAt::NONE || (loc.field != FieldTable::ABSENT && loc.byte < 4));
        }
    }
    assert_eq!(
        FIELDS_5875.shape.shapeshift_form.field,
        FIELDS_5875.unit_bytes_1
    );
    assert_eq!(
        FIELDS_8606.shape.shapeshift_form.field,
        FIELDS_8606.unit_bytes_2
    );
    assert_eq!(
        FIELDS_5875.shape.combo_points.field,
        FIELDS_5875.player_field_bytes
    );
    assert_eq!(FIELDS_5875.shape.pvp_rank.field, FIELDS_5875.player_bytes_3);
    assert_eq!(
        FIELDS_5875.shape.honor_rank_bar.field,
        FIELDS_5875.player_field_bytes2
    );
    assert_eq!(
        FIELDS_8606.shape.aura_positive,
        AuraSplit::Byte(ByteAt::new(FIELDS_8606.unit_bytes_2, 1), 40)
    );
}
