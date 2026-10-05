//! The cast sends on 2.4.3: `CMSG_CAST_SPELL`, `CMSG_USE_ITEM` and the gossip selection. Both casts
//! put the same targets block after their fixed head, but 2.4.3 widens its flag word to a `u32` and
//! adds a `u8` cast count (cmangos-tbc `HandleCastSpellOpcode`, `HandleUseItemOpcode`,
//! `SpellCastTargets::read`). Single-source (cmangos-tbc): wow_messages' 2.4.3 `CMSG_CAST_SPELL` has
//! no cast count, while the server packets that echo it (`SMSG_SPELL_START`, `SMSG_CAST_RESULT`) were
//! seen live carrying one.

/// The next cast count: a per-session counter the client increments per cast and the server echoes
/// in its cast packets; 0 is never used, since cmangos-tbc reads a non-zero count as a cast sent
/// while another is in progress (`Spell::prepare`) and answers `SPELL_FAILED_SPELL_IN_PROGRESS`.
pub fn next_cast_count(count: &mut u8) -> u8 {
    *count = if *count == u8::MAX { 1 } else { *count + 1 };
    *count
}

/// Rebuild a 1.12.1 body whose targets block starts at `head` as the 2.4.3 one: `extra` is inserted
/// before the flag word, which widens from a `u16` to a `u32`; the rest of the block is the same.
fn widen_targets(body: &[u8], head: usize, extra: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + extra.len() + 2);
    out.extend_from_slice(&body[..head]);
    out.extend_from_slice(extra);
    out.extend_from_slice(&body[head..head + 2]);
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&body[head + 2..]);
    out
}

/// The 2.4.3 `CMSG_CAST_SPELL` body from a 1.12.1 one (any of the `cast_spell*` builders): spell id,
/// `u8` cast count, `u32` flag word, the targets block.
pub fn cast_spell_tbc(body_112: &[u8], cast_count: u8) -> Vec<u8> {
    widen_targets(body_112, 4, &[cast_count])
}

/// The 2.4.3 `CMSG_USE_ITEM` body from a 1.12.1 one: bag, slot, spell index, `u8` cast count, the
/// item's full `u64` guid (the server checks it against the slot), `u32` flag word, the targets block.
pub fn use_item_tbc(body_112: &[u8], cast_count: u8, item_guid: u64) -> Vec<u8> {
    let mut extra = vec![cast_count];
    extra.extend_from_slice(&item_guid.to_le_bytes());
    widen_targets(body_112, 3, &extra)
}

/// The 2.4.3 `CMSG_GOSSIP_SELECT_OPTION` body from a 1.12.1 one: the NPC's guid, the `u32` id of the
/// menu on screen (cmangos-tbc `OnGossipSelect` looks the option up by it), then the option's index
/// and, for a coded option, its code.
pub fn gossip_select_option_tbc(body_112: &[u8], menu_id: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(body_112.len() + 4);
    out.extend_from_slice(&body_112[..8]);
    out.extend_from_slice(&menu_id.to_le_bytes());
    out.extend_from_slice(&body_112[8..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{
        cast_spell, cast_spell_at_dest, cast_spell_corpse, cast_spell_gameobject, cast_spell_item,
        gossip_select_option, use_item, CorpseTarget, UseItemTarget,
    };

    #[test]
    fn the_cast_count_counts_from_one_and_wraps_past_zero() {
        let mut c = 0;
        assert_eq!(next_cast_count(&mut c), 1);
        assert_eq!(next_cast_count(&mut c), 2);
        c = 254;
        assert_eq!(next_cast_count(&mut c), 255);
        assert_eq!(next_cast_count(&mut c), 1);
    }

    #[test]
    fn a_self_cast_is_spell_count_and_a_u32_zero_mask() {
        assert_eq!(
            cast_spell_tbc(&cast_spell(133, None), 1),
            [0x85, 0, 0, 0, 0x01, 0, 0, 0, 0]
        );
    }

    #[test]
    fn a_unit_cast_keeps_its_packed_guid_after_the_widened_mask() {
        // Spell 78, count 7, TARGET_FLAG_UNIT, packed guid 0x1234 (mask 0x03, 34 12).
        assert_eq!(
            cast_spell_tbc(&cast_spell(78, Some(0x1234)), 7),
            [0x4e, 0, 0, 0, 0x07, 0x02, 0, 0, 0, 0x03, 0x34, 0x12]
        );
    }

    #[test]
    fn every_target_shape_widens_the_same_way() {
        let gameobject = cast_spell_gameobject(1, 0x1234);
        assert_eq!(
            cast_spell_tbc(&gameobject, 2),
            [1, 0, 0, 0, 2, 0x00, 0x08, 0, 0, 0x03, 0x34, 0x12]
        );
        let corpse = cast_spell_corpse(2006, CorpseTarget::Ally, 0x1234);
        assert_eq!(
            cast_spell_tbc(&corpse, 3),
            [0xd6, 0x07, 0, 0, 3, 0x00, 0x80, 0, 0, 0x03, 0x34, 0x12]
        );
        let item = cast_spell_item(7, 0x1234);
        assert_eq!(
            cast_spell_tbc(&item, 4),
            [7, 0, 0, 0, 4, 0x10, 0, 0, 0, 0x03, 0x34, 0x12]
        );
        let dest = cast_spell_at_dest(9, [1.0, 2.0, 3.0]);
        let mut want = vec![9, 0, 0, 0, 5, 0x40, 0, 0, 0];
        for c in [1.0f32, 2.0, 3.0] {
            want.extend_from_slice(&c.to_le_bytes());
        }
        assert_eq!(cast_spell_tbc(&dest, 5), want);
    }

    #[test]
    fn use_item_carries_the_count_and_the_items_full_guid() {
        // Backpack slot 24 (bag 255), spell index 0, count 1, guid 0xA01, self.
        assert_eq!(
            use_item_tbc(&use_item(255, 24, 0, UseItemTarget::SelfImplicit), 1, 0xA01),
            [0xff, 0x18, 0x00, 0x01, 0x01, 0x0a, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        // A unit target keeps its packed guid after the u32 mask.
        assert_eq!(
            use_item_tbc(&use_item(255, 24, 0, UseItemTarget::Unit(1)), 2, 5),
            [0xff, 0x18, 0, 0x02, 5, 0, 0, 0, 0, 0, 0, 0, 0x02, 0, 0, 0, 0x01, 0x01]
        );
    }

    #[test]
    fn a_gossip_selection_gains_the_menu_id_after_the_guid() {
        let body = gossip_select_option(0x1122_3344_5566_7788, 3, None);
        assert_eq!(
            gossip_select_option_tbc(&body, 0xAABB),
            [0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0xbb, 0xaa, 0, 0, 3, 0, 0, 0]
        );
        let coded = gossip_select_option(1, 0, Some("ab"));
        assert_eq!(
            gossip_select_option_tbc(&coded, 1)[12..],
            [0, 0, 0, 0, b'a', b'b', 0]
        );
    }
}
