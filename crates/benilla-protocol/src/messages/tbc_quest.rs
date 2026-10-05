//! The quest panels and the quest query reply on 2.4.3. Every layout is cmangos-tbc's
//! (`GossipDef.cpp`, `QuestHandler.cpp`, `Player.cpp`) and agrees with wow_messages' 2.4.3
//! definition, except where a comment says a part is single-source or the sources differ.

use std::io;

use crate::wire::{
    capacity_hint, read_cstring, read_f32_le, read_i32_le, read_u32_le, read_u64_le,
};

use super::quest::{
    QuestComplete, QuestDetails, QuestObjective, QuestOfferReward, QuestRequestItems,
    QuestRewardItem, QuestTemplate, QUEST_OBJECTIVES_COUNT, QUEST_REWARDS_COUNT,
    QUEST_REWARD_CHOICES_COUNT,
};

/// A counted run of `{u32 item, u32 count, u32 display id}` rows. wow_messages' 2.4.3 DETAILS rows
/// omit the display id; cmangos-tbc writes it, as do both 1.12.1 emulators for the same rows
/// (the existing 1.12.1 reader), so the three-word row is read.
fn read_item_rows(r: &mut &[u8]) -> io::Result<Vec<QuestRewardItem>> {
    let count = read_u32_le(r)?;
    let mut items = Vec::with_capacity(capacity_hint(count, 6));
    for _ in 0..count {
        items.push(QuestRewardItem {
            item_id: read_u32_le(r)?,
            count: read_u32_le(r)?,
            display_id: read_u32_le(r)?,
        });
    }
    Ok(items)
}

/// `SMSG_QUESTGIVER_QUEST_DETAILS` (2.4.3): npc, id, title, details, objectives, auto-finish, then
/// `u32` suggested players (new), the choice and reward rows and money, `u32` honor (new), the
/// reward spell and the cast spell (the second new), `u32` title bit (new), and `{emote, delay}`
/// pairs. A hidden-rewards quest writes three zeros, which read as empty rows. New values are
/// dropped.
pub(super) fn read_quest_details(r: &mut &[u8]) -> io::Result<QuestDetails> {
    let npc = read_u64_le(r)?;
    let quest_id = read_u32_le(r)?;
    let title = read_cstring(r)?;
    let details = read_cstring(r)?;
    let objectives = read_cstring(r)?;
    let auto_finish = read_u32_le(r)?;
    let _suggested_players = read_u32_le(r)?;
    let choices = read_item_rows(r)?;
    let rewards = read_item_rows(r)?;
    let money = read_i32_le(r)?;
    let _honor = read_u32_le(r)?;
    let reward_spell = read_u32_le(r)?;
    let _cast_spell = read_u32_le(r)?;
    let _title_bit = read_u32_le(r)?;
    let emote_count = read_u32_le(r)?;
    for _ in 0..emote_count {
        let _emote = read_u32_le(r)?;
        let _delay = read_u32_le(r)?;
    }
    Ok(QuestDetails {
        npc,
        quest_id,
        title,
        details,
        objectives,
        auto_finish,
        choices,
        rewards,
        money,
        reward_spell,
    })
}

/// `SMSG_QUESTGIVER_OFFER_REWARD` (2.4.3): the 1.12.1 order with `u32` suggested players after the
/// auto-finish word, `u32` honor after the money, and after the `0x08` word (read where 1.12.1
/// reads its flags) the reward spell, a cast spell and a title bit (the last two dropped).
pub(super) fn read_offer_reward(r: &mut &[u8]) -> io::Result<QuestOfferReward> {
    let npc = read_u64_le(r)?;
    let quest_id = read_u32_le(r)?;
    let title = read_cstring(r)?;
    let offer_text = read_cstring(r)?;
    let auto_finish = read_u32_le(r)?;
    let _suggested_players = read_u32_le(r)?;
    let emote_count = read_u32_le(r)?;
    for _ in 0..emote_count {
        let _delay = read_u32_le(r)?;
        let _emote = read_u32_le(r)?;
    }
    let choices = read_item_rows(r)?;
    let rewards = read_item_rows(r)?;
    let money = read_i32_le(r)?;
    let _honor = read_u32_le(r)?;
    let quest_flags = read_u32_le(r)?;
    let reward_spell = read_u32_le(r)?;
    let _cast_spell = read_u32_le(r)?;
    let _title_bit = read_u32_le(r)?;
    Ok(QuestOfferReward {
        npc,
        quest_id,
        title,
        offer_text,
        auto_finish,
        choices,
        rewards,
        money,
        quest_flags,
        reward_spell,
    })
}

/// `SMSG_QUESTGIVER_REQUEST_ITEMS` (2.4.3): the 1.12.1 head with `u32` suggested players after the
/// close-on-cancel word; the four trailing words are `0x03` or `0` (completable) first, then
/// `0x04, 0x08, 0x10` (1.12.1: `0x02`, completable, `0x04`, `0x08`).
pub(super) fn read_request_items(r: &mut &[u8]) -> io::Result<QuestRequestItems> {
    let npc = read_u64_le(r)?;
    let quest_id = read_u32_le(r)?;
    let title = read_cstring(r)?;
    let request_text = read_cstring(r)?;
    let _emote_delay = read_u32_le(r)?;
    let emote = read_u32_le(r)?;
    let close_on_cancel = read_u32_le(r)?;
    let _suggested_players = read_u32_le(r)?;
    let required_money = read_u32_le(r)?;
    let required_items = read_item_rows(r)?;
    let completable = read_u32_le(r)?;
    for _ in 0..3 {
        let _flags = read_u32_le(r)?;
    }
    Ok(QuestRequestItems {
        npc,
        quest_id,
        title,
        request_text,
        emote,
        close_on_cancel,
        required_money,
        required_items,
        is_complete: completable != 0,
    })
}

/// `SMSG_QUESTGIVER_QUEST_COMPLETE` (2.4.3): the 1.12.1 body with a `u32` honor (dropped) between
/// the money and the item count.
pub(super) fn read_quest_complete(r: &mut &[u8]) -> io::Result<QuestComplete> {
    let quest_id = read_u32_le(r)?;
    let _unknown = read_u32_le(r)?;
    let xp = read_u32_le(r)?;
    let money = read_u32_le(r)?;
    let _honor = read_u32_le(r)?;
    let count = read_u32_le(r)?;
    let mut items = Vec::with_capacity(capacity_hint(count, 5));
    for _ in 0..count {
        items.push((read_u32_le(r)?, read_u32_le(r)?));
    }
    Ok(QuestComplete {
        quest_id,
        xp,
        money,
        items,
    })
}

/// The 1.12.1 `QuestFailedReason` of a 2.4.3 `SMSG_QUESTGIVER_QUEST_INVALID` reason, so the typed
/// reason means the same in both builds: 2.4.3 inserts `QUEST_ALREADY_DONE` (7) and
/// `QUEST_FAILED_EXPANSION` (16) and moves the missing-items and money reasons up one (21, 23; both
/// cmangos-tbc `QuestDef.h` and wow_messages' 2.4.3 enum), and its second already-on-quest value
/// (18) is the same line as 13. A reason with no 1.12.1 entry is `DONT_HAVE_REQ` (0), the generic
/// refusal, never another reason's meaning.
pub fn quest_invalid_reason_from_tbc(reason: u32) -> u32 {
    match reason {
        0 | 1 | 6 | 12 | 13 => reason,
        18 => 13,
        21 => 20,
        23 => 22,
        _ => 0,
    }
}

/// The 1.12.1 reason of a 2.4.3 `SMSG_QUESTGIVER_QUEST_FAILED` reason: inventory full (4) and
/// duplicate item (17) are the same numbers in both builds; any other is `DONT_HAVE_REQ` (0).
pub fn quest_failed_reason_from_tbc(reason: u32) -> u32 {
    match reason {
        4 | 17 => reason,
        _ => 0,
    }
}

/// `SMSG_QUEST_QUERY_RESPONSE` (2.4.3): the 1.12.1 template with a `u32` suggested players after the
/// quest type, and after the spell a cast spell and a `u32` honor (before the source item and
/// flags) and a `u32` title id after them (all dropped); then the four and six `{item, count}`
/// reward pairs, the point map, x, y and option, the four texts, four objective quads and four
/// objective texts.
pub(super) fn read_quest_query_response(r: &mut &[u8]) -> io::Result<QuestTemplate> {
    let quest_id = read_u32_le(r)?;
    let method = read_u32_le(r)?;
    let level = read_u32_le(r)?;
    let zone_or_sort = read_i32_le(r)?;
    let quest_type = read_u32_le(r)?;
    let _suggested_players = read_u32_le(r)?;
    let rep_objective_faction = read_u32_le(r)?;
    let rep_objective_value = read_u32_le(r)?;
    let _opposite_rep_faction = read_u32_le(r)?;
    let _opposite_rep_value = read_u32_le(r)?;
    let next_quest_in_chain = read_u32_le(r)?;
    let money = read_i32_le(r)?;
    let money_max_level = read_u32_le(r)?;
    let reward_spell = read_u32_le(r)?;
    let _cast_spell = read_u32_le(r)?;
    let _honor = read_u32_le(r)?;
    let src_item_id = read_u32_le(r)?;
    let flags = read_u32_le(r)?;
    let _title_id = read_u32_le(r)?;
    let mut rewards = [(0u32, 0u32); QUEST_REWARDS_COUNT as usize];
    for slot in rewards.iter_mut() {
        *slot = (read_u32_le(r)?, read_u32_le(r)?);
    }
    let mut choices = [(0u32, 0u32); QUEST_REWARD_CHOICES_COUNT as usize];
    for slot in choices.iter_mut() {
        *slot = (read_u32_le(r)?, read_u32_le(r)?);
    }
    let point_map_id = read_u32_le(r)?;
    let point_x = read_f32_le(r)?;
    let point_y = read_f32_le(r)?;
    let point_opt = read_u32_le(r)?;
    let title = read_cstring(r)?;
    let objectives_text = read_cstring(r)?;
    let details = read_cstring(r)?;
    let end_text = read_cstring(r)?;
    let mut quads = [(0u32, 0u32, 0u32, 0u32); QUEST_OBJECTIVES_COUNT as usize];
    for slot in quads.iter_mut() {
        *slot = (
            read_u32_le(r)?,
            read_u32_le(r)?,
            read_u32_le(r)?,
            read_u32_le(r)?,
        );
    }
    let mut texts = Vec::with_capacity(QUEST_OBJECTIVES_COUNT as usize);
    for _ in 0..QUEST_OBJECTIVES_COUNT {
        texts.push(read_cstring(r)?);
    }
    let mut texts = texts.into_iter();
    let objectives =
        quads.map(
            |(creature_or_go, required_count, item_id, item_count)| QuestObjective {
                creature_or_go,
                required_count,
                item_id,
                item_count,
                text: texts.next().unwrap_or_default(),
            },
        );
    Ok(QuestTemplate {
        quest_id,
        method,
        level,
        zone_or_sort,
        quest_type,
        rep_objective_faction,
        rep_objective_value,
        next_quest_in_chain,
        money,
        money_max_level,
        reward_spell,
        src_item_id,
        flags,
        rewards,
        choices,
        point_map_id,
        point_x,
        point_y,
        point_opt,
        title,
        objectives_text,
        details,
        end_text,
        objectives,
    })
}

/// `SMSG_QUESTUPDATE_FAILED` (2.4.3, single-source): one `u32` quest id. wow_messages gives it for
/// every build; neither cmangos tree builds it, so no emulator confirms the form.
pub(super) fn read_quest_update_failed(r: &mut &[u8]) -> io::Result<u32> {
    read_u32_le(r)
}
