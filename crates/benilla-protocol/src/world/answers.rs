//! The answers the server's movement orders are owed: which ack answers which order, with which
//! body. The orders are the same in both builds (the 2.4.3 fly family aside); only the bytes of the
//! answer follow the session's build.

use crate::messages::{self, opcode, tbc_flag, tbc_opcode, FlightSpeed, ServerPacket};
use crate::messages::{MoveMode, TbcPacket};

use super::movement::{
    client_uptime_ms, force_speed_ack_body, full_info, move_flag_ack_body, movement_info,
};

/// Our own mover as the answers need it: who it is and where it stands. The caller keeps it
/// current; an answer is built from it at the moment the order is read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoverPose {
    pub guid: u64,
    pub position: [f32; 3],
    pub orientation: f32,
    /// The mover's live movement flags, in the session's build.
    pub flags: u32,
}

/// One packet to send back: its opcode, body and the name of the order it answers.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub opcode: u16,
    pub body: Vec<u8>,
    pub answers: &'static str,
}

/// The movement bit of `mode` in the build's flag word.
fn mode_flag(tbc: bool, mode: MoveMode) -> u32 {
    if !tbc {
        return mode.flag();
    }
    match mode {
        MoveMode::Root => tbc_flag::ROOT,
        MoveMode::WaterWalk => tbc_flag::WATER_WALK,
        MoveMode::FeatherFall => tbc_flag::FEATHER_FALL,
        MoveMode::Hover => tbc_flag::HOVER,
    }
}

/// The bits a rooted mover may not carry beside `ROOT`: every one that moves it. Turning is allowed.
fn moving_bits(tbc: bool) -> u32 {
    // Forward, backward, strafes; the airborne bit of the build.
    0x0F | if tbc {
        tbc_flag::FALLING | tbc_flag::FLYING | tbc_flag::FLYING2
    } else {
        0x2000
    }
}

/// `flags` with `bit` set or cleared; a root clears every moving bit as well.
fn with_bit(flags: u32, bit: u32, on: bool, root: bool, tbc: bool) -> u32 {
    let mut flags = if on { flags | bit } else { flags & !bit };
    if on && root {
        flags &= !moving_bits(tbc);
    }
    flags
}

/// The answer a server order is owed, or `None` for a packet that is not an order of ours (or is
/// addressed to another unit). `tbc` is the session's build.
pub fn answer_for(tbc: bool, packet: &ServerPacket, mover: &MoverPose) -> Option<Answer> {
    let pose = |flags: u32| movement_info(mover.position, mover.orientation, flags);
    match packet {
        ServerPacket::ForceSpeedChange {
            guid,
            kind,
            counter,
            speed,
        } if *guid == mover.guid => Some(Answer {
            opcode: if tbc {
                messages::tbc_movement::speed_ack_opcode(*kind)
            } else {
                kind.ack_opcode()
            },
            body: force_speed_ack_body(tbc, *guid, *counter, &pose(mover.flags), *speed),
            answers: "a force speed change",
        }),
        ServerPacket::Tbc(TbcPacket::ForceFlightSpeedChange {
            guid,
            kind,
            counter,
            speed,
        }) if *guid == mover.guid => Some(Answer {
            opcode: FlightSpeed::ack_opcode(*kind),
            body: force_speed_ack_body(tbc, *guid, *counter, &pose(mover.flags), *speed),
            answers: "a force flight speed change",
        }),
        ServerPacket::MoveMode {
            guid,
            counter,
            mode,
            apply,
        } if *guid == mover.guid => {
            let flags = with_bit(
                mover.flags,
                mode_flag(tbc, *mode),
                *apply,
                *mode == MoveMode::Root,
                tbc,
            );
            let applied = mode.ack_carries_apply().then_some(*apply);
            Some(Answer {
                opcode: mode.ack_opcode(*apply),
                body: move_flag_ack_body(tbc, *guid, *counter, &pose(flags), applied),
                answers: "a mover mode change",
            })
        }
        ServerPacket::Tbc(TbcPacket::MoveSetCanFly {
            guid,
            counter,
            apply,
        }) if *guid == mover.guid => Some(Answer {
            opcode: tbc_opcode::CMSG_MOVE_SET_CAN_FLY_ACK,
            body: move_flag_ack_body(
                tbc,
                *guid,
                *counter,
                &pose(with_bit(mover.flags, tbc_flag::CAN_FLY, *apply, false, tbc)),
                Some(*apply),
            ),
            answers: "a can-fly change",
        }),
        ServerPacket::KnockBack {
            guid,
            counter,
            launch,
        } if *guid == mover.guid => {
            // The ack's pose is ours at launch with the server's quad as the jump block.
            let airborne = if tbc { tbc_flag::FALLING } else { 0x2000 };
            let info = full_info(
                mover.position,
                mover.orientation,
                mover.flags | airborne,
                0.0,
                0,
                Some(*launch),
                None,
            );
            Some(Answer {
                opcode: opcode::CMSG_MOVE_KNOCK_BACK_ACK,
                body: move_flag_ack_body(tbc, *guid, *counter, &info, None),
                answers: "a knock back",
            })
        }
        ServerPacket::Teleport { guid, counter, .. } if *guid == mover.guid => Some(Answer {
            opcode: opcode::MSG_MOVE_TELEPORT_ACK,
            body: messages::teleport_ack(*guid, *counter, client_uptime_ms()),
            answers: "a near teleport",
        }),
        ServerPacket::NewWorld { .. } => Some(Answer {
            opcode: opcode::MSG_MOVE_WORLDPORT_ACK,
            body: Vec::new(),
            answers: "a new world",
        }),
        ServerPacket::Tbc(TbcPacket::TimeSyncRequest { counter }) => Some(Answer {
            opcode: tbc_opcode::CMSG_TIME_SYNC_RESP,
            body: messages::time_sync_response(*counter, client_uptime_ms()),
            answers: "a time sync request",
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{tbc_movement as tm, JumpInfo, SpeedKind};
    use crate::wire::Vector3d;

    const ME: u64 = 0x709;

    fn me(flags: u32) -> MoverPose {
        MoverPose {
            guid: ME,
            position: [-8949.95, -132.49, 83.53],
            orientation: 1.5,
            flags,
        }
    }

    /// The pose inside an ack body that starts `skip` bytes in, read back as a 2.4.3 info.
    fn info_at(body: &[u8], skip: usize) -> tm::TbcMovementInfo {
        let mut r = &body[skip..];
        tm::TbcMovementInfo::read(&mut r).expect("an info")
    }

    #[test]
    fn a_force_speed_change_is_acked_with_its_counter_and_exact_speed() {
        let order = ServerPacket::ForceSpeedChange {
            guid: ME,
            kind: SpeedKind::Run,
            counter: 5,
            speed: 8.25,
        };
        let a = answer_for(true, &order, &me(0)).expect("owed");
        assert_eq!(a.opcode, 0xE3);
        assert_eq!(&a.body[..8], &ME.to_le_bytes());
        assert_eq!(&a.body[8..12], &5u32.to_le_bytes());
        assert_eq!(&a.body[a.body.len() - 4..], &8.25f32.to_le_bytes());
        assert_eq!(info_at(&a.body, 12).position.x, -8949.95);
        // The 1.12.1 body has no flags2 byte.
        let old = answer_for(false, &order, &me(0)).expect("owed");
        assert_eq!(old.body.len() + 1, a.body.len());
        // Not ours: no answer.
        let other = ServerPacket::ForceSpeedChange {
            guid: 1,
            kind: SpeedKind::Run,
            counter: 5,
            speed: 8.25,
        };
        assert!(answer_for(true, &other, &me(0)).is_none());
    }

    #[test]
    fn the_flight_speeds_are_acked_on_their_own_opcodes() {
        for (kind, op) in [
            (FlightSpeed::Flight, 0x382),
            (FlightSpeed::FlightBack, 0x384),
        ] {
            let order = ServerPacket::Tbc(TbcPacket::ForceFlightSpeedChange {
                guid: ME,
                kind,
                counter: 9,
                speed: 7.0,
            });
            let a = answer_for(true, &order, &me(0)).expect("owed");
            assert_eq!(a.opcode, op);
            assert_eq!(&a.body[8..12], &9u32.to_le_bytes());
        }
    }

    #[test]
    fn a_mode_ack_carries_the_mode_bit_and_the_applied_word_except_for_root() {
        let hover = ServerPacket::MoveMode {
            guid: ME,
            counter: 3,
            mode: MoveMode::Hover,
            apply: true,
        };
        let a = answer_for(true, &hover, &me(tbc_flag::FORWARD)).expect("owed");
        assert_eq!(a.opcode, 0xF6);
        let info = info_at(&a.body, 12);
        assert_eq!(info.flags, tbc_flag::FORWARD | tbc_flag::HOVER);
        assert_eq!(&a.body[a.body.len() - 4..], &1u32.to_le_bytes());

        // A root clears every moving bit and has no applied word.
        let root = ServerPacket::MoveMode {
            guid: ME,
            counter: 4,
            mode: MoveMode::Root,
            apply: true,
        };
        let a = answer_for(
            true,
            &root,
            &me(tbc_flag::FORWARD | tbc_flag::TURN_LEFT | tbc_flag::FALLING),
        )
        .expect("owed");
        assert_eq!(a.opcode, 0xE9);
        let info = info_at(&a.body, 12);
        assert_eq!(info.flags, tbc_flag::ROOT | tbc_flag::TURN_LEFT);
        assert_eq!(a.body.len(), 12 + info.to_bytes().len(), "no applied word");

        // The unroot goes on the other opcode and clears the bit.
        let unroot = ServerPacket::MoveMode {
            guid: ME,
            counter: 5,
            mode: MoveMode::Root,
            apply: false,
        };
        let a = answer_for(true, &unroot, &me(tbc_flag::ROOT)).expect("owed");
        assert_eq!(a.opcode, 0xEB);
        assert_eq!(info_at(&a.body, 12).flags, 0);
    }

    #[test]
    fn can_fly_and_knock_back_are_acked() {
        let fly = ServerPacket::Tbc(TbcPacket::MoveSetCanFly {
            guid: ME,
            counter: 6,
            apply: true,
        });
        let a = answer_for(true, &fly, &me(0)).expect("owed");
        assert_eq!(a.opcode, 0x345);
        assert_eq!(info_at(&a.body, 12).flags, tbc_flag::CAN_FLY);
        assert_eq!(&a.body[a.body.len() - 4..], &1u32.to_le_bytes());

        let launch = JumpInfo {
            zspeed: -4.0,
            cos_angle: 0.6,
            sin_angle: 0.8,
            xy_speed: 9.0,
        };
        let knock = ServerPacket::KnockBack {
            guid: ME,
            counter: 7,
            launch,
        };
        let a = answer_for(true, &knock, &me(0)).expect("owed");
        assert_eq!(a.opcode, 0xF0);
        let info = info_at(&a.body, 12);
        assert_eq!(info.flags, tbc_flag::FALLING);
        assert_eq!(info.jump, Some(launch), "the server's quad echoed exactly");
    }

    #[test]
    fn a_teleport_and_a_new_world_are_acked_and_a_time_sync_answered() {
        let tele = ServerPacket::Teleport {
            guid: ME,
            counter: 2,
            position: Vector3d {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            orientation: 0.0,
        };
        let a = answer_for(true, &tele, &me(0)).expect("owed");
        assert_eq!(a.opcode, 0xC7);
        assert_eq!(a.body.len(), 16);
        assert_eq!(&a.body[..8], &ME.to_le_bytes());
        assert_eq!(&a.body[8..12], &2u32.to_le_bytes());
        let world = ServerPacket::NewWorld {
            map: 530,
            position: Vector3d {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            orientation: 0.0,
        };
        let a = answer_for(true, &world, &me(0)).expect("owed");
        assert_eq!((a.opcode, a.body.len()), (0xDC, 0));
        let sync = ServerPacket::Tbc(TbcPacket::TimeSyncRequest { counter: 3 });
        let a = answer_for(true, &sync, &me(0)).expect("owed");
        assert_eq!(a.opcode, 0x391);
        assert_eq!(&a.body[..4], &3u32.to_le_bytes());
    }

    #[test]
    fn relays_and_observer_packets_need_no_answer() {
        let relay = ServerPacket::SplineMoveMode {
            guid: ME,
            mode: messages::SplineMode::Root,
            apply: true,
        };
        assert!(answer_for(true, &relay, &me(0)).is_none());
        assert!(answer_for(true, &ServerPacket::CancelCombat, &me(0)).is_none());
    }
}
