//! Names for the 2.4.3 opcode numbers the 2.4.3 dispatch reads, sends or tells apart from 1.12.1, from
//! cmangos-tbc `Opcodes.h`. Five numbers changed meaning (0x66, 0x67, 0x6B, 0x14F, 0x293) and
//! `SMSG_DEFENSE_MESSAGE` moved from 0x33B to 0x33A, so a 1.12.1 name for a 2.4.3 number misleads;
//! the 1.12.1 table (`opcode_names`) is left as it is. Sorted by number for the binary search.

/// The 2.4.3 name of `opcode`, `None` for a number outside this table.
pub fn tbc_opcode_name(opcode: u16) -> Option<&'static str> {
    NAMES
        .binary_search_by_key(&opcode, |&(n, _)| n)
        .ok()
        .map(|i| NAMES[i].1)
}

/// The 2.4.3 numbers the slice uses by name.
pub mod tbc_opcode {
    pub const CMSG_CHAR_CREATE: u16 = 0x0036;
    pub const CMSG_CHAR_ENUM: u16 = 0x0037;
    pub const SMSG_CHAR_CREATE: u16 = 0x003a;
    pub const SMSG_CHAR_ENUM: u16 = 0x003b;
    pub const CMSG_PLAYER_LOGIN: u16 = 0x003d;
    pub const SMSG_NEW_WORLD: u16 = 0x003e;
    pub const SMSG_TRANSFER_PENDING: u16 = 0x003f;
    pub const SMSG_CHARACTER_LOGIN_FAILED: u16 = 0x0041;
    pub const SMSG_LOGIN_SETTIMESPEED: u16 = 0x0042;
    pub const CMSG_LOGOUT_REQUEST: u16 = 0x004b;
    pub const SMSG_LOGOUT_RESPONSE: u16 = 0x004c;
    pub const SMSG_LOGOUT_COMPLETE: u16 = 0x004d;
    pub const CMSG_LOGOUT_CANCEL: u16 = 0x004e;
    pub const SMSG_LOGOUT_CANCEL_ACK: u16 = 0x004f;
    pub const CMSG_NAME_QUERY: u16 = 0x0050;
    pub const SMSG_NAME_QUERY_RESPONSE: u16 = 0x0051;
    pub const CMSG_ITEM_QUERY_SINGLE: u16 = 0x0056;
    pub const SMSG_ITEM_QUERY_SINGLE_RESPONSE: u16 = 0x0058;
    pub const CMSG_PAGE_TEXT_QUERY: u16 = 0x005a;
    pub const SMSG_PAGE_TEXT_QUERY_RESPONSE: u16 = 0x005b;
    pub const CMSG_QUEST_QUERY: u16 = 0x005c;
    pub const SMSG_QUEST_QUERY_RESPONSE: u16 = 0x005d;
    pub const CMSG_GAMEOBJECT_QUERY: u16 = 0x005e;
    pub const SMSG_GAMEOBJECT_QUERY_RESPONSE: u16 = 0x005f;
    pub const CMSG_CREATURE_QUERY: u16 = 0x0060;
    pub const SMSG_CREATURE_QUERY_RESPONSE: u16 = 0x0061;
    pub const CMSG_CONTACT_LIST: u16 = 0x0066;
    pub const SMSG_CONTACT_LIST: u16 = 0x0067;
    pub const SMSG_FRIEND_STATUS: u16 = 0x0068;
    pub const CMSG_SET_CONTACT_NOTES: u16 = 0x006b;
    pub const SMSG_MESSAGECHAT: u16 = 0x0096;
    pub const SMSG_UPDATE_OBJECT: u16 = 0x00a9;
    pub const SMSG_DESTROY_OBJECT: u16 = 0x00aa;
    pub const SMSG_MONSTER_MOVE: u16 = 0x00dd;
    pub const SMSG_TRIGGER_CINEMATIC: u16 = 0x00fa;
    pub const SMSG_TUTORIAL_FLAGS: u16 = 0x00fd;
    pub const SMSG_INITIALIZE_FACTIONS: u16 = 0x0122;
    pub const SMSG_SET_PROFICIENCY: u16 = 0x0127;
    pub const SMSG_ACTION_BUTTONS: u16 = 0x0129;
    pub const SMSG_INITIAL_SPELLS: u16 = 0x012a;
    pub const SMSG_SPELL_START: u16 = 0x0131;
    pub const SMSG_SPELL_GO: u16 = 0x0132;
    pub const SMSG_SPELL_COOLDOWN: u16 = 0x0134;
    pub const SMSG_UPDATE_AURA_DURATION: u16 = 0x0137;
    pub const SMSG_CANCEL_COMBAT: u16 = 0x014e;
    pub const SMSG_SPELLBREAKLOG: u16 = 0x014f;
    pub const SMSG_BINDPOINTUPDATE: u16 = 0x0155;
    pub const CMSG_NPC_TEXT_QUERY: u16 = 0x017f;
    pub const SMSG_NPC_TEXT_UPDATE: u16 = 0x0180;
    pub const SMSG_QUESTGIVER_STATUS: u16 = 0x0183;
    pub const SMSG_NOTIFICATION: u16 = 0x01cb;
    pub const CMSG_QUERY_TIME: u16 = 0x01ce;
    pub const SMSG_QUERY_TIME_RESPONSE: u16 = 0x01cf;
    pub const CMSG_PING: u16 = 0x01dc;
    pub const SMSG_PONG: u16 = 0x01dd;
    pub const SMSG_AUTH_CHALLENGE: u16 = 0x01ec;
    pub const CMSG_AUTH_SESSION: u16 = 0x01ed;
    pub const SMSG_AUTH_RESPONSE: u16 = 0x01ee;
    pub const SMSG_COMPRESSED_UPDATE_OBJECT: u16 = 0x01f6;
    pub const SMSG_ACCOUNT_DATA_TIMES: u16 = 0x0209;
    pub const SMSG_GAMEOBJECT_DESPAWN_ANIM: u16 = 0x0215;
    pub const SMSG_SET_REST_START: u16 = 0x021e;
    pub const SMSG_LOGIN_VERIFY_WORLD: u16 = 0x0236;
    pub const SMSG_SPELLLOGEXECUTE: u16 = 0x024c;
    pub const SMSG_PERIODICAURALOG: u16 = 0x024e;
    pub const SMSG_ZONE_UNDER_ATTACK: u16 = 0x0254;
    pub const SMSG_SERVER_MESSAGE: u16 = 0x0291;
    pub const SMSG_MEETINGSTONE_LEAVE: u16 = 0x0293;
    pub const SMSG_MONSTER_MOVE_TRANSPORT: u16 = 0x02ae;
    pub const MSG_PETITION_RENAME: u16 = 0x02c1;
    pub const SMSG_INIT_WORLD_STATES: u16 = 0x02c2;
    pub const SMSG_PLAY_SOUND: u16 = 0x02d2;
    pub const SMSG_ADDON_INFO: u16 = 0x02ef;
    pub const SMSG_WEATHER: u16 = 0x02f4;
    pub const MSG_SET_DUNGEON_DIFFICULTY: u16 = 0x0329;
    pub const SMSG_EXPECTED_SPAM_RECORDS: u16 = 0x0332;
    pub const SMSG_DEFENSE_MESSAGE: u16 = 0x033a;
    pub const SMSG_INSTANCE_DIFFICULTY: u16 = 0x033b;
    pub const SMSG_MOTD: u16 = 0x033d;
    pub const SMSG_LFG_UPDATE: u16 = 0x036c;
    pub const SMSG_REALM_SPLIT: u16 = 0x038b;
    pub const CMSG_REALM_SPLIT: u16 = 0x038c;
    pub const SMSG_TIME_SYNC_REQ: u16 = 0x0390;
    pub const CMSG_TIME_SYNC_RESP: u16 = 0x0391;
    pub const SMSG_INIT_EXTRA_AURA_INFO: u16 = 0x03a3;
    pub const SMSG_SET_EXTRA_AURA_INFO: u16 = 0x03a4;
    pub const SMSG_SET_EXTRA_AURA_INFO_NEED_UPDATE: u16 = 0x03a5;
    pub const SMSG_CLEAR_EXTRA_AURA_INFO: u16 = 0x03a6;
    pub const SMSG_GM_MESSAGECHAT: u16 = 0x03b2;
    pub const SMSG_FEATURE_SYSTEM_STATUS: u16 = 0x03c8;
    pub const CMSG_KEEP_ALIVE: u16 = 0x0406;
    pub const SMSG_SEND_UNLEARN_SPELLS: u16 = 0x041d;
}

#[rustfmt::skip]
static NAMES: &[(u16, &str)] = &[
    (0x0036, "CMSG_CHAR_CREATE"),
    (0x0037, "CMSG_CHAR_ENUM"),
    (0x003a, "SMSG_CHAR_CREATE"),
    (0x003b, "SMSG_CHAR_ENUM"),
    (0x003d, "CMSG_PLAYER_LOGIN"),
    (0x003e, "SMSG_NEW_WORLD"),
    (0x003f, "SMSG_TRANSFER_PENDING"),
    (0x0041, "SMSG_CHARACTER_LOGIN_FAILED"),
    (0x0042, "SMSG_LOGIN_SETTIMESPEED"),
    (0x004b, "CMSG_LOGOUT_REQUEST"),
    (0x004c, "SMSG_LOGOUT_RESPONSE"),
    (0x004d, "SMSG_LOGOUT_COMPLETE"),
    (0x004e, "CMSG_LOGOUT_CANCEL"),
    (0x004f, "SMSG_LOGOUT_CANCEL_ACK"),
    (0x0050, "CMSG_NAME_QUERY"),
    (0x0051, "SMSG_NAME_QUERY_RESPONSE"),
    (0x0056, "CMSG_ITEM_QUERY_SINGLE"),
    (0x0058, "SMSG_ITEM_QUERY_SINGLE_RESPONSE"),
    (0x005a, "CMSG_PAGE_TEXT_QUERY"),
    (0x005b, "SMSG_PAGE_TEXT_QUERY_RESPONSE"),
    (0x005c, "CMSG_QUEST_QUERY"),
    (0x005d, "SMSG_QUEST_QUERY_RESPONSE"),
    (0x005e, "CMSG_GAMEOBJECT_QUERY"),
    (0x005f, "SMSG_GAMEOBJECT_QUERY_RESPONSE"),
    (0x0060, "CMSG_CREATURE_QUERY"),
    (0x0061, "SMSG_CREATURE_QUERY_RESPONSE"),
    (0x0066, "CMSG_CONTACT_LIST"),
    (0x0067, "SMSG_CONTACT_LIST"),
    (0x0068, "SMSG_FRIEND_STATUS"),
    (0x006b, "CMSG_SET_CONTACT_NOTES"),
    (0x0096, "SMSG_MESSAGECHAT"),
    (0x00a9, "SMSG_UPDATE_OBJECT"),
    (0x00aa, "SMSG_DESTROY_OBJECT"),
    (0x00dd, "SMSG_MONSTER_MOVE"),
    (0x00fa, "SMSG_TRIGGER_CINEMATIC"),
    (0x00fd, "SMSG_TUTORIAL_FLAGS"),
    (0x0122, "SMSG_INITIALIZE_FACTIONS"),
    (0x0127, "SMSG_SET_PROFICIENCY"),
    (0x0129, "SMSG_ACTION_BUTTONS"),
    (0x012a, "SMSG_INITIAL_SPELLS"),
    (0x0131, "SMSG_SPELL_START"),
    (0x0132, "SMSG_SPELL_GO"),
    (0x0134, "SMSG_SPELL_COOLDOWN"),
    (0x0137, "SMSG_UPDATE_AURA_DURATION"),
    (0x014e, "SMSG_CANCEL_COMBAT"),
    (0x014f, "SMSG_SPELLBREAKLOG"),
    (0x0155, "SMSG_BINDPOINTUPDATE"),
    (0x017f, "CMSG_NPC_TEXT_QUERY"),
    (0x0180, "SMSG_NPC_TEXT_UPDATE"),
    (0x0183, "SMSG_QUESTGIVER_STATUS"),
    (0x01cb, "SMSG_NOTIFICATION"),
    (0x01ce, "CMSG_QUERY_TIME"),
    (0x01cf, "SMSG_QUERY_TIME_RESPONSE"),
    (0x01dc, "CMSG_PING"),
    (0x01dd, "SMSG_PONG"),
    (0x01ec, "SMSG_AUTH_CHALLENGE"),
    (0x01ed, "CMSG_AUTH_SESSION"),
    (0x01ee, "SMSG_AUTH_RESPONSE"),
    (0x01f6, "SMSG_COMPRESSED_UPDATE_OBJECT"),
    (0x0209, "SMSG_ACCOUNT_DATA_TIMES"),
    (0x0215, "SMSG_GAMEOBJECT_DESPAWN_ANIM"),
    (0x021e, "SMSG_SET_REST_START"),
    (0x0236, "SMSG_LOGIN_VERIFY_WORLD"),
    (0x024c, "SMSG_SPELLLOGEXECUTE"),
    (0x024e, "SMSG_PERIODICAURALOG"),
    (0x0254, "SMSG_ZONE_UNDER_ATTACK"),
    (0x0291, "SMSG_SERVER_MESSAGE"),
    (0x0293, "SMSG_MEETINGSTONE_LEAVE"),
    (0x02ae, "SMSG_MONSTER_MOVE_TRANSPORT"),
    (0x02c1, "MSG_PETITION_RENAME"),
    (0x02c2, "SMSG_INIT_WORLD_STATES"),
    (0x02d2, "SMSG_PLAY_SOUND"),
    (0x02ef, "SMSG_ADDON_INFO"),
    (0x02f4, "SMSG_WEATHER"),
    (0x0329, "MSG_SET_DUNGEON_DIFFICULTY"),
    (0x0332, "SMSG_EXPECTED_SPAM_RECORDS"),
    (0x033a, "SMSG_DEFENSE_MESSAGE"),
    (0x033b, "SMSG_INSTANCE_DIFFICULTY"),
    (0x033d, "SMSG_MOTD"),
    (0x036c, "SMSG_LFG_UPDATE"),
    (0x038b, "SMSG_REALM_SPLIT"),
    (0x038c, "CMSG_REALM_SPLIT"),
    (0x0390, "SMSG_TIME_SYNC_REQ"),
    (0x0391, "CMSG_TIME_SYNC_RESP"),
    (0x03a3, "SMSG_INIT_EXTRA_AURA_INFO"),
    (0x03a4, "SMSG_SET_EXTRA_AURA_INFO"),
    (0x03a5, "SMSG_SET_EXTRA_AURA_INFO_NEED_UPDATE"),
    (0x03a6, "SMSG_CLEAR_EXTRA_AURA_INFO"),
    (0x03b2, "SMSG_GM_MESSAGECHAT"),
    (0x03c8, "SMSG_FEATURE_SYSTEM_STATUS"),
    (0x0406, "CMSG_KEEP_ALIVE"),
    (0x041d, "SMSG_SEND_UNLEARN_SPELLS"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_unique() {
        assert!(NAMES.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn the_numbers_that_changed_meaning_carry_their_2_4_3_names() {
        assert_eq!(tbc_opcode_name(0x0066), Some("CMSG_CONTACT_LIST"));
        assert_eq!(tbc_opcode_name(0x0067), Some("SMSG_CONTACT_LIST"));
        assert_eq!(tbc_opcode_name(0x006B), Some("CMSG_SET_CONTACT_NOTES"));
        assert_eq!(tbc_opcode_name(0x014F), Some("SMSG_SPELLBREAKLOG"));
        assert_eq!(tbc_opcode_name(0x0293), Some("SMSG_MEETINGSTONE_LEAVE"));
        assert_eq!(tbc_opcode_name(0x033A), Some("SMSG_DEFENSE_MESSAGE"));
        assert_eq!(tbc_opcode_name(0x033B), Some("SMSG_INSTANCE_DIFFICULTY"));
        assert_eq!(tbc_opcode_name(0x0209), Some("SMSG_ACCOUNT_DATA_TIMES"));
    }

    #[test]
    fn a_number_outside_the_table_is_none() {
        assert_eq!(tbc_opcode_name(0xFFFF), None);
    }
}
