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
        tbc_state: Default::default(),
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

#[test]
fn the_slot_verbs_renumber_for_2_4_3_and_refuse_a_slot_it_has_no_number_for() {
    let (mut t, _a) = writer(true);
    let (mut v, _b) = writer(false);
    // Bank-bag, buyback and keyring slots exist on both; the bytes keep their length.
    for w in [&mut t, &mut v] {
        w.swap_inv_item(63, 81).unwrap();
        w.destroy_item(255, 69, 1).unwrap();
        w.buyback_item(5, 70).unwrap();
        w.repair_item(5, 0).unwrap();
    }
    assert_eq!(
        sent(&t),
        [
            (opcode::CMSG_SWAP_INV_ITEM, 2),
            (opcode::CMSG_DESTROYITEM, 6),
            (opcode::CMSG_BUYBACK_ITEM, 12),
            (opcode::CMSG_REPAIR_ITEM, 17),
        ]
    );
    assert_eq!(sent(&v)[3], (opcode::CMSG_REPAIR_ITEM, 16));
    // A slot past the keyring's 96 is an error on 2.4.3 and sent as before on 1.12.1.
    assert!(t
        .swap_inv_item(100, 23)
        .unwrap_err()
        .to_string()
        .contains("no 2.4.3 number"));
    assert!(t.buyback_item(5, 300).is_err());
    assert_eq!(sent(&t).len(), 4);
    v.swap_inv_item(100, 23).unwrap();
}

#[test]
fn the_cast_family_sends_the_2_4_3_form_with_a_counting_cast_count() {
    let (mut t, _a) = writer(true);
    let (mut v, _b) = writer(false);
    for w in [&mut t, &mut v] {
        w.cast_spell(133, None).unwrap();
        w.cast_spell(78, Some(5)).unwrap();
        w.cast_spell_gameobject(1, 5).unwrap();
        w.use_item(255, 24, 0, messages::UseItemTarget::SelfImplicit, 0xA01)
            .unwrap();
        w.gossip_select_option(9, 2, None).unwrap();
    }
    // 2.4.3 adds a count and two flag bytes to a cast, a count and a u64 guid and the two flag
    // bytes to an item use, a menu id to a gossip selection; 1.12.1 keeps its bytes.
    assert_eq!(
        sent(&t),
        [
            (opcode::CMSG_CAST_SPELL, 9),
            (opcode::CMSG_CAST_SPELL, 11),
            (opcode::CMSG_CAST_SPELL, 11),
            (opcode::CMSG_USE_ITEM, 16),
            (opcode::CMSG_GOSSIP_SELECT_OPTION, 16),
        ]
    );
    assert_eq!(
        sent(&v),
        [
            (opcode::CMSG_CAST_SPELL, 6),
            (opcode::CMSG_CAST_SPELL, 8),
            (opcode::CMSG_CAST_SPELL, 8),
            (opcode::CMSG_USE_ITEM, 5),
            (opcode::CMSG_GOSSIP_SELECT_OPTION, 12),
        ]
    );
    // One counter for casts and item uses: four sends, count 4; 1.12.1 never counts.
    assert_eq!(t.tbc_state.cast_count, 4);
    assert_eq!(v.tbc_state.cast_count, 0);
}
