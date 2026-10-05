//! 2.4.3 items and inventory: the slot numbering, the repair request, the push result and the
//! inventory failure. Fixtures are built from the facts tables (cmangos-tbc builders and
//! `Player.h`, agreeing with wow_messages' 2.4.3 definitions and `ItemSlot` / `InventoryResult`
//! enums; wow_messages ships no 2.4.3 vector for these).

use benilla_build::{TBC_2_4_3, VANILLA_1_12_1};
use benilla_protocol::messages::{
    self, opcode, parse_server_with_tail_for, tbc_opcode as t, ServerPacket, TbcPacket, FIELDS_5875,
};

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn u64b(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn tbc(op: u16, body: &[u8]) -> ServerPacket {
    let (packet, tail) = parse_server_with_tail_for(&TBC_2_4_3, None, op, body).unwrap();
    assert_eq!(tail, 0, "{} left {tail} bytes unread", packet.name());
    packet
}

#[test]
fn the_slot_numbers_move_where_the_enums_say() {
    // (1.12.1 slot, 2.4.3 slot): the `ItemSlot` entries both builds name.
    for (vanilla, tbc_slot) in [
        (0u8, 0u8), // HEAD
        (18, 18),   // TABARD
        (19, 19),   // BAG1
        (23, 23),   // INVENTORY_0
        (38, 38),   // INVENTORY_15
        (39, 39),   // BANK_1
        (62, 62),   // BANK_24
        (63, 67),   // BANK_BAG_SLOT_1
        (68, 72),   // BANK_BAG_SLOT_6
        (69, 74),   // VENDOR_BUYBACK_1
        (80, 85),   // VENDOR_BUYBACK_12
        (81, 86),   // KEYRING_1
        (96, 101),  // KEYRING_16
        (255, 255), // no bag: the player's own grid
    ] {
        assert_eq!(messages::slot_to_tbc(vanilla), Some(tbc_slot), "{vanilla}");
    }
    // cmangos-classic ends the keyring at 96 (wow_messages' 1.12.1 goes on to 112): not mapped.
    for slot in [97u8, 100, 112, 113, 200, 254] {
        assert_eq!(messages::slot_to_tbc(slot), None, "{slot}");
    }
    // Every mapped slot is distinct.
    let mut seen = std::collections::HashSet::new();
    for slot in 0..=255u8 {
        if let Some(t) = messages::slot_to_tbc(slot) {
            assert!(seen.insert(t), "{slot} collides");
        }
    }
}

#[test]
fn the_repair_request_has_a_guild_bank_byte_in_2_4_3() {
    let mut want = u64b(0x11);
    want.extend(u64b(0x22));
    assert_eq!(messages::repair_item(0x11, 0x22), want);
    want.push(0);
    assert_eq!(messages::repair_item_tbc(0x11, 0x22), want);
}

fn push(extra: bool) -> Vec<u8> {
    let mut b = u64b(0x17);
    for v in [0u32, 0, 1] {
        b.extend(u32b(v));
    }
    b.push(255);
    for v in [24u32, 6948, 0, 0, 1] {
        b.extend(u32b(v));
    }
    if extra {
        b.extend(u32b(3));
    }
    b
}

#[test]
fn a_push_result_has_the_inventory_count_in_2_4_3() {
    match tbc(t::SMSG_ITEM_PUSH_RESULT, &push(true)) {
        ServerPacket::ItemPushResult(p) => {
            assert_eq!(
                (p.player_guid, p.item_slot, p.item_entry, p.count),
                (0x17, 24, 6948, 1)
            );
            assert_eq!(p.bag_slot, 255);
        }
        other => panic!("{}", other.name()),
    }
    // The 1.12.1 body is a body short in 2.4.3 and still reads in 1.12.1.
    assert!(
        parse_server_with_tail_for(&TBC_2_4_3, None, t::SMSG_ITEM_PUSH_RESULT, &push(false))
            .is_err()
    );
    let (v, tail) = parse_server_with_tail_for(
        &VANILLA_1_12_1,
        Some(&FIELDS_5875),
        opcode::SMSG_ITEM_PUSH_RESULT,
        &push(false),
    )
    .unwrap();
    assert_eq!(tail, 0);
    assert_eq!(v.name(), "SMSG_ITEM_PUSH_RESULT");
}

fn change_failure(result: u8, slot: u8, tail: &[u8]) -> Vec<u8> {
    let mut b = vec![result];
    b.extend(u64b(0x30));
    b.extend(u64b(0));
    b.push(slot);
    b.extend(tail);
    b
}

#[test]
fn an_inventory_failure_has_its_guids_before_the_level() {
    // Result 0 is bare.
    assert!(matches!(
        tbc(t::SMSG_INVENTORY_CHANGE_FAILURE, &[0]),
        ServerPacket::InventoryChangeFailure { reason: 0, .. }
    ));
    // Result 16 (a full bag in 1.12.1's numbering, the same value in 2.4.3): guids and a bag slot.
    match tbc(
        t::SMSG_INVENTORY_CHANGE_FAILURE,
        &change_failure(16, 255, &[]),
    ) {
        ServerPacket::InventoryChangeFailure {
            reason,
            required_level,
            item_guid,
            bag_slot,
        } => assert_eq!(
            (reason, required_level, item_guid, bag_slot),
            (16, None, 0x30, 255)
        ),
        other => panic!("{}", other.name()),
    }
    // Result 1 (level too low): the level follows the bag slot, after the guids.
    match tbc(
        t::SMSG_INVENTORY_CHANGE_FAILURE,
        &change_failure(1, 255, &u32b(40)),
    ) {
        ServerPacket::InventoryChangeFailure { required_level, .. } => {
            assert_eq!(required_level, Some(40));
        }
        other => panic!("{}", other.name()),
    }
    // A result 2.4.3 added (67, ITEM_UNIQUE_EQUIPABLE) and cmangos-tbc's bind confirm (its tail
    // is dropped) have no 1.12.1 reason.
    for (result, tail) in [
        (67u8, vec![]),
        (80, vec![]),
        (81, [u64b(0), u32b(0), u64b(0)].concat()),
    ] {
        match tbc(
            t::SMSG_INVENTORY_CHANGE_FAILURE,
            &change_failure(result, 255, &tail),
        ) {
            ServerPacket::Tbc(TbcPacket::InventoryChangeFailed {
                result: r,
                item_guid,
                bag_slot,
            }) => assert_eq!((r, item_guid, bag_slot), (result, 0x30, 255)),
            other => panic!("{}", other.name()),
        }
    }
}
