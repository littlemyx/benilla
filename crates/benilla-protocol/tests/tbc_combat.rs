//! 2.4.3 combat readout: the combat log packets whose bytes or values differ from 1.12.1 (the
//! school is a mask, a few packets gain fields). Fixtures are built from the facts tables
//! (cmangos-tbc builders, agreeing with wow_messages' 2.4.3 definitions); the melee swing also reads
//! wow_messages' test vector of the message, whose layout its 2.4.3 definition shares.

use benilla_build::{TBC_2_4_3, VANILLA_1_12_1};
use benilla_protocol::messages::{
    opcode, parse_server_with_tail_for, tbc_opcode as t, PeriodicTick, ServerPacket, FIELDS_5875,
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

fn cat(parts: &[Vec<u8>]) -> Vec<u8> {
    parts.concat()
}

fn tbc(op: u16, body: &[u8]) -> ServerPacket {
    let (packet, tail) = parse_server_with_tail_for(&TBC_2_4_3, None, op, body).unwrap();
    assert_eq!(tail, 0, "{} left {tail} bytes unread", packet.name());
    packet
}

fn vanilla(op: u16, body: &[u8]) -> ServerPacket {
    let (packet, tail) =
        parse_server_with_tail_for(&VANILLA_1_12_1, Some(&FIELDS_5875), op, body).unwrap();
    assert_eq!(tail, 0, "{} left {tail} bytes unread", packet.name());
    packet
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// A swing body: one sub-damage block with `school`, then the tail.
fn swing(school: u32) -> Vec<u8> {
    cat(&[
        u32b(0x80),
        packed(0x17),
        packed(0x64),
        u32b(1337),
        vec![1],
        u32b(school),
        1332f32.to_le_bytes().to_vec(),
        u32b(1332),
        u32b(0),
        u32b(0),
        u32b(0), // target state
        u32b(0), // attacker state
        u32b(0), // melee spell
        u32b(0), // blocked
    ])
}

#[test]
fn the_melee_swing_reads_wow_messages_vector_and_takes_its_school_from_a_mask() {
    // wow_messages' `SMSG_ATTACKERSTATEUPDATE` vector (a critical hit for 1337), after its 4-byte header.
    let vector = hex(
        "80000000011701643905000001000000000080a64434050000000000000000000000000000000000000000000000000000",
    );
    for packet in [
        tbc(t::SMSG_ATTACKERSTATEUPDATE, &vector),
        vanilla(opcode::SMSG_ATTACKERSTATEUPDATE, &vector),
    ] {
        match packet {
            ServerPacket::AttackerState(s) => {
                assert_eq!((s.hit_info, s.attacker, s.victim), (0x80, 0x17, 0x64));
                assert_eq!((s.damage, s.absorb, s.school), (1337, 0, 0));
            }
            other => panic!("{}", other.name()),
        }
    }
    // 2.4.3 sends the school mask (cmangos-tbc), 1.12.1 the index (cmangos-classic):
    // fire is 4 and 2, arcane 0x40 and 6, and a mask of two schools reads as its first.
    for (mask, index) in [(1u32, 0u8), (2, 1), (4, 2), (0x40, 6), (0x06, 1), (0, 0)] {
        match tbc(t::SMSG_ATTACKERSTATEUPDATE, &swing(mask)) {
            ServerPacket::AttackerState(s) => assert_eq!(s.school, index, "mask {mask:#x}"),
            other => panic!("{}", other.name()),
        }
    }
    match vanilla(opcode::SMSG_ATTACKERSTATEUPDATE, &swing(4)) {
        ServerPacket::AttackerState(s) => assert_eq!(s.school, 4),
        other => panic!("{}", other.name()),
    }
}

#[test]
fn a_spell_damage_log_takes_its_school_from_a_mask() {
    let body = |school: u8| {
        cat(&[
            packed(0x64),
            packed(0x17),
            u32b(133),
            u32b(50),
            vec![school],
            u32b(2),
            u32b(0),
            vec![0, 0],
            u32b(0),
            u32b(0x2),
            vec![0],
        ])
    };
    match tbc(t::SMSG_SPELLNONMELEEDAMAGELOG, &body(0x10)) {
        ServerPacket::SpellDamageLog(l) => {
            assert_eq!((l.spell_id, l.damage, l.school, l.absorb), (133, 50, 4, 2));
            assert_eq!(l.hit_info, 2);
        }
        other => panic!("{}", other.name()),
    }
    match vanilla(opcode::SMSG_SPELLNONMELEEDAMAGELOG, &body(4)) {
        ServerPacket::SpellDamageLog(l) => assert_eq!(l.school, 4),
        other => panic!("{}", other.name()),
    }
}

#[test]
fn a_periodic_tick_takes_its_school_from_a_mask() {
    let damage = |school: u32| {
        cat(&[
            packed(0x64),
            packed(0x17),
            u32b(172),
            u32b(1),
            u32b(3),
            u32b(9),
            u32b(school),
            u32b(1),
            u32b(0),
        ])
    };
    match tbc(t::SMSG_PERIODICAURALOG, &damage(0x20)) {
        ServerPacket::PeriodicAuraLog(l) => assert_eq!(
            l.ticks,
            [PeriodicTick::Damage {
                amount: 9,
                school: 5,
                absorb: 1,
                resist: 0
            }]
        ),
        other => panic!("{}", other.name()),
    }
    match vanilla(opcode::SMSG_PERIODICAURALOG, &damage(5)) {
        ServerPacket::PeriodicAuraLog(l) => {
            assert!(matches!(l.ticks[0], PeriodicTick::Damage { school: 5, .. }))
        }
        other => panic!("{}", other.name()),
    }
    // A heal tick has no school and reads the same in both builds.
    let heal = cat(&[packed(1), packed(2), u32b(774), u32b(1), u32b(8), u32b(40)]);
    assert!(matches!(
        tbc(t::SMSG_PERIODICAURALOG, &heal),
        ServerPacket::PeriodicAuraLog(_)
    ));
}

#[test]
fn a_damage_shield_names_its_spell_and_a_school_mask() {
    let body = cat(&[
        0x64u64.to_le_bytes().to_vec(),
        0x17u64.to_le_bytes().to_vec(),
        u32b(467),
        u32b(3),
        u32b(0x8),
    ]);
    match tbc(t::SMSG_SPELLDAMAGESHIELD, &body) {
        ServerPacket::DamageShield(d) => {
            assert_eq!(
                (d.victim, d.attacker, d.damage, d.school),
                (0x64, 0x17, 3, 3)
            );
        }
        other => panic!("{}", other.name()),
    }
    let one = cat(&[
        0x64u64.to_le_bytes().to_vec(),
        0x17u64.to_le_bytes().to_vec(),
        u32b(3),
        u32b(3),
    ]);
    match vanilla(opcode::SMSG_SPELLDAMAGESHIELD, &one) {
        ServerPacket::DamageShield(d) => assert_eq!((d.damage, d.school), (3, 3)),
        other => panic!("{}", other.name()),
    }
}

#[test]
fn heal_instakill_and_dispel_logs() {
    // Heal: the 1.12.1 body and one unused byte.
    let heal = cat(&[
        packed(0x64),
        packed(0x17),
        u32b(2050),
        u32b(120),
        vec![1, 0],
    ]);
    match tbc(t::SMSG_SPELLHEALLOG, &heal) {
        ServerPacket::SpellHealLog(h) => {
            assert_eq!(
                (h.target, h.healer, h.spell_id, h.amount),
                (0x64, 0x17, 2050, 120)
            );
            assert!(h.critical);
        }
        other => panic!("{}", other.name()),
    }
    // Instakill: the caster's raw guid, the victim's, the spell.
    let kill = cat(&[
        0x17u64.to_le_bytes().to_vec(),
        0x64u64.to_le_bytes().to_vec(),
        u32b(5308),
    ]);
    match tbc(t::SMSG_SPELLINSTAKILLLOG, &kill) {
        ServerPacket::SpellInstaKillLog(k) => assert_eq!((k.victim, k.spell_id), (0x64, 5308)),
        other => panic!("{}", other.name()),
    }
    // Dispel: packed victim and caster, the dispelling spell, a u8, then {spell, method} pairs.
    let dispel = cat(&[
        packed(0x64),
        packed(0x17),
        u32b(527),
        vec![0],
        u32b(2),
        u32b(139),
        vec![0],
        u32b(774),
        vec![1],
    ]);
    match tbc(t::SMSG_SPELLDISPELLOG, &dispel) {
        ServerPacket::SpellDispelLog(d) => {
            assert_eq!((d.victim, d.caster), (0x64, 0x17));
            assert_eq!(d.spell_ids, [139, 774]);
        }
        other => panic!("{}", other.name()),
    }
}

#[test]
fn the_unchanged_combat_packets_read_wow_messages_vectors_in_both_builds() {
    // wow_messages' vectors of ATTACKSTART and ATTACKSTOP, after their 4-byte headers.
    let start = hex("17000000000000006400000000000000");
    let stop = hex("0117016400000000");
    for (op, body) in [(0x143u16, &start), (0x144, &stop)] {
        let (a, b) = (tbc(op, body), vanilla(op, body));
        assert_eq!(a.name(), b.name());
    }
    assert!(matches!(
        tbc(0x143, &start),
        ServerPacket::AttackStart { .. }
    ));
    assert!(matches!(tbc(0x144, &stop), ServerPacket::AttackStop { .. }));
}

#[test]
fn the_attack_swing_request_is_wow_messages_2_4_3_vector() {
    // `CMSG_ATTACKSWING` of guid 100, from wow_messages' vector tagged 2.x, after its 6-byte header.
    assert_eq!(
        benilla_protocol::messages::attack_swing(100),
        hex("6400000000000000")
    );
}
