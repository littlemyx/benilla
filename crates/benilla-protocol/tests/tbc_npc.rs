//! 2.4.3 NPC interaction: the gossip menu, the vendor list and the quest giver marker. Fixtures are
//! built from the facts tables (cmangos-tbc builders, agreeing with wow_messages' 2.4.3
//! definitions; wow_messages ships no 2.4.3 vector for these).

use benilla_build::TBC_2_4_3;
use benilla_protocol::messages::{
    self, parse_server_with_tail_for, tbc_opcode as t, ServerPacket, TbcPacket,
};

/// 1.12.1's `DIALOG_STATUS_*` values (cmangos-classic).
mod dialog_status {
    pub const NONE: u32 = 0;
    pub const UNAVAILABLE: u32 = 1;
    pub const CHAT: u32 = 2;
    pub const INCOMPLETE: u32 = 3;
    pub const REWARD_REP: u32 = 4;
    pub const AVAILABLE: u32 = 5;
}

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

#[test]
fn a_gossip_menu_has_a_menu_id_and_box_fields_in_2_4_3() {
    let mut b = 0x55u64.to_le_bytes().to_vec();
    b.extend(u32b(4321)); // menu id
    b.extend(u32b(7)); // title text id
    b.extend(u32b(2)); // options
                       // option 0: icon 1 (vendor), not coded, no box money, text, no accept text
    b.extend(u32b(0));
    b.push(1);
    b.push(0);
    b.extend(u32b(0));
    b.extend(cstr("I want to browse your goods."));
    b.extend(cstr(""));
    // option 1: coded, 50 copper box, accept text
    b.extend(u32b(1));
    b.push(0);
    b.push(1);
    b.extend(u32b(50));
    b.extend(cstr("Pay the toll"));
    b.extend(cstr("Pay 50 copper?"));
    b.extend(u32b(1)); // quests
    b.extend(u32b(783));
    b.extend(u32b(2));
    b.extend(u32b(1));
    b.extend(cstr("A Threat Within"));
    match tbc(t::SMSG_GOSSIP_MESSAGE, &b) {
        ServerPacket::GossipMessage {
            npc,
            text_id,
            options,
            quests,
        } => {
            assert_eq!((npc, text_id), (0x55, 7));
            assert_eq!(options.len(), 2);
            assert_eq!(
                (
                    options[0].icon,
                    options[0].coded,
                    options[0].message.as_str()
                ),
                (1, false, "I want to browse your goods.")
            );
            assert_eq!((options[1].index, options[1].coded), (1, true));
            assert_eq!(quests.len(), 1);
            assert_eq!((quests[0].quest_id, quests[0].level), (783, 1));
            assert_eq!(quests[0].title, "A Threat Within");
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn a_vendor_row_ends_in_its_extended_cost() {
    let mut b = 0x66u64.to_le_bytes().to_vec();
    b.push(2);
    for (slot, entry, ext) in [(1u32, 159u32, 0u32), (2, 2070, 1234)] {
        for v in [slot, entry, 1103, 0xFFFF_FFFF, 25, 0, 1, ext] {
            b.extend(u32b(v));
        }
    }
    match tbc(t::SMSG_LIST_INVENTORY, &b) {
        ServerPacket::VendorList { vendor, items } => {
            assert_eq!(vendor, 0x66);
            assert_eq!(items.len(), 2);
            assert_eq!(
                (
                    items[1].slot,
                    items[1].entry,
                    items[1].price,
                    items[1].buy_count
                ),
                (2, 2070, 25, 1)
            );
        }
        other => panic!("{}", other.name()),
    }
    // An empty list is followed by one byte, as in 1.12.1.
    let mut empty = 0x66u64.to_le_bytes().to_vec();
    empty.extend([0, 0]);
    assert!(matches!(
        tbc(t::SMSG_LIST_INVENTORY, &empty),
        ServerPacket::VendorList { items, .. } if items.is_empty()
    ));
}

#[test]
fn the_quest_marker_is_a_byte_and_its_available_value_moved() {
    let body = |status: u8| {
        let mut b = 0x77u64.to_le_bytes().to_vec();
        b.push(status);
        b
    };
    // 0-4 keep their numbers; AVAILABLE is 6 in 2.4.3 and 5 in 1.12.1.
    for (status, want) in [
        (0u8, dialog_status::NONE),
        (1, dialog_status::UNAVAILABLE),
        (2, dialog_status::CHAT),
        (3, dialog_status::INCOMPLETE),
        (4, dialog_status::REWARD_REP),
        (6, dialog_status::AVAILABLE),
    ] {
        match tbc(t::SMSG_QUESTGIVER_STATUS, &body(status)) {
            ServerPacket::QuestGiverStatus { npc, status: s } => {
                assert_eq!((npc, s), (0x77, want), "status {status}");
            }
            other => panic!("{}", other.name()),
        }
    }
    // 5 (AVAILABLE_REP, new) and 7 and 8 (the sources disagree on their names) are not mapped.
    for status in [5u8, 7, 8] {
        assert_eq!(messages::quest_status_from_tbc(status), None);
        assert!(matches!(
            tbc(t::SMSG_QUESTGIVER_STATUS, &body(status)),
            ServerPacket::Tbc(TbcPacket::QuestGiverStatus { npc: 0x77, status: s }) if s == status
        ));
    }
}
