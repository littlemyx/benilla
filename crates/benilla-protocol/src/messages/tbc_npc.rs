//! NPC interaction on 2.4.3: the gossip menu, the vendor list and the quest giver marker. Each
//! layout is cmangos-tbc's (`GossipDef.cpp`, `ItemHandler.cpp`) and agrees with wow_messages' 2.4.3
//! definition, except where a comment says a part is single-source.

use std::io;

use crate::wire::{capacity_hint, read_cstring, read_u32_le, read_u64_le, read_u8};

use super::gossip::{GossipOption, QuestOption};
use super::vendor::VendorItem;
use super::{ServerPacket, TbcPacket};

/// Read `SMSG_GOSSIP_MESSAGE` (2.4.3) into the 1.12.1 reader's tuple. After the guid comes a `u32`
/// menu id (the value a `CMSG_GOSSIP_SELECT_OPTION` echoes; dropped here), then the text id; each
/// option adds a `u32` box money and a second cstring (the box's accept text), both dropped.
pub(super) fn read_gossip_message(
    r: &mut &[u8],
) -> io::Result<(u64, u32, Vec<GossipOption>, Vec<QuestOption>)> {
    let npc_guid = read_u64_le(r)?;
    let _menu_id = read_u32_le(r)?;
    let text_id = read_u32_le(r)?;
    let option_count = read_u32_le(r)?;
    let mut options = Vec::with_capacity(capacity_hint(option_count, 32));
    for _ in 0..option_count {
        let index = read_u32_le(r)?;
        let icon = read_u8(r)?;
        let coded = read_u8(r)? != 0;
        let _box_money = read_u32_le(r)?;
        let message = read_cstring(r)?;
        let _box_message = read_cstring(r)?;
        options.push(GossipOption {
            index,
            icon,
            coded,
            message,
        });
    }
    let quest_count = read_u32_le(r)?;
    let mut quests = Vec::with_capacity(capacity_hint(quest_count, 32));
    for _ in 0..quest_count {
        quests.push(QuestOption {
            quest_id: read_u32_le(r)?,
            icon: read_u32_le(r)?,
            level: read_u32_le(r)?,
            title: read_cstring(r)?,
        });
    }
    Ok((npc_guid, text_id, options, quests))
}

/// Read `SMSG_LIST_INVENTORY` (2.4.3): the 1.12.1 list with a `u32` extended cost (an
/// `ItemExtendedCost.dbc` row, dropped) ending each row.
pub(super) fn read_list_inventory(r: &mut &[u8]) -> io::Result<(u64, Vec<VendorItem>)> {
    let vendor_guid = read_u64_le(r)?;
    let count = read_u8(r)?;
    if count == 0 {
        let _no_inventory = read_u8(r)?;
        return Ok((vendor_guid, Vec::new()));
    }
    let mut items = Vec::with_capacity(capacity_hint(count, 150));
    for _ in 0..count {
        let item = VendorItem {
            slot: read_u32_le(r)?,
            entry: read_u32_le(r)?,
            display_id: read_u32_le(r)?,
            current_count: read_u32_le(r)?,
            price: read_u32_le(r)?,
            max_durability: read_u32_le(r)?,
            buy_count: read_u32_le(r)?,
        };
        let _extended_cost = read_u32_le(r)?;
        items.push(item);
    }
    Ok((vendor_guid, items))
}

/// The 1.12.1 quest giver status of a 2.4.3 one, by entry name: `NONE`, `UNAVAILABLE`, `CHAT`,
/// `INCOMPLETE` and `REWARD_REP` keep 0-4, and `AVAILABLE` moves from 6 to 5 (cmangos-tbc =
/// wow_messages 2.4.3, cmangos-classic = wow_messages 1.12). 5 (`AVAILABLE_REP`) has no 1.12.1
/// entry, and 7 and 8 are not mapped: the sources disagree on which of them is `REWARD2` (the
/// turn-in marker) and which is the older reward value.
pub fn quest_status_from_tbc(status: u8) -> Option<u32> {
    match status {
        0..=4 => Some(u32::from(status)),
        6 => Some(5),
        _ => None,
    }
}

/// Read `SMSG_QUESTGIVER_STATUS` (2.4.3): the NPC's guid and a `u8` status (a `u32` in 1.12.1).
pub(super) fn read_questgiver_status(r: &mut &[u8]) -> io::Result<ServerPacket> {
    let npc = read_u64_le(r)?;
    let status = read_u8(r)?;
    Ok(match quest_status_from_tbc(status) {
        Some(status) => ServerPacket::QuestGiverStatus { npc, status },
        None => ServerPacket::Tbc(TbcPacket::QuestGiverStatus { npc, status }),
    })
}
