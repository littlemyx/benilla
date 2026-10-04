//! The 2.4.3 server-packet dispatch. A packet is read for 2.4.3 only when its layout there has
//! been checked, so this is an allow-list; every other opcode is [`ServerPacket::Other`], never
//! 1.12.1's parser (five opcode numbers changed meaning between the builds).

use std::io;

use crate::wire::{capacity_hint, read_u32_le, read_u8};

use super::parse::read_addon_info;
use super::{opcode, Character, ServerPacket};

/// The billing group of an `AUTH_OK`: `u32` time remaining, `u8` plan flags, `u32` time rested.
const BILLING_GROUP: usize = 9;

fn invalid(what: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

/// Decode one 2.4.3 server body; `cursor` advances past what the arm read only on success.
pub(super) fn parse_tbc_body(opcode: u16, cursor: &mut &[u8]) -> io::Result<ServerPacket> {
    let mut r: &[u8] = cursor;
    let packet = match opcode {
        opcode::SMSG_AUTH_CHALLENGE => ServerPacket::AuthChallenge {
            server_seed: read_u32_le(&mut r)?,
        },
        opcode::SMSG_AUTH_RESPONSE => read_auth_response(&mut r)?,
        opcode::SMSG_CHAR_ENUM => {
            let count = read_u8(&mut r)?;
            let mut characters = Vec::with_capacity(capacity_hint(count, 10));
            for _ in 0..count {
                characters.push(Character::read_tbc(&mut r)?);
            }
            ServerPacket::CharEnum { characters }
        }
        opcode::SMSG_CHAR_CREATE => ServerPacket::CharCreate {
            result: char_create_result(read_u8(&mut r)?),
        },
        opcode::SMSG_ADDON_INFO => ServerPacket::AddonInfo {
            statuses: read_addon_info(&mut r, true),
        },
        other => ServerPacket::Other { opcode: other },
    };
    *cursor = r;
    Ok(packet)
}

/// The 1.12.1 `WorldResult` for the 2.4.3 `SMSG_CHAR_CREATE` result `code`, so the typed result
/// means the same in both builds: 2.4.3 renumbers the enum (one entry is inserted before
/// `CHAR_LIST_RETRIEVING`), and the app reads 1.12.1 numbers. Two-source tables (cmangos-tbc
/// `SharedDefines.h`, wow_messages `world_result.wowm`); the 1.12.1 side is wow_messages, since
/// cmangos-classic marks several of its values unsure. A 2.4.3 code with no 1.12.1 counterpart
/// becomes the generic refusal of its kind, never another code's meaning: a create refused for the
/// race's expansion is `CHAR_CREATE_FAILED`, a name no 1.12.1 rule names is `CHAR_NAME_FAILURE`.
/// The raw code stays available from `WorldSession::last_char_create_code`.
pub fn char_create_result(code: u8) -> u8 {
    const CREATE_FAILED: u8 = 0x30;
    const NAME_FAILURE: u8 = 0x51;
    match code {
        // IN_PROGRESS .. ONLY_EXISTING: 2.4.3 is one above 1.12.1.
        0x2E..=0x38 => code - 1,
        // CHAR_CREATE_EXPANSION: a 2.4.3 rule (the race needs the expansion).
        0x39 => CREATE_FAILED,
        // CHAR_NAME_SUCCESS / FAILURE.
        0x4A => 0x50,
        0x4B => NAME_FAILURE,
        // NO_NAME .. INVALID_SPACE: 1.12.1 numbers them 0x45..=0x4F.
        0x4C..=0x56 => code - 7,
        // CONSECUTIVE_SPACES and the three Russian-name rules: not in the 1.12.1 enum.
        0x57..=0x5A => NAME_FAILURE,
        _ => CREATE_FAILED,
    }
}

/// Parse a `SMSG_CHAR_ENUM` body into full records (guild, first-login flag, every slot with its
/// enchant), for probes; the dispatch above keeps only [`Character`]. `tbc` picks the 2.4.3 form.
pub fn read_char_enum_records(body: &[u8], tbc: bool) -> io::Result<Vec<super::CharRecord>> {
    let mut r = body;
    let count = read_u8(&mut r)?;
    let mut records = Vec::with_capacity(capacity_hint(count, 10));
    for _ in 0..count {
        records.push(Character::read_record(&mut r, tbc)?);
    }
    Ok(records)
}

/// The billing group and the expansion byte that follow it: `(time rested, expansion)`.
fn read_billing_and_expansion(r: &mut &[u8]) -> io::Result<(u32, u8)> {
    let _billing_time_remaining = read_u32_le(r)?;
    let _billing_plan_flags = read_u8(r)?;
    let rested = read_u32_le(r)?;
    Ok((rested, read_u8(r)?))
}

/// `SMSG_AUTH_RESPONSE` in 2.4.3: `u8 code`; `AUTH_OK` adds the billing group and a `u8` expansion
/// (two sources); `AUTH_WAIT_QUEUE` is `u32 position`, which cmangos-tbc precedes, on its first
/// packet, with the billing group and the expansion (one source). The forms are told by length;
/// any other length is an error naming it, since the real client's grammar is not established.
fn read_auth_response(r: &mut &[u8]) -> io::Result<ServerPacket> {
    let result = read_u8(r)?;
    let left = r.len();
    let with_group = BILLING_GROUP + 1;
    let (mut billing_time_rested, mut expansion, mut queue_position) = (None, None, None);
    match result {
        super::AUTH_OK if left == 0 => {}
        super::AUTH_OK if left == with_group => {
            let (rested, exp) = read_billing_and_expansion(r)?;
            (billing_time_rested, expansion) = (Some(rested), Some(exp));
        }
        super::AUTH_WAIT_QUEUE if left == 4 => queue_position = Some(read_u32_le(r)?),
        super::AUTH_WAIT_QUEUE if left == with_group + 4 => {
            let (rested, exp) = read_billing_and_expansion(r)?;
            (billing_time_rested, expansion) = (Some(rested), Some(exp));
            queue_position = Some(read_u32_le(r)?);
        }
        super::AUTH_OK | super::AUTH_WAIT_QUEUE => {
            return Err(invalid(format!(
                "SMSG_AUTH_RESPONSE {result:#04x} with {left} bytes after the code, \
                 expected 0 or 10 (OK), 4 or 14 (queue)"
            )))
        }
        // A refusal is the code alone; anything after it is left unread.
        _ => {}
    }
    Ok(ServerPacket::AuthResponse {
        result,
        queue_position,
        billing_time_rested,
        expansion,
    })
}

#[cfg(test)]
mod tests {
    use benilla_build::{TBC_2_4_3, VANILLA_1_12_1};

    use super::super::{parse_server_for, parse_server_with_tail_for, AUTH_OK, AUTH_WAIT_QUEUE};
    use super::*;

    fn parse(opcode: u16, body: &[u8]) -> io::Result<ServerPacket> {
        parse_server_for(&TBC_2_4_3, None, opcode, body)
    }

    fn billing() -> Vec<u8> {
        let mut b = 0x0102_0304u32.to_le_bytes().to_vec(); // remaining
        b.push(0x08); // plan flags
        b.extend_from_slice(&600u32.to_le_bytes()); // rested
        b
    }

    #[test]
    fn auth_ok_carries_the_billing_group_and_the_expansion_byte() {
        let mut body = vec![AUTH_OK];
        body.extend(billing());
        body.push(1);
        match parse(opcode::SMSG_AUTH_RESPONSE, &body).unwrap() {
            ServerPacket::AuthResponse {
                result,
                queue_position,
                billing_time_rested,
                expansion,
            } => {
                assert_eq!(result, AUTH_OK);
                assert_eq!(queue_position, None);
                assert_eq!(billing_time_rested, Some(600));
                assert_eq!(expansion, Some(1));
            }
            other => panic!("{}", other.name()),
        }
    }

    #[test]
    fn a_queue_packet_is_read_with_or_without_the_billing_prefix() {
        let mut long = vec![AUTH_WAIT_QUEUE];
        long.extend(billing());
        long.push(1);
        long.extend_from_slice(&7u32.to_le_bytes());
        let mut short = vec![AUTH_WAIT_QUEUE];
        short.extend_from_slice(&7u32.to_le_bytes());
        for body in [long, short] {
            match parse(opcode::SMSG_AUTH_RESPONSE, &body).unwrap() {
                ServerPacket::AuthResponse { queue_position, .. } => {
                    assert_eq!(queue_position, Some(7))
                }
                other => panic!("{}", other.name()),
            }
        }
    }

    #[test]
    fn an_unexpected_auth_response_length_is_an_error_naming_it() {
        let err = parse(opcode::SMSG_AUTH_RESPONSE, &[AUTH_OK, 1, 2, 3])
            .err()
            .expect("an error");
        assert!(err.to_string().contains("3 bytes"), "{err}");
    }

    #[test]
    fn a_refusal_is_the_code_alone() {
        match parse(opcode::SMSG_AUTH_RESPONSE, &[0x0D]).unwrap() {
            ServerPacket::AuthResponse {
                result, expansion, ..
            } => {
                assert_eq!(result, 0x0D);
                assert_eq!(expansion, None);
            }
            other => panic!("{}", other.name()),
        }
    }

    /// One 2.4.3 roster entry built from the field list: 58 fixed bytes plus the name, then 20
    /// slots of `u32 display, u8 inventory type, u32 enchant aura`.
    fn tbc_character(name: &str, guid: u64) -> Vec<u8> {
        let mut b = guid.to_le_bytes().to_vec();
        b.extend_from_slice(name.as_bytes());
        b.push(0);
        b.extend_from_slice(&[2, 3, 1]); // race, class, gender
        b.extend_from_slice(&[4, 5, 6, 7, 8]); // skin, face, hair style, hair colour, facial hair
        b.push(60); // level
        b.extend_from_slice(&1519u32.to_le_bytes()); // zone
        b.extend_from_slice(&0u32.to_le_bytes()); // map
        for f in [1.5f32, -2.5, 3.5] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&9u32.to_le_bytes()); // guild id
        b.extend_from_slice(&0x400u32.to_le_bytes()); // flags
        b.push(1); // first login
        for v in [11u32, 12, 13] {
            b.extend_from_slice(&v.to_le_bytes()); // pet display, level, family
        }
        for slot in 0..20u32 {
            b.extend_from_slice(&(100 + slot).to_le_bytes());
            b.push(slot as u8);
            b.extend_from_slice(&(900 + slot).to_le_bytes());
        }
        b
    }

    #[test]
    fn a_char_enum_record_is_238_bytes_plus_the_name_and_reads_through() {
        assert_eq!(tbc_character("Ab", 1).len(), 238 + "Ab".len() + 1);
        let mut body = vec![2u8];
        body.extend(tbc_character("Ab", 0x11));
        body.extend(tbc_character("Cde", 0x22));
        let (packet, tail) =
            parse_server_with_tail_for(&TBC_2_4_3, None, opcode::SMSG_CHAR_ENUM, &body).unwrap();
        assert_eq!(tail, 0, "the whole body is consumed");
        let ServerPacket::CharEnum { characters } = packet else {
            panic!("not a char enum")
        };
        assert_eq!(characters.len(), 2);
        let c = &characters[0];
        assert_eq!((c.guid, c.name.as_str()), (0x11, "Ab"));
        assert_eq!((c.race, c.class, c.gender, c.level), (2, 3, 1, 60));
        assert_eq!(
            (c.skin, c.face, c.hair_style, c.hair_color, c.facial_hair),
            (4, 5, 6, 7, 8)
        );
        assert_eq!((c.zone, c.map, c.flags), (1519, 0, 0x400));
        assert_eq!((c.position.x, c.position.y, c.position.z), (1.5, -2.5, 3.5));
        assert_eq!((c.pet_display_id, c.pet_level, c.pet_family), (11, 12, 13));
        assert_eq!(c.equipment[0].display_id, 100);
        assert_eq!(c.equipment[18].display_id, 118);
        assert_eq!(c.equipment[18].inventory_type, 18);
        assert_eq!(characters[1].name, "Cde");
    }

    #[test]
    fn an_empty_roster_is_a_zero_count() {
        let ServerPacket::CharEnum { characters } = parse(opcode::SMSG_CHAR_ENUM, &[0]).unwrap()
        else {
            panic!("not a char enum")
        };
        assert!(characters.is_empty());
    }

    /// `u8 2, u8 1, u8 0, u32 0, u8 0` per record: the minimal form both servers send.
    fn addon_record() -> Vec<u8> {
        vec![2, 1, 0, 0, 0, 0, 0, 0]
    }

    #[test]
    fn addon_info_reads_with_and_without_the_banned_tail() {
        let records: Vec<u8> = (0..3).flat_map(|_| addon_record()).collect();
        let mut with_tail = records.clone();
        with_tail.extend_from_slice(&0u32.to_le_bytes());
        for body in [records, with_tail] {
            match parse(opcode::SMSG_ADDON_INFO, &body).unwrap() {
                ServerPacket::AddonInfo { statuses } => assert_eq!(statuses, vec![2, 2, 2]),
                other => panic!("{}", other.name()),
            }
        }
    }

    /// The 1.12.1 parser is not reachable from 2.4.3: a number whose meaning changed (0x6B was
    /// `SMSG_IGNORE_LIST`) and an opcode only 1.12.1 reads both come back as `Other`.
    #[test]
    fn everything_off_the_allow_list_is_other() {
        for op in [0x006B, opcode::SMSG_UPDATE_OBJECT, 0x014F, 0x0293] {
            match parse(op, &[0xff; 16]).unwrap() {
                ServerPacket::Other { opcode } => assert_eq!(opcode, op),
                other => panic!("{:#x} read as {}", op, other.name()),
            }
        }
    }

    #[test]
    fn vanilla_without_a_field_table_is_an_error_not_a_panic() {
        let err = parse_server_for(&VANILLA_1_12_1, None, opcode::SMSG_UPDATE_OBJECT, &[])
            .err()
            .expect("an error");
        assert!(err.to_string().contains("5875"), "{err}");
    }

    #[test]
    fn a_char_create_result_is_told_in_1_12_1_numbers() {
        // (2.4.3 code, 1.12.1 code), each pair named by the same enum entry in both tables.
        let table = [
            (0x2E, 0x2D), // IN_PROGRESS
            (0x2F, 0x2E), // SUCCESS
            (0x30, 0x2F), // ERROR
            (0x31, 0x30), // FAILED
            (0x32, 0x31), // NAME_IN_USE
            (0x33, 0x32), // DISABLED
            (0x34, 0x33), // PVP_TEAMS_VIOLATION
            (0x35, 0x34), // SERVER_LIMIT
            (0x36, 0x35), // ACCOUNT_LIMIT
            (0x37, 0x36), // SERVER_QUEUE
            (0x38, 0x37), // ONLY_EXISTING
            (0x39, 0x30), // EXPANSION has no 1.12.1 entry: the generic failure
            (0x4A, 0x50), // NAME_SUCCESS
            (0x4B, 0x51), // NAME_FAILURE
            (0x4C, 0x45), // NAME_NO_NAME
            (0x4D, 0x46), // TOO_SHORT
            (0x4E, 0x47), // TOO_LONG
            (0x4F, 0x48), // INVALID_CHARACTER
            (0x50, 0x49), // MIXED_LANGUAGES
            (0x51, 0x4A), // PROFANE
            (0x52, 0x4B), // RESERVED
            (0x53, 0x4C), // INVALID_APOSTROPHE
            (0x54, 0x4D), // MULTIPLE_APOSTROPHES
            (0x55, 0x4E), // THREE_CONSECUTIVE
            (0x56, 0x4F), // INVALID_SPACE
            (0x57, 0x51), // CONSECUTIVE_SPACES: no 1.12.1 entry
            (0x58, 0x51), // Russian rules: no 1.12.1 entry
            (0x59, 0x51),
            (0x5A, 0x51),
            (0x00, 0x30), // anything else is the generic failure
            (0xFF, 0x30),
        ];
        for (tbc, vanilla) in table {
            assert_eq!(char_create_result(tbc), vanilla, "2.4.3 code {tbc:#04x}");
            match parse(opcode::SMSG_CHAR_CREATE, &[tbc]).unwrap() {
                ServerPacket::CharCreate { result } => assert_eq!(result, vanilla),
                other => panic!("{}", other.name()),
            }
        }
        assert_eq!(char_create_result(0x2F), super::super::CHAR_CREATE_SUCCESS);
        assert_eq!(
            char_create_result(0x32),
            super::super::CHAR_CREATE_NAME_IN_USE
        );
        assert_eq!(
            char_create_result(0x35),
            super::super::CHAR_CREATE_SERVER_LIMIT
        );
    }

    #[test]
    fn the_1_12_1_char_create_result_is_not_renumbered() {
        let packet = parse_server_for(
            &VANILLA_1_12_1,
            Some(&super::super::FIELDS_5875),
            opcode::SMSG_CHAR_CREATE,
            &[0x2F],
        )
        .unwrap();
        match packet {
            ServerPacket::CharCreate { result } => assert_eq!(result, 0x2F),
            other => panic!("{}", other.name()),
        }
    }

    #[test]
    fn full_records_expose_the_20_slots_with_enchants() {
        let mut body = vec![1u8];
        body.extend(tbc_character("Zed", 7));
        let records = read_char_enum_records(&body, true).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].slots.len(), 20);
        assert!(records[0].slots.iter().all(|s| s.enchant_aura_id.is_some()));
        assert_eq!(records[0].character.name, "Zed");
    }

    #[test]
    fn the_char_create_request_is_the_same_bytes_in_both_builds() {
        // The wow_messages test vector (one message tagged for 1.12.1 and 2.4.3).
        let req = super::super::CharCreateReq {
            name: "Deadbeef".into(),
            race: 1,
            class: 1,
            gender: 1,
            skin: 0x08,
            face: 0x00,
            hair_style: 0x0e,
            hair_color: 0x02,
            facial_hair: 0x04,
        };
        let mut want = b"Deadbeef\0".to_vec();
        want.extend_from_slice(&[1, 1, 1, 0x08, 0, 0x0e, 0x02, 0x04, 0]);
        assert_eq!(super::super::char_create(&req), want);
    }
}
