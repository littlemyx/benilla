//! 2.4.3 quest panels, quest query reply and quest log pushes. Fixtures are built from the cmangos-tbc
//! builders (`GossipDef.cpp`, `QuestHandler.cpp`, `Player.cpp`), which agree with wow_messages' 2.4.3
//! definitions (it ships no 2.4.3 vector for these); the one disagreement, the DETAILS item rows'
//! display id, is noted in `tbc_quest.rs`.

use benilla_build::TBC_2_4_3;
use benilla_protocol::messages::{self, parse_server_with_tail_for, tbc_opcode as t, ServerPacket};

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn cstr(s: &str) -> Vec<u8> {
    let mut b = s.as_bytes().to_vec();
    b.push(0);
    b
}

fn tbc(op: u16, body: &[u8]) -> ServerPacket {
    let (packet, tail) = parse_server_with_tail_for(&TBC_2_4_3, None, op, body).unwrap();
    assert_eq!(tail, 0, "{} left {tail} bytes unread", packet.name());
    packet
}

fn words(b: &mut Vec<u8>, vs: &[u32]) {
    for v in vs {
        b.extend(u32b(*v));
    }
}

#[test]
fn quest_details_carry_suggested_players_honor_spells_and_a_title() {
    let mut b = 0x55u64.to_le_bytes().to_vec();
    words(&mut b, &[783]);
    b.extend(cstr("A Threat Within"));
    b.extend(cstr("Details text"));
    b.extend(cstr("Objectives text"));
    words(&mut b, &[1, 3]); // auto finish, suggested players
    words(&mut b, &[1, 2092, 1, 5000]); // one choice row: item, count, display
    words(&mut b, &[2, 25, 2, 6000, 26, 1, 6001]); // two reward rows
    words(&mut b, &[250, 40, 9000, 9001, 17]); // money, honor, spell, cast spell, title bit
    words(&mut b, &[2, 1, 100, 5, 0]); // two {emote, delay}
    match tbc(t::SMSG_QUESTGIVER_QUEST_DETAILS, &b) {
        ServerPacket::QuestGiverDetails(d) => {
            assert_eq!((d.npc, d.quest_id, d.auto_finish), (0x55, 783, 1));
            assert_eq!(
                (d.title.as_str(), d.details.as_str(), d.objectives.as_str()),
                ("A Threat Within", "Details text", "Objectives text")
            );
            assert_eq!(d.choices.len(), 1);
            assert_eq!(
                (
                    d.choices[0].item_id,
                    d.choices[0].count,
                    d.choices[0].display_id
                ),
                (2092, 1, 5000)
            );
            assert_eq!(d.rewards.len(), 2);
            assert_eq!(d.rewards[1].item_id, 26);
            assert_eq!((d.money, d.reward_spell), (250, 9000));
        }
        other => panic!("{}", other.name()),
    }
    // A hidden-rewards quest writes three zeros where the rows and money go.
    let mut h = 0x55u64.to_le_bytes().to_vec();
    words(&mut h, &[9]);
    for s in ["T", "D", "O"] {
        h.extend(cstr(s));
    }
    words(&mut h, &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    // auto finish, suggested, then rows 0, rows 0, money 0, honor 0, spell 0, cast 0, title 0, emotes 0
    match tbc(t::SMSG_QUESTGIVER_QUEST_DETAILS, &h) {
        ServerPacket::QuestGiverDetails(d) => {
            assert!(d.choices.is_empty() && d.rewards.is_empty());
            assert_eq!(d.money, 0);
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn an_offer_reward_has_suggested_players_honor_and_two_spells() {
    let mut b = 0x55u64.to_le_bytes().to_vec();
    words(&mut b, &[783]);
    b.extend(cstr("A Threat Within"));
    b.extend(cstr("Well done."));
    words(&mut b, &[1, 0]); // auto finish, suggested players
    words(&mut b, &[1, 100, 1]); // one emote: delay, emote
    words(&mut b, &[0]); // no choices
    words(&mut b, &[1, 25, 4, 6000]); // one reward row
    words(&mut b, &[1100, 0, 8, 12, 0, 0]); // money, honor, 0x08, spell, cast spell, title
    match tbc(t::SMSG_QUESTGIVER_OFFER_REWARD, &b) {
        ServerPacket::QuestGiverOfferReward(o) => {
            assert_eq!((o.npc, o.quest_id, o.auto_finish), (0x55, 783, 1));
            assert_eq!(o.offer_text, "Well done.");
            assert!(o.choices.is_empty());
            assert_eq!(o.rewards.len(), 1);
            assert_eq!((o.rewards[0].item_id, o.rewards[0].count), (25, 4));
            assert_eq!((o.money, o.quest_flags, o.reward_spell), (1100, 8, 12));
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn request_items_completable_is_the_first_of_the_four_trailing_words() {
    let build = |completable: u32| {
        let mut b = 0x55u64.to_le_bytes().to_vec();
        words(&mut b, &[783]);
        b.extend(cstr("T"));
        b.extend(cstr("Bring me wolf pelts."));
        words(&mut b, &[0, 6, 1, 2]); // emote delay, emote, close on cancel, suggested players
        words(&mut b, &[50]); // required money
        words(&mut b, &[1, 750, 8, 2000]); // one required item
        words(&mut b, &[completable, 4, 8, 0x10]);
        b
    };
    for (word, want) in [(0u32, false), (3, true)] {
        match tbc(t::SMSG_QUESTGIVER_REQUEST_ITEMS, &build(word)) {
            ServerPacket::QuestGiverRequestItems(q) => {
                assert_eq!((q.npc, q.quest_id, q.emote), (0x55, 783, 6));
                assert_eq!((q.close_on_cancel, q.required_money), (1, 50));
                assert_eq!(q.required_items.len(), 1);
                assert_eq!(
                    (q.required_items[0].item_id, q.required_items[0].count),
                    (750, 8)
                );
                assert_eq!(q.is_complete, want);
            }
            other => panic!("{}", other.name()),
        }
    }
}

#[test]
fn a_quest_complete_has_honor_between_money_and_the_items() {
    let mut b = Vec::new();
    words(&mut b, &[783, 3, 450, 120, 0, 1, 25, 2]);
    match tbc(t::SMSG_QUESTGIVER_QUEST_COMPLETE, &b) {
        ServerPacket::QuestGiverComplete(c) => {
            assert_eq!((c.quest_id, c.xp, c.money), (783, 450, 120));
            assert_eq!(c.items, vec![(25, 2)]);
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn invalid_and_failed_reasons_are_mapped_to_the_1_12_1_numbers() {
    // 2.4.3: missing items 21, money 23, second already-on 18; 7 and 16 have no 1.12.1 entry.
    for (raw, want) in [
        (0u32, 0u32),
        (1, 1),
        (6, 6),
        (12, 12),
        (13, 13),
        (18, 13),
        (21, 20),
        (23, 22),
        (7, 0),
        (16, 0),
        (26, 0),
        (27, 0),
        (20, 0),
        (22, 0),
    ] {
        match tbc(t::SMSG_QUESTGIVER_QUEST_INVALID, &u32b(raw)) {
            ServerPacket::QuestGiverInvalid { msg } => assert_eq!(msg, want, "raw {raw}"),
            other => panic!("{}", other.name()),
        }
        assert_eq!(messages::quest_invalid_reason_from_tbc(raw), want);
    }
    for (raw, want) in [(4u32, 4u32), (17, 17), (0, 0), (50, 0), (21, 0)] {
        let mut b = u32b(783);
        b.extend(u32b(raw));
        match tbc(t::SMSG_QUESTGIVER_QUEST_FAILED, &b) {
            ServerPacket::QuestGiverFailed { quest_id, reason } => {
                assert_eq!((quest_id, reason), (783, want), "raw {raw}");
            }
            other => panic!("{}", other.name()),
        }
    }
}

/// The query reply for a quest like 783 (the fields, in cmangos-tbc order).
fn query_body() -> Vec<u8> {
    let mut b = Vec::new();
    words(&mut b, &[783, 2, 1, 0xFFFF_FFFF - 9 + 1, 0]); // id, method, level, zone -9, type
    words(&mut b, &[3]); // suggested players
    words(&mut b, &[0, 0, 0, 0]); // rep objective faction/value, opposite faction/value
    words(&mut b, &[784, 450, 4000, 1234, 1235, 6, 0, 0x8]); // next, money, max money, spell, cast, honor, source item, flags
    words(&mut b, &[0]); // title id
    for (id, n) in [(25u32, 1u32), (0, 0), (0, 0), (0, 0)] {
        words(&mut b, &[id, n]);
    }
    for i in 0..6u32 {
        words(
            &mut b,
            &[if i < 2 { 2092 + i } else { 0 }, u32::from(i < 2)],
        );
    }
    words(&mut b, &[0]); // point map
    b.extend(1.5f32.to_le_bytes());
    b.extend((-2.5f32).to_le_bytes());
    words(&mut b, &[0]); // point option
    for s in ["A Threat Within", "Objectives", "Details", "End text"] {
        b.extend(cstr(s));
    }
    for i in 0..4u32 {
        let (c, n, item, ic) = if i == 0 { (823, 1, 0, 0) } else { (0, 0, 0, 0) };
        words(&mut b, &[c, n, item, ic]);
    }
    for s in ["Speak with Deputy Willem", "", "", ""] {
        b.extend(cstr(s));
    }
    b
}

#[test]
fn a_quest_query_reply_reads_the_2_4_3_template() {
    match tbc(t::SMSG_QUEST_QUERY_RESPONSE, &query_body()) {
        ServerPacket::QuestQueryResponse(q) => {
            assert_eq!((q.quest_id, q.method, q.level), (783, 2, 1));
            assert_eq!((q.zone_or_sort, q.quest_type), (-9, 0));
            assert_eq!(q.next_quest_in_chain, 784);
            assert_eq!((q.money, q.money_max_level), (450, 4000));
            assert_eq!((q.reward_spell, q.src_item_id, q.flags), (1234, 0, 8));
            assert_eq!(q.rewards[0], (25, 1));
            assert_eq!(q.choices[1], (2093, 1));
            assert_eq!((q.point_x, q.point_y), (1.5, -2.5));
            assert_eq!(
                (
                    q.title.as_str(),
                    q.objectives_text.as_str(),
                    q.details.as_str()
                ),
                ("A Threat Within", "Objectives", "Details")
            );
            assert_eq!(q.end_text, "End text");
            assert_eq!(
                (
                    q.objectives[0].creature_or_go,
                    q.objectives[0].required_count
                ),
                (823, 1)
            );
            assert_eq!(q.objectives[0].text, "Speak with Deputy Willem");
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn the_quest_log_pushes_read_on_2_4_3() {
    // Same bytes as 1.12.1 (cmangos-tbc `SendQuestUpdateAddCreatureOrGo`, `SendQuestUpdateAddItem`).
    let mut kill = Vec::new();
    words(&mut kill, &[783, 823, 1, 8]);
    kill.extend(0xF130_0000_0000_0007u64.to_le_bytes());
    assert!(matches!(
        tbc(0x0199, &kill),
        ServerPacket::QuestUpdateAddKill { quest_id: 783, .. }
    ));
    let mut item = Vec::new();
    words(&mut item, &[750, 3]);
    assert!(matches!(
        tbc(0x019A, &item),
        ServerPacket::QuestUpdateAddItem { .. }
    ));
    assert!(matches!(
        tbc(0x0198, &u32b(783)),
        ServerPacket::QuestUpdateComplete { quest_id: 783 }
    ));
    // Single-source: wow_messages only (no cmangos builder).
    assert!(matches!(
        tbc(t::SMSG_QUESTUPDATE_FAILED, &u32b(783)),
        ServerPacket::QuestUpdateFailed { quest_id: 783 }
    ));
    assert!(matches!(tbc(0x0195, &[]), ServerPacket::QuestLogFull));
    // The greeting list: the 1.12.1 bytes.
    let mut list = 0x55u64.to_le_bytes().to_vec();
    list.extend(cstr("Greetings."));
    words(&mut list, &[0, 0]);
    list.push(1);
    words(&mut list, &[783, 5, 1]);
    list.extend(cstr("A Threat Within"));
    match tbc(0x0185, &list) {
        ServerPacket::QuestGiverQuestList(l) => assert_eq!(l.quests.len(), 1),
        other => panic!("{}", other.name()),
    }
}
