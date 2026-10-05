//! 2.4.3 byte tests for the packets of world entry, the queries and time sync: each fixture is built
//! from the packet's field list. A packet whose bytes equal 1.12.1's is also read through the 1.12.1
//! dispatch to show both give the same value; the allow-list test names every opcode it may read.

use benilla_build::{TBC_2_4_3, VANILLA_1_12_1};
use benilla_protocol::messages::{
    self, chat_tag, opcode, parse_server_with_tail_for, tbc_opcode as t, ServerPacket, TbcPacket,
    FIELDS_5875,
};
use benilla_protocol::wire::write_packed_guid;

fn u32b(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn f32b(v: f32) -> [u8; 4] {
    v.to_le_bytes()
}

fn cstr(s: &str) -> Vec<u8> {
    let mut b = s.as_bytes().to_vec();
    b.push(0);
    b
}

fn packed(guid: u64) -> Vec<u8> {
    let mut b = Vec::new();
    write_packed_guid(guid, &mut b).unwrap();
    b
}

/// Read `body` as 2.4.3 `op`; the whole body must be consumed.
fn tbc(op: u16, body: &[u8]) -> ServerPacket {
    let (packet, tail) = parse_server_with_tail_for(&TBC_2_4_3, None, op, body).unwrap();
    assert_eq!(tail, 0, "{} left {tail} bytes unread", packet.name());
    packet
}

fn vanilla(op: u16, body: &[u8]) -> ServerPacket {
    let (packet, tail) =
        parse_server_with_tail_for(&VANILLA_1_12_1, Some(&FIELDS_5875), op, body).unwrap();
    assert_eq!(tail, 0, "{} left {tail} bytes unread", packet.name());
    packet
}

fn tbc_tbcpacket(op: u16, body: &[u8]) -> TbcPacket {
    match tbc(op, body) {
        ServerPacket::Tbc(p) => p,
        other => panic!("{} is not a Tbc packet", other.name()),
    }
}

// --- login state --------------------------------------------------------------------------------

#[test]
fn login_time_and_speed_read_the_same_in_both_builds() {
    // 02:57, day 5 (index 4) of October (index 9) of 2026 (year 26), the live packet.
    let packed_time = 57 | (2 << 6) | (4 << 14) | (9 << 20) | (26 << 24);
    let mut body = u32b(packed_time).to_vec();
    body.extend(f32b(0.016_666_67));
    for packet in [tbc(t::SMSG_LOGIN_SETTIMESPEED, &body), vanilla(0x42, &body)] {
        match packet {
            ServerPacket::TimeSpeed {
                hours,
                minutes,
                day_serial,
                timescale,
            } => {
                assert_eq!((hours, minutes), (2, 57));
                assert_eq!(day_serial, 26 * 372 + 9 * 31 + 4);
                assert!((timescale - 0.016_666_67).abs() < 1e-9);
            }
            other => panic!("{}", other.name()),
        }
    }
}

#[test]
fn account_data_times_is_128_bytes_and_a_short_body_is_an_error() {
    let mut body = vec![0u8; 128];
    body[16] = 7;
    match tbc_tbcpacket(t::SMSG_ACCOUNT_DATA_TIMES, &body) {
        TbcPacket::AccountDataTimes { data } => assert_eq!((data.len(), data[16]), (128, 7)),
        other => panic!("{}", other.name()),
    }
    assert!(
        parse_server_with_tail_for(&TBC_2_4_3, None, t::SMSG_ACCOUNT_DATA_TIMES, &[0; 127])
            .is_err()
    );
}

#[test]
fn feature_status_spam_records_and_motd() {
    match tbc_tbcpacket(t::SMSG_FEATURE_SYSTEM_STATUS, &[2, 0]) {
        TbcPacket::FeatureSystemStatus {
            complaint_mode,
            voice_chat_enabled,
        } => assert_eq!((complaint_mode, voice_chat_enabled), (2, false)),
        other => panic!("{}", other.name()),
    }
    let mut spam = u32b(2).to_vec();
    spam.extend(cstr("a"));
    spam.extend(cstr("bc"));
    match tbc_tbcpacket(t::SMSG_EXPECTED_SPAM_RECORDS, &spam) {
        TbcPacket::ExpectedSpamRecords { records } => assert_eq!(records, ["a", "bc"]),
        other => panic!("{}", other.name()),
    }
    let mut motd = u32b(2).to_vec();
    motd.extend(cstr("Welcome."));
    motd.extend(cstr("Second line."));
    match tbc_tbcpacket(t::SMSG_MOTD, &motd) {
        TbcPacket::Motd { lines } => assert_eq!(lines, ["Welcome.", "Second line."]),
        other => panic!("{}", other.name()),
    }
    // A count the body cannot hold is an error, not a huge allocation.
    let mut lying = u32b(u32::MAX).to_vec();
    lying.extend(cstr("x"));
    assert!(parse_server_with_tail_for(&TBC_2_4_3, None, t::SMSG_MOTD, &lying).is_err());
}

#[test]
fn tutorial_flags_and_bind_point_read_the_same_in_both_builds() {
    let flags: Vec<u8> = (0..32).collect();
    for packet in [
        tbc(t::SMSG_TUTORIAL_FLAGS, &flags),
        vanilla(opcode::SMSG_TUTORIAL_FLAGS, &flags),
    ] {
        match packet {
            ServerPacket::TutorialFlags(f) => assert_eq!(f.bytes, flags),
            other => panic!("{}", other.name()),
        }
    }
    let mut bind = Vec::new();
    for v in [-8949.95f32, -132.493, 83.5312] {
        bind.extend(f32b(v));
    }
    bind.extend(u32b(0));
    bind.extend(u32b(12));
    for packet in [
        tbc(t::SMSG_BINDPOINTUPDATE, &bind),
        vanilla(opcode::SMSG_BINDPOINTUPDATE, &bind),
    ] {
        match packet {
            ServerPacket::BindPoint {
                position,
                map,
                area,
            } => {
                assert_eq!(
                    (position.x, position.y, position.z),
                    (-8949.95, -132.493, 83.5312)
                );
                assert_eq!((map, area), (0, 12));
            }
            other => panic!("{}", other.name()),
        }
    }
}

#[test]
fn difficulty_and_rest_start() {
    let mut b = u32b(1).to_vec();
    b.extend(u32b(0));
    match tbc_tbcpacket(t::SMSG_INSTANCE_DIFFICULTY, &b) {
        TbcPacket::InstanceDifficulty {
            difficulty,
            unknown,
        } => assert_eq!((difficulty, unknown), (1, 0)),
        other => panic!("{}", other.name()),
    }
    let mut b = u32b(1).to_vec();
    b.extend(u32b(1));
    b.extend(u32b(1));
    match tbc_tbcpacket(t::MSG_SET_DUNGEON_DIFFICULTY, &b) {
        TbcPacket::SetDungeonDifficulty {
            difficulty,
            unknown,
            in_group,
        } => assert_eq!((difficulty, unknown, in_group), (1, 1, true)),
        other => panic!("{}", other.name()),
    }
    match tbc_tbcpacket(t::SMSG_SET_REST_START, &u32b(9)) {
        TbcPacket::SetRestStart { value } => assert_eq!(value, 9),
        other => panic!("{}", other.name()),
    }
}

// --- character state ----------------------------------------------------------------------------

fn initial_spells_body() -> Vec<u8> {
    let mut b = vec![0u8];
    b.extend(2u16.to_le_bytes());
    for spell in [78u16, 2457] {
        b.extend(spell.to_le_bytes());
        b.extend(0u16.to_le_bytes());
    }
    b.extend(1u16.to_le_bytes());
    b.extend(2457u16.to_le_bytes()); // spell
    b.extend(0u16.to_le_bytes()); // item
    b.extend(47u16.to_le_bytes()); // category
    b.extend(u32b(0)); // cooldown
    b.extend(u32b(1000)); // category cooldown
    b
}

#[test]
fn initial_spells_read_the_same_in_both_builds() {
    let body = initial_spells_body();
    for packet in [
        tbc(t::SMSG_INITIAL_SPELLS, &body),
        vanilla(opcode::SMSG_INITIAL_SPELLS, &body),
    ] {
        match packet {
            ServerPacket::InitialSpells {
                spell_ids,
                cooldowns,
            } => {
                assert_eq!(spell_ids, [78, 2457]);
                assert_eq!(cooldowns.len(), 1);
                assert_eq!(
                    (
                        cooldowns[0].spell_id,
                        cooldowns[0].category,
                        cooldowns[0].category_cd_ms
                    ),
                    (2457, 47, 1000)
                );
            }
            other => panic!("{}", other.name()),
        }
    }
}

#[test]
fn unlearn_list_reads_a_count_and_spells() {
    let mut b = u32b(2).to_vec();
    b.extend(u32b(100));
    b.extend(u32b(200));
    match tbc_tbcpacket(t::SMSG_SEND_UNLEARN_SPELLS, &b) {
        TbcPacket::SendUnlearnSpells { spells } => assert_eq!(spells, [100, 200]),
        other => panic!("{}", other.name()),
    }
}

#[test]
fn action_buttons_are_132_words() {
    let mut words = [0u32; 132];
    words[72] = 6603;
    words[83] = 117 | (0x80 << 24);
    words[131] = 5;
    let body: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    match tbc(t::SMSG_ACTION_BUTTONS, &body) {
        ServerPacket::ActionButtons { buttons } => {
            let got: Vec<_> = buttons.iter().map(|b| (b.slot, b.action, b.kind)).collect();
            assert_eq!(got, [(72, 6603, 0), (83, 117, 0x80), (131, 5, 0)]);
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn factions_are_a_count_of_flag_and_standing_pairs() {
    let mut b = u32b(128).to_vec();
    for i in 0..128u32 {
        b.push(if i == 3 { 0x11 } else { 0 });
        b.extend((if i == 3 { -42i32 } else { 0 }).to_le_bytes());
    }
    match tbc(t::SMSG_INITIALIZE_FACTIONS, &b) {
        ServerPacket::InitializeFactions { standings } => {
            assert_eq!(standings.len(), 128);
            assert_eq!(standings[3], (0x11, -42));
            assert_eq!(standings[4], (0, 0));
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn proficiency_reads_the_same_in_both_builds() {
    let mut b = vec![4u8];
    b.extend(u32b(0x4e));
    for packet in [
        tbc(t::SMSG_SET_PROFICIENCY, &b),
        vanilla(opcode::SMSG_SET_PROFICIENCY, &b),
    ] {
        match packet {
            ServerPacket::SetProficiency {
                item_class,
                subclass_mask,
            } => assert_eq!((item_class, subclass_mask), (4, 0x4e)),
            other => panic!("{}", other.name()),
        }
    }
}

#[test]
fn contact_list_reads_friends_ignored_and_muted() {
    let mut b = u32b(0x03).to_vec(); // friend and ignore lists
    b.extend(u32b(3));
    // an online friend: status, area, level, class
    b.extend(0x11u64.to_le_bytes());
    b.extend(u32b(1));
    b.extend(cstr("pal"));
    b.push(1);
    b.extend(u32b(12));
    b.extend(u32b(60));
    b.extend(u32b(1));
    // an offline friend: a status only
    b.extend(0x22u64.to_le_bytes());
    b.extend(u32b(1));
    b.extend(cstr(""));
    b.push(0);
    // an ignored player: no status
    b.extend(0x33u64.to_le_bytes());
    b.extend(u32b(2));
    b.extend(cstr("spam"));
    match tbc_tbcpacket(t::SMSG_CONTACT_LIST, &b) {
        TbcPacket::ContactList {
            list_mask,
            contacts,
        } => {
            assert_eq!((list_mask, contacts.len()), (3, 3));
            assert_eq!(contacts[0].online, Some((12, 60, 1)));
            assert_eq!(contacts[0].note, "pal");
            assert_eq!((contacts[1].status, contacts[1].online), (Some(0), None));
            assert_eq!((contacts[2].relation, contacts[2].status), (2, None));
            assert_eq!(contacts[2].note, "spam");
        }
        other => panic!("{}", other.name()),
    }
}

// --- chat at entry ------------------------------------------------------------------------------

fn chat_prefix(ty: u8, language: u32, sender: u64) -> Vec<u8> {
    let mut b = vec![ty];
    b.extend(u32b(language));
    b.extend(sender.to_le_bytes());
    b.extend(u32b(0));
    b
}

fn len_str(s: &str) -> Vec<u8> {
    let mut b = u32b(s.len() as u32 + 1).to_vec();
    b.extend(cstr(s));
    b
}

fn chat(body: &[u8]) -> messages::ChatMessage {
    match tbc(t::SMSG_MESSAGECHAT, body) {
        ServerPacket::MessageChat(m) => m,
        other => panic!("{}", other.name()),
    }
}

#[test]
fn a_system_line_is_type_0_and_reads_as_1_12_1_system() {
    // The live packet: type 0, language 0, no sender, the text, tag 0.
    let mut b = chat_prefix(0, 0, 0);
    b.extend(0u64.to_le_bytes()); // target
    b.extend(len_str("[Tickets]: Queue system status: on"));
    b.push(0);
    assert_eq!(b.len(), 65);
    let m = chat(&b);
    assert_eq!(m.chat_type, messages::CHAT_MSG_SYSTEM);
    assert_eq!(m.text, "[Tickets]: Queue system status: on");
    assert_eq!((m.language, m.sender_guid, m.chat_tag), (0, 0, 0));
}

#[test]
fn say_yell_party_whisper_and_afk_use_the_default_shape() {
    for (tbc_type, vanilla_type) in [
        (1u8, messages::CHAT_MSG_SAY),
        (2, messages::CHAT_MSG_PARTY),
        (6, messages::CHAT_MSG_YELL),
        (7, messages::CHAT_MSG_WHISPER),
        (9, messages::CHAT_MSG_WHISPER_INFORM),
        (0x17, messages::CHAT_MSG_AFK),
    ] {
        let mut b = chat_prefix(tbc_type, 7, 0x709);
        b.extend(0x709u64.to_le_bytes());
        b.extend(len_str("hi"));
        b.push(0);
        let m = chat(&b);
        assert_eq!(m.chat_type, vanilla_type, "2.4.3 type {tbc_type:#x}");
        assert_eq!(
            (m.sender_guid, m.target_guid, m.language),
            (0x709, 0x709, 7)
        );
        assert_eq!(m.text, "hi");
    }
}

#[test]
fn a_channel_line_names_the_channel_before_the_target() {
    let mut b = chat_prefix(0x11, 7, 0x44);
    b.extend(cstr("General"));
    b.extend(0x44u64.to_le_bytes());
    b.extend(len_str("anyone?"));
    b.push(0);
    let m = chat(&b);
    assert_eq!(m.chat_type, messages::CHAT_MSG_CHANNEL);
    assert_eq!(m.channel.as_deref(), Some("General"));
    assert_eq!((m.sender_guid, m.text.as_str()), (0x44, "anyone?"));
}

#[test]
fn monster_lines_carry_the_sender_name_and_a_creature_targets_name() {
    let creature = 0xF130_0000_0000_0007u64; // a creature guid (high part 0xF130)
    let mut b = chat_prefix(0x0C, 0, creature);
    b.extend(len_str("Hogger"));
    b.extend(creature.to_le_bytes()); // target: a creature, so its name follows
    b.extend(len_str("Hogger"));
    b.extend(len_str("Grrr!"));
    b.push(0);
    let m = chat(&b);
    assert_eq!(m.chat_type, messages::CHAT_MSG_MONSTER_SAY);
    assert_eq!(m.sender_name.as_deref(), Some("Hogger"));
    assert_eq!(m.text, "Grrr!");
    // A player target has no name.
    let mut b = chat_prefix(0x10, 0, creature);
    b.extend(len_str("Hogger"));
    b.extend(0x709u64.to_le_bytes());
    b.extend(len_str("looks at you"));
    b.push(0);
    let m = chat(&b);
    assert_eq!(m.chat_type, messages::CHAT_MSG_MONSTER_EMOTE);
    assert_eq!((m.target_guid, m.text.as_str()), (0x709, "looks at you"));
}

#[test]
fn a_battleground_line_names_a_creature_target_only() {
    let mut b = chat_prefix(0x24, 0, 0);
    b.extend(0u64.to_le_bytes());
    b.extend(len_str("The flag is taken"));
    b.push(0);
    assert_eq!(chat(&b).chat_type, messages::CHAT_MSG_BG_SYSTEM_NEUTRAL);
    let creature = 0xF130_0000_0000_0007u64;
    let mut b = chat_prefix(0x25, 0, 0);
    b.extend(creature.to_le_bytes());
    b.extend(len_str("Flag"));
    b.extend(len_str("Alliance wins"));
    b.push(0);
    assert_eq!(chat(&b).text, "Alliance wins");
}

#[test]
fn the_gm_flag_adds_a_sender_name_and_reads_as_1_12_1_gm() {
    let mut b = chat_prefix(1, 0, 0x55);
    b.extend(0x55u64.to_le_bytes());
    b.extend(len_str("hello"));
    b.push(0x04);
    b.extend(len_str("Admin"));
    let m = chat(&b);
    assert_eq!(m.chat_tag, chat_tag::GM);
    assert_eq!(m.sender_name.as_deref(), Some("Admin"));
    // DND and AFK map to their 1.12.1 values.
    for (tag, want) in [(2u8, chat_tag::DND), (1, chat_tag::AFK)] {
        let mut b = chat_prefix(1, 0, 0x55);
        b.extend(0x55u64.to_le_bytes());
        b.extend(len_str("x"));
        b.push(tag);
        assert_eq!(chat(&b).chat_tag, want);
    }
}

#[test]
fn a_chat_type_without_a_1_12_1_number_is_other_and_its_bytes_stay_unread() {
    // WHISPER_FOREIGN (8), MONSTER_PARTY (0x0D), CHANNEL_JOIN (0x12), MONEY (0x1C), RESTRICTED (0x2E).
    for ty in [0x08u8, 0x0D, 0x12, 0x1C, 0x2E] {
        let body = chat_prefix(ty, 0, 0);
        match parse_server_with_tail_for(&TBC_2_4_3, None, t::SMSG_MESSAGECHAT, &body).unwrap() {
            (ServerPacket::Other { opcode }, 0) => assert_eq!(opcode, t::SMSG_MESSAGECHAT),
            _ => panic!("type {ty:#x} should be Other"),
        }
    }
    assert_eq!(messages::chat_type_from_tbc(0x1C), None);
    assert_eq!(
        messages::chat_type_from_tbc(0),
        Some(messages::CHAT_MSG_SYSTEM)
    );
}

#[test]
fn notification_and_weather_and_world_states() {
    match tbc(
        t::SMSG_NOTIFICATION,
        &cstr("|cFFFFFF00Ticket notifications: ON|r"),
    ) {
        ServerPacket::Notification { text } => assert!(text.ends_with("ON|r")),
        other => panic!("{}", other.name()),
    }
    let mut w = u32b(3).to_vec();
    w.extend(f32b(0.5));
    w.push(1);
    match tbc(t::SMSG_WEATHER, &w) {
        ServerPacket::Weather {
            weather_type,
            grade,
            sound_id,
            instant,
        } => assert_eq!((weather_type, grade, sound_id, instant), (3, 0.5, 0, true)),
        other => panic!("{}", other.name()),
    }
    let mut s = u32b(0).to_vec(); // map
    s.extend(u32b(12)); // zone
    s.extend(u32b(9)); // area
    s.extend(2u16.to_le_bytes());
    for (id, v) in [(3191u32, 1u32), (7, 8)] {
        s.extend(u32b(id));
        s.extend(u32b(v));
    }
    match tbc(t::SMSG_INIT_WORLD_STATES, &s) {
        ServerPacket::InitWorldStates(w) => {
            assert_eq!((w.map, w.zone), (0, 12));
            assert_eq!(w.states, [(3191, 1), (7, 8)]);
        }
        other => panic!("{}", other.name()),
    }
    match tbc_tbcpacket(t::SMSG_LFG_UPDATE, &[1, 0, 1, 9, 0, 0, 0]) {
        TbcPacket::LfgUpdate {
            queued,
            looking_for_more,
            more,
            ..
        } => assert_eq!((queued, looking_for_more, more), (true, true, Some(9))),
        other => panic!("{}", other.name()),
    }
    match tbc_tbcpacket(t::SMSG_LFG_UPDATE, &[0, 0, 0]) {
        TbcPacket::LfgUpdate { more, .. } => assert_eq!(more, None),
        other => panic!("{}", other.name()),
    }
}

// --- auras and spell start / go -----------------------------------------------------------------

fn extra_aura(slot: u8, spell: u32, max: i32, left: u32) -> Vec<u8> {
    let mut b = vec![slot];
    b.extend(u32b(spell));
    b.extend(max.to_le_bytes());
    b.extend(u32b(left));
    b
}

#[test]
fn extra_aura_info_packets() {
    let mut init = packed(0x709);
    init.extend(extra_aura(0, 2457, -1, 0));
    init.extend(extra_aura(3, 99, 30_000, 12_000));
    match tbc_tbcpacket(t::SMSG_INIT_EXTRA_AURA_INFO, &init) {
        TbcPacket::InitExtraAuraInfo { guid, auras } => {
            assert_eq!((guid, auras.len()), (0x709, 2));
            assert_eq!(
                (
                    auras[1].slot,
                    auras[1].max_duration_ms,
                    auras[1].remaining_ms
                ),
                (3, 30_000, 12_000)
            );
            assert_eq!(auras[0].max_duration_ms, -1);
        }
        other => panic!("{}", other.name()),
    }
    let mut set = packed(0x709);
    set.extend(extra_aura(61, 33379, 0, 0));
    for (op, need) in [
        (t::SMSG_SET_EXTRA_AURA_INFO, false),
        (t::SMSG_SET_EXTRA_AURA_INFO_NEED_UPDATE, true),
    ] {
        match tbc_tbcpacket(op, &set) {
            TbcPacket::SetExtraAuraInfo {
                guid,
                aura,
                need_update,
            } => {
                assert_eq!(
                    (guid, aura.slot, aura.spell_id, need_update),
                    (0x709, 61, 33379, need)
                );
            }
            other => panic!("{}", other.name()),
        }
    }
    let mut clear = packed(0x709);
    clear.extend(u32b(2457));
    match tbc_tbcpacket(t::SMSG_CLEAR_EXTRA_AURA_INFO, &clear) {
        TbcPacket::ClearExtraAuraInfo { guid, spell_id } => {
            assert_eq!((guid, spell_id), (0x709, 2457))
        }
        other => panic!("{}", other.name()),
    }
    // An empty list is a guid and no entries.
    match tbc_tbcpacket(t::SMSG_INIT_EXTRA_AURA_INFO, &packed(5)) {
        TbcPacket::InitExtraAuraInfo { auras, .. } => assert!(auras.is_empty()),
        other => panic!("{}", other.name()),
    }
}

#[test]
fn update_aura_duration_reads_the_same_in_both_builds() {
    let mut b = vec![3u8];
    b.extend(u32b(4500));
    for packet in [
        tbc(t::SMSG_UPDATE_AURA_DURATION, &b),
        vanilla(opcode::SMSG_UPDATE_AURA_DURATION, &b),
    ] {
        match packet {
            ServerPacket::UpdateAuraDuration { slot, remaining_ms } => {
                assert_eq!((slot, remaining_ms), (3, 4500))
            }
            other => panic!("{}", other.name()),
        }
    }
}

fn targets(mask: u32, tail: &[u8]) -> Vec<u8> {
    let mut b = u32b(mask).to_vec();
    b.extend(tail);
    b
}

#[test]
fn spell_start_has_a_cast_count_and_a_u32_target_mask() {
    // The live packet: item-or-caster, caster, spell 836, cast count 0, flags 2, timer 0, unit target.
    let mut b = packed(0x709);
    b.extend(packed(0x709));
    b.extend(u32b(836));
    b.push(0);
    b.extend(2u16.to_le_bytes());
    b.extend(u32b(0));
    b.extend(targets(2, &packed(0x709)));
    assert_eq!(b.len(), 24);
    match tbc(t::SMSG_SPELL_START, &b) {
        ServerPacket::SpellStart(s) => {
            assert_eq!(
                (s.item_or_caster, s.caster, s.spell_id),
                (0x709, 0x709, 836)
            );
            assert_eq!((s.cast_flags, s.cast_time_ms), (2, 0));
            assert_eq!((s.targets.mask, s.targets.unit_target), (2, Some(0x709)));
            assert_eq!(s.ammo_display_id, None);
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn spell_start_reads_a_destination_and_the_ammo_block() {
    let mut b = packed(0x709);
    b.extend(packed(0x709));
    b.extend(u32b(75));
    b.push(3); // cast count
    b.extend(0x0022u16.to_le_bytes()); // unknown2 | ammo
    b.extend(u32b(1500));
    let mut dest = Vec::new();
    for v in [1.0f32, 2.0, 3.0] {
        dest.extend(f32b(v));
    }
    b.extend(targets(0x40, &dest));
    b.extend(u32b(5555)); // ammo display
    b.extend(u32b(24)); // ammo inventory type
    match tbc(t::SMSG_SPELL_START, &b) {
        ServerPacket::SpellStart(s) => {
            assert_eq!(s.cast_time_ms, 1500);
            assert_eq!(
                s.targets.dest.map(|d| (d.x, d.y, d.z)),
                Some((1.0, 2.0, 3.0))
            );
            assert_eq!(s.ammo_display_id, Some(5555));
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn spell_go_has_a_timestamp_hits_misses_and_a_u32_mask() {
    // The live packet: flags 0x101, a timestamp, one hit (the caster), no misses, a unit target.
    let mut b = packed(0x709);
    b.extend(packed(0x709));
    b.extend(u32b(836));
    b.extend(0x0101u16.to_le_bytes());
    b.extend(u32b(0x0176_da03));
    b.push(1);
    b.extend(0x709u64.to_le_bytes());
    b.push(0);
    b.extend(targets(2, &packed(0x709)));
    assert_eq!(b.len(), 33);
    match tbc(t::SMSG_SPELL_GO, &b) {
        ServerPacket::SpellGo(g) => {
            assert_eq!((g.spell_id, g.cast_flags), (836, 0x101));
            assert_eq!(g.hits, [0x709]);
            assert!(g.misses.is_empty());
            assert_eq!(g.targets.unit_target, Some(0x709));
        }
        other => panic!("{}", other.name()),
    }
    // Two misses, the second a reflect (an extra byte), and a game-object target.
    let mut b = packed(0x709);
    b.extend(packed(0x709));
    b.extend(u32b(5));
    b.extend(0x0100u16.to_le_bytes());
    b.extend(u32b(77));
    b.push(0);
    b.push(2);
    b.extend(0x11u64.to_le_bytes());
    b.push(3);
    b.extend(0x12u64.to_le_bytes());
    b.push(11);
    b.push(1);
    b.extend(targets(0x800, &packed(0x99)));
    match tbc(t::SMSG_SPELL_GO, &b) {
        ServerPacket::SpellGo(g) => {
            assert_eq!(g.misses, [(0x11, 3), (0x12, 11)]);
            assert_eq!(g.targets.go_target, Some(0x99));
        }
        other => panic!("{}", other.name()),
    }
}

// --- creature movement and destroy --------------------------------------------------------------

fn pack_xyz(x: f32, y: f32, z: f32) -> u32 {
    ((x / 0.25) as i32 as u32 & 0x7FF)
        | (((y / 0.25) as i32 as u32 & 0x7FF) << 11)
        | (((z / 0.25) as i32 as u32 & 0x3FF) << 22)
}

fn move_head(guid: u64, start: [f32; 3], id: u32, move_type: u8) -> Vec<u8> {
    let mut b = packed(guid);
    for v in start {
        b.extend(f32b(v));
    }
    b.extend(u32b(id));
    b.push(move_type);
    b
}

/// cmangos-tbc `WriteLinearPath`: the count, the endpoint, then each middle point as the packed
/// offset from the midpoint of the start and the endpoint.
fn linear_path(start: [f32; 3], middle: &[[f32; 3]], end: [f32; 3]) -> Vec<u8> {
    let mut b = u32b(middle.len() as u32 + 1).to_vec();
    for v in end {
        b.extend(f32b(v));
    }
    let mid: Vec<f32> = (0..3).map(|i| (start[i] + end[i]) / 2.0).collect();
    for p in middle {
        b.extend(pack_xyz(mid[0] - p[0], mid[1] - p[1], mid[2] - p[2]).to_le_bytes());
    }
    b
}

#[test]
fn a_linear_move_takes_its_offsets_from_the_midpoint() {
    let start = [100.0f32, 200.0, 30.0];
    let end = [120.0f32, 216.0, 32.0];
    let middle = [[104.0f32, 204.0, 30.5], [112.0, 210.0, 31.0]];
    let mut b = move_head(0xF130_0000_0000_0005, start, 7, 0);
    b.extend(u32b(0x100)); // run mode
    b.extend(u32b(4000)); // duration
    b.extend(linear_path(start, &middle, end));
    match tbc(t::SMSG_MONSTER_MOVE, &b) {
        ServerPacket::MonsterMove {
            guid,
            path,
            stop,
            duration_ms,
            run_mode,
            flying,
            spline_id,
            ..
        } => {
            assert_eq!(guid, 0xF130_0000_0000_0005);
            assert_eq!(
                (spline_id, duration_ms, stop, run_mode, flying),
                (7, 4000, false, true, false)
            );
            assert_eq!(path.len(), 4);
            let want = [start, middle[0], middle[1], end];
            for (p, w) in path.iter().zip(want) {
                assert!(
                    (p.x - w[0]).abs() < 0.3
                        && (p.y - w[1]).abs() < 0.3
                        && (p.z - w[2]).abs() < 0.3,
                    "{p:?} vs {w:?}"
                );
            }
        }
        other => panic!("{}", other.name()),
    }
    // The same bytes through the 1.12.1 dispatch hang the offsets on the endpoint instead.
    match vanilla(opcode::SMSG_MONSTER_MOVE, &b) {
        ServerPacket::MonsterMove { path, .. } => {
            assert!(
                (path[1].x - middle[0][0]).abs() > 5.0,
                "1.12.1 reads offsets from the endpoint"
            );
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn a_straight_hop_a_stop_and_every_facing() {
    let start = [1.0f32, 2.0, 3.0];
    let end = [5.0f32, 6.0, 3.0];
    // One point after the start: the endpoint alone, no offsets.
    let mut b = move_head(9, start, 1, 0);
    b.extend(u32b(0x100));
    b.extend(u32b(1000));
    b.extend(linear_path(start, &[], end));
    match tbc(t::SMSG_MONSTER_MOVE, &b) {
        ServerPacket::MonsterMove { path, .. } => {
            assert_eq!(path.len(), 2);
            assert_eq!((path[1].x, path[1].y), (5.0, 6.0));
        }
        other => panic!("{}", other.name()),
    }
    // A stop: the head alone.
    match tbc(t::SMSG_MONSTER_MOVE, &move_head(9, start, 2, 1)) {
        ServerPacket::MonsterMove { stop, path, .. } => assert!(stop && path.is_empty()),
        other => panic!("{}", other.name()),
    }
    // Facing a target, an angle and a spot precede the flags.
    for (ty, extra, check) in [
        (3u8, 0xAAu64.to_le_bytes().to_vec(), "target"),
        (4, f32b(1.5).to_vec(), "angle"),
        (2, [f32b(1.0), f32b(2.0), f32b(3.0)].concat(), "spot"),
    ] {
        let mut b = move_head(9, start, 3, ty);
        b.extend(extra);
        b.extend(u32b(0x100));
        b.extend(u32b(1000));
        b.extend(linear_path(start, &[], end));
        match tbc(t::SMSG_MONSTER_MOVE, &b) {
            ServerPacket::MonsterMove { facing, .. } => {
                let ok = match (check, facing) {
                    ("target", messages::MonsterMoveFacing::Target(g)) => g == 0xAA,
                    ("angle", messages::MonsterMoveFacing::Angle(a)) => a == 1.5,
                    ("spot", messages::MonsterMoveFacing::Spot(s)) => s == [1.0, 2.0, 3.0],
                    _ => false,
                };
                assert!(ok, "{check}");
            }
            other => panic!("{}", other.name()),
        }
    }
}

#[test]
fn a_flying_move_is_a_list_of_absolute_points() {
    let mut b = move_head(9, [0.0, 0.0, 0.0], 4, 0);
    b.extend(u32b(0x300)); // run mode and flying
    b.extend(u32b(2000));
    b.extend(u32b(2));
    for p in [[1.0f32, 1.0, 1.0], [2.0, 2.0, 2.0]] {
        for v in p {
            b.extend(f32b(v));
        }
    }
    match tbc(t::SMSG_MONSTER_MOVE, &b) {
        ServerPacket::MonsterMove { path, flying, .. } => {
            assert!(flying);
            assert_eq!(path.len(), 3);
            assert_eq!(path[2].x, 2.0);
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn a_transport_move_names_the_transport_after_the_mover() {
    let mut b = packed(0xF130_0000_0000_0005);
    b.extend(packed(0x1_0000_0000_0001));
    let rest = move_head(0, [1.0, 2.0, 3.0], 5, 1);
    // `move_head` begins with a (zero) packed guid: replace it with the head's tail.
    b.extend(&rest[1..]);
    match tbc(t::SMSG_MONSTER_MOVE_TRANSPORT, &b) {
        ServerPacket::MonsterMove {
            transport, stop, ..
        } => {
            assert_eq!(transport, Some(0x1_0000_0000_0001));
            assert!(stop);
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn destroy_object_reads_the_same_in_both_builds() {
    let body = 0xF130_0000_0000_0042u64.to_le_bytes();
    for packet in [
        tbc(t::SMSG_DESTROY_OBJECT, &body),
        vanilla(opcode::SMSG_DESTROY_OBJECT, &body),
    ] {
        match packet {
            ServerPacket::DestroyObject { guid } => assert_eq!(guid, 0xF130_0000_0000_0042),
            other => panic!("{}", other.name()),
        }
    }
}

#[test]
fn cancel_combat_is_empty_in_both_builds() {
    assert!(matches!(
        tbc(t::SMSG_CANCEL_COMBAT, &[]),
        ServerPacket::CancelCombat
    ));
    assert!(matches!(
        vanilla(opcode::SMSG_CANCEL_COMBAT, &[]),
        ServerPacket::CancelCombat
    ));
}

// --- time sync ----------------------------------------------------------------------------------

#[test]
fn time_sync_request_and_response() {
    match tbc_tbcpacket(t::SMSG_TIME_SYNC_REQ, &u32b(2)) {
        TbcPacket::TimeSyncRequest { counter } => assert_eq!(counter, 2),
        other => panic!("{}", other.name()),
    }
    assert_eq!(
        messages::time_sync_response(2, 0x0102_0304),
        [2, 0, 0, 0, 4, 3, 2, 1]
    );
    assert_eq!(t::CMSG_TIME_SYNC_RESP, 0x391);
    // A request with no counter is an error.
    assert!(parse_server_with_tail_for(&TBC_2_4_3, None, t::SMSG_TIME_SYNC_REQ, &[]).is_err());
}

// --- queries ------------------------------------------------------------------------------------

#[test]
fn the_name_creature_and_game_object_requests_are_the_same_bytes_in_both_builds() {
    // The 2.4.3 client sends the same bodies (wow_messages `versions` "1 2 3"); the writers have one form.
    assert_eq!(messages::full_guid(0x709), 0x709u64.to_le_bytes());
    let c = messages::creature_query(1234, 0xF130_0000_0000_0001);
    assert_eq!(&c[..4], 1234u32.to_le_bytes());
    assert_eq!(c.len(), 12);
    let g = messages::gameobject_query(55, 0xF110_0000_0000_0001);
    assert_eq!((&g[..4], g.len()), (&55u32.to_le_bytes()[..], 12));
}

#[test]
fn the_item_request_is_the_entry_alone_in_2_4_3() {
    assert_eq!(messages::item_query_tbc(2362), 2362u32.to_le_bytes());
    assert_eq!(messages::item_query(2362, 9).len(), 12);
}

#[test]
fn a_name_reply_has_a_full_guid_and_a_declined_tail() {
    let mut b = 0x709u64.to_le_bytes().to_vec();
    b.extend(cstr("Benilla"));
    b.extend(cstr(""));
    for v in [1u32, 0, 1] {
        b.extend(u32b(v));
    }
    b.push(0);
    match tbc(t::SMSG_NAME_QUERY_RESPONSE, &b) {
        ServerPacket::NameQueryResponse {
            guid,
            name,
            race,
            gender,
            class,
        } => {
            assert_eq!(
                (guid, name.as_str(), race, gender, class),
                (0x709, "Benilla", 1, 0, 1)
            );
        }
        other => panic!("{}", other.name()),
    }
    // With declined names, five more strings follow the flag.
    let mut b = 0x709u64.to_le_bytes().to_vec();
    b.extend(cstr("Иван"));
    b.extend(cstr(""));
    for v in [1u32, 0, 1] {
        b.extend(u32b(v));
    }
    b.push(1);
    for case in ["a", "b", "c", "d", "e"] {
        b.extend(cstr(case));
    }
    match tbc(t::SMSG_NAME_QUERY_RESPONSE, &b) {
        ServerPacket::NameQueryResponse { name, .. } => assert_eq!(name, "Иван"),
        other => panic!("{}", other.name()),
    }
}

fn creature_reply(entry: u32) -> Vec<u8> {
    let mut b = u32b(entry).to_vec();
    b.extend(cstr("Hogger"));
    b.extend([0, 0, 0]); // name2..4
    b.extend(cstr("")); // subname
    b.extend(cstr("Directions")); // icon name
    for v in [0x10u32, 7, 2, 1, 0, 33] {
        b.extend(u32b(v)); // flags, type, family, rank, unknown, pet spell data
    }
    for d in [3345u32, 0, 0, 0] {
        b.extend(u32b(d)); // display ids
    }
    b.extend(f32b(1.5));
    b.extend(f32b(1.0));
    b.push(1); // racial leader
    b
}

#[test]
fn a_creature_reply_has_an_icon_name_four_display_ids_and_two_multipliers() {
    match tbc(t::SMSG_CREATURE_QUERY_RESPONSE, &creature_reply(448)) {
        ServerPacket::CreatureQueryResponse { entry, info } => {
            let i = info.expect("found");
            assert_eq!(entry, 448);
            assert_eq!(
                (i.name.as_str(), i.creature_type, i.pet_family, i.rank),
                ("Hogger", 7, 2, 1)
            );
            assert_eq!(
                (i.type_flags, i.display_id, i.racial_leader, i.civilian),
                (0x10, 3345, true, false)
            );
        }
        other => panic!("{}", other.name()),
    }
    match tbc(t::SMSG_CREATURE_QUERY_RESPONSE, &u32b(448 | 0x8000_0000)) {
        ServerPacket::CreatureQueryResponse { entry, info } => {
            assert_eq!((entry, info.is_none()), (448, true))
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn a_game_object_reply_has_24_data_words_and_a_size() {
    let mut b = u32b(176_213).to_vec();
    b.extend(u32b(3)); // type: chest
    b.extend(u32b(259)); // display
    b.extend(cstr("Chest"));
    b.extend([0, 0, 0]);
    b.extend(cstr("")); // icon
    b.extend(cstr("Opening")); // cast bar caption
    b.extend(cstr("")); // unknown
    for i in 0..24u32 {
        b.extend(u32b(i));
    }
    b.extend(f32b(1.25));
    match tbc(t::SMSG_GAMEOBJECT_QUERY_RESPONSE, &b) {
        ServerPacket::GameObjectQueryResponse { entry, info } => {
            let i = info.expect("found");
            assert_eq!(
                (entry, i.type_id, i.display_id, i.name.as_str()),
                (176_213, 3, 259, "Chest")
            );
            assert_eq!((i.data[0], i.data[23]), (0, 23));
        }
        other => panic!("{}", other.name()),
    }
    match tbc(t::SMSG_GAMEOBJECT_QUERY_RESPONSE, &u32b(9 | 0x8000_0000)) {
        ServerPacket::GameObjectQueryResponse { entry, info } => {
            assert_eq!((entry, info.is_none()), (9, true))
        }
        other => panic!("{}", other.name()),
    }
}

fn item_reply() -> Vec<u8> {
    let mut b = u32b(2362).to_vec();
    b.extend(u32b(4)); // class armor
    b.extend(u32b(6)); // subclass shield
    b.extend(u32b(u32::MAX)); // sound override
    b.extend(cstr("Worn Wooden Shield"));
    b.extend([0, 0, 0]);
    for v in [18730u32, 1, 0, 0, 0, 14] {
        b.extend(u32b(v)); // display, quality, flags, buy, sell, inventory type
    }
    b.extend(u32b(u32::MAX)); // allowable class
    b.extend(u32b(u32::MAX)); // allowable race
    b.extend(u32b(3)); // item level
    b.extend(u32b(2)); // required level
    for _ in 0..7 {
        b.extend(u32b(0)); // skill, rank, spell, honor, city, faction, faction rank
    }
    b.extend(u32b(0)); // max count
    b.extend(u32b(1)); // stackable
    b.extend(u32b(0)); // container slots
    for _ in 0..10 {
        b.extend(u32b(0));
        b.extend(0i32.to_le_bytes());
    }
    for _ in 0..5 {
        b.extend(f32b(0.0));
        b.extend(f32b(0.0));
        b.extend(u32b(0));
    }
    for r in [4u32, 0, 0, 0, 0, 0, 0] {
        b.extend(u32b(r)); // armor and six resistances
    }
    b.extend(u32b(0)); // delay
    b.extend(u32b(0)); // ammo
    b.extend(f32b(0.0)); // range
    for _ in 0..5 {
        for v in [0u32, 0, 0, u32::MAX, 0, u32::MAX] {
            b.extend(u32b(v)); // an empty spell block
        }
    }
    b.extend(u32b(0)); // bonding
    b.extend(cstr("")); // description
    for v in [0u32, 0, 0, 0, 0, 7, 1, 0] {
        b.extend(u32b(v)); // page text, language, page material, start quest, lock, material, sheath, random property
    }
    b.extend(u32b(0)); // random suffix
    for v in [5u32, 0, 25, 0, 0, 0] {
        b.extend(u32b(v)); // block, item set, max durability, area, map, bag family
    }
    b.extend(u32b(0)); // totem category
    for _ in 0..3 {
        b.extend(u32b(0));
        b.extend(u32b(0));
    }
    b.extend(u32b(0)); // socket bonus
    b.extend(u32b(0)); // gem properties
    b.extend(0i32.to_le_bytes()); // disenchant skill
    b.extend(f32b(0.0)); // armor damage modifier
    b.extend(u32b(0)); // duration
    b
}

#[test]
fn an_item_reply_has_the_sound_override_the_random_suffix_and_the_socket_tail() {
    match tbc(t::SMSG_ITEM_QUERY_SINGLE_RESPONSE, &item_reply()) {
        ServerPacket::ItemQueryResponse { entry, info } => {
            let i = info.expect("found");
            assert_eq!(entry, 2362);
            assert_eq!(
                (i.name.as_str(), i.class, i.subclass),
                ("Worn Wooden Shield", 4, 6)
            );
            assert_eq!(
                (i.display_info_id, i.quality, i.inventory_type),
                (18730, 1, 14)
            );
            assert_eq!(
                (i.item_level, i.required_level, i.armor, i.stackable),
                (3, 2, 4, 1)
            );
            assert_eq!(
                (i.block, i.max_durability, i.material, i.sheath),
                (5, 25, 7, 1)
            );
        }
        other => panic!("{}", other.name()),
    }
    match tbc(
        t::SMSG_ITEM_QUERY_SINGLE_RESPONSE,
        &u32b(2362 | 0x8000_0000),
    ) {
        ServerPacket::ItemQueryResponse { entry, info } => {
            assert_eq!((entry, info.is_none()), (2362, true))
        }
        other => panic!("{}", other.name()),
    }
    // A reply cut short is an error, not a partial read.
    let cut = item_reply();
    assert!(parse_server_with_tail_for(
        &TBC_2_4_3,
        None,
        t::SMSG_ITEM_QUERY_SINGLE_RESPONSE,
        &cut[..cut.len() - 1]
    )
    .is_err());
}

// --- opcode names, the numbers that changed meaning, the allow-list -----------------------------

#[test]
fn the_numbers_that_changed_meaning_route_by_their_2_4_3_meaning() {
    // 0x67: SMSG_FRIEND_LIST in 1.12.1 (u8 count first), SMSG_CONTACT_LIST in 2.4.3 (u32 mask first).
    let contact = [u32b(0).to_vec(), u32b(0).to_vec()].concat();
    assert!(matches!(
        tbc(0x67, &contact),
        ServerPacket::Tbc(TbcPacket::ContactList { .. })
    ));
    assert!(matches!(
        vanilla(0x67, &[0]),
        ServerPacket::FriendList { .. }
    ));
    // 0x6B is a client opcode in 2.4.3 (CMSG_SET_CONTACT_NOTES): never read; 1.12.1 reads an ignore list.
    assert!(matches!(
        parse_server_with_tail_for(&TBC_2_4_3, None, 0x6B, &[0; 4])
            .unwrap()
            .0,
        ServerPacket::Other { opcode: 0x6B }
    ));
    assert!(matches!(
        vanilla(0x6B, &[0]),
        ServerPacket::IgnoreList { .. }
    ));
    // 0x14F (SMSG_SPELLBREAKLOG) and 0x293 (SMSG_MEETINGSTONE_LEAVE) are not read in 2.4.3.
    for op in [0x14Fu16, 0x293] {
        assert!(matches!(
            parse_server_with_tail_for(&TBC_2_4_3, None, op, &[0; 8])
                .unwrap()
                .0,
            ServerPacket::Other { .. }
        ));
    }
    // SMSG_DEFENSE_MESSAGE moved from 0x33B (1.12.1) to 0x33A; 0x33B is the instance difficulty in 2.4.3.
    let mut defense = u32b(12).to_vec();
    defense.extend(len_str("Goldshire is under attack!"));
    match tbc(t::SMSG_DEFENSE_MESSAGE, &defense) {
        ServerPacket::DefenseMessage { zone_id, text } => {
            assert_eq!((zone_id, text.as_str()), (12, "Goldshire is under attack!"));
        }
        other => panic!("{}", other.name()),
    }
    assert!(parse_server_with_tail_for(&TBC_2_4_3, None, 0x33A, &[0xff; 4]).is_err());
    let diff = [u32b(1), u32b(0)].concat();
    assert!(matches!(
        tbc(0x33B, &diff),
        ServerPacket::Tbc(TbcPacket::InstanceDifficulty { .. })
    ));
    match vanilla(opcode::SMSG_DEFENSE_MESSAGE, &defense) {
        ServerPacket::DefenseMessage { zone_id, .. } => assert_eq!(zone_id, 12),
        other => panic!("{}", other.name()),
    }
    assert_eq!(opcode::SMSG_DEFENSE_MESSAGE, 0x33B);
    assert_eq!(t::SMSG_DEFENSE_MESSAGE, 0x33A);
    // 0x209: SMSG_ACCOUNT_DATA_TIMES in 2.4.3, never modelled in 1.12.1.
    assert!(matches!(
        vanilla(0x209, &[0; 128]),
        ServerPacket::Other { .. }
    ));
}

/// Every opcode the 2.4.3 dispatch reads. A new arm without an entry here fails the test below.
const ALLOWED: &[u16] = &[
    t::SMSG_AUTH_CHALLENGE,
    t::SMSG_AUTH_RESPONSE,
    t::SMSG_CHAR_ENUM,
    t::SMSG_CHAR_CREATE,
    t::SMSG_ADDON_INFO,
    t::SMSG_CHARACTER_LOGIN_FAILED,
    t::SMSG_LOGIN_VERIFY_WORLD,
    t::SMSG_LOGOUT_COMPLETE,
    t::SMSG_LOGOUT_RESPONSE,
    t::SMSG_UPDATE_OBJECT,
    t::SMSG_COMPRESSED_UPDATE_OBJECT,
    t::SMSG_LOGIN_SETTIMESPEED,
    t::SMSG_ACCOUNT_DATA_TIMES,
    t::SMSG_FEATURE_SYSTEM_STATUS,
    t::SMSG_EXPECTED_SPAM_RECORDS,
    t::SMSG_MOTD,
    t::SMSG_TUTORIAL_FLAGS,
    t::SMSG_BINDPOINTUPDATE,
    t::SMSG_INSTANCE_DIFFICULTY,
    t::MSG_SET_DUNGEON_DIFFICULTY,
    t::SMSG_SET_REST_START,
    t::SMSG_DEFENSE_MESSAGE,
    t::SMSG_INITIAL_SPELLS,
    t::SMSG_SEND_UNLEARN_SPELLS,
    t::SMSG_ACTION_BUTTONS,
    t::SMSG_INITIALIZE_FACTIONS,
    t::SMSG_SET_PROFICIENCY,
    t::SMSG_CONTACT_LIST,
    t::SMSG_MESSAGECHAT,
    t::SMSG_NOTIFICATION,
    t::SMSG_WEATHER,
    t::SMSG_INIT_WORLD_STATES,
    t::SMSG_LFG_UPDATE,
    t::SMSG_UPDATE_AURA_DURATION,
    t::SMSG_INIT_EXTRA_AURA_INFO,
    t::SMSG_SET_EXTRA_AURA_INFO,
    t::SMSG_SET_EXTRA_AURA_INFO_NEED_UPDATE,
    t::SMSG_CLEAR_EXTRA_AURA_INFO,
    t::SMSG_SPELL_START,
    t::SMSG_SPELL_GO,
    t::SMSG_MONSTER_MOVE,
    t::SMSG_MONSTER_MOVE_TRANSPORT,
    t::SMSG_DESTROY_OBJECT,
    t::SMSG_CANCEL_COMBAT,
    t::SMSG_TIME_SYNC_REQ,
    t::SMSG_NAME_QUERY_RESPONSE,
    t::SMSG_CREATURE_QUERY_RESPONSE,
    t::SMSG_GAMEOBJECT_QUERY_RESPONSE,
    t::SMSG_ITEM_QUERY_SINGLE_RESPONSE,
    // The movement family (slice 3).
    t::MSG_MOVE_START_FORWARD,
    t::MSG_MOVE_START_BACKWARD,
    t::MSG_MOVE_STOP,
    t::MSG_MOVE_START_STRAFE_LEFT,
    t::MSG_MOVE_START_STRAFE_RIGHT,
    t::MSG_MOVE_STOP_STRAFE,
    t::MSG_MOVE_JUMP,
    t::MSG_MOVE_START_TURN_LEFT,
    t::MSG_MOVE_START_TURN_RIGHT,
    t::MSG_MOVE_STOP_TURN,
    t::MSG_MOVE_START_PITCH_UP,
    t::MSG_MOVE_START_PITCH_DOWN,
    t::MSG_MOVE_STOP_PITCH,
    t::MSG_MOVE_SET_RUN_MODE,
    t::MSG_MOVE_SET_WALK_MODE,
    t::MSG_MOVE_TELEPORT,
    t::MSG_MOVE_FALL_LAND,
    t::MSG_MOVE_START_SWIM,
    t::MSG_MOVE_STOP_SWIM,
    t::MSG_MOVE_SET_FACING,
    t::MSG_MOVE_SET_PITCH,
    t::MSG_MOVE_HEARTBEAT,
    t::MSG_MOVE_ROOT,
    t::MSG_MOVE_UNROOT,
    t::MSG_MOVE_HOVER,
    t::MSG_MOVE_FEATHER_FALL,
    t::MSG_MOVE_WATER_WALK,
    t::MSG_MOVE_UPDATE_CAN_FLY,
    t::MSG_MOVE_START_ASCEND,
    t::MSG_MOVE_STOP_ASCEND,
    t::MSG_MOVE_START_DESCEND,
    t::MSG_MOVE_SET_WALK_SPEED,
    t::MSG_MOVE_SET_RUN_SPEED,
    t::MSG_MOVE_SET_RUN_BACK_SPEED,
    t::MSG_MOVE_SET_SWIM_SPEED,
    t::MSG_MOVE_SET_SWIM_BACK_SPEED,
    t::MSG_MOVE_SET_TURN_RATE,
    t::MSG_MOVE_SET_FLIGHT_SPEED,
    t::MSG_MOVE_SET_FLIGHT_BACK_SPEED,
    t::MSG_MOVE_KNOCK_BACK,
    t::MSG_MOVE_TIME_SKIPPED,
    t::MSG_MOVE_TELEPORT_ACK,
    t::SMSG_NEW_WORLD,
    t::SMSG_TRANSFER_PENDING,
    t::SMSG_CLIENT_CONTROL_UPDATE,
    t::SMSG_FORCE_WALK_SPEED_CHANGE,
    t::SMSG_FORCE_RUN_SPEED_CHANGE,
    t::SMSG_FORCE_RUN_BACK_SPEED_CHANGE,
    t::SMSG_FORCE_SWIM_SPEED_CHANGE,
    t::SMSG_FORCE_SWIM_BACK_SPEED_CHANGE,
    t::SMSG_FORCE_TURN_RATE_CHANGE,
    t::SMSG_FORCE_FLIGHT_SPEED_CHANGE,
    t::SMSG_FORCE_FLIGHT_BACK_SPEED_CHANGE,
    t::SMSG_SPLINE_SET_WALK_SPEED,
    t::SMSG_SPLINE_SET_RUN_SPEED,
    t::SMSG_SPLINE_SET_RUN_BACK_SPEED,
    t::SMSG_SPLINE_SET_SWIM_SPEED,
    t::SMSG_SPLINE_SET_SWIM_BACK_SPEED,
    t::SMSG_SPLINE_SET_TURN_RATE,
    t::SMSG_SPLINE_SET_FLIGHT_SPEED,
    t::SMSG_SPLINE_SET_FLIGHT_BACK_SPEED,
    t::SMSG_FORCE_MOVE_ROOT,
    t::SMSG_FORCE_MOVE_UNROOT,
    t::SMSG_MOVE_WATER_WALK,
    t::SMSG_MOVE_LAND_WALK,
    t::SMSG_MOVE_FEATHER_FALL,
    t::SMSG_MOVE_NORMAL_FALL,
    t::SMSG_MOVE_SET_HOVER,
    t::SMSG_MOVE_UNSET_HOVER,
    t::SMSG_MOVE_SET_CAN_FLY,
    t::SMSG_MOVE_UNSET_CAN_FLY,
    t::SMSG_SPLINE_MOVE_ROOT,
    t::SMSG_SPLINE_MOVE_UNROOT,
    t::SMSG_SPLINE_MOVE_WATER_WALK,
    t::SMSG_SPLINE_MOVE_LAND_WALK,
    t::SMSG_SPLINE_MOVE_FEATHER_FALL,
    t::SMSG_SPLINE_MOVE_NORMAL_FALL,
    t::SMSG_SPLINE_MOVE_SET_HOVER,
    t::SMSG_SPLINE_MOVE_UNSET_HOVER,
    t::SMSG_SPLINE_MOVE_START_SWIM,
    t::SMSG_SPLINE_MOVE_STOP_SWIM,
    t::SMSG_SPLINE_MOVE_SET_WALK_MODE,
    t::SMSG_SPLINE_MOVE_SET_RUN_MODE,
    t::SMSG_SPLINE_MOVE_SET_FLYING,
    t::SMSG_SPLINE_MOVE_UNSET_FLYING,
    t::SMSG_MOVE_KNOCK_BACK,
];

#[test]
fn the_allow_list_names_every_opcode_it_reads_and_nothing_else_is_read() {
    for op in 0..0x500u16 {
        let outcome = parse_server_with_tail_for(&TBC_2_4_3, None, op, &[0xFF; 3]);
        let other = matches!(outcome, Ok((ServerPacket::Other { .. }, _)));
        if ALLOWED.contains(&op) {
            // A listed opcode is read (or fails to parse a junk body); it is never silently `Other`,
            // except a chat type the dispatch declines, which the 0xFF type is.
            assert!(
                !other || op == t::SMSG_MESSAGECHAT,
                "{op:#x} is listed but came back Other"
            );
        } else {
            assert!(other, "{op:#x} is read but not in ALLOWED");
        }
    }
}

#[test]
fn the_2_4_3_names_cover_every_allowed_opcode_the_slice_added() {
    for op in ALLOWED {
        assert!(
            messages::tbc_opcode_name(*op).is_some(),
            "{op:#x} has no 2.4.3 name"
        );
    }
}
