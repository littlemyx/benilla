//! 2.4.3 loot window packets. The loot layouts are the same in cmangos-tbc (`LootMgr.cpp`
//! `GetLootContentFor`, `SendReleaseFor`, `NotifyItemRemoved`) and cmangos-classic, and these were
//! read for 2.4.3 as same-bytes; this pins them. The response body is wow_messages' test vector of
//! `SMSG_LOOT_RESPONSE` (its 3.3.5 vector, whose row layout is cmangos-tbc's, not its 2.4.3 row
//! struct, which omits the count, display id and suffix words).

use benilla_build::TBC_2_4_3;
use benilla_protocol::messages::{parse_server_with_tail_for, ServerPacket};

fn tbc(op: u16, body: &[u8]) -> ServerPacket {
    let (packet, tail) = parse_server_with_tail_for(&TBC_2_4_3, None, op, body).unwrap();
    assert_eq!(tail, 0, "{} left {tail} bytes unread", packet.name());
    packet
}

#[test]
fn a_loot_window_reads_gold_and_item_rows() {
    // wow_messages' vector: guid 1, corpse, no gold, one row: slot 0, item 117, count 1.
    let mut body = 1u64.to_le_bytes().to_vec();
    body.push(1);
    body.extend(0u32.to_le_bytes());
    body.push(1);
    body.push(0);
    for v in [117u32, 1, 0, 0, 0] {
        body.extend(v.to_le_bytes());
    }
    body.push(0);
    match tbc(0x0160, &body) {
        ServerPacket::LootResponse {
            guid,
            loot_type,
            gold,
            items,
        } => {
            assert_eq!((guid, loot_type, gold), (1, 1, 0));
            assert_eq!(items.len(), 1);
            assert_eq!(
                (
                    items[0].slot,
                    items[0].item_id,
                    items[0].count,
                    items[0].slot_type
                ),
                (0, 117, 1, 0)
            );
        }
        other => panic!("{}", other.name()),
    }
    // Money and two rows (cmangos-tbc: gold, count, then slot, item, count, display, suffix,
    // property, slot type per row).
    let mut two = 0x42u64.to_le_bytes().to_vec();
    two.push(1);
    two.extend(25u32.to_le_bytes());
    two.push(2);
    for (slot, item, display) in [(0u8, 159u32, 1103u32), (1, 2070, 6392)] {
        two.push(slot);
        for v in [item, 1, display, 0, 0] {
            two.extend(v.to_le_bytes());
        }
        two.push(0);
    }
    match tbc(0x0160, &two) {
        ServerPacket::LootResponse { gold, items, .. } => {
            assert_eq!((gold, items.len()), (25, 2));
            assert_eq!((items[1].item_id, items[1].display_info_id), (2070, 6392));
        }
        other => panic!("{}", other.name()),
    }
    // The error shape: loot type 0 and a `LootError` (4: too far).
    let mut err = 0x42u64.to_le_bytes().to_vec();
    err.extend([0, 4]);
    assert!(matches!(
        tbc(0x0160, &err),
        ServerPacket::LootError {
            guid: 0x42,
            error: 4
        }
    ));
}

#[test]
fn the_money_removal_and_release_notices_read() {
    // `SMSG_LOOT_MONEY_NOTIFY`: our share; `SMSG_LOOT_CLEAR_MONEY`: empty; `SMSG_LOOT_REMOVED`: the
    // slot; `SMSG_LOOT_RELEASE_RESPONSE`: guid and 1.
    assert!(matches!(
        tbc(0x0163, &25u32.to_le_bytes()),
        ServerPacket::LootMoneyNotify { amount: 25 }
    ));
    assert!(matches!(tbc(0x0165, &[]), ServerPacket::LootClearMoney));
    assert!(matches!(
        tbc(0x0162, &[1]),
        ServerPacket::LootRemoved { slot: 1 }
    ));
    let mut release = 0x42u64.to_le_bytes().to_vec();
    release.push(1);
    assert!(matches!(
        tbc(0x0161, &release),
        ServerPacket::LootReleaseResponse {
            guid: 0x42,
            result: 1
        }
    ));
}
