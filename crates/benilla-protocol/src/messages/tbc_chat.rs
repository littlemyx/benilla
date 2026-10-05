//! Chat on 2.4.3: the client sends that carry a different body than in 1.12.1, and the server
//! packets with no 1.12.1 counterpart. Each layout is cmangos-tbc's (`ChatHandler.cpp`,
//! `ChannelHandler.cpp`, `Channel.cpp`) and agrees with wow_messages' 2.4.3 definition, except where
//! a comment says a part is single-source.

use std::io;

use crate::wire::{read_cstring, read_u32_le, read_u64_le, read_u8};

use super::channel::{channel_notice, read_notice_tail};
use super::{chat_type_from_tbc, ChannelNoticeTail, ChannelNotify, ServerPacket, TbcPacket};

/// 2.4.3 `CHAT_MSG_WHISPER`, which names a target before the text.
const TBC_WHISPER: u32 = 0x07;
/// 2.4.3 `CHAT_MSG_CHANNEL`, which names a channel before the text.
const TBC_CHANNEL: u32 = 0x11;
/// 2.4.3 `CHAT_MSG_WHISPER_INFORM`; the two sources disagree on its number (cmangos-tbc 9,
/// wow_messages 8), so a send never uses it.
const TBC_WHISPER_INFORM: u8 = 0x09;

/// The 2.4.3 chat type number for a 1.12.1 one (the inverse of [`chat_type_from_tbc`], by entry
/// name; both numberings agree between cmangos-tbc and wow_messages for every type a client sends).
/// `None` for a type with no 2.4.3 number or one whose number the sources dispute.
pub fn chat_type_to_tbc(vanilla: u32) -> Option<u32> {
    let vanilla = u8::try_from(vanilla).ok()?;
    (0..=0x2Eu8)
        .find(|&t| t != TBC_WHISPER_INFORM && chat_type_from_tbc(t) == Some(vanilla))
        .map(u32::from)
}

/// The 2.4.3 `CMSG_MESSAGECHAT` body: `u32` type (2.4.3's numbers), `u32` language, the target
/// cstring for a whisper or the channel cstring for a channel line, then the text cstring.
/// `chat_type` is 1.12.1's number; `None` when it has no sendable 2.4.3 number.
pub fn messagechat_tbc(
    chat_type: u32,
    language: u32,
    target: Option<&str>,
    message: &str,
) -> Option<Vec<u8>> {
    let ty = chat_type_to_tbc(chat_type)?;
    let named = matches!(ty, TBC_WHISPER | TBC_CHANNEL).then(|| target.unwrap_or_default());
    Some(super::messagechat_kind(ty, language, named, message))
}

/// The 2.4.3 `CMSG_JOIN_CHANNEL` body: `u32` channel id, two `u8`s (both unknown, 0), the channel
/// name and the password. The id is the `ChatChannels.dbc` row of a built-in channel, 0 for a name
/// alone.
pub fn join_channel_tbc(name: &str, password: &str) -> Vec<u8> {
    let mut body = vec![0; 6];
    body.extend_from_slice(name.as_bytes());
    body.push(0);
    body.extend_from_slice(password.as_bytes());
    body.push(0);
    body
}

/// The 2.4.3 `CMSG_LEAVE_CHANNEL` body: a `u32` (the channel id, 0 here) before the name.
pub fn leave_channel_tbc(name: &str) -> Vec<u8> {
    let mut body = vec![0; 4];
    body.extend_from_slice(name.as_bytes());
    body.push(0);
    body
}

/// The 2.4.3 `CMSG_CHAT_IGNORED` body: the ignoring player's guid, then a `u8` (spam-report related
/// in cmangos-tbc, read and unused; 0 here).
pub fn chat_ignored_tbc(guid: u64) -> Vec<u8> {
    let mut body = guid.to_le_bytes().to_vec();
    body.push(0);
    body
}

/// Read a 2.4.3 `SMSG_CHANNEL_NOTIFY`. Notices 0..=31 are 1.12.1's with the same tails except
/// `YOU_JOINED` (a `u8` flags byte, the `u32` channel id, a `u32` 0: cmangos-tbc `MakeYouJoined`,
/// wow_messages models no tail) and `YOU_LEFT` (a `u32` id and a `u8`); 32..=35 are new in 2.4.3.
pub(super) fn read_channel_notify_tbc(r: &mut &[u8]) -> io::Result<ServerPacket> {
    let notice = read_u8(r)?;
    let channel = read_cstring(r)?;
    Ok(match notice {
        channel_notice::YOU_JOINED => {
            let flags = u32::from(read_u8(r)?);
            // The channel id, and the index that is 0 until a channel splits: not modelled.
            let _id = read_u32_le(r)?;
            let _index = read_u32_le(r)?;
            ServerPacket::ChannelNotify(ChannelNotify {
                notice,
                channel,
                tail: ChannelNoticeTail::YouJoined { flags },
            })
        }
        // `MakeYouLeft` ends the name with the channel id and a `u8` 0 (left) or 1 (suspended).
        channel_notice::YOU_LEFT => {
            let _id = read_u32_le(r)?;
            let _suspended = read_u8(r)?;
            ServerPacket::ChannelNotify(ChannelNotify {
                notice,
                channel,
                tail: ChannelNoticeTail::Empty,
            })
        }
        // Not in the area, not queued for the LFG channel: the name alone.
        TBC_NOT_IN_AREA | TBC_NOT_IN_LFG => ServerPacket::Tbc(TbcPacket::ChannelNotice {
            notice,
            channel,
            guid: None,
        }),
        TBC_VOICE_ON | TBC_VOICE_OFF => ServerPacket::Tbc(TbcPacket::ChannelNotice {
            notice,
            channel,
            guid: Some(read_u64_le(r)?),
        }),
        _ => ServerPacket::ChannelNotify(read_notice_tail(notice, channel, r)?),
    })
}

/// 2.4.3's four added `ChatNotify` values (cmangos-tbc `Channel.h`, wow_messages `ChatNotify`).
pub const TBC_NOT_IN_AREA: u8 = 0x20;
pub const TBC_NOT_IN_LFG: u8 = 0x21;
pub const TBC_VOICE_ON: u8 = 0x22;
pub const TBC_VOICE_OFF: u8 = 0x23;

/// Which of the three user-list packets a [`TbcPacket::UserList`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserListChange {
    /// `SMSG_USERLIST_ADD`: a member joined a built-in (constant) channel.
    Add,
    /// `SMSG_USERLIST_UPDATE`: a member joined a custom channel or changed its flags.
    Update,
    /// `SMSG_USERLIST_REMOVE`: a member left.
    Remove,
}

/// Read `SMSG_USERLIST_ADD` / `UPDATE` (guid, the member's flags, the channel's flags, the member
/// count, the channel) or `REMOVE` (the same without the member's flags).
pub(super) fn read_userlist(change: UserListChange, r: &mut &[u8]) -> io::Result<TbcPacket> {
    let guid = read_u64_le(r)?;
    let player_flags = match change {
        UserListChange::Remove => None,
        _ => Some(read_u8(r)?),
    };
    let channel_flags = read_u8(r)?;
    let member_count = read_u32_le(r)?;
    let channel = read_cstring(r)?;
    Ok(TbcPacket::UserList {
        change,
        guid,
        player_flags,
        channel_flags,
        member_count,
        channel,
    })
}

/// Read `SMSG_CHAT_RESTRICTED`: one `u8` reason.
pub(super) fn read_chat_restricted(r: &mut &[u8]) -> io::Result<TbcPacket> {
    Ok(TbcPacket::ChatRestricted {
        reason: read_u8(r)?,
    })
}
