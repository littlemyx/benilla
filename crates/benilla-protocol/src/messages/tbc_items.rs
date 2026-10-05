//! Items and inventory on 2.4.3: the slot numbering, the repair request and the inventory packets
//! whose bytes differ. Each layout is cmangos-tbc's (`Player.h`, `Player.cpp`, `ItemHandler.cpp`) and
//! agrees with wow_messages' 2.4.3 definition and `ItemSlot` / `InventoryResult` enums, except where
//! a comment says a part is single-source.

use std::io;

use crate::wire::{read_u32_le, read_u64_le, read_u8};

use super::{ItemPushResult, ServerPacket, TbcPacket};

/// The 2.4.3 number of a 1.12.1 inventory slot or bag slot. Equipment (0-18), bag slots (19-22),
/// the pack (23-38) and the bank (39-62) keep their numbers, and 255 (no bag, the player's own
/// grid) stays; the six bank bag slots 63-68 become 67-72, the twelve buyback slots 69-80 become
/// 74-85 and the sixteen keyring slots 81-96 become 86-101 (cmangos-classic and cmangos-tbc
/// `Player.h`; wow_messages' `ItemSlot` agrees on every one of these names). `None` for a number
/// with no agreed 2.4.3 counterpart (wow_messages' 1.12.1 keyring goes on to 112, cmangos-classic
/// stops at 96).
pub fn slot_to_tbc(slot: u8) -> Option<u8> {
    match slot {
        0..=62 | 255 => Some(slot),
        63..=68 => Some(slot + 4),
        69..=96 => Some(slot + 5),
        _ => None,
    }
}

/// The 2.4.3 `CMSG_REPAIR_ITEM` body: the 1.12.1 body and a `u8` that is 1 to repair from the
/// guild bank (0 here).
pub fn repair_item_tbc(vendor_guid: u64, item_guid: u64) -> Vec<u8> {
    let mut body = super::repair_item(vendor_guid, item_guid);
    body.push(0);
    body
}

/// Read `SMSG_ITEM_PUSH_RESULT` (2.4.3): the 1.12.1 body and the `u32` count of that item now in
/// the inventory (dropped).
pub(super) fn read_item_push_result(r: &mut &[u8]) -> io::Result<ItemPushResult> {
    let push = super::loot::read_item_push_result(r)?;
    let _in_inventory = read_u32_le(r)?;
    Ok(push)
}

/// The highest `InventoryResult` 1.12.1 also has; cmangos-tbc = wow_messages 2.4.3 and
/// cmangos-classic = wow_messages 1.12 give every one of 0..=66 the same value in both builds, and
/// 67..=80 (plus cmangos-tbc's `EVENT_AUTOEQUIP_BIND_CONFIRM`) have no 1.12.1 entry.
const LAST_SHARED_INVENTORY_RESULT: u8 = 66;

/// Read `SMSG_INVENTORY_CHANGE_FAILURE` (2.4.3): `u8` result and, unless it is 0, two item guids,
/// a `u8` bag slot, then a `u32` required level for result 1 (1.12.1 puts the level before the
/// guids). A result 1.12.1 lacks reads as [`TbcPacket::InventoryChangeFailed`]; any tail past what
/// is read (cmangos-tbc's bind-confirm arm) is dropped.
pub(super) fn read_inventory_change_failure(r: &mut &[u8]) -> io::Result<ServerPacket> {
    let result = read_u8(r)?;
    if result == 0 {
        return Ok(ServerPacket::InventoryChangeFailure {
            reason: 0,
            required_level: None,
            item_guid: 0,
            bag_slot: 0,
        });
    }
    let item_guid = read_u64_le(r)?;
    let _item2 = read_u64_le(r)?;
    let bag_slot = read_u8(r)?;
    let required_level = if result == 1 {
        Some(read_u32_le(r)?)
    } else {
        None
    };
    *r = &r[r.len()..];
    Ok(if result <= LAST_SHARED_INVENTORY_RESULT {
        ServerPacket::InventoryChangeFailure {
            reason: result,
            required_level,
            item_guid,
            bag_slot,
        }
    } else {
        ServerPacket::Tbc(TbcPacket::InventoryChangeFailed {
            result,
            item_guid,
            bag_slot,
        })
    })
}
