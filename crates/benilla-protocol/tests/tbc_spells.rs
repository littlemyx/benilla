//! 2.4.3 spells: the cast failure and its renumbered result table, the cooldown list and the channel
//! packets. Fixtures are built from the facts tables (cmangos-tbc builders, agreeing with
//! wow_messages' 2.4.3 definitions; wow_messages ships no 2.4.3 vector for these).

use benilla_build::TBC_2_4_3;
use benilla_protocol::messages::{
    self, parse_server_with_tail_for, tbc_opcode as t, CastOutcome, ServerPacket, TbcPacket,
    CAST_RESULT_TBC_TO_112,
};
use benilla_protocol::wire::write_packed_guid;

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn packed(guid: u64) -> Vec<u8> {
    let mut b = Vec::new();
    write_packed_guid(guid, &mut b).unwrap();
    b
}

fn tbc(op: u16, body: &[u8]) -> ServerPacket {
    let (packet, tail) = parse_server_with_tail_for(&TBC_2_4_3, None, op, body).unwrap();
    assert_eq!(tail, 0, "{} left {tail} bytes unread", packet.name());
    packet
}

fn failed(spell: u32, result: u8, count: u8, args: &[u8]) -> Vec<u8> {
    let mut b = u32b(spell);
    b.extend([result, count]);
    b.extend(args);
    b
}

#[test]
fn the_result_table_is_sorted_and_agrees_with_what_1_12_1_already_documents() {
    assert_eq!(CAST_RESULT_TBC_TO_112.len(), 133);
    for pair in CAST_RESULT_TBC_TO_112.windows(2) {
        assert!(pair[0].0 < pair[1].0, "{:#x} is out of order", pair[1].0);
    }
    // The 1.12.1 numbers the app's own notes name: area 0x5d, spell focus 0x5e, not ready 0x3c,
    // equipped item class 0x19-0x1b, interrupted 0x23.
    for (tbc_value, vanilla) in [
        (0x60u8, 0x5du8),
        (0x61, 0x5e),
        (0x3f, 0x3c),
        (0x1b, 0x19),
        (0x1c, 0x1a),
        (0x1d, 0x1b),
        (0x25, 0x23),
    ] {
        assert_eq!(messages::cast_result_from_tbc(tbc_value), Some(vanilla));
    }
    // Results 2.4.3 added have no 1.12.1 number: not a neighbouring one.
    for added in [0x02u8, 0x5F, 0x7D, 0x96, 0xA6] {
        assert_eq!(messages::cast_result_from_tbc(added), None, "{added:#x}");
    }
}

#[test]
fn a_cast_failure_maps_to_1_12_1s_reason_with_its_first_argument() {
    // REQUIRES_SPELL_FOCUS (0x61) names a focus object, REQUIRES_AREA (0x60) an area.
    for (result, reason) in [(0x61u8, 0x5eu8), (0x60, 0x5d)] {
        match tbc(t::SMSG_CAST_RESULT, &failed(133, result, 0, &u32b(1234))) {
            ServerPacket::CastResult { spell_id, outcome } => {
                assert_eq!(spell_id, 133);
                assert_eq!(
                    outcome,
                    CastOutcome::Failed {
                        reason,
                        arg: Some(1234)
                    }
                );
            }
            other => panic!("{}", other.name()),
        }
    }
    // The equipped-item arm is two words in cmangos-tbc and three in wow_messages: the class is
    // read, the rest dropped, and both lengths leave nothing unread.
    for args in [
        [u32b(2), u32b(8)].concat(),
        [u32b(2), u32b(8), u32b(5)].concat(),
    ] {
        match tbc(t::SMSG_CAST_RESULT, &failed(133, 0x1b, 1, &args)) {
            ServerPacket::CastResult { outcome, .. } => assert_eq!(
                outcome,
                CastOutcome::Failed {
                    reason: 0x19,
                    arg: Some(2)
                }
            ),
            other => panic!("{}", other.name()),
        }
    }
    // A result with no argument; the cast count is not part of the 1.12.1 outcome.
    match tbc(t::SMSG_CAST_RESULT, &failed(133, 0x25, 1, &[])) {
        ServerPacket::CastResult { outcome, .. } => assert_eq!(
            outcome,
            CastOutcome::Failed {
                reason: 0x23,
                arg: None
            }
        ),
        other => panic!("{}", other.name()),
    }
}

#[test]
fn a_result_without_a_1_12_1_entry_reads_as_its_own_form() {
    // REAGENTS (0x5F) sends the missing item; TOTEMS (0x7D) up to two totem ids: dropped.
    match tbc(t::SMSG_CAST_RESULT, &failed(2637, 0x5F, 2, &u32b(2447))) {
        ServerPacket::Tbc(TbcPacket::CastFailed {
            spell_id,
            result,
            cast_count,
            arg,
        }) => assert_eq!((spell_id, result, cast_count, arg), (2637, 0x5F, 2, None)),
        other => panic!("{}", other.name()),
    }
    assert!(matches!(
        tbc(
            t::SMSG_CAST_RESULT,
            &failed(1, 0x7D, 0, &[u32b(6), u32b(7)].concat())
        ),
        ServerPacket::Tbc(TbcPacket::CastFailed { result: 0x7D, .. })
    ));
    // A body that ends inside the fixed part is an error.
    assert!(
        parse_server_with_tail_for(&TBC_2_4_3, None, t::SMSG_CAST_RESULT, &[1, 0, 0, 0, 0x25])
            .is_err()
    );
}

#[test]
fn a_cooldown_list_has_a_flags_byte() {
    let mut body = 0x17u64.to_le_bytes().to_vec();
    body.push(1);
    body.extend(u32b(133));
    body.extend(u32b(6000));
    body.extend(u32b(2136));
    body.extend(u32b(0));
    match tbc(t::SMSG_SPELL_COOLDOWN, &body) {
        ServerPacket::SpellCooldownList { caster, cooldowns } => {
            assert_eq!(caster, 0x17);
            assert_eq!(cooldowns, [(133, 6000), (2136, 0)]);
        }
        other => panic!("{}", other.name()),
    }
    // A list with no pairs (flags only) is empty.
    let mut empty = 0x17u64.to_le_bytes().to_vec();
    empty.push(0);
    assert!(matches!(
        tbc(t::SMSG_SPELL_COOLDOWN, &empty),
        ServerPacket::SpellCooldownList { cooldowns, .. } if cooldowns.is_empty()
    ));
}

#[test]
fn channel_start_and_update_name_their_caster() {
    let mut start = packed(0x1234);
    start.extend(u32b(5143));
    start.extend(u32b(3000));
    assert_eq!(
        match tbc(t::MSG_CHANNEL_START, &start) {
            ServerPacket::Tbc(p) => p,
            other => panic!("{}", other.name()),
        },
        TbcPacket::ChannelStart {
            caster: 0x1234,
            spell_id: 5143,
            duration_ms: 3000
        }
    );
    let mut update = packed(0x1234);
    update.extend(u32b(1500));
    let packet = tbc(t::MSG_CHANNEL_UPDATE, &update);
    assert_eq!(packet.name(), "MSG_CHANNEL_UPDATE");
    assert!(matches!(
        packet,
        ServerPacket::Tbc(TbcPacket::ChannelUpdate {
            caster: 0x1234,
            remaining_ms: 1500
        })
    ));
}
