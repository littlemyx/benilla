use crate::messages::tbc_movement as tbc;
use crate::messages::{self, JumpInfo, MovementInfo, TransportPose};
use crate::wire::Vector3d;

pub(super) const MOVEMENT_FLAG_FORWARD: u32 = 0x1;

/// Ms since start for the `MovementInfo` time, as the 1.12 client's `GetTickCount()`, never 0:
/// vmangos pauses extrapolation on 0 and otherwise uses only deltas, so a full `u32` wrap is safe.
pub(super) fn client_uptime_ms() -> u32 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    (START.get_or_init(Instant::now).elapsed().as_millis() as u32).max(1)
}

/// Build a `MovementInfo` stamped with [`client_uptime_ms`].
pub(super) fn movement_info(pos: [f32; 3], orientation: f32, flags: u32) -> MovementInfo {
    MovementInfo {
        flags,
        timestamp: client_uptime_ms(),
        position: Vector3d {
            x: pos[0],
            y: pos[1],
            z: pos[2],
        },
        orientation,
        // Written only under ON_TRANSPORT, which benilla does not send.
        transport: None,
        // Written only under SWIMMING; `send_movement` supplies the live pitch then.
        pitch: 0.0,
        fall_time: 0,
        jump: None,
    }
}

// The bodies of the movement sends, in the session's build: every verb builds a 1.12.1-shaped pose
// and this picks the layout. On 2.4.3 `flags` are taken as that build's bits (`tbc_flag`).

/// A `MSG_MOVE_*` body.
pub(super) fn movement_body(tbc_build: bool, info: &MovementInfo) -> Vec<u8> {
    if tbc_build {
        tbc::movement(&tbc::TbcMovementInfo::from(info))
    } else {
        messages::movement(info)
    }
}

/// A force-speed ack body.
pub(super) fn force_speed_ack_body(
    tbc_build: bool,
    guid: u64,
    counter: u32,
    info: &MovementInfo,
    speed: f32,
) -> Vec<u8> {
    if tbc_build {
        tbc::force_speed_ack(guid, counter, &tbc::TbcMovementInfo::from(info), speed)
    } else {
        messages::force_speed_ack(guid, counter, info, speed)
    }
}

/// A mode ack body; `applied` is the trailing word of every mode but root and knock-back.
pub(super) fn move_flag_ack_body(
    tbc_build: bool,
    guid: u64,
    counter: u32,
    info: &MovementInfo,
    applied: Option<bool>,
) -> Vec<u8> {
    if tbc_build {
        tbc::move_flag_ack(guid, counter, &tbc::TbcMovementInfo::from(info), applied)
    } else {
        messages::move_flag_ack(guid, counter, info, applied)
    }
}

/// A `CMSG_MOVE_SPLINE_DONE` body: the 2.4.3 form ends in the counter alone.
pub(super) fn spline_done_body(tbc_build: bool, info: &MovementInfo, spline_id: u32) -> Vec<u8> {
    if tbc_build {
        tbc::move_spline_done(&tbc::TbcMovementInfo::from(info), spline_id)
    } else {
        messages::move_spline_done(info, spline_id)
    }
}

/// A `CMSG_MOVE_NOT_ACTIVE_MOVER` body.
pub(super) fn not_active_mover_body(tbc_build: bool, guid: u64, info: &MovementInfo) -> Vec<u8> {
    if tbc_build {
        tbc::not_active_mover(guid, &tbc::TbcMovementInfo::from(info))
    } else {
        let mut body = messages::full_guid(guid);
        body.extend_from_slice(&messages::movement(info));
        body
    }
}

/// A pose with its optional blocks filled, for the verbs that carry them.
pub(super) fn full_info(
    pos: [f32; 3],
    orientation: f32,
    flags: u32,
    pitch: f32,
    fall_time: u32,
    jump: Option<JumpInfo>,
    transport: Option<TransportPose>,
) -> MovementInfo {
    let mut info = movement_info(pos, orientation, flags);
    // Each tail is written only when its flag is set, so flag and value must agree.
    info.pitch = pitch;
    info.fall_time = fall_time;
    info.jump = jump;
    info.transport = transport;
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Never 0, and a stamp taken right after is not smaller; it wraps at `u32::MAX` ms.
    #[test]
    fn client_uptime_ms_is_nonzero_and_back_to_back_calls_do_not_decrease() {
        let a = client_uptime_ms();
        let b = client_uptime_ms();
        assert!(a >= 1, "stamp is non-zero: {a}");
        assert!(b >= a, "the next stamp is not smaller: {a} -> {b}");
    }

    #[test]
    fn movement_info_is_stamped_nonzero() {
        let info = movement_info([1.0, 2.0, 3.0], 0.5, MOVEMENT_FLAG_FORWARD);
        assert_ne!(
            info.timestamp, 0,
            "a movement packet must carry a non-zero time"
        );
    }

    fn pose(flags: u32) -> MovementInfo {
        let mut info = movement_info([-8949.95, -132.49, 83.53], 1.5, flags);
        info.timestamp = 0x0102_0304;
        info
    }

    /// The 1.12.1 selection returns exactly the bytes of the 1.12.1 builders.
    #[test]
    fn the_1_12_1_bodies_are_the_old_builders_bytes() {
        let info = pose(MOVEMENT_FLAG_FORWARD);
        assert_eq!(movement_body(false, &info), messages::movement(&info));
        assert_eq!(
            force_speed_ack_body(false, 7, 2, &info, 7.0),
            messages::force_speed_ack(7, 2, &info, 7.0)
        );
        assert_eq!(
            move_flag_ack_body(false, 7, 2, &info, Some(true)),
            messages::move_flag_ack(7, 2, &info, Some(true))
        );
        assert_eq!(
            spline_done_body(false, &info, 9),
            messages::move_spline_done(&info, 9)
        );
        let mut nam = messages::full_guid(7);
        nam.extend_from_slice(&messages::movement(&info));
        assert_eq!(not_active_mover_body(false, 7, &info), nam);
    }

    /// 2.4.3 adds the extra flags byte and the fall-time word stays; a spline-done body ends in
    /// the counter alone.
    #[test]
    fn the_2_4_3_bodies_add_the_flags_byte_and_drop_the_spline_floats_tail() {
        let info = pose(MOVEMENT_FLAG_FORWARD);
        let old = movement_body(false, &info);
        let new = movement_body(true, &info);
        assert_eq!(new.len(), old.len() + 1);
        assert_eq!(&new[..4], &old[..4], "the flags word");
        assert_eq!(new[4], 0, "flags2");
        assert_eq!(
            &new[5..],
            &old[4..],
            "time, position, facing, fall time follow"
        );
        assert_eq!(
            spline_done_body(true, &info, 9).len(),
            new.len() + 4,
            "info and the counter, no float"
        );
        assert_eq!(
            spline_done_body(false, &info, 9).len(),
            old.len() + 8,
            "1.12.1: info, id and the float"
        );
        let ack = force_speed_ack_body(true, 7, 2, &info, 7.0);
        assert_eq!(ack.len(), 8 + 4 + new.len() + 4);
        assert_eq!(&ack[12..12 + new.len()], &new[..]);
        let mode = move_flag_ack_body(true, 7, 2, &info, Some(true));
        assert_eq!(&mode[mode.len() - 4..], &1u32.to_le_bytes());
    }

    /// A 2.4.3 pose with the jump block rides `FALLING`, a 1.12.1 one rides `JUMPING`.
    #[test]
    fn the_jump_block_rides_the_builds_own_flag() {
        use crate::messages::tbc_flag;
        let jump = JumpInfo {
            zspeed: -7.9,
            cos_angle: 1.0,
            sin_angle: 0.0,
            xy_speed: 7.0,
        };
        let falling = full_info([0.0; 3], 0.0, tbc_flag::FALLING, 0.0, 300, Some(jump), None);
        let plain = pose(0);
        assert_eq!(
            movement_body(true, &falling).len(),
            movement_body(true, &plain).len() + 16
        );
        // The same bit means nothing to 1.12.1: no jump block follows it there.
        assert_eq!(
            movement_body(false, &falling).len(),
            movement_body(false, &plain).len()
        );
    }
}
