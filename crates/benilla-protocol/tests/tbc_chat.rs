//! 2.4.3 chat: the client bodies and the server packets 2.4.3 adds. Every fixture is built from the
//! facts tables (cmangos-tbc handlers and builders, agreeing with wow_messages' 2.4.3 definitions);
//! wow_messages ships no 2.4.3 test vector for any of these messages.

use benilla_build::TBC_2_4_3;
use benilla_protocol::messages::{
    self, channel_notice, chat_tag, parse_server_with_tail_for, tbc_opcode as t, ChannelNoticeTail,
    ServerPacket, TbcPacket, UserListChange,
};

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn cstr(s: &str) -> Vec<u8> {
    let mut b = s.as_bytes().to_vec();
    b.push(0);
    b
}

fn len_str(s: &str) -> Vec<u8> {
    let mut b = u32b(s.len() as u32 + 1);
    b.extend(cstr(s));
    b
}

fn tbc(op: u16, body: &[u8]) -> ServerPacket {
    let (packet, tail) = parse_server_with_tail_for(&TBC_2_4_3, None, op, body).unwrap();
    assert_eq!(tail, 0, "{} left {tail} bytes unread", packet.name());
    packet
}

fn tbc_packet(op: u16, body: &[u8]) -> TbcPacket {
    match tbc(op, body) {
        ServerPacket::Tbc(p) => p,
        other => panic!("{} is not a Tbc packet", other.name()),
    }
}

#[test]
fn a_1_12_1_chat_type_goes_out_under_its_2_4_3_number() {
    // (1.12.1 number, 2.4.3 number) for every type a client sends; both sources name these numbers.
    let table: [(u32, u32); 15] = [
        (messages::CHAT_TYPE_SAY, 1),
        (1, 2), // PARTY
        (2, 3), // RAID
        (3, 4), // GUILD
        (4, 5), // OFFICER
        (5, 6), // YELL
        (messages::CHAT_TYPE_WHISPER, 7),
        (8, 0x0A), // EMOTE
        (messages::CHAT_TYPE_CHANNEL, 0x11),
        (0x14, 0x17), // AFK
        (0x15, 0x18), // DND
        (0x57, 0x27), // RAID_LEADER
        (0x58, 0x28), // RAID_WARNING
        (0x5C, 0x2C), // BATTLEGROUND
        (0x5D, 0x2D), // BATTLEGROUND_LEADER
    ];
    for (vanilla, tbc) in table {
        assert_eq!(
            messages::chat_type_to_tbc(vanilla),
            Some(tbc),
            "{vanilla:#x}"
        );
    }
    // The whisper echo is the one number the sources dispute, and 1.12.1's own `0x30` has none.
    assert_eq!(messages::chat_type_to_tbc(7), None);
    assert_eq!(messages::chat_type_to_tbc(0x30), None);
    assert_eq!(messages::chat_type_to_tbc(0x1_0000), None);
}

#[test]
fn the_2_4_3_chat_body_is_type_language_target_text() {
    let say = messages::messagechat_tbc(messages::CHAT_TYPE_SAY, 7, None, "hi").unwrap();
    assert_eq!(say, [1, 0, 0, 0, 7, 0, 0, 0, b'h', b'i', 0]);
    // A whisper names its target before the text (type 7), a channel line its channel (type 0x11).
    let whisper =
        messages::messagechat_tbc(messages::CHAT_TYPE_WHISPER, 1, Some("Bo"), "yo").unwrap();
    assert_eq!(
        whisper,
        [7, 0, 0, 0, 1, 0, 0, 0, b'B', b'o', 0, b'y', b'o', 0]
    );
    let channel =
        messages::messagechat_tbc(messages::CHAT_TYPE_CHANNEL, 0, Some("Trade"), "WTS").unwrap();
    let mut want = vec![0x11, 0, 0, 0, 0, 0, 0, 0];
    want.extend(cstr("Trade"));
    want.extend(cstr("WTS"));
    assert_eq!(channel, want);
    // A channel with no name still writes the empty cstring.
    let nameless = messages::messagechat_tbc(messages::CHAT_TYPE_CHANNEL, 0, None, "x").unwrap();
    assert_eq!(nameless, [0x11, 0, 0, 0, 0, 0, 0, 0, 0, b'x', 0]);
    // Party and guild lines carry no target even when one is passed.
    let party = messages::messagechat_tbc(1, 7, Some("ignored"), "x").unwrap();
    assert_eq!(party, [2, 0, 0, 0, 7, 0, 0, 0, b'x', 0]);
    // A type with no 2.4.3 number is no body.
    assert_eq!(messages::messagechat_tbc(0x30, 7, None, "x"), None);
}

#[test]
fn the_1_12_1_chat_bodies_are_unchanged() {
    assert_eq!(
        messages::messagechat(messages::CHAT_TYPE_SAY, 7, "hi"),
        [0, 0, 0, 0, 7, 0, 0, 0, b'h', b'i', 0]
    );
}

#[test]
fn channel_join_leave_and_ignore_bodies() {
    // join: u32 channel id, two unknown u8, name, password.
    let mut want = vec![0, 0, 0, 0, 0, 0];
    want.extend(cstr("General - Elwynn Forest"));
    want.extend(cstr(""));
    assert_eq!(
        messages::join_channel_tbc("General - Elwynn Forest", ""),
        want
    );
    // leave: u32, name.
    let mut want = vec![0, 0, 0, 0];
    want.extend(cstr("Mine"));
    assert_eq!(messages::leave_channel_tbc("Mine"), want);
    // ignored: the full guid, then the u8.
    let mut want = 0x0102u64.to_le_bytes().to_vec();
    want.push(0);
    assert_eq!(messages::chat_ignored_tbc(0x0102), want);
    // 1.12.1 bodies stay as they were.
    assert_eq!(messages::leave_channel("Mine"), cstr("Mine"));
    assert_eq!(messages::full_guid(0x0102), 0x0102u64.to_le_bytes());
}

fn chat_prefix(ty: u8, language: u32, sender: u64) -> Vec<u8> {
    let mut b = vec![ty];
    b.extend(u32b(language));
    b.extend(sender.to_le_bytes());
    b.extend(u32b(0));
    b
}

#[test]
fn a_gm_line_is_an_ordinary_line_under_its_own_opcode() {
    // The default arm: [channel], target guid, text, tag, then the sender's name when the GM tag is set.
    let mut b = chat_prefix(1, 0, 0x55);
    b.extend(0x55u64.to_le_bytes());
    b.extend(len_str("hello"));
    b.push(0x04);
    b.extend(len_str("Admin"));
    match tbc(t::SMSG_GM_MESSAGECHAT, &b) {
        ServerPacket::MessageChat(m) => {
            assert_eq!(m.chat_type, messages::CHAT_MSG_SAY);
            assert_eq!((m.text.as_str(), m.chat_tag), ("hello", chat_tag::GM));
            assert_eq!(m.sender_name.as_deref(), Some("Admin"));
            assert_eq!(m.sender_guid, 0x55);
        }
        other => panic!("{}", other.name()),
    }
    // A monster line has no trailing name; a type 1.12.1 has no number for stays Other.
    let mut mon = chat_prefix(0x0C, 0, 0x77);
    mon.extend(len_str("Defias"));
    mon.extend(0u64.to_le_bytes());
    mon.extend(len_str("Stop!"));
    mon.push(0);
    assert!(matches!(
        tbc(t::SMSG_GM_MESSAGECHAT, &mon),
        ServerPacket::MessageChat(_)
    ));
    let none = chat_prefix(0x1C, 0, 0);
    assert!(matches!(
        parse_server_with_tail_for(&TBC_2_4_3, None, t::SMSG_GM_MESSAGECHAT, &none).unwrap(),
        (ServerPacket::Other { .. }, 0)
    ));
}

fn notify(notice: u8, channel: &str, tail: &[u8]) -> Vec<u8> {
    let mut b = vec![notice];
    b.extend(cstr(channel));
    b.extend(tail);
    b
}

#[test]
fn a_channel_notice_has_1_12_1s_tail_but_for_you_joined() {
    // JOINED / LEFT: the member's guid.
    match tbc(
        t::SMSG_CHANNEL_NOTIFY,
        &notify(0, "Trade", &9u64.to_le_bytes()),
    ) {
        ServerPacket::ChannelNotify(n) => {
            assert_eq!(n.channel, "Trade");
            assert_eq!(n.tail, ChannelNoticeTail::Guid(9));
        }
        other => panic!("{}", other.name()),
    }
    // YOU_JOINED: u8 flags, u32 channel id, u32 0.
    let mut tail = vec![0x18u8];
    tail.extend(u32b(2));
    tail.extend(u32b(0));
    match tbc(t::SMSG_CHANNEL_NOTIFY, &notify(2, "Trade", &tail)) {
        ServerPacket::ChannelNotify(n) => {
            assert_eq!(n.notice, channel_notice::YOU_JOINED);
            assert_eq!(n.tail, ChannelNoticeTail::YouJoined { flags: 0x18 });
        }
        other => panic!("{}", other.name()),
    }
    // A kick names its target and its source; a name notice a cstring; a mode change two flag bytes.
    let mut kick = 1u64.to_le_bytes().to_vec();
    kick.extend(2u64.to_le_bytes());
    match tbc(t::SMSG_CHANNEL_NOTIFY, &notify(0x12, "c", &kick)) {
        ServerPacket::ChannelNotify(n) => {
            assert_eq!(
                n.tail,
                ChannelNoticeTail::Actors {
                    target: 1,
                    source: 2
                }
            )
        }
        other => panic!("{}", other.name()),
    }
    match tbc(t::SMSG_CHANNEL_NOTIFY, &notify(0x0B, "c", &cstr("Bo"))) {
        ServerPacket::ChannelNotify(n) => {
            assert_eq!(n.tail, ChannelNoticeTail::Name("Bo".into()))
        }
        other => panic!("{}", other.name()),
    }
    let mut mode = 5u64.to_le_bytes().to_vec();
    mode.extend([1, 3]);
    assert!(matches!(
        tbc(t::SMSG_CHANNEL_NOTIFY, &notify(0x0C, "c", &mode)),
        ServerPacket::ChannelNotify(_)
    ));
    // Empty tails.
    assert!(matches!(
        tbc(t::SMSG_CHANNEL_NOTIFY, &notify(5, "c", &[])),
        ServerPacket::ChannelNotify(_)
    ));
    // YOU_LEFT, as cmangos-tbc sent it live: the channel id and a `u8` 0.
    let live = hex_bytes("0362656e696c6c615f70726f6265000000000000");
    match tbc(t::SMSG_CHANNEL_NOTIFY, &live) {
        ServerPacket::ChannelNotify(n) => {
            assert_eq!(n.notice, channel_notice::YOU_LEFT);
            assert_eq!(n.channel, "benilla_probe");
            assert_eq!(n.tail, ChannelNoticeTail::Empty);
        }
        other => panic!("{}", other.name()),
    }
}

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn the_four_added_notices_have_no_1_12_1_form() {
    for notice in [0x20u8, 0x21] {
        assert_eq!(
            tbc_packet(t::SMSG_CHANNEL_NOTIFY, &notify(notice, "LFG", &[])),
            TbcPacket::ChannelNotice {
                notice,
                channel: "LFG".into(),
                guid: None
            }
        );
    }
    for notice in [0x22u8, 0x23] {
        assert_eq!(
            tbc_packet(
                t::SMSG_CHANNEL_NOTIFY,
                &notify(notice, "Trade", &7u64.to_le_bytes())
            ),
            TbcPacket::ChannelNotice {
                notice,
                channel: "Trade".into(),
                guid: Some(7)
            }
        );
    }
    // 1.12.1's last notice is 0x1F; a byte past the new four stays an error, its tail unknown.
    assert!(parse_server_with_tail_for(
        &TBC_2_4_3,
        None,
        t::SMSG_CHANNEL_NOTIFY,
        &notify(0x24, "c", &[])
    )
    .is_err());
}

#[test]
fn the_user_list_packets() {
    let mut add = 0x0102u64.to_le_bytes().to_vec();
    add.extend([0x03, 0x18]);
    add.extend(u32b(5));
    add.extend(cstr("General"));
    for (op, change) in [
        (t::SMSG_USERLIST_ADD, UserListChange::Add),
        (t::SMSG_USERLIST_UPDATE, UserListChange::Update),
    ] {
        assert_eq!(
            tbc_packet(op, &add),
            TbcPacket::UserList {
                change,
                guid: 0x0102,
                player_flags: Some(3),
                channel_flags: 0x18,
                member_count: 5,
                channel: "General".into()
            }
        );
    }
    let mut remove = 0x0102u64.to_le_bytes().to_vec();
    remove.push(0x18);
    remove.extend(u32b(4));
    remove.extend(cstr("General"));
    assert_eq!(
        tbc_packet(t::SMSG_USERLIST_REMOVE, &remove),
        TbcPacket::UserList {
            change: UserListChange::Remove,
            guid: 0x0102,
            player_flags: None,
            channel_flags: 0x18,
            member_count: 4,
            channel: "General".into()
        }
    );
    assert_eq!(
        tbc_packet(t::SMSG_USERLIST_ADD, &add).name(),
        "SMSG_USERLIST_ADD"
    );
}

#[test]
fn chat_restricted_is_one_byte() {
    assert_eq!(
        tbc_packet(t::SMSG_CHAT_RESTRICTED, &[2]),
        TbcPacket::ChatRestricted { reason: 2 }
    );
    assert!(parse_server_with_tail_for(&TBC_2_4_3, None, t::SMSG_CHAT_RESTRICTED, &[]).is_err());
}

#[test]
fn the_2_4_3_names_of_the_chat_opcodes() {
    for (op, name) in [
        (0x0095u16, "CMSG_MESSAGECHAT"),
        (0x0097, "CMSG_JOIN_CHANNEL"),
        (0x0098, "CMSG_LEAVE_CHANNEL"),
        (0x0225, "CMSG_CHAT_IGNORED"),
        (0x02FD, "SMSG_CHAT_RESTRICTED"),
        (0x03B2, "SMSG_GM_MESSAGECHAT"),
        (0x03EF, "SMSG_USERLIST_ADD"),
        (0x03F0, "SMSG_USERLIST_REMOVE"),
        (0x03F1, "SMSG_USERLIST_UPDATE"),
    ] {
        assert_eq!(messages::tbc_opcode_name(op), Some(name));
    }
}
