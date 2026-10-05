//! The verbs with a 2.4.3 form send it on a 2.4.3 session and 1.12.1's bytes on a 1.12.1 one:
//! what reaches the socket, as `(opcode, body length)`. The bodies themselves are pinned in the
//! tests of their builders.

use std::net::{TcpListener, TcpStream};

use benilla_srp::vanilla_header::HeaderCrypto;

use super::WorldWriter;
use crate::messages::{self, opcode};

fn writer(tbc: bool) -> (WorldWriter, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (peer, _) = listener.accept().unwrap();
    let (encrypter, _) = HeaderCrypto::from_session_key([7; 40]).split();
    let w = WorldWriter {
        stream,
        encrypter,
        sent: Some(Vec::new()),
        chat_language: 7,
        tbc,
    };
    (w, peer)
}

fn sent(w: &WorldWriter) -> Vec<(u16, usize)> {
    w.sent.clone().unwrap()
}

#[test]
fn the_chat_and_channel_verbs_send_their_2_4_3_bodies() {
    let (mut t, _a) = writer(true);
    let (mut v, _b) = writer(false);
    for w in [&mut t, &mut v] {
        w.send_chat("hi").unwrap();
        w.send_message_chat(messages::CHAT_TYPE_WHISPER, None, Some("Bo"), "yo")
            .unwrap();
        w.send_addon_message(messages::CHAT_TYPE_PARTY, "P\tx")
            .unwrap();
        w.chat_ignored(5).unwrap();
        w.join_channel("a", "").unwrap();
        w.leave_channel("a").unwrap();
    }
    // Chat bodies keep their length (only the type number moves); the channel and ignore bodies
    // gain the 2.4.3 fields.
    let chat = [
        (opcode::CMSG_MESSAGECHAT, 11),
        (opcode::CMSG_MESSAGECHAT, 14),
        (opcode::CMSG_MESSAGECHAT, 12),
    ];
    let t_sent = sent(&t);
    assert_eq!(&t_sent[..3], &chat);
    assert_eq!(&sent(&v)[..3], &chat);
    assert_eq!(
        &t_sent[3..],
        &[
            (opcode::CMSG_CHAT_IGNORED, 9),
            (opcode::CMSG_JOIN_CHANNEL, 6 + 2 + 1),
            (opcode::CMSG_LEAVE_CHANNEL, 4 + 2),
        ]
    );
    assert_eq!(
        &sent(&v)[3..],
        &[
            (opcode::CMSG_CHAT_IGNORED, 8),
            (opcode::CMSG_JOIN_CHANNEL, 2 + 1),
            (opcode::CMSG_LEAVE_CHANNEL, 2),
        ]
    );
}

#[test]
fn a_chat_type_with_no_2_4_3_number_is_an_error_on_2_4_3_only() {
    let (mut t, _a) = writer(true);
    let (mut v, _b) = writer(false);
    // 1.12.1's whisper echo (7) is a server type a client never sends.
    assert!(t
        .send_message_chat(7, None, None, "x")
        .unwrap_err()
        .to_string()
        .contains("no 2.4.3 number"));
    assert!(sent(&t).is_empty());
    v.send_message_chat(7, None, None, "x").unwrap();
}
