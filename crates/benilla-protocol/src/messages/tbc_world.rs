//! 2.4.3 server packets of world entry that have no 1.12.1 counterpart the app models: a value the
//! 2.4.3 client receives and 1.12.1 never did. They ride [`super::ServerPacket::Tbc`], so no
//! 1.12.1 variant changes shape and no event is made for them yet.

use std::io;

use super::tbc_movement::{FlightSpeed, RelayTail, TbcMovementInfo};
use crate::wire::{
    capacity_hint, read_cstring, read_f32_le, read_i32_le, read_packed_guid, read_u32_le,
    read_u64_le, read_u8,
};

/// One aura's timing as the server tells its caster (`SMSG_SET_EXTRA_AURA_INFO` and its siblings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtraAura {
    /// The aura slot; slots 56 and up hold passives (cmangos-tbc sends them as singles).
    pub slot: u8,
    pub spell_id: u32,
    /// The full duration in ms, `-1` for a permanent aura.
    pub max_duration_ms: i32,
    /// The time left in ms, `0` for a permanent aura.
    pub remaining_ms: u32,
}

/// One entry of the 2.4.3 contact list (friends, ignored, muted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub guid: u64,
    /// `0x01` friend, `0x02` ignored, `0x04` muted.
    pub relation: u32,
    pub note: String,
    /// A friend's online status (`0` offline, `1` online, `2` AFK, `4` DND).
    pub status: Option<u8>,
    /// Area, level and class of an online friend.
    pub online: Option<(u32, u32, u32)>,
}

/// A 2.4.3-only server packet; the variant names follow the 2.4.3 opcode names.
#[derive(Debug, Clone, PartialEq)]
pub enum TbcPacket {
    /// `SMSG_ACCOUNT_DATA_TIMES`: 128 bytes, an MD5 per account-data type (zeros for an empty one).
    AccountDataTimes { data: Vec<u8> },
    /// `SMSG_FEATURE_SYSTEM_STATUS`.
    FeatureSystemStatus {
        complaint_mode: u8,
        voice_chat_enabled: bool,
    },
    /// `SMSG_EXPECTED_SPAM_RECORDS`.
    ExpectedSpamRecords { records: Vec<String> },
    /// `SMSG_MOTD`: the message of the day, one string per line.
    Motd { lines: Vec<String> },
    /// `SMSG_INSTANCE_DIFFICULTY`.
    InstanceDifficulty { difficulty: u32, unknown: u32 },
    /// `MSG_SET_DUNGEON_DIFFICULTY` from the server.
    SetDungeonDifficulty {
        difficulty: u32,
        unknown: u32,
        in_group: bool,
    },
    /// `SMSG_SET_REST_START`.
    SetRestStart { value: u32 },
    /// `SMSG_SEND_UNLEARN_SPELLS`: spells the client forgets.
    SendUnlearnSpells { spells: Vec<u32> },
    /// `SMSG_CONTACT_LIST`: the friend, ignore and mute lists in one packet.
    ContactList {
        list_mask: u32,
        contacts: Vec<Contact>,
    },
    /// `SMSG_LFG_UPDATE`.
    LfgUpdate {
        queued: bool,
        looking_for_group: bool,
        looking_for_more: bool,
        /// Present only while looking for more.
        more: Option<u32>,
    },
    /// `SMSG_INIT_EXTRA_AURA_INFO`: every aura the player cast on `guid`.
    InitExtraAuraInfo { guid: u64, auras: Vec<ExtraAura> },
    /// `SMSG_SET_EXTRA_AURA_INFO`, or `..._NEED_UPDATE` (`need_update`).
    SetExtraAuraInfo {
        guid: u64,
        aura: ExtraAura,
        need_update: bool,
    },
    /// `SMSG_CLEAR_EXTRA_AURA_INFO`.
    ClearExtraAuraInfo { guid: u64, spell_id: u32 },
    /// `SMSG_TIME_SYNC_REQ`: the counter a `CMSG_TIME_SYNC_RESP` echoes.
    TimeSyncRequest { counter: u32 },
    /// A relayed `MSG_MOVE_*` of another mover (`opcode` names which): the movement info in
    /// 2.4.3's flag bits, and the speed or launch that ends a few of them.
    MoveRelay {
        guid: u64,
        opcode: u16,
        info: TbcMovementInfo,
        tail: RelayTail,
    },
    /// `SMSG_FORCE_FLIGHT_SPEED_CHANGE` / `..._BACK_...`: owes the matching ack with `counter`.
    ForceFlightSpeedChange {
        guid: u64,
        kind: FlightSpeed,
        counter: u32,
        speed: f32,
    },
    /// `SMSG_SPLINE_SET_FLIGHT_SPEED` / `..._BACK_SPEED`: a unit we do not control, no ack.
    SplineFlightSpeedChange {
        guid: u64,
        kind: FlightSpeed,
        speed: f32,
    },
    /// `SMSG_MOVE_SET_CAN_FLY` (`apply`) / `SMSG_MOVE_UNSET_CAN_FLY`: owes
    /// `CMSG_MOVE_SET_CAN_FLY_ACK` with `counter`.
    MoveSetCanFly {
        guid: u64,
        counter: u32,
        apply: bool,
    },
    /// `SMSG_SPLINE_MOVE_SET_FLYING` / `..._UNSET_FLYING`: any unit's flying flag, no ack.
    SplineMoveFlying { guid: u64, apply: bool },
    /// `SMSG_CHANNEL_NOTIFY` for the four notices 1.12.1 lacks (`NOT_IN_AREA`, `NOT_IN_LFG`,
    /// `VOICE_ON`, `VOICE_OFF`); `guid` is the voice notices' member.
    ChannelNotice {
        notice: u8,
        channel: String,
        guid: Option<u64>,
    },
    /// `SMSG_USERLIST_ADD` / `UPDATE` / `REMOVE`: a member's change in a channel's roster.
    UserList {
        change: super::tbc_chat::UserListChange,
        guid: u64,
        /// The member's flags; absent on a removal.
        player_flags: Option<u8>,
        channel_flags: u8,
        member_count: u32,
        channel: String,
    },
    /// `SMSG_CHAT_RESTRICTED`: why the chat line was refused (a `ChatRestrictionType`).
    ChatRestricted { reason: u8 },
    /// `SMSG_CAST_RESULT` with a result 1.12.1 has no entry for (`result` is 2.4.3's number).
    CastFailed {
        spell_id: u32,
        result: u8,
        cast_count: u8,
        /// The area, spell focus or equipped item class word of the results that carry one.
        arg: Option<u32>,
    },
    /// `MSG_CHANNEL_START`: `caster` began channelling `spell_id` for `duration_ms`. Sent to every
    /// unit in range, so a 1.12.1 `ChannelStart` (ours alone) cannot stand for it.
    ChannelStart {
        caster: u64,
        spell_id: u32,
        duration_ms: u32,
    },
    /// `MSG_CHANNEL_UPDATE`: `caster`'s channel has `remaining_ms` left (0 ends it).
    ChannelUpdate { caster: u64, remaining_ms: u32 },
    /// `SMSG_INVENTORY_CHANGE_FAILURE` with an `InventoryResult` 1.12.1 lacks (67 and up).
    InventoryChangeFailed {
        result: u8,
        item_guid: u64,
        bag_slot: u8,
    },
}

impl TbcPacket {
    /// The 2.4.3 opcode name.
    pub fn name(&self) -> &'static str {
        match self {
            TbcPacket::AccountDataTimes { .. } => "SMSG_ACCOUNT_DATA_TIMES",
            TbcPacket::FeatureSystemStatus { .. } => "SMSG_FEATURE_SYSTEM_STATUS",
            TbcPacket::ExpectedSpamRecords { .. } => "SMSG_EXPECTED_SPAM_RECORDS",
            TbcPacket::Motd { .. } => "SMSG_MOTD",
            TbcPacket::InstanceDifficulty { .. } => "SMSG_INSTANCE_DIFFICULTY",
            TbcPacket::SetDungeonDifficulty { .. } => "MSG_SET_DUNGEON_DIFFICULTY",
            TbcPacket::SetRestStart { .. } => "SMSG_SET_REST_START",
            TbcPacket::SendUnlearnSpells { .. } => "SMSG_SEND_UNLEARN_SPELLS",
            TbcPacket::ContactList { .. } => "SMSG_CONTACT_LIST",
            TbcPacket::LfgUpdate { .. } => "SMSG_LFG_UPDATE",
            TbcPacket::InitExtraAuraInfo { .. } => "SMSG_INIT_EXTRA_AURA_INFO",
            TbcPacket::SetExtraAuraInfo { need_update, .. } => {
                if *need_update {
                    "SMSG_SET_EXTRA_AURA_INFO_NEED_UPDATE"
                } else {
                    "SMSG_SET_EXTRA_AURA_INFO"
                }
            }
            TbcPacket::ClearExtraAuraInfo { .. } => "SMSG_CLEAR_EXTRA_AURA_INFO",
            TbcPacket::TimeSyncRequest { .. } => "SMSG_TIME_SYNC_REQ",
            TbcPacket::MoveRelay { opcode, .. } => {
                super::tbc_opcode_name(*opcode).unwrap_or("MSG_MOVE_*")
            }
            TbcPacket::ForceFlightSpeedChange { kind, .. } => match kind {
                FlightSpeed::Flight => "SMSG_FORCE_FLIGHT_SPEED_CHANGE",
                FlightSpeed::FlightBack => "SMSG_FORCE_FLIGHT_BACK_SPEED_CHANGE",
            },
            TbcPacket::SplineFlightSpeedChange { kind, .. } => match kind {
                FlightSpeed::Flight => "SMSG_SPLINE_SET_FLIGHT_SPEED",
                FlightSpeed::FlightBack => "SMSG_SPLINE_SET_FLIGHT_BACK_SPEED",
            },
            TbcPacket::MoveSetCanFly { apply, .. } => {
                if *apply {
                    "SMSG_MOVE_SET_CAN_FLY"
                } else {
                    "SMSG_MOVE_UNSET_CAN_FLY"
                }
            }
            TbcPacket::SplineMoveFlying { apply, .. } => {
                if *apply {
                    "SMSG_SPLINE_MOVE_SET_FLYING"
                } else {
                    "SMSG_SPLINE_MOVE_UNSET_FLYING"
                }
            }
            TbcPacket::ChannelNotice { .. } => "SMSG_CHANNEL_NOTIFY",
            TbcPacket::UserList { change, .. } => match change {
                super::tbc_chat::UserListChange::Add => "SMSG_USERLIST_ADD",
                super::tbc_chat::UserListChange::Update => "SMSG_USERLIST_UPDATE",
                super::tbc_chat::UserListChange::Remove => "SMSG_USERLIST_REMOVE",
            },
            TbcPacket::ChatRestricted { .. } => "SMSG_CHAT_RESTRICTED",
            TbcPacket::CastFailed { .. } => "SMSG_CAST_RESULT",
            TbcPacket::ChannelStart { .. } => "MSG_CHANNEL_START",
            TbcPacket::ChannelUpdate { .. } => "MSG_CHANNEL_UPDATE",
            TbcPacket::InventoryChangeFailed { .. } => "SMSG_INVENTORY_CHANGE_FAILURE",
        }
    }
}

/// The account-data block: 8 types of 16 bytes (cmangos-tbc `SendAccountDataTimes`).
const ACCOUNT_DATA_BYTES: usize = 128;

pub(super) fn read_account_data_times(r: &mut &[u8]) -> io::Result<TbcPacket> {
    if r.len() < ACCOUNT_DATA_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!(
                "SMSG_ACCOUNT_DATA_TIMES: {} of {ACCOUNT_DATA_BYTES} bytes",
                r.len()
            ),
        ));
    }
    let (data, rest) = r.split_at(ACCOUNT_DATA_BYTES);
    *r = rest;
    Ok(TbcPacket::AccountDataTimes {
        data: data.to_vec(),
    })
}

pub(super) fn read_feature_system_status(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::FeatureSystemStatus {
        complaint_mode: read_u8(r)?,
        voice_chat_enabled: read_u8(r)? != 0,
    })
}

/// A `u32` count and that many CStrings: the spam records and the MOTD lines share the shape.
fn read_counted_strings(r: &mut &[u8]) -> io::Result<Vec<String>> {
    let count = read_u32_le(r)?;
    // Each string takes at least its terminator, which bounds the count by the bytes left.
    let mut out = Vec::with_capacity(capacity_hint(count, r.len()));
    for _ in 0..count {
        out.push(read_cstring(r)?);
    }
    Ok(out)
}

pub(super) fn read_expected_spam_records(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::ExpectedSpamRecords {
        records: read_counted_strings(r)?,
    })
}

pub(super) fn read_motd(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::Motd {
        lines: read_counted_strings(r)?,
    })
}

pub(super) fn read_instance_difficulty(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::InstanceDifficulty {
        difficulty: read_u32_le(r)?,
        unknown: read_u32_le(r)?,
    })
}

pub(super) fn read_set_dungeon_difficulty(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::SetDungeonDifficulty {
        difficulty: read_u32_le(r)?,
        unknown: read_u32_le(r)?,
        in_group: read_u32_le(r)? != 0,
    })
}

pub(super) fn read_set_rest_start(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::SetRestStart {
        value: read_u32_le(r)?,
    })
}

pub(super) fn read_send_unlearn_spells(r: &mut &[u8]) -> io::Result<TbcPacket> {
    let count = read_u32_le(r)?;
    let mut spells = Vec::with_capacity(capacity_hint(count, r.len() / 4));
    for _ in 0..count {
        spells.push(read_u32_le(r)?);
    }
    Ok(TbcPacket::SendUnlearnSpells { spells })
}

/// The friend bit of a relation mask, which is what puts a status after the note.
const RELATION_FRIEND: u32 = 0x01;

pub(super) fn read_contact_list(r: &mut &[u8]) -> io::Result<TbcPacket> {
    let list_mask = read_u32_le(r)?;
    let count = read_u32_le(r)?;
    let mut contacts = Vec::with_capacity(capacity_hint(count, r.len() / 13));
    for _ in 0..count {
        let guid = read_u64_le(r)?;
        let relation = read_u32_le(r)?;
        let note = read_cstring(r)?;
        let (mut status, mut online) = (None, None);
        if relation & RELATION_FRIEND != 0 {
            let s = read_u8(r)?;
            status = Some(s);
            if s != 0 {
                online = Some((read_u32_le(r)?, read_u32_le(r)?, read_u32_le(r)?));
            }
        }
        contacts.push(Contact {
            guid,
            relation,
            note,
            status,
            online,
        });
    }
    Ok(TbcPacket::ContactList {
        list_mask,
        contacts,
    })
}

pub(super) fn read_lfg_update(r: &mut &[u8]) -> io::Result<TbcPacket> {
    let queued = read_u8(r)? != 0;
    let looking_for_group = read_u8(r)? != 0;
    let looking_for_more = read_u8(r)? != 0;
    let more = if looking_for_more {
        Some(read_u32_le(r)?)
    } else {
        None
    };
    Ok(TbcPacket::LfgUpdate {
        queued,
        looking_for_group,
        looking_for_more,
        more,
    })
}

fn read_extra_aura(r: &mut &[u8]) -> io::Result<ExtraAura> {
    Ok(ExtraAura {
        slot: read_u8(r)?,
        spell_id: read_u32_le(r)?,
        max_duration_ms: read_i32_le(r)?,
        remaining_ms: read_u32_le(r)?,
    })
}

/// An entry is 13 bytes; the list runs to the end of the body.
pub(super) fn read_init_extra_aura_info(r: &mut &[u8]) -> io::Result<TbcPacket> {
    let guid = read_packed_guid(r)?;
    let mut auras = Vec::with_capacity(r.len() / 13);
    while !r.is_empty() {
        auras.push(read_extra_aura(r)?);
    }
    Ok(TbcPacket::InitExtraAuraInfo { guid, auras })
}

pub(super) fn read_set_extra_aura_info(r: &mut &[u8], need_update: bool) -> io::Result<TbcPacket> {
    let guid = read_packed_guid(r)?;
    Ok(TbcPacket::SetExtraAuraInfo {
        guid,
        aura: read_extra_aura(r)?,
        need_update,
    })
}

pub(super) fn read_clear_extra_aura_info(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::ClearExtraAuraInfo {
        guid: read_packed_guid(r)?,
        spell_id: read_u32_le(r)?,
    })
}

pub(super) fn read_time_sync_request(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::TimeSyncRequest {
        counter: read_u32_le(r)?,
    })
}

/// The 2.4.3 `SMSG_WEATHER`: `u32 type`, `f32 grade`, `u8 instant` (no sound id).
pub(super) fn read_weather(r: &mut &[u8]) -> io::Result<(u32, f32, bool)> {
    Ok((read_u32_le(r)?, read_f32_le(r)?, read_u8(r)? != 0))
}
