//! Movement on 2.4.3: the movement info (one layout description behind both its reader and its
//! writer), the bodies the client sends, and the movement packets the server sends. The opcode
//! numbers are 1.12.1's wherever both builds have the packet; only the fly family is new. Every
//! layout is cmangos-tbc's (`MovementInfo::{Read,Write}`, `MovementHandler.cpp`, `Unit.cpp`) and
//! agrees with wow_messages, except where a comment says otherwise.

use std::io::{self, Read};

use crate::wire::{read_f32_le, read_packed_guid, read_u32_le, read_u64_le, read_u8, Vector3d};

use super::{
    tbc_opcode as t, JumpInfo, MoveMode, MovementInfo, ServerPacket, SpeedKind, SplineMode,
    TbcPacket, TransportPose,
};

/// The 2.4.3 movement-flag bits (cmangos-tbc `MovementFlags`). They differ from 1.12.1's: the
/// transport bit is 0x200 (0x0200_0000 there) and 0x0200_0000 is the flying bit.
pub mod tbc_flag {
    pub const FORWARD: u32 = 0x0000_0001;
    pub const BACKWARD: u32 = 0x0000_0002;
    pub const STRAFE_LEFT: u32 = 0x0000_0004;
    pub const STRAFE_RIGHT: u32 = 0x0000_0008;
    pub const TURN_LEFT: u32 = 0x0000_0010;
    pub const TURN_RIGHT: u32 = 0x0000_0020;
    pub const WALK_MODE: u32 = 0x0000_0100;
    pub const ON_TRANSPORT: u32 = 0x0000_0200;
    /// cmangos-tbc 0x800; wow_messages 0x1000. Unresolved, the server is the arbiter.
    pub const ROOT: u32 = 0x0000_0800;
    /// The jump block's flag in cmangos-tbc (wow_messages names 0x2000 for it).
    pub const FALLING: u32 = 0x0000_1000;
    pub const SWIMMING: u32 = 0x0020_0000;
    pub const CAN_FLY: u32 = 0x0080_0000;
    pub const FLYING: u32 = 0x0100_0000;
    /// Actual flight: carries the pitch like swimming does.
    pub const FLYING2: u32 = 0x0200_0000;
    pub const SPLINE_ELEVATION: u32 = 0x0400_0000;
    pub const SPLINE_ENABLED: u32 = 0x0800_0000;
    pub const WATER_WALK: u32 = 0x1000_0000;
    pub const FEATHER_FALL: u32 = 0x2000_0000;
    pub const HOVER: u32 = 0x4000_0000;
}

/// Which optional blocks follow the fixed head, decided by the flags word alone: the one place
/// the layout lives, so the reader and the writer cannot disagree.
struct Tails {
    transport: bool,
    pitch: bool,
    jump: bool,
    elevation: bool,
}

fn tails(flags: u32) -> Tails {
    Tails {
        transport: flags & tbc_flag::ON_TRANSPORT != 0,
        pitch: flags & (tbc_flag::SWIMMING | tbc_flag::FLYING2) != 0,
        jump: flags & tbc_flag::FALLING != 0,
        elevation: flags & tbc_flag::SPLINE_ELEVATION != 0,
    }
}

/// The transport block of a movement info: the pose, then the transport's clock stamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TbcTransport {
    pub pose: TransportPose,
    pub time: u32,
}

/// A 2.4.3 movement info: `flags u32, flags2 u8, time u32, position, facing`, then the transport,
/// pitch, `fall_time`, jump and spline-elevation blocks the flags select.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TbcMovementInfo {
    pub flags: u32,
    /// The extra movement flags (`moveFlags2`).
    pub flags2: u8,
    /// The sender's tick clock in ms; on a relay, the server's.
    pub timestamp: u32,
    pub position: Vector3d,
    pub orientation: f32,
    pub transport: Option<TbcTransport>,
    /// Swim or flight pitch in radians; on the wire only while `SWIMMING` or `FLYING2`.
    pub pitch: f32,
    /// Ms airborne.
    pub fall_time: u32,
    /// On the wire only while `FALLING`.
    pub jump: Option<JumpInfo>,
    /// On the wire only while `SPLINE_ELEVATION`.
    pub spline_elevation: Option<f32>,
}

impl TbcMovementInfo {
    /// A grounded info at `position` with `flags` and no optional block.
    pub fn at(flags: u32, timestamp: u32, position: Vector3d, orientation: f32) -> Self {
        Self {
            flags,
            flags2: 0,
            timestamp,
            position,
            orientation,
            transport: None,
            pitch: 0.0,
            fall_time: 0,
            jump: None,
            spline_elevation: None,
        }
    }

    /// Read one, as the server reads a client's and a client reads a relay's.
    pub fn read(r: &mut impl Read) -> io::Result<Self> {
        let flags = read_u32_le(r)?;
        let flags2 = read_u8(r)?;
        let timestamp = read_u32_le(r)?;
        let position = Vector3d::read(r)?;
        let orientation = read_f32_le(r)?;
        let layout = tails(flags);
        let transport = if layout.transport {
            Some(TbcTransport {
                pose: TransportPose {
                    guid: read_u64_le(r)?,
                    pos: Vector3d::read(r)?,
                    orientation: read_f32_le(r)?,
                },
                time: read_u32_le(r)?,
            })
        } else {
            None
        };
        let pitch = if layout.pitch { read_f32_le(r)? } else { 0.0 };
        let fall_time = read_u32_le(r)?;
        let jump = if layout.jump {
            Some(JumpInfo {
                zspeed: read_f32_le(r)?,
                cos_angle: read_f32_le(r)?,
                sin_angle: read_f32_le(r)?,
                xy_speed: read_f32_le(r)?,
            })
        } else {
            None
        };
        let spline_elevation = if layout.elevation {
            Some(read_f32_le(r)?)
        } else {
            None
        };
        Ok(Self {
            flags,
            flags2,
            timestamp,
            position,
            orientation,
            transport,
            pitch,
            fall_time,
            jump,
            spline_elevation,
        })
    }

    /// Write it; each optional block is written exactly when its flag is set (a missing value
    /// goes out as zeros), so the bytes always parse back to the same flags.
    pub fn write(&self, w: &mut Vec<u8>) {
        w.extend_from_slice(&self.flags.to_le_bytes());
        w.push(self.flags2);
        w.extend_from_slice(&self.timestamp.to_le_bytes());
        for v in [
            self.position.x,
            self.position.y,
            self.position.z,
            self.orientation,
        ] {
            w.extend_from_slice(&v.to_le_bytes());
        }
        let layout = tails(self.flags);
        if layout.transport {
            let t = self.transport.unwrap_or(TbcTransport {
                pose: TransportPose {
                    guid: 0,
                    pos: Vector3d {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    orientation: 0.0,
                },
                time: 0,
            });
            w.extend_from_slice(&t.pose.guid.to_le_bytes());
            for v in [t.pose.pos.x, t.pose.pos.y, t.pose.pos.z, t.pose.orientation] {
                w.extend_from_slice(&v.to_le_bytes());
            }
            w.extend_from_slice(&t.time.to_le_bytes());
        }
        if layout.pitch {
            w.extend_from_slice(&self.pitch.to_le_bytes());
        }
        w.extend_from_slice(&self.fall_time.to_le_bytes());
        if layout.jump {
            let j = self.jump.unwrap_or_default();
            for v in [j.zspeed, j.cos_angle, j.sin_angle, j.xy_speed] {
                w.extend_from_slice(&v.to_le_bytes());
            }
        }
        if layout.elevation {
            w.extend_from_slice(&self.spline_elevation.unwrap_or(0.0).to_le_bytes());
        }
    }

    /// The bytes of [`Self::write`].
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut body = Vec::with_capacity(40);
        self.write(&mut body);
        body
    }
}

impl From<&MovementInfo> for TbcMovementInfo {
    /// A pose built for the 1.12.1 verbs, in 2.4.3's form: `flags` are taken as 2.4.3 bits, and a
    /// transport block gets a zero clock stamp (the 2.4.3 client sends the transport's own).
    fn from(info: &MovementInfo) -> Self {
        Self {
            flags: info.flags,
            flags2: 0,
            timestamp: info.timestamp,
            position: info.position,
            orientation: info.orientation,
            transport: info.transport.map(|pose| TbcTransport { pose, time: 0 }),
            pitch: info.pitch,
            fall_time: info.fall_time,
            jump: info.jump,
            spline_elevation: None,
        }
    }
}

// --- client to server --------------------------------------------------------------------------

/// The body of every `MSG_MOVE_*` input packet and of `CMSG_MOVE_FALL_RESET` / `CMSG_MOVE_SET_FLY`:
/// the movement info alone, no guid (`HandleMovementOpcodes`).
pub fn movement(info: &TbcMovementInfo) -> Vec<u8> {
    info.to_bytes()
}

/// `CMSG_MOVE_NOT_ACTIVE_MOVER`: the released unit's full guid, then its movement info.
pub fn not_active_mover(guid: u64, info: &TbcMovementInfo) -> Vec<u8> {
    let mut body = guid.to_le_bytes().to_vec();
    info.write(&mut body);
    body
}

/// `CMSG_MOVE_SPLINE_DONE`: the movement info, then the spline counter. 1.12.1 has a trailing
/// float the 2.4.3 body lacks (cmangos-tbc `HandleMoveSplineDoneOpcode`, wow_messages).
pub fn move_spline_done(info: &TbcMovementInfo, counter: u32) -> Vec<u8> {
    let mut body = info.to_bytes();
    body.extend_from_slice(&counter.to_le_bytes());
    body
}

/// Body of the eight `CMSG_FORCE_*_SPEED_CHANGE_ACK`s: full guid, the echoed counter, the movement
/// info, the speed.
pub fn force_speed_ack(guid: u64, counter: u32, info: &TbcMovementInfo, speed: f32) -> Vec<u8> {
    let mut body = Vec::with_capacity(48);
    body.extend_from_slice(&guid.to_le_bytes());
    body.extend_from_slice(&counter.to_le_bytes());
    info.write(&mut body);
    body.extend_from_slice(&speed.to_le_bytes());
    body
}

/// Body of a mode ack: full guid, the echoed counter, the movement info, then `u32 applied` for
/// the hover, feather-fall, water-walk and can-fly acks (root and knock-back have none).
pub fn move_flag_ack(
    guid: u64,
    counter: u32,
    info: &TbcMovementInfo,
    applied: Option<bool>,
) -> Vec<u8> {
    let mut body = Vec::with_capacity(48);
    body.extend_from_slice(&guid.to_le_bytes());
    body.extend_from_slice(&counter.to_le_bytes());
    info.write(&mut body);
    if let Some(applied) = applied {
        body.extend_from_slice(&u32::from(applied).to_le_bytes());
    }
    body
}

/// The 2.4.3 `CMSG_FORCE_*_SPEED_CHANGE_ACK` opcode of a speed kind. The six kinds the app
/// models share 1.12.1's numbers; [`FlightSpeed`] names the two new ones.
pub fn speed_ack_opcode(kind: SpeedKind) -> u16 {
    match kind {
        SpeedKind::Walk => t::CMSG_FORCE_WALK_SPEED_CHANGE_ACK,
        SpeedKind::Run => t::CMSG_FORCE_RUN_SPEED_CHANGE_ACK,
        SpeedKind::RunBack => t::CMSG_FORCE_RUN_BACK_SPEED_CHANGE_ACK,
        SpeedKind::Swim => t::CMSG_FORCE_SWIM_SPEED_CHANGE_ACK,
        SpeedKind::SwimBack => t::CMSG_FORCE_SWIM_BACK_SPEED_CHANGE_ACK,
        SpeedKind::TurnRate => t::CMSG_FORCE_TURN_RATE_CHANGE_ACK,
    }
}

/// A 2.4.3 flight speed, which the 1.12.1 [`SpeedKind`] has no variant for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightSpeed {
    Flight,
    FlightBack,
}

impl FlightSpeed {
    /// The `CMSG_FORCE_FLIGHT_*_SPEED_CHANGE_ACK` answering this speed.
    pub fn ack_opcode(self) -> u16 {
        match self {
            FlightSpeed::Flight => t::CMSG_FORCE_FLIGHT_SPEED_CHANGE_ACK,
            FlightSpeed::FlightBack => t::CMSG_FORCE_FLIGHT_BACK_SPEED_CHANGE_ACK,
        }
    }
}

// --- server to client --------------------------------------------------------------------------

/// What follows the movement info of a relay beyond the pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RelayTail {
    None,
    /// `MSG_MOVE_SET_*_SPEED`: the new speed.
    Speed(f32),
    /// `MSG_MOVE_KNOCK_BACK`: `cos, sin, xy_speed, z_speed` as cmangos-tbc writes them (wow_messages
    /// lists sin first, which the server contradicts).
    Launch {
        cos_angle: f32,
        sin_angle: f32,
        xy_speed: f32,
        z_speed: f32,
    },
}

/// Whether a relay of this opcode ends in a speed or a launch rather than the info.
fn relay_tail_kind(op: u16) -> Option<bool> {
    match op {
        t::MSG_MOVE_SET_WALK_SPEED
        | t::MSG_MOVE_SET_RUN_SPEED
        | t::MSG_MOVE_SET_RUN_BACK_SPEED
        | t::MSG_MOVE_SET_SWIM_SPEED
        | t::MSG_MOVE_SET_SWIM_BACK_SPEED
        | t::MSG_MOVE_SET_TURN_RATE
        | t::MSG_MOVE_SET_FLIGHT_SPEED
        | t::MSG_MOVE_SET_FLIGHT_BACK_SPEED => Some(true),
        t::MSG_MOVE_KNOCK_BACK => Some(false),
        _ => None,
    }
}

/// True for an opcode the server relays as `[packed guid][movement info]` (and a tail for a few).
pub const fn is_relay(op: u16) -> bool {
    matches!(
        op,
        t::MSG_MOVE_START_FORWARD
            | t::MSG_MOVE_START_BACKWARD
            | t::MSG_MOVE_STOP
            | t::MSG_MOVE_START_STRAFE_LEFT
            | t::MSG_MOVE_START_STRAFE_RIGHT
            | t::MSG_MOVE_STOP_STRAFE
            | t::MSG_MOVE_JUMP
            | t::MSG_MOVE_START_TURN_LEFT
            | t::MSG_MOVE_START_TURN_RIGHT
            | t::MSG_MOVE_STOP_TURN
            | t::MSG_MOVE_START_PITCH_UP
            | t::MSG_MOVE_START_PITCH_DOWN
            | t::MSG_MOVE_STOP_PITCH
            | t::MSG_MOVE_SET_RUN_MODE
            | t::MSG_MOVE_SET_WALK_MODE
            | t::MSG_MOVE_TELEPORT
            | t::MSG_MOVE_FALL_LAND
            | t::MSG_MOVE_START_SWIM
            | t::MSG_MOVE_STOP_SWIM
            | t::MSG_MOVE_SET_FACING
            | t::MSG_MOVE_SET_PITCH
            | t::MSG_MOVE_HEARTBEAT
            | t::MSG_MOVE_ROOT
            | t::MSG_MOVE_UNROOT
            | t::MSG_MOVE_HOVER
            | t::MSG_MOVE_FEATHER_FALL
            | t::MSG_MOVE_WATER_WALK
            | t::MSG_MOVE_UPDATE_CAN_FLY
            | t::MSG_MOVE_START_ASCEND
            | t::MSG_MOVE_STOP_ASCEND
            | t::MSG_MOVE_START_DESCEND
            | t::MSG_MOVE_SET_WALK_SPEED
            | t::MSG_MOVE_SET_RUN_SPEED
            | t::MSG_MOVE_SET_RUN_BACK_SPEED
            | t::MSG_MOVE_SET_SWIM_SPEED
            | t::MSG_MOVE_SET_SWIM_BACK_SPEED
            | t::MSG_MOVE_SET_TURN_RATE
            | t::MSG_MOVE_SET_FLIGHT_SPEED
            | t::MSG_MOVE_SET_FLIGHT_BACK_SPEED
            | t::MSG_MOVE_KNOCK_BACK
    )
}

fn read_relay(op: u16, r: &mut &[u8]) -> io::Result<TbcPacket> {
    let guid = read_packed_guid(r)?;
    let info = TbcMovementInfo::read(r)?;
    let tail = match relay_tail_kind(op) {
        Some(true) => RelayTail::Speed(read_f32_le(r)?),
        Some(false) => RelayTail::Launch {
            cos_angle: read_f32_le(r)?,
            sin_angle: read_f32_le(r)?,
            xy_speed: read_f32_le(r)?,
            z_speed: read_f32_le(r)?,
        },
        None => RelayTail::None,
    };
    Ok(TbcPacket::MoveRelay {
        guid,
        opcode: op,
        info,
        tail,
    })
}

/// Read a force-speed order: packed guid, counter, a zero byte for run only, the speed.
fn read_force_speed(kind: SpeedKind, r: &mut &[u8]) -> io::Result<ServerPacket> {
    let guid = read_packed_guid(r)?;
    let counter = read_u32_le(r)?;
    if kind == SpeedKind::Run {
        // "new 2.1.0 - update tracking run speed", always 0 (cmangos-tbc `SetSpeedRate`).
        let _ = read_u8(r)?;
    }
    Ok(ServerPacket::ForceSpeedChange {
        guid,
        kind,
        counter,
        speed: read_f32_le(r)?,
    })
}

fn read_spline_speed(kind: SpeedKind, r: &mut &[u8]) -> io::Result<ServerPacket> {
    Ok(ServerPacket::SplineSpeedChange {
        guid: read_packed_guid(r)?,
        kind,
        speed: read_f32_le(r)?,
    })
}

fn read_mode(mode: MoveMode, apply: bool, r: &mut &[u8]) -> io::Result<ServerPacket> {
    Ok(ServerPacket::MoveMode {
        guid: read_packed_guid(r)?,
        counter: read_u32_le(r)?,
        mode,
        apply,
    })
}

fn read_spline_mode(mode: SplineMode, apply: bool, r: &mut &[u8]) -> io::Result<ServerPacket> {
    Ok(ServerPacket::SplineMoveMode {
        guid: read_packed_guid(r)?,
        mode,
        apply,
    })
}

/// The 2.4.3 movement packets the server sends, or `None` for an opcode that is not one.
pub(super) fn read_server(op: u16, r: &mut &[u8]) -> io::Result<Option<ServerPacket>> {
    let packet = match op {
        o if is_relay(o) => ServerPacket::Tbc(read_relay(o, r)?),
        t::MSG_MOVE_TIME_SKIPPED => ServerPacket::MoveTimeSkipped {
            guid: read_packed_guid(r)?,
            lag_ms: read_u32_le(r)?,
        },
        // The server's leg of the teleport: the destination rides the movement info.
        t::MSG_MOVE_TELEPORT_ACK => {
            let guid = read_packed_guid(r)?;
            let counter = read_u32_le(r)?;
            let info = TbcMovementInfo::read(r)?;
            ServerPacket::Teleport {
                guid,
                counter,
                position: info.position,
                orientation: info.orientation,
            }
        }
        t::SMSG_NEW_WORLD => ServerPacket::NewWorld {
            map: read_u32_le(r)?,
            position: Vector3d::read(r)?,
            orientation: read_f32_le(r)?,
        },
        // `u32 map`, then the transport entry and the old map only when riding one.
        t::SMSG_TRANSFER_PENDING => {
            let map = read_u32_le(r)?;
            let transport = if r.is_empty() {
                None
            } else {
                Some((read_u32_le(r)?, read_u32_le(r)?))
            };
            ServerPacket::TransferPending { map, transport }
        }
        t::SMSG_CLIENT_CONTROL_UPDATE => ServerPacket::ClientControlUpdate {
            mover: read_packed_guid(r)?,
            allow_move: read_u8(r)? != 0,
        },
        t::SMSG_FORCE_WALK_SPEED_CHANGE => read_force_speed(SpeedKind::Walk, r)?,
        t::SMSG_FORCE_RUN_SPEED_CHANGE => read_force_speed(SpeedKind::Run, r)?,
        t::SMSG_FORCE_RUN_BACK_SPEED_CHANGE => read_force_speed(SpeedKind::RunBack, r)?,
        t::SMSG_FORCE_SWIM_SPEED_CHANGE => read_force_speed(SpeedKind::Swim, r)?,
        t::SMSG_FORCE_SWIM_BACK_SPEED_CHANGE => read_force_speed(SpeedKind::SwimBack, r)?,
        t::SMSG_FORCE_TURN_RATE_CHANGE => read_force_speed(SpeedKind::TurnRate, r)?,
        t::SMSG_FORCE_FLIGHT_SPEED_CHANGE | t::SMSG_FORCE_FLIGHT_BACK_SPEED_CHANGE => {
            ServerPacket::Tbc(TbcPacket::ForceFlightSpeedChange {
                guid: read_packed_guid(r)?,
                kind: if op == t::SMSG_FORCE_FLIGHT_SPEED_CHANGE {
                    FlightSpeed::Flight
                } else {
                    FlightSpeed::FlightBack
                },
                counter: read_u32_le(r)?,
                speed: read_f32_le(r)?,
            })
        }
        t::SMSG_SPLINE_SET_WALK_SPEED => read_spline_speed(SpeedKind::Walk, r)?,
        t::SMSG_SPLINE_SET_RUN_SPEED => read_spline_speed(SpeedKind::Run, r)?,
        t::SMSG_SPLINE_SET_RUN_BACK_SPEED => read_spline_speed(SpeedKind::RunBack, r)?,
        t::SMSG_SPLINE_SET_SWIM_SPEED => read_spline_speed(SpeedKind::Swim, r)?,
        t::SMSG_SPLINE_SET_SWIM_BACK_SPEED => read_spline_speed(SpeedKind::SwimBack, r)?,
        t::SMSG_SPLINE_SET_TURN_RATE => read_spline_speed(SpeedKind::TurnRate, r)?,
        t::SMSG_SPLINE_SET_FLIGHT_SPEED | t::SMSG_SPLINE_SET_FLIGHT_BACK_SPEED => {
            ServerPacket::Tbc(TbcPacket::SplineFlightSpeedChange {
                guid: read_packed_guid(r)?,
                kind: if op == t::SMSG_SPLINE_SET_FLIGHT_SPEED {
                    FlightSpeed::Flight
                } else {
                    FlightSpeed::FlightBack
                },
                speed: read_f32_le(r)?,
            })
        }
        t::SMSG_FORCE_MOVE_ROOT => read_mode(MoveMode::Root, true, r)?,
        t::SMSG_FORCE_MOVE_UNROOT => read_mode(MoveMode::Root, false, r)?,
        t::SMSG_MOVE_WATER_WALK => read_mode(MoveMode::WaterWalk, true, r)?,
        t::SMSG_MOVE_LAND_WALK => read_mode(MoveMode::WaterWalk, false, r)?,
        t::SMSG_MOVE_FEATHER_FALL => read_mode(MoveMode::FeatherFall, true, r)?,
        t::SMSG_MOVE_NORMAL_FALL => read_mode(MoveMode::FeatherFall, false, r)?,
        t::SMSG_MOVE_SET_HOVER => read_mode(MoveMode::Hover, true, r)?,
        t::SMSG_MOVE_UNSET_HOVER => read_mode(MoveMode::Hover, false, r)?,
        t::SMSG_MOVE_SET_CAN_FLY | t::SMSG_MOVE_UNSET_CAN_FLY => {
            ServerPacket::Tbc(TbcPacket::MoveSetCanFly {
                guid: read_packed_guid(r)?,
                counter: read_u32_le(r)?,
                apply: op == t::SMSG_MOVE_SET_CAN_FLY,
            })
        }
        t::SMSG_SPLINE_MOVE_ROOT => read_spline_mode(SplineMode::Root, true, r)?,
        t::SMSG_SPLINE_MOVE_UNROOT => read_spline_mode(SplineMode::Root, false, r)?,
        t::SMSG_SPLINE_MOVE_WATER_WALK => read_spline_mode(SplineMode::WaterWalk, true, r)?,
        t::SMSG_SPLINE_MOVE_LAND_WALK => read_spline_mode(SplineMode::WaterWalk, false, r)?,
        t::SMSG_SPLINE_MOVE_FEATHER_FALL => read_spline_mode(SplineMode::FeatherFall, true, r)?,
        t::SMSG_SPLINE_MOVE_NORMAL_FALL => read_spline_mode(SplineMode::FeatherFall, false, r)?,
        t::SMSG_SPLINE_MOVE_SET_HOVER => read_spline_mode(SplineMode::Hover, true, r)?,
        t::SMSG_SPLINE_MOVE_UNSET_HOVER => read_spline_mode(SplineMode::Hover, false, r)?,
        t::SMSG_SPLINE_MOVE_START_SWIM => read_spline_mode(SplineMode::Swimming, true, r)?,
        t::SMSG_SPLINE_MOVE_STOP_SWIM => read_spline_mode(SplineMode::Swimming, false, r)?,
        // Inverted as in 1.12.1: the opcode's bool feeds `SetRunMode`, so RUN_MODE clears the
        // walk-mode flag.
        t::SMSG_SPLINE_MOVE_SET_WALK_MODE => read_spline_mode(SplineMode::WalkMode, true, r)?,
        t::SMSG_SPLINE_MOVE_SET_RUN_MODE => read_spline_mode(SplineMode::WalkMode, false, r)?,
        t::SMSG_SPLINE_MOVE_SET_FLYING | t::SMSG_SPLINE_MOVE_UNSET_FLYING => {
            ServerPacket::Tbc(TbcPacket::SplineMoveFlying {
                guid: read_packed_guid(r)?,
                apply: op == t::SMSG_SPLINE_MOVE_SET_FLYING,
            })
        }
        // `packed guid, u32 counter, cos, sin, xy_speed, z_speed`: the jump block the ack echoes.
        t::SMSG_MOVE_KNOCK_BACK => {
            let guid = read_packed_guid(r)?;
            let counter = read_u32_le(r)?;
            let cos_angle = read_f32_le(r)?;
            let sin_angle = read_f32_le(r)?;
            let xy_speed = read_f32_le(r)?;
            let zspeed = read_f32_le(r)?;
            ServerPacket::KnockBack {
                guid,
                counter,
                launch: JumpInfo {
                    zspeed,
                    cos_angle,
                    sin_angle,
                    xy_speed,
                },
            }
        }
        _ => return Ok(None),
    };
    Ok(Some(packet))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::write_packed_guid;

    fn v(x: f32, y: f32, z: f32) -> Vector3d {
        Vector3d { x, y, z }
    }

    fn base(flags: u32) -> TbcMovementInfo {
        TbcMovementInfo::at(flags, 0x0102_0304, v(-8949.95, -132.49, 83.53), 1.5)
    }

    fn round(info: &TbcMovementInfo) -> TbcMovementInfo {
        let bytes = info.to_bytes();
        let mut r = &bytes[..];
        let back = TbcMovementInfo::read(&mut r).expect("reads back");
        assert!(r.is_empty(), "the writer's bytes are all read");
        back
    }

    #[test]
    fn a_plain_info_is_the_head_and_the_fall_time() {
        let info = base(tbc_flag::FORWARD);
        let bytes = info.to_bytes();
        // flags u32, flags2 u8, time u32, 4 floats, fall time u32.
        assert_eq!(bytes.len(), 4 + 1 + 4 + 16 + 4);
        assert_eq!(&bytes[..5], &[1, 0, 0, 0, 0]);
        assert_eq!(&bytes[5..9], &0x0102_0304u32.to_le_bytes());
        assert_eq!(round(&info), info);
    }

    #[test]
    fn a_falling_info_carries_the_jump_block() {
        let mut info = base(tbc_flag::FORWARD | tbc_flag::FALLING);
        info.fall_time = 640;
        info.jump = Some(JumpInfo {
            zspeed: -7.955547,
            cos_angle: 0.6,
            sin_angle: 0.8,
            xy_speed: 7.0,
        });
        let plain = base(tbc_flag::FORWARD).to_bytes().len();
        assert_eq!(info.to_bytes().len(), plain + 16);
        assert_eq!(round(&info), info);
    }

    #[test]
    fn a_swimming_info_carries_its_pitch_and_a_flying_one_too() {
        for flag in [tbc_flag::SWIMMING, tbc_flag::FLYING2 | tbc_flag::CAN_FLY] {
            let mut info = base(flag);
            info.pitch = -0.7;
            assert_eq!(
                info.to_bytes().len(),
                base(0).to_bytes().len() + 4,
                "the pitch adds one float"
            );
            assert_eq!(round(&info), info);
        }
        // The 1.12.1 pitch rule does not hold here: the flying bit alone is not swimming.
        let mut cruising = base(tbc_flag::FLYING);
        cruising.pitch = 0.3;
        assert_eq!(
            round(&cruising).pitch,
            0.0,
            "FLYING (0x1000000) carries no pitch"
        );
    }

    #[test]
    fn a_transport_info_carries_the_pose_and_the_clock_stamp() {
        let mut info = base(tbc_flag::ON_TRANSPORT);
        info.transport = Some(TbcTransport {
            pose: TransportPose {
                guid: 0x1FC0_0000_0000_2B10,
                pos: v(-5.5, 2.25, 8.0),
                orientation: 1.5,
            },
            time: 123_456,
        });
        // u64 guid, 4 floats, u32 time.
        assert_eq!(info.to_bytes().len(), base(0).to_bytes().len() + 8 + 16 + 4);
        assert_eq!(round(&info), info);
    }

    #[test]
    fn a_spline_elevation_info_ends_in_its_float() {
        let mut info = base(tbc_flag::SPLINE_ELEVATION);
        info.spline_elevation = Some(2.5);
        let bytes = info.to_bytes();
        assert_eq!(&bytes[bytes.len() - 4..], &2.5f32.to_le_bytes());
        assert_eq!(round(&info), info);
    }

    #[test]
    fn the_flag_word_alone_picks_the_blocks() {
        // A set flag with no value writes zeros; a value with no flag is not written.
        let mut with_flag = base(tbc_flag::FALLING);
        with_flag.jump = None;
        assert_eq!(round(&with_flag).jump, Some(JumpInfo::default()));
        let mut no_flag = base(0);
        no_flag.jump = Some(JumpInfo {
            zspeed: 1.0,
            ..JumpInfo::default()
        });
        no_flag.pitch = 1.0;
        assert_eq!(no_flag.to_bytes(), base(0).to_bytes());
    }

    #[test]
    fn the_one_point_one_twelve_pose_converts_with_the_flags_taken_as_is() {
        let old = MovementInfo {
            flags: tbc_flag::FORWARD,
            timestamp: 77,
            position: v(1.0, 2.0, 3.0),
            orientation: 0.25,
            transport: None,
            pitch: 0.0,
            fall_time: 5,
            jump: None,
        };
        let new = TbcMovementInfo::from(&old);
        assert_eq!(
            new,
            TbcMovementInfo {
                fall_time: 5,
                ..TbcMovementInfo::at(tbc_flag::FORWARD, 77, v(1.0, 2.0, 3.0), 0.25)
            }
        );
    }

    #[test]
    fn the_client_bodies_are_the_info_with_a_guid_counter_or_tail_as_the_handler_reads() {
        let info = base(tbc_flag::FORWARD);
        let bytes = info.to_bytes();
        assert_eq!(movement(&info), bytes);
        let nam = not_active_mover(0x709, &info);
        assert_eq!(&nam[..8], &0x709u64.to_le_bytes());
        assert_eq!(&nam[8..], &bytes[..]);
        let spline = move_spline_done(&info, 9);
        assert_eq!(spline.len(), bytes.len() + 4);
        assert_eq!(&spline[bytes.len()..], &9u32.to_le_bytes());
        let speed = force_speed_ack(0x709, 3, &info, 7.0);
        assert_eq!(&speed[..8], &0x709u64.to_le_bytes());
        assert_eq!(&speed[8..12], &3u32.to_le_bytes());
        assert_eq!(&speed[12..12 + bytes.len()], &bytes[..]);
        assert_eq!(&speed[12 + bytes.len()..], &7.0f32.to_le_bytes());
        let hover = move_flag_ack(0x709, 4, &info, Some(true));
        assert_eq!(&hover[hover.len() - 4..], &1u32.to_le_bytes());
        assert_eq!(hover.len(), 12 + bytes.len() + 4);
        assert_eq!(move_flag_ack(0x709, 4, &info, None).len(), 12 + bytes.len());
    }

    fn packed(guid: u64) -> Vec<u8> {
        let mut b = Vec::new();
        write_packed_guid(guid, &mut b).expect("packs");
        b
    }

    fn parse(op: u16, body: &[u8]) -> ServerPacket {
        let mut r = body;
        let p = read_server(op, &mut r)
            .expect("reads")
            .unwrap_or_else(|| panic!("{op:#x} is a movement packet"));
        assert!(r.is_empty(), "{op:#x} reads to the end");
        p
    }

    #[test]
    fn the_relays_read_guid_info_and_their_tail() {
        let info = base(tbc_flag::FORWARD);
        let mut body = packed(0x709);
        info.write(&mut body);
        match parse(t::MSG_MOVE_START_FORWARD, &body) {
            ServerPacket::Tbc(TbcPacket::MoveRelay {
                guid,
                opcode,
                info: got,
                tail,
            }) => {
                assert_eq!(
                    (guid, opcode, tail),
                    (0x709, t::MSG_MOVE_START_FORWARD, RelayTail::None)
                );
                assert_eq!(got, info);
            }
            other => panic!("unexpected {}", other.name()),
        }
        body.extend_from_slice(&7.0f32.to_le_bytes());
        match parse(t::MSG_MOVE_SET_FLIGHT_SPEED, &body) {
            ServerPacket::Tbc(TbcPacket::MoveRelay { tail, .. }) => {
                assert_eq!(tail, RelayTail::Speed(7.0))
            }
            other => panic!("unexpected {}", other.name()),
        }
        // The knock-back relay's tail is four floats after the info.
        let mut knock = packed(0x709);
        info.write(&mut knock);
        for f in [0.6f32, 0.8, 9.0, -4.0] {
            knock.extend_from_slice(&f.to_le_bytes());
        }
        match parse(t::MSG_MOVE_KNOCK_BACK, &knock) {
            ServerPacket::Tbc(TbcPacket::MoveRelay { tail, .. }) => assert_eq!(
                tail,
                RelayTail::Launch {
                    cos_angle: 0.6,
                    sin_angle: 0.8,
                    xy_speed: 9.0,
                    z_speed: -4.0
                }
            ),
            other => panic!("unexpected {}", other.name()),
        }
    }

    #[test]
    fn force_speed_orders_read_with_the_extra_byte_for_run_only() {
        let mut body = packed(0x709);
        body.extend_from_slice(&5u32.to_le_bytes());
        body.extend_from_slice(&0u8.to_le_bytes());
        body.extend_from_slice(&8.5f32.to_le_bytes());
        match parse(t::SMSG_FORCE_RUN_SPEED_CHANGE, &body) {
            ServerPacket::ForceSpeedChange {
                guid,
                kind,
                counter,
                speed,
            } => assert_eq!(
                (guid, kind, counter, speed),
                (0x709, SpeedKind::Run, 5, 8.5)
            ),
            other => panic!("unexpected {}", other.name()),
        }
        let mut walk = packed(0x709);
        walk.extend_from_slice(&6u32.to_le_bytes());
        walk.extend_from_slice(&3.0f32.to_le_bytes());
        match parse(t::SMSG_FORCE_WALK_SPEED_CHANGE, &walk) {
            ServerPacket::ForceSpeedChange { kind, speed, .. } => {
                assert_eq!((kind, speed), (SpeedKind::Walk, 3.0))
            }
            other => panic!("unexpected {}", other.name()),
        }
        match parse(t::SMSG_FORCE_FLIGHT_BACK_SPEED_CHANGE, &walk) {
            ServerPacket::Tbc(TbcPacket::ForceFlightSpeedChange {
                kind,
                counter,
                speed,
                ..
            }) => assert_eq!((kind, counter, speed), (FlightSpeed::FlightBack, 6, 3.0)),
            other => panic!("unexpected {}", other.name()),
        }
    }

    #[test]
    fn the_acked_modes_and_the_observer_modes_read_with_the_1_12_1_meaning() {
        let mut order = packed(0x709);
        order.extend_from_slice(&2u32.to_le_bytes());
        for (op, mode, apply) in [
            (t::SMSG_FORCE_MOVE_ROOT, MoveMode::Root, true),
            (t::SMSG_FORCE_MOVE_UNROOT, MoveMode::Root, false),
            (t::SMSG_MOVE_WATER_WALK, MoveMode::WaterWalk, true),
            (t::SMSG_MOVE_LAND_WALK, MoveMode::WaterWalk, false),
            (t::SMSG_MOVE_FEATHER_FALL, MoveMode::FeatherFall, true),
            (t::SMSG_MOVE_NORMAL_FALL, MoveMode::FeatherFall, false),
            (t::SMSG_MOVE_SET_HOVER, MoveMode::Hover, true),
            (t::SMSG_MOVE_UNSET_HOVER, MoveMode::Hover, false),
        ] {
            match parse(op, &order) {
                ServerPacket::MoveMode {
                    mode: m,
                    apply: a,
                    counter,
                    guid,
                } => assert_eq!((m, a, counter, guid), (mode, apply, 2, 0x709), "{op:#x}"),
                other => panic!("unexpected {}", other.name()),
            }
        }
        match parse(t::SMSG_MOVE_SET_CAN_FLY, &order) {
            ServerPacket::Tbc(TbcPacket::MoveSetCanFly {
                apply: true,
                counter: 2,
                ..
            }) => {}
            other => panic!("unexpected {}", other.name()),
        }
        let bare = packed(0x709);
        for (op, mode, apply) in [
            (t::SMSG_SPLINE_MOVE_ROOT, SplineMode::Root, true),
            (t::SMSG_SPLINE_MOVE_UNROOT, SplineMode::Root, false),
            (
                t::SMSG_SPLINE_MOVE_SET_WALK_MODE,
                SplineMode::WalkMode,
                true,
            ),
            (
                t::SMSG_SPLINE_MOVE_SET_RUN_MODE,
                SplineMode::WalkMode,
                false,
            ),
            (t::SMSG_SPLINE_MOVE_START_SWIM, SplineMode::Swimming, true),
        ] {
            match parse(op, &bare) {
                ServerPacket::SplineMoveMode {
                    mode: m, apply: a, ..
                } => assert_eq!((m, a), (mode, apply), "{op:#x}"),
                other => panic!("unexpected {}", other.name()),
            }
        }
        assert!(matches!(
            parse(t::SMSG_SPLINE_MOVE_UNSET_FLYING, &bare),
            ServerPacket::Tbc(TbcPacket::SplineMoveFlying { apply: false, .. })
        ));
    }

    #[test]
    fn knock_back_teleport_new_world_transfer_and_control_read_as_in_1_12_1() {
        let mut knock = packed(0x709);
        knock.extend_from_slice(&11u32.to_le_bytes());
        for f in [0.6f32, 0.8, 9.0, -4.0] {
            knock.extend_from_slice(&f.to_le_bytes());
        }
        match parse(t::SMSG_MOVE_KNOCK_BACK, &knock) {
            ServerPacket::KnockBack {
                counter, launch, ..
            } => {
                assert_eq!(counter, 11);
                assert_eq!(
                    launch,
                    JumpInfo {
                        zspeed: -4.0,
                        cos_angle: 0.6,
                        sin_angle: 0.8,
                        xy_speed: 9.0
                    }
                );
            }
            other => panic!("unexpected {}", other.name()),
        }
        let mut tele = packed(0x709);
        tele.extend_from_slice(&1u32.to_le_bytes());
        base(0).write(&mut tele);
        match parse(t::MSG_MOVE_TELEPORT_ACK, &tele) {
            ServerPacket::Teleport {
                counter,
                position,
                orientation,
                ..
            } => assert_eq!(
                (counter, position, orientation),
                (1, v(-8949.95, -132.49, 83.53), 1.5)
            ),
            other => panic!("unexpected {}", other.name()),
        }
        let mut world = 530u32.to_le_bytes().to_vec();
        for f in [-1.0f32, 2.0, 3.0, 0.5] {
            world.extend_from_slice(&f.to_le_bytes());
        }
        assert!(matches!(
            parse(t::SMSG_NEW_WORLD, &world),
            ServerPacket::NewWorld { map: 530, .. }
        ));
        assert!(matches!(
            parse(t::SMSG_TRANSFER_PENDING, &530u32.to_le_bytes()),
            ServerPacket::TransferPending {
                map: 530,
                transport: None
            }
        ));
        let mut boat = 530u32.to_le_bytes().to_vec();
        boat.extend_from_slice(&176244u32.to_le_bytes());
        boat.extend_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            parse(t::SMSG_TRANSFER_PENDING, &boat),
            ServerPacket::TransferPending {
                map: 530,
                transport: Some((176244, 0))
            }
        ));
        let mut control = packed(0x709);
        control.push(1);
        assert!(matches!(
            parse(t::SMSG_CLIENT_CONTROL_UPDATE, &control),
            ServerPacket::ClientControlUpdate {
                allow_move: true,
                ..
            }
        ));
        let mut skipped = packed(0x709);
        skipped.extend_from_slice(&40u32.to_le_bytes());
        assert!(matches!(
            parse(t::MSG_MOVE_TIME_SKIPPED, &skipped),
            ServerPacket::MoveTimeSkipped { lag_ms: 40, .. }
        ));
    }

    #[test]
    fn a_non_movement_opcode_is_left_to_the_caller() {
        let mut r: &[u8] = &[];
        assert!(read_server(t::SMSG_MOTD, &mut r)
            .expect("no read")
            .is_none());
    }

    #[test]
    fn the_speed_ack_opcodes_are_the_2_4_3_names() {
        assert_eq!(speed_ack_opcode(SpeedKind::Run), 0xE3);
        assert_eq!(speed_ack_opcode(SpeedKind::TurnRate), 0x2DF);
        assert_eq!(FlightSpeed::Flight.ack_opcode(), 0x382);
        assert_eq!(FlightSpeed::FlightBack.ack_opcode(), 0x384);
        for kind in [
            SpeedKind::Walk,
            SpeedKind::Run,
            SpeedKind::RunBack,
            SpeedKind::Swim,
            SpeedKind::SwimBack,
            SpeedKind::TurnRate,
        ] {
            assert_eq!(
                speed_ack_opcode(kind),
                kind.ack_opcode(),
                "same number as 1.12.1"
            );
        }
    }
}
