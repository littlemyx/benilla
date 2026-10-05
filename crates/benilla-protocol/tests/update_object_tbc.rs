//! 2.4.3 `SMSG_UPDATE_OBJECT` wire tests: the movement block (extra flags byte, transport time, eight
//! speeds, the build's flag bits), the framing both builds share, and a create read through the
//! typed accessors over the 8606 table.

use std::io::Write;

use benilla_build::TBC_2_4_3;
use benilla_protocol::messages::{
    self, opcode, parse_server_with_tail_for, MovementBlock, Object, ObjectType, ServerPacket,
    FIELDS_8606,
};

fn f(v: f32) -> [u8; 4] {
    v.to_le_bytes()
}

fn u(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

/// A living block's bytes after the update flags: flags, extra flags, timestamp, position,
/// the given middle bytes (transport, pitch, fall, ...), then `speeds` and `tail`.
struct Living {
    flags: u32,
    middle: Vec<u8>,
    speeds: [f32; 8],
    tail: Vec<u8>,
}

impl Living {
    fn standing() -> Self {
        Living {
            flags: 0,
            middle: f(0.0).to_vec(), // fall time
            speeds: [2.5, 7.0, 4.5, 4.722222, 2.5, 7.0, 4.5, 3.141594],
            tail: vec![],
        }
    }
    fn bytes(&self, update_flag: u8) -> Vec<u8> {
        let mut b = vec![update_flag];
        b.extend(u(self.flags));
        b.push(0x00); // extra flags
        b.extend(u(1234)); // timestamp
        for v in [-8949.95f32, -132.493, 83.5312, 3.5] {
            b.extend(f(v));
        }
        b.extend(&self.middle);
        for s in self.speeds {
            b.extend(f(s));
        }
        b.extend(&self.tail);
        b
    }
}

fn read_block(bytes: &[u8]) -> MovementBlock {
    // One create of a creature (a unit, `HIGHGUID | LIVING | POSITION`) around the block; the
    // body is `0` fields. The framing is checked separately.
    let mut body = Vec::new();
    body.extend(u(1)); // one block
    body.push(0); // no transport
    body.push(3); // create object 2
    body.extend([0x01, 0x07]); // packed guid 7
    body.push(3); // unit
    body.extend(bytes);
    body.push(0); // zero mask words
    let (packet, tail) =
        parse_server_with_tail_for(&TBC_2_4_3, None, opcode::SMSG_UPDATE_OBJECT, &body).unwrap();
    assert_eq!(tail, 0, "the whole body is consumed");
    let ServerPacket::UpdateObject { mut objects } = packet else {
        panic!("not an update object")
    };
    match objects.remove(0) {
        Object::Create { movement, .. } => movement,
        _ => panic!("not a create"),
    }
}

#[test]
fn a_standing_living_block_reads_eight_speeds_into_the_six_and_the_flight_pair() {
    let mut living = Living::standing();
    living.tail = u(0).to_vec(); // 0x10: guid high
    let block = read_block(&living.bytes(0x10 | 0x20 | 0x40));
    let (pos, o) = block.position.unwrap();
    assert_eq!((pos.x, pos.y, pos.z, o), (-8949.95, -132.493, 83.5312, 3.5));
    let mover = block.mover.unwrap();
    assert_eq!((mover.flags, mover.pitch), (0, 0.0));
    // walk, run, run back, swim, swim back, turn rate
    assert_eq!(block.speeds, Some([2.5, 7.0, 4.5, 4.722222, 2.5, 3.141594]));
    assert_eq!(block.flight_speeds, Some([7.0, 4.5]));
    assert!(block.transport.is_none() && block.spline.is_none());
}

#[test]
fn a_riding_block_carries_the_transport_pose_and_its_time() {
    let mut living = Living::standing();
    living.flags = 0x0000_0200; // ON_TRANSPORT in 2.4.3
    let mut middle = 0xDEAD_BEEF_0000_0001u64.to_le_bytes().to_vec();
    for v in [1.5f32, -2.5, 3.5, 0.5] {
        middle.extend(f(v));
    }
    middle.extend(u(9999)); // transport time
    middle.extend(f(0.0)); // fall time
    living.middle = middle;
    let block = read_block(&living.bytes(0x20));
    let t = block.transport.unwrap();
    assert_eq!(t.guid, 0xDEAD_BEEF_0000_0001);
    assert_eq!(
        (t.pos.x, t.pos.y, t.pos.z, t.orientation),
        (1.5, -2.5, 3.5, 0.5)
    );
    assert_eq!(
        block.speeds.unwrap()[1],
        7.0,
        "speeds follow the transport time"
    );
}

#[test]
fn swimming_and_the_flying_bit_each_carry_a_pitch_and_the_1_12_1_transport_bit_means_nothing() {
    for (flags, pitch) in [(0x0020_0000u32, -0.4f32), (0x0200_0000, 0.7)] {
        let mut living = Living::standing();
        living.flags = flags;
        living.middle = [f(pitch), f(0.0)].concat();
        let block = read_block(&living.bytes(0x20));
        assert_eq!(block.mover.unwrap().pitch, pitch, "flags {flags:#x}");
        assert!(block.transport.is_none());
        assert_eq!(block.speeds.unwrap()[1], 7.0);
    }
    // 0x0200_0000 was ON_TRANSPORT in 1.12.1; here it is the flying bit, so no transport is read.
}

#[test]
fn a_falling_block_skips_the_jump_quad_and_spline_elevation_one_float() {
    let mut living = Living::standing();
    living.flags = 0x0000_1000 | 0x0400_0000; // FALLING | SPLINE_ELEVATION
    living.middle = [
        f(250.0), // fall time
        f(3.0),
        f(0.5),
        f(0.8),
        f(6.0),  // z speed, cos, sin, xy speed
        f(1.25), // spline elevation
    ]
    .concat();
    let block = read_block(&living.bytes(0x20));
    assert_eq!(block.speeds.unwrap()[0], 2.5);
    assert_eq!(block.flight_speeds, Some([7.0, 4.5]));
}

#[test]
fn a_spline_follows_the_eight_speeds_under_the_2_4_3_bit() {
    let mut living = Living::standing();
    living.flags = 0x0800_0000; // SPLINE_ENABLED
    let mut tail = u(0x0000_0100 | 0x0004_0000).to_vec(); // RUNMODE | FINAL_ANGLE
    tail.extend(f(1.5)); // final angle
    tail.extend(u(40)); // time passed
    tail.extend(u(3000)); // duration
    tail.extend(u(77)); // id
    tail.extend(u(4)); // nodes
    for n in 0..4 {
        for v in [n as f32, 2.0 * n as f32, 3.0] {
            tail.extend(f(v));
        }
    }
    for v in [3.0f32, 6.0, 3.0] {
        tail.extend(f(v)); // final node
    }
    living.tail = tail;
    let block = read_block(&living.bytes(0x20));
    let spline = block.spline.expect("a four-node spline is a path");
    assert_eq!(
        (spline.id, spline.time_passed_ms, spline.duration_ms),
        (77, 40, 3000)
    );
    assert_eq!(spline.path, vec![[1.0, 2.0, 3.0], [2.0, 4.0, 3.0]]);
    assert!(spline.run_mode && !spline.cyclic);
}

#[test]
fn the_position_only_block_and_the_trailing_parts_keep_their_order() {
    // A gameobject: `LOWGUID | HIGHGUID | HAS_POSITION`, then 0x04 a packed guid, 0x02 progress.
    let mut b = vec![0x08 | 0x10 | 0x40 | 0x04 | 0x02];
    for v in [10.0f32, 20.0, 30.0, 1.0] {
        b.extend(f(v));
    }
    b.extend(u(0xAAAA)); // 0x08
    b.extend(u(0xBBBB)); // 0x10
    b.extend([0x01, 0x05]); // 0x04: packed guid 5
    b.extend(u(31337)); // 0x02
    let block = read_block(&b);
    let (pos, o) = block.position.unwrap();
    assert_eq!((pos.x, pos.y, pos.z, o), (10.0, 20.0, 30.0, 1.0));
    assert_eq!(block.transport_progress, Some(31337));
    assert!(block.mover.is_none() && block.speeds.is_none() && block.flight_speeds.is_none());
}

/// The own player's create body over the 8606 table: `SELF | HIGHGUID | LIVING | HAS_POSITION`,
/// a warrior's few fields, 50 mask words.
fn own_player_create() -> Vec<u8> {
    let mut b = vec![3u8, 0x01, 0x01, 4]; // create object 2, packed guid 1, player
    let mut living = Living::standing();
    living.tail = u(0).to_vec();
    b.extend(living.bytes(0x71));
    let fields: [(usize, u32); 14] = [
        (2, 0x19),                      // OBJECT_FIELD_TYPE: object | unit | player
        (4, 1.0f32.to_bits()),          // OBJECT_FIELD_SCALE_X
        (22, 60),                       // UNIT_FIELD_HEALTH
        (28, 60),                       // UNIT_FIELD_MAXHEALTH
        (30, 1000),                     // UNIT_FIELD_MAXPOWER2: rage
        (34, 1),                        // UNIT_FIELD_LEVEL
        (35, 1),                        // UNIT_FIELD_FACTIONTEMPLATE
        (36, 1 | (1 << 8) | (1 << 24)), // UNIT_FIELD_BYTES_0: human warrior male, rage
        (46, 0x8),                      // UNIT_FIELD_FLAGS
        (152, 49),                      // UNIT_FIELD_DISPLAYID
        (153, 49),                      // UNIT_FIELD_NATIVEDISPLAYID
        (239, 0x0a_01_02_03),           // PLAYER_BYTES
        (926, 0),                       // PLAYER_XP: zero, so omitted on the wire
        (1461, 12345),                  // PLAYER_FIELD_COINAGE
    ];
    let mut mask = [0u32; 50];
    let mut values = Vec::new();
    for (index, value) in fields {
        if value != 0 {
            mask[index / 32] |= 1 << (index % 32);
        }
    }
    let mut set: Vec<(usize, u32)> = fields.iter().copied().filter(|&(_, v)| v != 0).collect();
    set.sort();
    for (_, v) in set {
        values.extend(u(v));
    }
    b.push(50);
    for w in mask {
        b.extend(u(w));
    }
    b.extend(values);
    b
}

fn framed(blocks: &[Vec<u8>], count: u32) -> Vec<u8> {
    let mut body = u(count).to_vec();
    body.push(0);
    for b in blocks {
        body.extend(b);
    }
    body
}

#[test]
fn an_own_player_create_reads_through_the_typed_accessors() {
    let body = framed(&[own_player_create()], 1);
    let (packet, tail) =
        parse_server_with_tail_for(&TBC_2_4_3, None, opcode::SMSG_UPDATE_OBJECT, &body).unwrap();
    assert_eq!(tail, 0);
    let ServerPacket::UpdateObject { objects } = packet else {
        panic!("not an update object")
    };
    let Object::Create {
        guid,
        object_type,
        movement,
        mask,
    } = &objects[0]
    else {
        panic!("not a create")
    };
    assert_eq!((*guid, *object_type), (1, ObjectType::Player));
    assert_eq!(mask.table(), &FIELDS_8606);
    assert_eq!(mask.object_type(), Some(ObjectType::Player));
    assert_eq!(mask.object_scale_x(), Some(1.0));
    assert_eq!(mask.unit_level(), Some(1));
    assert_eq!(
        (mask.unit_race(), mask.unit_class(), mask.unit_gender()),
        (Some(1), Some(1), Some(0))
    );
    assert_eq!(mask.unit_power_type(), 1);
    assert_eq!(
        (mask.unit_health(), mask.unit_max_health()),
        (Some(60), Some(60))
    );
    // Rage: an absent power in a create reads 0, and its maximum 1000 raw.
    assert_eq!(mask.unit_power(1), Some(0));
    assert_eq!(mask.unit_max_power(1), Some(1000));
    assert_eq!(mask.unit_shown_max_power(1), Some(100));
    assert_eq!(mask.unit_faction_template(), Some(1));
    assert_eq!(
        (mask.unit_displayid(), mask.unit_native_displayid()),
        (Some(49), Some(49))
    );
    assert_eq!(mask.unit_flags(), 8);
    assert_eq!(mask.player_skin(), Some(3));
    assert_eq!(mask.player_money(), Some(12345));
    assert_eq!(mask.player_xp(), Some(0));
    // The groups 2.4.3 lays out differently read through the shape: 25 empty quest-log slots and
    // no aura on a fresh create; the combo points are not a descriptor field.
    assert_eq!(mask.player_quest_log(0).map(|q| q.quest_id), Some(0));
    assert_eq!(mask.player_quest_log(24).map(|q| q.quest_id), Some(0));
    assert_eq!(mask.player_quest_log(25), None);
    assert_eq!(mask.unit_aura(0), None);
    assert_eq!(mask.player_combo_points(), None);
    assert_eq!(movement.speeds.unwrap()[1], 7.0);
}

#[test]
fn the_framing_counts_an_out_of_range_block_and_reads_near_lists() {
    // OUT_OF_RANGE [0x10, 0x20], NEAR [0x30], then the create: three blocks.
    let mut oor = vec![4u8];
    oor.extend(u(2));
    oor.extend([0x01, 0x10, 0x01, 0x20]);
    let mut near = vec![5u8];
    near.extend(u(1));
    near.extend([0x01, 0x30]);
    let body = framed(&[oor, near, own_player_create()], 3);
    let ServerPacket::UpdateObject { objects } =
        messages::parse_server_for(&TBC_2_4_3, None, opcode::SMSG_UPDATE_OBJECT, &body).unwrap()
    else {
        panic!("not an update object")
    };
    assert_eq!(objects.len(), 3);
    assert!(matches!(&objects[0], Object::OutOfRange { guids } if guids == &[0x10, 0x20]));
    assert!(matches!(&objects[1], Object::Near { guids } if guids == &[0x30]));
    assert!(matches!(&objects[2], Object::Create { .. }));
}

#[test]
fn a_compressed_update_is_the_same_body_in_one_zlib_stream() {
    let body = framed(&[own_player_create()], 1);
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    z.write_all(&body).unwrap();
    let mut packet = u(body.len() as u32).to_vec();
    packet.extend(z.finish().unwrap());
    let (parsed, tail) = parse_server_with_tail_for(
        &TBC_2_4_3,
        None,
        opcode::SMSG_COMPRESSED_UPDATE_OBJECT,
        &packet,
    )
    .unwrap();
    assert_eq!(tail, 0);
    let ServerPacket::UpdateObject { objects } = parsed else {
        panic!("not an update object")
    };
    assert!(matches!(
        &objects[0],
        Object::Create {
            object_type: ObjectType::Player,
            ..
        }
    ));
}

#[test]
fn an_unknown_update_type_and_a_truncated_block_are_errors() {
    let body = framed(&[vec![9u8]], 1);
    assert!(
        parse_server_with_tail_for(&TBC_2_4_3, None, opcode::SMSG_UPDATE_OBJECT, &body).is_err()
    );
    // A truncated living block is an error too.
    let mut cut = own_player_create();
    cut.truncate(20);
    let body = framed(&[cut], 1);
    assert!(
        parse_server_with_tail_for(&TBC_2_4_3, None, opcode::SMSG_UPDATE_OBJECT, &body).is_err()
    );
}

#[test]
fn the_entry_and_logout_packets_read_the_same_bytes_as_1_12_1() {
    let mut verify = u(0).to_vec();
    for v in [-8949.95f32, -132.493, 83.5312, 3.5] {
        verify.extend(f(v));
    }
    match messages::parse_server_for(&TBC_2_4_3, None, opcode::SMSG_LOGIN_VERIFY_WORLD, &verify)
        .unwrap()
    {
        ServerPacket::LoginVerifyWorld {
            map,
            position,
            orientation,
        } => assert_eq!(
            (map, position.x, position.y, position.z, orientation),
            (0, -8949.95, -132.493, 83.5312, 3.5)
        ),
        other => panic!("{}", other.name()),
    }
    assert!(matches!(
        messages::parse_server_for(&TBC_2_4_3, None, opcode::SMSG_CHARACTER_LOGIN_FAILED, &[5])
            .unwrap(),
        ServerPacket::CharacterLoginFailed { result: 5 }
    ));
    assert!(matches!(
        messages::parse_server_for(&TBC_2_4_3, None, opcode::SMSG_LOGOUT_COMPLETE, &[]).unwrap(),
        ServerPacket::LogoutComplete
    ));
    let mut resp = u(0).to_vec();
    resp.push(1);
    assert!(matches!(
        messages::parse_server_for(&TBC_2_4_3, None, opcode::SMSG_LOGOUT_RESPONSE, &resp).unwrap(),
        ServerPacket::LogoutResponse {
            reason: 0,
            instant: true
        }
    ));
}
