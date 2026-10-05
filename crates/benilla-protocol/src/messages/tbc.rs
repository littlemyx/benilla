//! The 2.4.3 server-packet dispatch. A packet is read for 2.4.3 only when its layout there has
//! been checked, so this is an allow-list; every other opcode is [`ServerPacket::Other`], never
//! 1.12.1's parser (five opcode numbers changed meaning between the builds). A packet whose bytes
//! are the same in both builds is read here by its own arm, or by the 1.12.1 arm that the generated
//! `tbc_same` table names.

use std::io::{self, Read};

use crate::wire::{
    capacity_hint, read_cstring, read_f32_le, read_i32_le, read_u16_le, read_u32_le, read_u64_le,
    read_u8, Vector3d,
};

use super::parse::read_addon_info;
use super::{opcode, tbc_opcode, tbc_world, update_object, Character, ServerPacket};

/// The billing group of an `AUTH_OK`: `u32` time remaining, `u8` plan flags, `u32` time rested.
const BILLING_GROUP: usize = 9;

fn invalid(what: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

/// Decode one 2.4.3 server body; `cursor` advances past what the arm read only on success.
/// `inner_tail` takes the unread rest of an inflated update object, the one arm with a second stream.
pub(super) fn parse_tbc_body(
    opcode: u16,
    cursor: &mut &[u8],
    inner_tail: &mut usize,
) -> io::Result<ServerPacket> {
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
        // Same bytes as 1.12.1 (checked in both emulators); the update blocks differ inside.
        opcode::SMSG_CHARACTER_LOGIN_FAILED => ServerPacket::CharacterLoginFailed {
            result: read_u8(&mut r)?,
        },
        opcode::SMSG_LOGIN_VERIFY_WORLD => ServerPacket::LoginVerifyWorld {
            map: read_u32_le(&mut r)?,
            position: Vector3d::read(&mut r)?,
            orientation: read_f32_le(&mut r)?,
        },
        opcode::SMSG_LOGOUT_COMPLETE => ServerPacket::LogoutComplete,
        opcode::SMSG_LOGOUT_RESPONSE => ServerPacket::LogoutResponse {
            reason: read_u32_le(&mut r)?,
            instant: read_u8(&mut r)? != 0,
        },
        opcode::SMSG_UPDATE_OBJECT => ServerPacket::UpdateObject {
            objects: update_object::read_update_object_tbc(&mut r)?,
        },
        opcode::SMSG_COMPRESSED_UPDATE_OBJECT => {
            let _decompressed_size = read_u32_le(&mut r)?;
            // `&mut r` so the cursor moves over the zlib bytes; `bufread`, so the decoder takes
            // no more than its stream.
            let mut decoder = flate2::bufread::ZlibDecoder::new(&mut r);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            drop(decoder);
            let mut dr = decompressed.as_slice();
            let objects = update_object::read_update_object_tbc(&mut dr)?;
            *inner_tail = dr.len();
            ServerPacket::UpdateObject { objects }
        }
        other => parse_tbc_entry_body(other, &mut r)?,
    };
    *cursor = r;
    Ok(packet)
}

/// The 2.4.3 arms of world entry, the queries and time sync. Every arm is named by
/// the 2.4.3 opcode; a number whose 1.12.1 meaning differs (0x67, 0x33A, 0x33B) is read by its
/// 2.4.3 meaning, and one this table does not hold is `Other`.
fn parse_tbc_entry_body(op: u16, r: &mut &[u8]) -> io::Result<ServerPacket> {
    use tbc_opcode as t;
    Ok(match op {
        // --- Login state -------------------------------------------------------------------
        // Same bytes as 1.12.1 (cmangos-tbc `SendInitialPacketsAfterAddToMap`, wow_messages).
        t::SMSG_LOGIN_SETTIMESPEED => read_time_speed(r)?,
        t::SMSG_ACCOUNT_DATA_TIMES => ServerPacket::Tbc(tbc_world::read_account_data_times(r)?),
        t::SMSG_FEATURE_SYSTEM_STATUS => {
            ServerPacket::Tbc(tbc_world::read_feature_system_status(r)?)
        }
        t::SMSG_EXPECTED_SPAM_RECORDS => {
            ServerPacket::Tbc(tbc_world::read_expected_spam_records(r)?)
        }
        t::SMSG_MOTD => ServerPacket::Tbc(tbc_world::read_motd(r)?),
        // Eight `u32`s, every remaining byte as in 1.12.1.
        t::SMSG_TUTORIAL_FLAGS => {
            ServerPacket::TutorialFlags(super::tutorial::read_tutorial_flags(r)?)
        }
        // x, y, z, map, area: the 1.12.1 bytes.
        t::SMSG_BINDPOINTUPDATE => ServerPacket::BindPoint {
            position: Vector3d::read(r)?,
            map: read_u32_le(r)?,
            area: read_u32_le(r)?,
        },
        t::SMSG_INSTANCE_DIFFICULTY => ServerPacket::Tbc(tbc_world::read_instance_difficulty(r)?),
        t::MSG_SET_DUNGEON_DIFFICULTY => {
            ServerPacket::Tbc(tbc_world::read_set_dungeon_difficulty(r)?)
        }
        t::SMSG_SET_REST_START => ServerPacket::Tbc(tbc_world::read_set_rest_start(r)?),
        // 2.4.3 meaning of 0x33A; 1.12.1 carries it at 0x33B.
        t::SMSG_DEFENSE_MESSAGE => {
            let (zone_id, text) = super::broadcast::read_defense_message(r)?;
            ServerPacket::DefenseMessage { zone_id, text }
        }
        // --- Character state ----------------------------------------------------------------
        t::SMSG_INITIAL_SPELLS => {
            let (spell_ids, cooldowns) = super::spellbook::read_initial_spells(r)?;
            ServerPacket::InitialSpells {
                spell_ids,
                cooldowns,
            }
        }
        t::SMSG_SEND_UNLEARN_SPELLS => ServerPacket::Tbc(tbc_world::read_send_unlearn_spells(r)?),
        // 132 words to the end of the body; 1.12.1 sends 120 in the same packing.
        t::SMSG_ACTION_BUTTONS => ServerPacket::ActionButtons {
            buttons: super::action_bar::read_action_buttons(r)?,
        },
        // `count` is 128 here, 64 in 1.12.1; the entry shape is the same.
        t::SMSG_INITIALIZE_FACTIONS => {
            let count = read_u32_le(r)?;
            let mut standings = Vec::with_capacity(capacity_hint(count, 128));
            for _ in 0..count {
                let flags = read_u8(r)?;
                standings.push((flags, read_u32_le(r)? as i32));
            }
            ServerPacket::InitializeFactions { standings }
        }
        t::SMSG_SET_PROFICIENCY => ServerPacket::SetProficiency {
            item_class: read_u8(r)?,
            subclass_mask: read_u32_le(r)?,
        },
        // 0x67 was `SMSG_FRIEND_LIST` in 1.12.1; 2.4.3 sends all three lists here.
        t::SMSG_CONTACT_LIST => ServerPacket::Tbc(tbc_world::read_contact_list(r)?),
        // --- Chat and world ----------------------------------------------------------------
        t::SMSG_MESSAGECHAT => match super::chat::read_message_chat_tbc(r)? {
            Some(message) => ServerPacket::MessageChat(message),
            // A chat type 1.12.1 has no number for: its bytes are left unread, never guessed at.
            None => return Ok(ServerPacket::Other { opcode: op }),
        },
        // The GM line's body is the ordinary line's: the tag carries the GM flag and the name.
        t::SMSG_GM_MESSAGECHAT => match super::chat::read_message_chat_tbc(r)? {
            Some(message) => ServerPacket::MessageChat(message),
            None => return Ok(ServerPacket::Other { opcode: op }),
        },
        t::SMSG_CHANNEL_NOTIFY => super::tbc_chat::read_channel_notify_tbc(r)?,
        t::SMSG_USERLIST_ADD => ServerPacket::Tbc(super::tbc_chat::read_userlist(
            super::UserListChange::Add,
            r,
        )?),
        t::SMSG_USERLIST_UPDATE => ServerPacket::Tbc(super::tbc_chat::read_userlist(
            super::UserListChange::Update,
            r,
        )?),
        t::SMSG_USERLIST_REMOVE => ServerPacket::Tbc(super::tbc_chat::read_userlist(
            super::UserListChange::Remove,
            r,
        )?),
        t::SMSG_CHAT_RESTRICTED => ServerPacket::Tbc(super::tbc_chat::read_chat_restricted(r)?),
        t::SMSG_NOTIFICATION => ServerPacket::Notification {
            text: read_cstring(r)?,
        },
        t::SMSG_WEATHER => {
            let (weather_type, grade, instant) = tbc_world::read_weather(r)?;
            // 2.4.3 carries no sound id (1.12.1 has a `u32` before the flag).
            ServerPacket::Weather {
                weather_type,
                grade,
                sound_id: 0,
                instant,
            }
        }
        t::SMSG_INIT_WORLD_STATES => {
            let map = read_u32_le(r)?;
            let zone = read_u32_le(r)?;
            let _area = read_u32_le(r)?; // new in 2.4.3
            let count = read_u16_le(r)?;
            let mut states = Vec::with_capacity(capacity_hint(count, r.len() / 8));
            for _ in 0..count {
                states.push((read_u32_le(r)?, read_u32_le(r)?));
            }
            ServerPacket::InitWorldStates(super::InitWorldStates { map, zone, states })
        }
        t::SMSG_LFG_UPDATE => ServerPacket::Tbc(tbc_world::read_lfg_update(r)?),
        // --- Auras and spells ---------------------------------------------------------------
        t::SMSG_UPDATE_AURA_DURATION => {
            let (slot, remaining_ms) = super::spells::read_update_aura_duration(r)?;
            ServerPacket::UpdateAuraDuration { slot, remaining_ms }
        }
        t::SMSG_INIT_EXTRA_AURA_INFO => ServerPacket::Tbc(tbc_world::read_init_extra_aura_info(r)?),
        t::SMSG_SET_EXTRA_AURA_INFO => {
            ServerPacket::Tbc(tbc_world::read_set_extra_aura_info(r, false)?)
        }
        t::SMSG_SET_EXTRA_AURA_INFO_NEED_UPDATE => {
            ServerPacket::Tbc(tbc_world::read_set_extra_aura_info(r, true)?)
        }
        t::SMSG_CLEAR_EXTRA_AURA_INFO => {
            ServerPacket::Tbc(tbc_world::read_clear_extra_aura_info(r)?)
        }
        t::SMSG_SPELL_START => ServerPacket::SpellStart(super::spells::read_spell_start_tbc(r)?),
        t::SMSG_SPELL_GO => ServerPacket::SpellGo(super::spells::read_spell_go_tbc(r)?),
        // --- Spells: failures, cooldowns and channels (`tbc_spells`) -------------------------
        t::SMSG_CAST_RESULT => super::tbc_spells::read_cast_failed(r)?,
        t::SMSG_SPELL_COOLDOWN => {
            let (caster, cooldowns) = super::tbc_spells::read_spell_cooldown(r)?;
            ServerPacket::SpellCooldownList { caster, cooldowns }
        }
        t::MSG_CHANNEL_START => ServerPacket::Tbc(super::tbc_spells::read_channel_start(r)?),
        t::MSG_CHANNEL_UPDATE => ServerPacket::Tbc(super::tbc_spells::read_channel_update(r)?),
        // --- Items and inventory (`tbc_items`) ---------------------------------------------
        t::SMSG_ITEM_PUSH_RESULT => {
            ServerPacket::ItemPushResult(super::tbc_items::read_item_push_result(r)?)
        }
        t::SMSG_INVENTORY_CHANGE_FAILURE => super::tbc_items::read_inventory_change_failure(r)?,
        // --- NPC interaction (`tbc_npc`) ----------------------------------------------------
        t::SMSG_GOSSIP_MESSAGE => {
            let (npc, text_id, options, quests) = super::tbc_npc::read_gossip_message(r)?;
            ServerPacket::GossipMessage {
                npc,
                text_id,
                options,
                quests,
            }
        }
        t::SMSG_LIST_INVENTORY => {
            let (vendor, items) = super::tbc_npc::read_list_inventory(r)?;
            ServerPacket::VendorList { vendor, items }
        }
        t::SMSG_QUESTGIVER_STATUS => super::tbc_npc::read_questgiver_status(r)?,
        // --- Combat readout: values or bytes that differ from 1.12.1 (`tbc_combat`) -----------
        t::SMSG_ATTACKERSTATEUPDATE => {
            ServerPacket::AttackerState(super::attack::read_attacker_state_in(r, true)?)
        }
        t::SMSG_SPELLNONMELEEDAMAGELOG => {
            ServerPacket::SpellDamageLog(super::combat_log::read_spell_damage_log_in(r, true)?)
        }
        t::SMSG_PERIODICAURALOG => {
            ServerPacket::PeriodicAuraLog(super::combat_log::read_periodic_aura_log_in(r, true)?)
        }
        t::SMSG_SPELLDAMAGESHIELD => {
            ServerPacket::DamageShield(super::tbc_combat::read_damage_shield_tbc(r)?)
        }
        t::SMSG_SPELLHEALLOG => {
            ServerPacket::SpellHealLog(super::tbc_combat::read_spell_heal_log_tbc(r)?)
        }
        t::SMSG_SPELLINSTAKILLLOG => {
            ServerPacket::SpellInstaKillLog(super::tbc_combat::read_spell_insta_kill_log_tbc(r)?)
        }
        t::SMSG_SPELLDISPELLOG => {
            ServerPacket::SpellDispelLog(super::tbc_combat::read_spell_dispel_log_tbc(r)?)
        }
        // --- Movement and objects -----------------------------------------------------------
        t::SMSG_MONSTER_MOVE => super::monster_move::read_monster_move_tbc(r, false)?,
        t::SMSG_MONSTER_MOVE_TRANSPORT => super::monster_move::read_monster_move_tbc(r, true)?,
        t::SMSG_DESTROY_OBJECT => ServerPacket::DestroyObject {
            guid: read_u64_le(r)?,
        },
        // Empty in both builds; sent when the player logs out or leaves combat.
        t::SMSG_CANCEL_COMBAT => ServerPacket::CancelCombat,
        t::SMSG_TIME_SYNC_REQ => ServerPacket::Tbc(tbc_world::read_time_sync_request(r)?),
        // One `u32`, the echoed ping sequence: the bytes of 1.12.1 (`SMSG_PONG`, cmangos-classic
        // and cmangos-tbc `HandlePing`).
        t::SMSG_PONG => ServerPacket::Pong {
            sequence: read_u32_le(r)?,
        },
        // Same fields in the same order in both builds (cmangos `SendGMTicketGetTicket` and the
        // no-ticket status): the 1.12.1 reader.
        t::SMSG_GMTICKET_GETTICKET => ServerPacket::GmTicketAnswer {
            ticket: super::gm_ticket::read_gm_ticket(r)?.map(Box::new),
        },
        // --- Queries ----------------------------------------------------------------------
        t::SMSG_NAME_QUERY_RESPONSE => {
            let guid = read_u64_le(r)?;
            let name = read_cstring(r)?;
            let _realm = read_cstring(r)?;
            let (race, gender, class) = (read_u32_le(r)?, read_u32_le(r)?, read_u32_le(r)?);
            // The declined-names tail: a flag, then five cases when set.
            if read_u8(r)? != 0 {
                for _ in 0..5 {
                    let _ = read_cstring(r)?;
                }
            }
            ServerPacket::NameQueryResponse {
                guid,
                name,
                race,
                gender,
                class,
            }
        }
        t::SMSG_CREATURE_QUERY_RESPONSE => read_creature_query_response(r)?,
        t::SMSG_GAMEOBJECT_QUERY_RESPONSE => read_gameobject_query_response(r)?,
        t::SMSG_ITEM_QUERY_SINGLE_RESPONSE => {
            let (entry, info) = super::items::read_item_query_response_tbc(r)?;
            ServerPacket::ItemQueryResponse {
                entry,
                info: info.map(Box::new),
            }
        }
        other => match super::tbc_movement::read_server(other, r)? {
            Some(packet) => packet,
            None => match super::same_reader_for(other) {
                // Same bytes in both builds (`tbc_same`): the 1.12.1 arm reads them.
                Some(vanilla) => super::parse::read_same_as_vanilla(vanilla, r)?,
                None => ServerPacket::Other { opcode: other },
            },
        },
    })
}

/// The packed game time and speed (min:6, hour:5, weekday:3, day:6, month:4, year:5, LSB first):
/// the bytes and the decoding of 1.12.1's `SMSG_LOGIN_SETTIMESPEED`.
fn read_time_speed(r: &mut &[u8]) -> io::Result<ServerPacket> {
    let datetime = read_u32_le(r)?;
    let timescale = read_f32_le(r)?;
    let (day, month, year) = (
        (datetime >> 14) & 0x3F,
        (datetime >> 20) & 0x0F,
        (datetime >> 24) & 0x1F,
    );
    Ok(ServerPacket::TimeSpeed {
        hours: ((datetime >> 6) & 0x1F) as u8,
        minutes: (datetime & 0x3F) as u8,
        day_serial: year * 372 + month * 31 + day,
        timescale,
    })
}

/// The 2.4.3 creature template: names, an icon name, the four type words, two `u32`s, four display
/// ids, two multipliers and the racial-leader byte (cmangos-tbc `HandleCreatureQueryOpcode`). A
/// miss is the entry with its top bit set.
fn read_creature_query_response(r: &mut &[u8]) -> io::Result<ServerPacket> {
    let entry = read_u32_le(r)?;
    if entry & 0x8000_0000 != 0 {
        return Ok(ServerPacket::CreatureQueryResponse {
            entry: entry & 0x7FFF_FFFF,
            info: None,
        });
    }
    let name = read_cstring(r)?;
    for _ in 0..3 {
        let _ = read_cstring(r)?; // name2..name4, always empty
    }
    let subname = read_cstring(r)?;
    let _icon_name = read_cstring(r)?;
    let type_flags = read_u32_le(r)?;
    let creature_type = read_u32_le(r)?;
    let pet_family = read_u32_le(r)?;
    let rank = read_u32_le(r)?;
    let _unknown = read_u32_le(r)?;
    let _pet_spell_data_id = read_u32_le(r)?;
    // Four display ids; the first is the template's primary one, as in 1.12.1's single id.
    let display_id = read_u32_le(r)?;
    for _ in 0..3 {
        let _ = read_u32_le(r)?;
    }
    let _health_multiplier = read_f32_le(r)?;
    let _mana_multiplier = read_f32_le(r)?;
    let racial_leader = read_u8(r)? != 0;
    Ok(ServerPacket::CreatureQueryResponse {
        entry,
        info: Some(super::CreatureQueryInfo {
            name,
            subname,
            creature_type,
            pet_family,
            rank,
            type_flags,
            display_id,
            // 2.4.3 has no civilian byte.
            civilian: false,
            racial_leader,
        }),
    })
}

/// The 2.4.3 game-object template: type, display id, four names, icon name, cast-bar caption, one
/// more string, 24 data words and the size (cmangos-tbc `HandleGameObjectQueryOpcode`).
fn read_gameobject_query_response(r: &mut &[u8]) -> io::Result<ServerPacket> {
    let entry = read_u32_le(r)?;
    if entry & 0x8000_0000 != 0 {
        return Ok(ServerPacket::GameObjectQueryResponse {
            entry: entry & 0x7FFF_FFFF,
            info: None,
        });
    }
    let type_id = read_u32_le(r)?;
    let display_id = read_u32_le(r)?;
    let name = read_cstring(r)?;
    for _ in 0..3 {
        let _ = read_cstring(r)?; // name2..name4
    }
    let _icon_name = read_cstring(r)?;
    let _cast_bar_caption = read_cstring(r)?;
    let _unknown = read_cstring(r)?;
    let mut data = [0i32; 24];
    for slot in &mut data {
        *slot = read_i32_le(r)?;
    }
    let _size = read_f32_le(r)?;
    Ok(ServerPacket::GameObjectQueryResponse {
        entry,
        info: Some(super::GameObjectQueryInfo {
            type_id,
            display_id,
            name,
            data,
        }),
    })
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
    /// `SMSG_IGNORE_LIST`) comes back as `Other`.
    #[test]
    fn everything_off_the_allow_list_is_other() {
        for op in [0x006B, 0x014F, 0x0293] {
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

    #[test]
    fn a_pong_is_its_sequence_in_both_builds() {
        let body = 0x0102_0304u32.to_le_bytes();
        for packet in [
            parse(opcode::SMSG_PONG, &body).unwrap(),
            super::super::parse_server(opcode::SMSG_PONG, &body).unwrap(),
        ] {
            match packet {
                ServerPacket::Pong { sequence } => assert_eq!(sequence, 0x0102_0304),
                other => panic!("{}", other.name()),
            }
        }
        assert!(
            parse(opcode::SMSG_PONG, &[1, 2]).is_err(),
            "a short pong is refused"
        );
    }

    #[test]
    fn a_gm_ticket_answer_is_read_as_in_1_12_1() {
        use super::super::gm_ticket::{GMTICKET_STATUS_DEFAULT, GMTICKET_STATUS_HASTEXT};
        // No ticket: the 4-byte status alone.
        let body = GMTICKET_STATUS_DEFAULT.to_le_bytes();
        match parse(opcode::SMSG_GMTICKET_GETTICKET, &body).unwrap() {
            ServerPacket::GmTicketAnswer { ticket } => assert!(ticket.is_none()),
            other => panic!("{}", other.name()),
        }
        // A ticket: status, text, category, three days, status, seen.
        let mut body = GMTICKET_STATUS_HASTEXT.to_le_bytes().to_vec();
        body.extend_from_slice(b"help\0");
        body.push(3);
        for d in [1.5f32, 2.5, 0.25] {
            body.extend_from_slice(&d.to_le_bytes());
        }
        body.extend_from_slice(&[2, 1]);
        let from_tbc = parse(opcode::SMSG_GMTICKET_GETTICKET, &body).unwrap();
        let from_classic =
            super::super::parse_server(opcode::SMSG_GMTICKET_GETTICKET, &body).unwrap();
        match (from_tbc, from_classic) {
            (
                ServerPacket::GmTicketAnswer { ticket: Some(a) },
                ServerPacket::GmTicketAnswer { ticket: Some(b) },
            ) => {
                assert_eq!(a, b);
                assert_eq!(
                    (a.text.as_str(), a.category, a.assigned_to_gm),
                    ("help", 3, 2)
                );
            }
            _ => panic!("expected a ticket from both"),
        }
    }

    /// The opcodes whose 2.4.3 bytes differ from 1.12.1's, or that 1.12.1 has no reader for, stay
    /// `Other`, each with its 2.4.3 name.
    #[test]
    fn the_unread_opcodes_stay_other_and_carry_their_2_4_3_names() {
        for (op, name) in [
            (0x0053, "SMSG_PET_NAME_QUERY_RESPONSE"),
            (0x01cf, "SMSG_QUERY_TIME_RESPONSE"),
            (0x0284, "MSG_QUERY_NEXT_MAIL_TIME"),
            (0x02cc, "SMSG_RAID_INSTANCE_INFO"),
            (0x036d, "SMSG_LFG_UPDATE_LFM"),
            (0x036e, "SMSG_LFG_UPDATE_LFG"),
        ] {
            assert_eq!(super::super::tbc_opcode_name(op), Some(name));
            assert!(
                matches!(parse(op, &[0; 16]), Ok(ServerPacket::Other { opcode }) if opcode == op),
                "{name}"
            );
        }
    }

    /// The extra-aura timing packets decode to one event per aura, so the app sees the durations.
    #[test]
    fn the_extra_aura_packets_decode_to_aura_timing_events() {
        use crate::SessionEvent as E;
        // Packed guid 0x10 (mask 0x01, byte 0x10), then slot 3, spell 6673, 120000 ms of 120000.
        let mut one = vec![0x01, 0x10, 3];
        one.extend(6673u32.to_le_bytes());
        one.extend(120_000i32.to_le_bytes());
        one.extend(120_000u32.to_le_bytes());
        for op in [
            tbc_opcode::SMSG_SET_EXTRA_AURA_INFO,
            tbc_opcode::SMSG_SET_EXTRA_AURA_INFO_NEED_UPDATE,
        ] {
            let events = crate::decode(parse(op, &one).unwrap());
            assert!(
                matches!(
                    events.as_slice(),
                    [E::ExtraAuraInfo { guid: 0x10, aura }]
                        if aura.slot == 3
                            && aura.spell_id == 6673
                            && aura.max_duration_ms == 120_000
                            && aura.remaining_ms == 120_000
                ),
                "{op:#x}: {events:?}"
            );
        }
        // Two entries of 13 bytes, the second permanent (-1, 0).
        let mut two = one.clone();
        two.push(4);
        two.extend(2457u32.to_le_bytes());
        two.extend((-1i32).to_le_bytes());
        two.extend(0u32.to_le_bytes());
        let events = crate::decode(parse(tbc_opcode::SMSG_INIT_EXTRA_AURA_INFO, &two).unwrap());
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(matches!(&events[1], E::ExtraAuraInfo { aura, .. }
            if aura.spell_id == 2457 && aura.max_duration_ms == -1));
        let mut clear = vec![0x01, 0x10];
        clear.extend(6673u32.to_le_bytes());
        let events = crate::decode(parse(tbc_opcode::SMSG_CLEAR_EXTRA_AURA_INFO, &clear).unwrap());
        assert!(matches!(
            events.as_slice(),
            [E::ExtraAuraCleared {
                guid: 0x10,
                spell_id: 6673
            }]
        ));
    }

    /// The MOTD lines, the feature flags and the dungeon difficulty reach the app as events.
    #[test]
    fn the_motd_feature_status_and_difficulty_decode_to_events() {
        use crate::SessionEvent as E;
        let mut motd = 2u32.to_le_bytes().to_vec();
        motd.extend(b"Welcome\0Have fun\0");
        let events = crate::decode(parse(tbc_opcode::SMSG_MOTD, &motd).unwrap());
        assert!(
            matches!(events.as_slice(), [E::Motd { lines }] if lines == &["Welcome", "Have fun"])
        );
        let events = crate::decode(parse(tbc_opcode::SMSG_FEATURE_SYSTEM_STATUS, &[2, 1]).unwrap());
        assert!(matches!(
            events.as_slice(),
            [E::FeatureSystemStatus {
                voice_chat_enabled: true
            }]
        ));
        let mut diff = 1u32.to_le_bytes().to_vec();
        diff.extend(0u32.to_le_bytes());
        let events = crate::decode(parse(tbc_opcode::SMSG_INSTANCE_DIFFICULTY, &diff).unwrap());
        assert!(matches!(
            events.as_slice(),
            [E::DungeonDifficulty { difficulty: 1 }]
        ));
    }

    #[test]
    fn a_time_sync_request_decodes_to_the_event_the_read_thread_answers() {
        let packet = parse(tbc_opcode::SMSG_TIME_SYNC_REQ, &7u32.to_le_bytes()).unwrap();
        let events = crate::decode(packet);
        assert!(matches!(
            events.as_slice(),
            [crate::SessionEvent::TimeSyncRequest { counter: 7 }]
        ));
    }
}
