//! The generated table of server packets whose bytes are the same in 1.12.1 and 2.4.3
//! (`messages::TBC_SAME_READERS`): its shape, its disjointness from the hand-written arms, and one
//! body per sampled packet read to the same value through both dispatches.

use benilla_build::{TBC_2_4_3, VANILLA_1_12_1};
use benilla_protocol::messages::{
    opcode, parse_server_with_tail_for, same_reader_for, tbc_opcode_name, ServerPacket,
    FIELDS_5875, TBC_SAME_READERS,
};

#[test]
fn every_entry_is_a_named_2_4_3_opcode_sorted_and_unique() {
    assert!(TBC_SAME_READERS.len() >= 100, "the table has shrunk");
    for pair in TBC_SAME_READERS.windows(2) {
        assert!(pair[0].0 < pair[1].0, "{:#x} is out of order", pair[1].0);
    }
    for &(tbc, vanilla) in TBC_SAME_READERS {
        assert!(tbc_opcode_name(tbc).is_some(), "{tbc:#x} has no 2.4.3 name");
        assert_eq!(tbc, vanilla, "the table lists same-numbered packets only");
        assert!(
            benilla_protocol::messages::opcode_name(vanilla).is_some(),
            "{vanilla:#x} has no 1.12.1 name"
        );
    }
}

#[test]
fn the_five_numbers_that_changed_meaning_are_not_on_the_table() {
    for op in [0x66u16, 0x67, 0x6B, 0x14F, 0x293] {
        assert_eq!(same_reader_for(op), None, "{op:#x} changed meaning");
    }
}

/// The 2.4.3 names the three hand-written dispatch files route by an arm of their own.
fn hand_routed_names() -> Vec<String> {
    let sources = [
        include_str!("../src/messages/tbc.rs"),
        include_str!("../src/messages/tbc_movement.rs"),
        include_str!("../src/messages/tbc_world.rs"),
    ];
    let mut names = Vec::new();
    for source in sources {
        let source = source.split("#[cfg(test)]").next().unwrap();
        for prefix in ["t::", "tbc_opcode::"] {
            let mut rest = source;
            while let Some(at) = rest.find(prefix) {
                let before = rest[..at].chars().last();
                let after = &rest[at + prefix.len()..];
                let end = after
                    .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
                    .unwrap_or(after.len());
                if !before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    names.push(after[..end].to_string());
                }
                rest = &after[end..];
            }
        }
    }
    names
}

#[test]
fn no_opcode_is_both_on_the_table_and_hand_routed() {
    let hand = hand_routed_names();
    assert!(
        hand.len() > 50,
        "the scan of the hand-written arms found {}",
        hand.len()
    );
    for &(tbc, _) in TBC_SAME_READERS {
        let name = tbc_opcode_name(tbc).unwrap();
        assert!(
            !hand.iter().any(|h| h == name),
            "{name} is on the table and hand-routed"
        );
    }
}

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn u64b(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn cstr(s: &str) -> Vec<u8> {
    let mut b = s.as_bytes().to_vec();
    b.push(0);
    b
}

fn packed(guid: u64) -> Vec<u8> {
    let mut b = Vec::new();
    benilla_protocol::wire::write_packed_guid(guid, &mut b).unwrap();
    b
}

fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.concat()
}

/// Read `body` for `op` in `build`; the whole body must be consumed.
fn read(tbc: bool, op: u16, body: &[u8]) -> ServerPacket {
    let result = if tbc {
        parse_server_with_tail_for(&TBC_2_4_3, None, op, body)
    } else {
        parse_server_with_tail_for(&VANILLA_1_12_1, Some(&FIELDS_5875), op, body)
    };
    let (packet, tail) = result.unwrap_or_else(|e| panic!("{op:#x} (tbc={tbc}) failed: {e}"));
    assert!(
        !matches!(packet, ServerPacket::Other { .. }),
        "{op:#x} (tbc={tbc}) is Other"
    );
    assert_eq!(tail, 0, "{op:#x} (tbc={tbc}) left {tail} bytes");
    packet
}

#[test]
fn a_sample_of_the_same_packets_reads_to_the_same_value_in_both_builds() {
    let samples: Vec<(u16, Vec<u8>)> = vec![
        (opcode::SMSG_ATTACKSTART, cat(&[u64b(0x11), u64b(0x22)])),
        (
            opcode::SMSG_ATTACKSTOP,
            cat(&[packed(0x11), packed(0x22), u32b(1)]),
        ),
        (opcode::SMSG_ATTACKSWING_NOTINRANGE, vec![]),
        (opcode::SMSG_ATTACKSWING_BADFACING, vec![]),
        (opcode::SMSG_LOGOUT_CANCEL_ACK, vec![]),
        (opcode::SMSG_EMOTE, cat(&[u32b(10), u64b(0x33)])),
        (opcode::SMSG_PLAY_SOUND, u32b(1234)),
        (opcode::SMSG_PLAY_OBJECT_SOUND, cat(&[u32b(55), u64b(0x44)])),
        (opcode::SMSG_LEARNED_SPELL, u32b(133)),
        (opcode::SMSG_COOLDOWN_EVENT, cat(&[u32b(133), u64b(0x55)])),
        (opcode::SMSG_CLEAR_COOLDOWN, cat(&[u32b(133), u64b(0x55)])),
        (opcode::SMSG_ITEM_COOLDOWN, cat(&[u64b(0x66), u32b(9)])),
        (opcode::SMSG_LOOT_MONEY_NOTIFY, u32b(250)),
        (opcode::SMSG_LOOT_CLEAR_MONEY, vec![]),
        (opcode::SMSG_LOOT_REMOVED, vec![3]),
        (
            opcode::SMSG_LOOT_RELEASE_RESPONSE,
            cat(&[u64b(0x77), vec![1]]),
        ),
        (opcode::SMSG_QUESTLOG_FULL, vec![]),
        (opcode::SMSG_QUESTUPDATE_COMPLETE, u32b(4)),
        (opcode::SMSG_QUESTUPDATE_FAILEDTIMER, u32b(4)),
        (
            opcode::SMSG_QUESTUPDATE_ADD_KILL,
            cat(&[u32b(4), u32b(100), u32b(2), u32b(8), u64b(0x88)]),
        ),
        (opcode::SMSG_PLAYED_TIME, cat(&[u32b(3600), u32b(60)])),
        (
            opcode::SMSG_SERVER_MESSAGE,
            cat(&[u32b(1), cstr("restart")]),
        ),
        (opcode::SMSG_CORPSE_RECLAIM_DELAY, u32b(30)),
        (opcode::SMSG_STANDSTATE_UPDATE, vec![1]),
        (opcode::SMSG_SPIRIT_HEALER_CONFIRM, u64b(0x99)),
        (opcode::SMSG_BINDER_CONFIRM, u64b(0x99)),
        (opcode::SMSG_DUEL_COUNTDOWN, u32b(3000)),
        (opcode::SMSG_DUEL_REQUESTED, cat(&[u64b(0x12), u64b(0x13)])),
        (opcode::SMSG_GROUP_DESTROYED, vec![]),
        (opcode::SMSG_GROUP_UNINVITE, vec![]),
        (opcode::SMSG_FISH_NOT_HOOKED, vec![]),
        (opcode::SMSG_FISH_ESCAPED, vec![]),
        (opcode::SMSG_RAID_GROUP_ONLY, cat(&[u32b(0), u32b(2)])),
        (
            opcode::SMSG_AREA_SPIRIT_HEALER_TIME,
            cat(&[u64b(0x14), u32b(5000)]),
        ),
        (opcode::SMSG_UPDATE_WORLD_STATE, cat(&[u32b(2313), u32b(1)])),
        (opcode::SMSG_INSTANCE_RESET, u32b(33)),
        (opcode::SMSG_PET_ACTION_SOUND, cat(&[u64b(0x15), u32b(2)])),
        (opcode::SMSG_SET_FACTION_VISIBLE, u32b(21)),
        (opcode::SMSG_AI_REACTION, cat(&[u64b(0x16), u32b(2)])),
    ];
    assert!(samples.len() >= 30);
    for (op, body) in &samples {
        assert_eq!(
            same_reader_for(*op),
            Some(*op),
            "{op:#x} is not on the table"
        );
        let tbc = read(true, *op, body);
        let vanilla = read(false, *op, body);
        assert_eq!(
            tbc.name(),
            vanilla.name(),
            "{op:#x} reads to another packet"
        );
        // `ServerPacket` has no `Debug`; the decoded events carry the values.
        let (a, b) = (
            benilla_protocol::decode(tbc),
            benilla_protocol::decode(vanilla),
        );
        assert_eq!(
            format!("{a:?}"),
            format!("{b:?}"),
            "{op:#x} reads differently"
        );
    }
}
