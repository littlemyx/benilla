//! The writer disposition: every verb has an entry, a refused verb is refused on 2.4.3 only, and
//! the 2.4.3 numbers the verbs with their own form send are never on the refusal list.

use std::net::{TcpListener, TcpStream};

use benilla_srp::vanilla_header::HeaderCrypto;

use super::{form_of, refusal_on_tbc, Form, VerbRefused, WorldWriter, VERBS};
use crate::messages::{opcode, tbc_opcode};

/// A writer over a loopback socket, on 2.4.3 or 1.12.1; the peer end is kept so the socket lives.
fn writer(tbc: bool) -> (WorldWriter, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (peer, _) = listener.accept().unwrap();
    let (encrypter, _) = HeaderCrypto::from_session_key([7; 40]).split();
    let w = WorldWriter {
        stream,
        encrypter,
        sent: Some(Vec::new()),
        chat_language: 0,
        tbc,
        tbc_state: Default::default(),
    };
    (w, peer)
}

/// Every `pub fn` of the writer's verb files, by name.
fn verbs_in_sources() -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/world/writer");
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let file = path.file_name().unwrap().to_str().unwrap().to_string();
        if path.extension().is_none_or(|e| e != "rs") || file == "mod.rs" {
            continue;
        }
        if file.starts_with("disposition") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let mut rest = text.as_str();
        while let Some(at) = rest.find("pub fn ") {
            let after = &rest[at + 7..];
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap();
            names.push(after[..end].to_string());
            rest = &after[end..];
        }
    }
    names
}

#[test]
fn every_verb_has_exactly_one_entry_and_no_entry_is_stale() {
    let in_sources = verbs_in_sources();
    assert!(
        in_sources.len() > 200,
        "found only {} verbs",
        in_sources.len()
    );
    for verb in &in_sources {
        let n = VERBS.iter().filter(|v| v.verb == verb).count();
        assert_eq!(n, 1, "verb `{verb}` has {n} disposition entries");
    }
    for v in VERBS {
        assert!(
            in_sources.iter().any(|n| n == v.verb),
            "entry `{}` names no verb",
            v.verb
        );
    }
}

#[test]
fn the_three_dispositions_are_all_in_use_and_a_refused_verb_names_an_opcode() {
    for form in [Form::Same, Form::Has243Form, Form::NotEstablished] {
        assert!(VERBS.iter().any(|v| v.form == form), "{form:?} is unused");
    }
    for v in VERBS.iter().filter(|v| v.form == Form::NotEstablished) {
        assert!(!v.opcodes.is_empty(), "`{}` sends no known opcode", v.verb);
    }
}

#[test]
fn an_unestablished_verb_is_refused_on_2_4_3_and_sent_unchanged_on_1_12_1() {
    let (mut tbc, _peer_a) = writer(true);
    let (mut vanilla, _peer_b) = writer(false);
    let mut tried = 0;
    for v in VERBS.iter().filter(|v| v.form == Form::NotEstablished) {
        for &op in v.opcodes {
            let err = tbc.send(op, &[1, 2, 3]).unwrap_err();
            let refused = err.downcast_ref::<VerbRefused>().expect("a VerbRefused");
            assert_eq!(refused.opcode, op);
            assert!(err.to_string().contains(refused.verb), "{err}");
            assert!(
                v.opcodes.contains(&op) && refusal_on_tbc(op) == Some(*refused),
                "{op:#x}"
            );
            vanilla.send(op, &[1, 2, 3]).unwrap();
            tried += 1;
        }
    }
    assert!(tried >= 10);
    assert!(
        tbc.sent.as_ref().unwrap().is_empty(),
        "a refused verb reached the socket"
    );
    assert_eq!(vanilla.sent.as_ref().unwrap().len(), tried);
    assert!(vanilla
        .sent
        .as_ref()
        .unwrap()
        .iter()
        .all(|&(_, len)| len == 3));
}

#[test]
fn a_same_verb_and_a_verb_with_its_own_form_send_on_2_4_3() {
    let (mut tbc, _peer) = writer(true);
    for v in VERBS.iter().filter(|v| v.form != Form::NotEstablished) {
        for &op in v.opcodes {
            tbc.send(op, &[])
                .unwrap_or_else(|e| panic!("`{}`: {e}", v.verb));
        }
    }
    assert!(tbc.sent.as_ref().unwrap().len() > 150);
}

#[test]
fn the_numbers_of_the_2_4_3_forms_are_not_on_the_refusal_list() {
    // The movement verbs pick their opcode by argument; every number of that family, in either
    // build, is outside the refused set, and so are the 2.4.3-only numbers a verb sends.
    for op in [
        tbc_opcode::CMSG_MOVE_SET_CAN_FLY_ACK,
        tbc_opcode::CMSG_TIME_SYNC_RESP,
    ] {
        assert_eq!(refusal_on_tbc(op), None, "{op:#x}");
    }
    for op in 0..0x500u16 {
        let moves = crate::messages::opcode_name(op)
            .into_iter()
            .chain(crate::messages::tbc_opcode_name(op))
            .any(|n| {
                n.starts_with("MSG_MOVE_")
                    || n.starts_with("CMSG_MOVE_")
                    || n.starts_with("CMSG_FORCE_")
            });
        if moves {
            assert_eq!(refusal_on_tbc(op), None, "{op:#x} is a movement number");
        }
    }
}

#[test]
fn the_item_query_has_its_2_4_3_form() {
    assert_eq!(form_of("item_query"), Some(Form::Has243Form));
    let (mut tbc, _a) = writer(true);
    let (mut vanilla, _b) = writer(false);
    tbc.item_query(25, 0).unwrap();
    vanilla.item_query(25, 0).unwrap();
    assert_eq!(
        tbc.sent.as_ref().unwrap().as_slice(),
        &[(opcode::CMSG_ITEM_QUERY_SINGLE, 4)]
    );
    assert_eq!(
        vanilla.sent.as_ref().unwrap().as_slice(),
        &[(opcode::CMSG_ITEM_QUERY_SINGLE, 12)]
    );
}
