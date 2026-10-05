//! 2.4.3 death and release: the packets from the death to the resurrection. Fixtures are built from
//! the cmangos-tbc builders (`Player.cpp`, `QueryHandler.cpp`, `Creature.cpp`, `SpellEffects.cpp`),
//! which agree with wow_messages' 2.4.3 definitions where it has them.

use benilla_build::TBC_2_4_3;
use benilla_protocol::messages::{
    parse_server_with_tail_for, tbc_opcode as t, ServerPacket, TbcPacket,
};

fn tbc(op: u16, body: &[u8]) -> ServerPacket {
    let (packet, tail) = parse_server_with_tail_for(&TBC_2_4_3, None, op, body).unwrap();
    assert_eq!(tail, 0, "{} left {tail} bytes unread", packet.name());
    packet
}

fn f(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

#[test]
fn the_release_location_names_a_graveyard_and_minus_one_removes_it() {
    // `RepopAtGraveyard`: map, x, y, z (Goldshire's graveyard).
    let mut b = 0u32.to_le_bytes().to_vec();
    for v in [-9617.5f32, 380.6, 56.7] {
        b.extend(f(v));
    }
    assert!(matches!(
        tbc(t::SMSG_DEATH_RELEASE_LOC, &b),
        ServerPacket::Tbc(TbcPacket::DeathReleaseLoc { map: Some(0), position })
            if position == [-9617.5, 380.6, 56.7]
    ));
    // `ResurrectPlayer` removes the marker with map -1 and a zero position.
    let mut gone = u32::MAX.to_le_bytes().to_vec();
    for _ in 0..3 {
        gone.extend(f(0.0));
    }
    assert!(matches!(
        tbc(t::SMSG_DEATH_RELEASE_LOC, &gone),
        ServerPacket::Tbc(TbcPacket::DeathReleaseLoc { map: None, position })
            if position == [0.0; 3]
    ));
    // Single-source (wow_messages): empty.
    assert!(matches!(
        tbc(t::SMSG_FORCED_DEATH_UPDATE, &[]),
        ServerPacket::Tbc(TbcPacket::ForcedDeathUpdate)
    ));
}

#[test]
fn the_corpse_query_reply_the_reclaim_delay_and_the_spirit_healer_packets_read_as_in_1_12_1() {
    // `HandleCorpseQueryOpcode`: found, map, x, y, z, corpse map; or one zero byte.
    let mut found = vec![1u8];
    found.extend((-1i32).to_le_bytes());
    for v in [1.0f32, 2.0, 3.0] {
        found.extend(f(v));
    }
    found.extend(0u32.to_le_bytes());
    let ev = |op: u16, body: &[u8]| tbc(op, body).name();
    assert_eq!(ev(0x0216, &found), "MSG_CORPSE_QUERY");
    assert_eq!(ev(0x0216, &[0]), "MSG_CORPSE_QUERY");
    // `SendCorpseReclaimDelay`: milliseconds.
    assert_eq!(
        ev(0x0269, &30_000u32.to_le_bytes()),
        "SMSG_CORPSE_RECLAIM_DELAY"
    );
    // `SMSG_SPIRIT_HEALER_CONFIRM`: the healer's guid.
    assert_eq!(
        ev(0x0222, &0xF130_0000_0000_0042u64.to_le_bytes()),
        "SMSG_SPIRIT_HEALER_CONFIRM"
    );
    // `SMSG_AREA_SPIRIT_HEALER_TIME`: guid and the time to the next wave.
    let mut wave = 0x55u64.to_le_bytes().to_vec();
    wave.extend(9000u32.to_le_bytes());
    assert_eq!(ev(0x02E4, &wave), "SMSG_AREA_SPIRIT_HEALER_TIME");
    // `SMSG_DURABILITY_DAMAGE_DEATH`: no body.
    assert_eq!(ev(0x02BD, &[]), "SMSG_DURABILITY_DAMAGE_DEATH");
    // `SendResurrectRequest`: guid, name length, name, spirit-healer flag, timer flag.
    let mut offer = 0x66u64.to_le_bytes().to_vec();
    offer.extend(6u32.to_le_bytes());
    offer.extend(b"Angel\0");
    offer.extend([1, 1]);
    assert_eq!(ev(0x015B, &offer), "SMSG_RESURRECT_REQUEST");
}
