//! The per-build disposition of every `WorldWriter` verb. A verb sends 1.12.1's opcode and body;
//! on a 2.4.3 session that is right only where the message is the same in both builds (`Same`) or the
//! verb builds its own 2.4.3 form (`Has243Form`). A verb with neither is `NotEstablished`: the writer
//! refuses it on 2.4.3 with a [`VerbRefused`] and nothing reaches the socket. 1.12.1 sessions send
//! every verb as before. Generated from the message classification; the opcodes are the 1.12.1
//! numbers the verb sends (empty for a verb whose opcode follows its arguments).

use std::fmt;

use crate::messages::{opcode, opcode_name, tbc_opcode_name};

/// How a verb behaves on a 2.4.3 session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// Same bytes in both builds: sent as is.
    Same,
    /// The verb picks its 2.4.3 opcode or body itself.
    Has243Form,
    /// No 2.4.3 form is established: refused on 2.4.3.
    NotEstablished,
}

/// One `WorldWriter` verb, its disposition, and the 1.12.1 opcodes it sends.
#[derive(Debug)]
pub struct VerbForm {
    pub verb: &'static str,
    pub form: Form,
    pub opcodes: &'static [u16],
}

/// A verb the writer will not send on a 2.4.3 session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerbRefused {
    pub verb: &'static str,
    pub opcode: u16,
}

impl fmt::Display for VerbRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "verb `{}` ({}) is not established for 2.4.3 and was not sent",
            self.verb,
            opcode_name(self.opcode)
                .or_else(|| tbc_opcode_name(self.opcode))
                .unwrap_or("?")
        )
    }
}

impl std::error::Error for VerbRefused {}

/// The refusal for sending `opcode` on a 2.4.3 session, `None` when its verbs may send it.
pub fn refusal_on_tbc(opcode: u16) -> Option<VerbRefused> {
    VERBS
        .iter()
        .find(|v| v.form == Form::NotEstablished && v.opcodes.contains(&opcode))
        .map(|v| VerbRefused {
            verb: v.verb,
            opcode,
        })
}

/// The disposition of `verb`, `None` for a name that is not a writer verb.
pub fn form_of(verb: &str) -> Option<Form> {
    VERBS.iter().find(|v| v.verb == verb).map(|v| v.form)
}

/// Every verb that sends, by name.
pub static VERBS: &[VerbForm] = &[
    VerbForm {
        verb: "accept_trade",
        form: Form::Same,
        opcodes: &[opcode::CMSG_ACCEPT_TRADE],
    },
    VerbForm {
        verb: "activate_taxi",
        form: Form::Same,
        opcodes: &[opcode::CMSG_ACTIVATETAXI],
    },
    VerbForm {
        verb: "activate_taxi_express",
        form: Form::Same,
        opcodes: &[opcode::CMSG_ACTIVATETAXIEXPRESS],
    },
    VerbForm {
        verb: "add_friend",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_ADD_FRIEND],
    },
    VerbForm {
        verb: "add_ignore",
        form: Form::Same,
        opcodes: &[opcode::CMSG_ADD_IGNORE],
    },
    VerbForm {
        verb: "answer_movement",
        form: Form::Has243Form,
        opcodes: &[],
    },
    VerbForm {
        verb: "area_spirit_healer_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_AREA_SPIRIT_HEALER_QUERY],
    },
    VerbForm {
        verb: "area_spirit_healer_queue",
        form: Form::Same,
        opcodes: &[opcode::CMSG_AREA_SPIRIT_HEALER_QUEUE],
    },
    VerbForm {
        verb: "area_trigger",
        form: Form::Same,
        opcodes: &[opcode::CMSG_AREATRIGGER],
    },
    VerbForm {
        verb: "attack_stop",
        form: Form::Same,
        opcodes: &[opcode::CMSG_ATTACKSTOP],
    },
    VerbForm {
        verb: "attack_swing",
        form: Form::Same,
        opcodes: &[opcode::CMSG_ATTACKSWING],
    },
    VerbForm {
        verb: "auction_hello",
        form: Form::Same,
        opcodes: &[opcode::MSG_AUCTION_HELLO],
    },
    VerbForm {
        verb: "auction_list_bidder_items",
        form: Form::Same,
        opcodes: &[opcode::CMSG_AUCTION_LIST_BIDDER_ITEMS],
    },
    VerbForm {
        verb: "auction_list_items",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_AUCTION_LIST_ITEMS],
    },
    VerbForm {
        verb: "auction_list_owner_items",
        form: Form::Same,
        opcodes: &[opcode::CMSG_AUCTION_LIST_OWNER_ITEMS],
    },
    VerbForm {
        verb: "auction_place_bid",
        form: Form::Same,
        opcodes: &[opcode::CMSG_AUCTION_PLACE_BID],
    },
    VerbForm {
        verb: "auction_remove_item",
        form: Form::Same,
        opcodes: &[opcode::CMSG_AUCTION_REMOVE_ITEM],
    },
    VerbForm {
        verb: "auction_sell_item",
        form: Form::Same,
        opcodes: &[opcode::CMSG_AUCTION_SELL_ITEM],
    },
    VerbForm {
        verb: "auto_equip_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_AUTOEQUIP_ITEM],
    },
    VerbForm {
        verb: "auto_store_bag_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_AUTOSTORE_BAG_ITEM],
    },
    VerbForm {
        verb: "autobank_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_AUTOBANK_ITEM],
    },
    VerbForm {
        verb: "autostore_bank_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_AUTOSTORE_BANK_ITEM],
    },
    VerbForm {
        verb: "autostore_loot_item",
        form: Form::Same,
        opcodes: &[opcode::CMSG_AUTOSTORE_LOOT_ITEM],
    },
    VerbForm {
        verb: "banker_activate",
        form: Form::Same,
        opcodes: &[opcode::CMSG_BANKER_ACTIVATE],
    },
    VerbForm {
        verb: "battlefield_join",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_BATTLEFIELD_JOIN],
    },
    VerbForm {
        verb: "battlefield_list",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_BATTLEFIELD_LIST],
    },
    VerbForm {
        verb: "battlefield_port",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_BATTLEFIELD_PORT],
    },
    VerbForm {
        verb: "battlefield_status",
        form: Form::Same,
        opcodes: &[opcode::CMSG_BATTLEFIELD_STATUS],
    },
    VerbForm {
        verb: "battlemaster_hello",
        form: Form::Same,
        opcodes: &[opcode::CMSG_BATTLEMASTER_HELLO],
    },
    VerbForm {
        verb: "battlemaster_join",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_BATTLEMASTER_JOIN],
    },
    VerbForm {
        verb: "begin_trade",
        form: Form::Same,
        opcodes: &[opcode::CMSG_BEGIN_TRADE],
    },
    VerbForm {
        verb: "binder_activate",
        form: Form::Same,
        opcodes: &[opcode::CMSG_BINDER_ACTIVATE],
    },
    VerbForm {
        verb: "busy_trade",
        form: Form::Same,
        opcodes: &[opcode::CMSG_BUSY_TRADE],
    },
    VerbForm {
        verb: "buy_bank_slot",
        form: Form::Same,
        opcodes: &[opcode::CMSG_BUY_BANK_SLOT],
    },
    VerbForm {
        verb: "buy_item",
        form: Form::Same,
        opcodes: &[opcode::CMSG_BUY_ITEM],
    },
    VerbForm {
        verb: "buy_item_in_slot",
        form: Form::Same,
        opcodes: &[opcode::CMSG_BUY_ITEM_IN_SLOT],
    },
    VerbForm {
        verb: "buy_stable_slot",
        form: Form::Same,
        opcodes: &[opcode::CMSG_BUY_STABLE_SLOT],
    },
    VerbForm {
        verb: "buyback_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_BUYBACK_ITEM],
    },
    VerbForm {
        verb: "can_fly_ack",
        form: Form::Has243Form,
        opcodes: &[],
    },
    VerbForm {
        verb: "cancel_aura",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CANCEL_AURA],
    },
    VerbForm {
        verb: "cancel_auto_repeat",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CANCEL_AUTO_REPEAT_SPELL],
    },
    VerbForm {
        verb: "cancel_cast",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CANCEL_CAST],
    },
    VerbForm {
        verb: "cancel_channelling",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CANCEL_CHANNELLING],
    },
    VerbForm {
        verb: "cancel_trade",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CANCEL_TRADE],
    },
    VerbForm {
        verb: "cast_spell",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_CAST_SPELL],
    },
    VerbForm {
        verb: "cast_spell_at_dest",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_CAST_SPELL],
    },
    VerbForm {
        verb: "cast_spell_at_source",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_CAST_SPELL],
    },
    VerbForm {
        verb: "cast_spell_corpse",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_CAST_SPELL],
    },
    VerbForm {
        verb: "cast_spell_gameobject",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_CAST_SPELL],
    },
    VerbForm {
        verb: "cast_spell_item",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_CAST_SPELL],
    },
    VerbForm {
        verb: "channel_announcements",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_ANNOUNCEMENTS],
    },
    VerbForm {
        verb: "channel_ban",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_BAN],
    },
    VerbForm {
        verb: "channel_invite",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_INVITE],
    },
    VerbForm {
        verb: "channel_kick",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_KICK],
    },
    VerbForm {
        verb: "channel_list",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_LIST],
    },
    VerbForm {
        verb: "channel_moderate",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_MODERATE],
    },
    VerbForm {
        verb: "channel_moderator",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_MODERATOR],
    },
    VerbForm {
        verb: "channel_mute",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_MUTE],
    },
    VerbForm {
        verb: "channel_owner",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_OWNER],
    },
    VerbForm {
        verb: "channel_password",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_PASSWORD],
    },
    VerbForm {
        verb: "channel_set_owner",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_SET_OWNER],
    },
    VerbForm {
        verb: "channel_unban",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_UNBAN],
    },
    VerbForm {
        verb: "channel_unmoderator",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_UNMODERATOR],
    },
    VerbForm {
        verb: "channel_unmute",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CHANNEL_UNMUTE],
    },
    VerbForm {
        verb: "chat_ignored",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_CHAT_IGNORED],
    },
    VerbForm {
        verb: "clear_trade_item",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CLEAR_TRADE_ITEM],
    },
    VerbForm {
        verb: "complete_cinematic",
        form: Form::Same,
        opcodes: &[opcode::CMSG_COMPLETE_CINEMATIC],
    },
    VerbForm {
        verb: "corpse_query",
        form: Form::Same,
        opcodes: &[opcode::MSG_CORPSE_QUERY],
    },
    VerbForm {
        verb: "creature_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_CREATURE_QUERY],
    },
    VerbForm {
        verb: "del_friend",
        form: Form::Same,
        opcodes: &[opcode::CMSG_DEL_FRIEND],
    },
    VerbForm {
        verb: "del_ignore",
        form: Form::Same,
        opcodes: &[opcode::CMSG_DEL_IGNORE],
    },
    VerbForm {
        verb: "destroy_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_DESTROYITEM],
    },
    VerbForm {
        verb: "duel_accepted",
        form: Form::Same,
        opcodes: &[opcode::CMSG_DUEL_ACCEPTED],
    },
    VerbForm {
        verb: "duel_cancelled",
        form: Form::Same,
        opcodes: &[opcode::CMSG_DUEL_CANCELLED],
    },
    VerbForm {
        verb: "far_sight",
        form: Form::Same,
        opcodes: &[opcode::CMSG_FAR_SIGHT],
    },
    VerbForm {
        verb: "force_flight_speed_ack",
        form: Form::Has243Form,
        opcodes: &[],
    },
    VerbForm {
        verb: "force_speed_change_ack",
        form: Form::Has243Form,
        opcodes: &[],
    },
    VerbForm {
        verb: "friend_list",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_FRIEND_LIST],
    },
    VerbForm {
        verb: "gameobj_use",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GAMEOBJ_USE],
    },
    VerbForm {
        verb: "gameobject_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GAMEOBJECT_QUERY],
    },
    VerbForm {
        verb: "get_mail_list",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GET_MAIL_LIST],
    },
    VerbForm {
        verb: "gm_ticket_create",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_GMTICKET_CREATE],
    },
    VerbForm {
        verb: "gm_ticket_delete",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GMTICKET_DELETETICKET],
    },
    VerbForm {
        verb: "gm_ticket_get",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GMTICKET_GETTICKET],
    },
    VerbForm {
        verb: "gm_ticket_system_status",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GMTICKET_SYSTEMSTATUS],
    },
    VerbForm {
        verb: "gm_ticket_updatetext",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_GMTICKET_UPDATETEXT],
    },
    VerbForm {
        verb: "gossip_hello",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GOSSIP_HELLO],
    },
    VerbForm {
        verb: "gossip_select_option",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_GOSSIP_SELECT_OPTION],
    },
    VerbForm {
        verb: "group_accept",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_ACCEPT],
    },
    VerbForm {
        verb: "group_assistant_leader",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_ASSISTANT_LEADER],
    },
    VerbForm {
        verb: "group_change_sub_group",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_CHANGE_SUB_GROUP],
    },
    VerbForm {
        verb: "group_decline",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_DECLINE],
    },
    VerbForm {
        verb: "group_disband",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_DISBAND],
    },
    VerbForm {
        verb: "group_invite",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_INVITE],
    },
    VerbForm {
        verb: "group_raid_convert",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_RAID_CONVERT],
    },
    VerbForm {
        verb: "group_set_leader",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_SET_LEADER],
    },
    VerbForm {
        verb: "group_swap_sub_group",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_SWAP_SUB_GROUP],
    },
    VerbForm {
        verb: "group_uninvite",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_UNINVITE],
    },
    VerbForm {
        verb: "group_uninvite_guid",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GROUP_UNINVITE_GUID],
    },
    VerbForm {
        verb: "guild_accept",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_ACCEPT],
    },
    VerbForm {
        verb: "guild_add_rank",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_ADD_RANK],
    },
    VerbForm {
        verb: "guild_create",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_CREATE],
    },
    VerbForm {
        verb: "guild_decline",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_DECLINE],
    },
    VerbForm {
        verb: "guild_del_rank",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_DEL_RANK],
    },
    VerbForm {
        verb: "guild_demote",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_DEMOTE],
    },
    VerbForm {
        verb: "guild_disband",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_DISBAND],
    },
    VerbForm {
        verb: "guild_info",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_INFO],
    },
    VerbForm {
        verb: "guild_info_text",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_INFO_TEXT],
    },
    VerbForm {
        verb: "guild_invite",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_INVITE],
    },
    VerbForm {
        verb: "guild_leader",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_LEADER],
    },
    VerbForm {
        verb: "guild_leave",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_LEAVE],
    },
    VerbForm {
        verb: "guild_motd",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_MOTD],
    },
    VerbForm {
        verb: "guild_promote",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_PROMOTE],
    },
    VerbForm {
        verb: "guild_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_QUERY],
    },
    VerbForm {
        verb: "guild_rank",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_GUILD_RANK],
    },
    VerbForm {
        verb: "guild_remove",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_REMOVE],
    },
    VerbForm {
        verb: "guild_roster",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_ROSTER],
    },
    VerbForm {
        verb: "guild_set_officer_note",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_SET_OFFICER_NOTE],
    },
    VerbForm {
        verb: "guild_set_public_note",
        form: Form::Same,
        opcodes: &[opcode::CMSG_GUILD_SET_PUBLIC_NOTE],
    },
    VerbForm {
        verb: "ignore_trade",
        form: Form::Same,
        opcodes: &[opcode::CMSG_IGNORE_TRADE],
    },
    VerbForm {
        verb: "initiate_trade",
        form: Form::Same,
        opcodes: &[opcode::CMSG_INITIATE_TRADE],
    },
    VerbForm {
        verb: "inspect",
        form: Form::Same,
        opcodes: &[opcode::CMSG_INSPECT],
    },
    VerbForm {
        verb: "inspect_honor_stats",
        form: Form::NotEstablished,
        opcodes: &[opcode::MSG_INSPECT_HONOR_STATS],
    },
    VerbForm {
        verb: "item_query",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_ITEM_QUERY_SINGLE],
    },
    VerbForm {
        verb: "item_text_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_ITEM_TEXT_QUERY],
    },
    VerbForm {
        verb: "join_channel",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_JOIN_CHANNEL],
    },
    VerbForm {
        verb: "knock_back_ack",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_MOVE_KNOCK_BACK_ACK],
    },
    VerbForm {
        verb: "learn_talent",
        form: Form::Same,
        opcodes: &[opcode::CMSG_LEARN_TALENT],
    },
    VerbForm {
        verb: "leave_battlefield",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_LEAVE_BATTLEFIELD],
    },
    VerbForm {
        verb: "leave_channel",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_LEAVE_CHANNEL],
    },
    VerbForm {
        verb: "list_inventory",
        form: Form::Same,
        opcodes: &[opcode::CMSG_LIST_INVENTORY],
    },
    VerbForm {
        verb: "list_stabled_pets",
        form: Form::Same,
        opcodes: &[opcode::MSG_LIST_STABLED_PETS],
    },
    VerbForm {
        verb: "logout_cancel",
        form: Form::Same,
        opcodes: &[opcode::CMSG_LOGOUT_CANCEL],
    },
    VerbForm {
        verb: "logout_request",
        form: Form::Same,
        opcodes: &[opcode::CMSG_LOGOUT_REQUEST],
    },
    VerbForm {
        verb: "loot",
        form: Form::Same,
        opcodes: &[opcode::CMSG_LOOT],
    },
    VerbForm {
        verb: "loot_master_give",
        form: Form::Same,
        opcodes: &[opcode::CMSG_LOOT_MASTER_GIVE],
    },
    VerbForm {
        verb: "loot_method",
        form: Form::Same,
        opcodes: &[opcode::CMSG_LOOT_METHOD],
    },
    VerbForm {
        verb: "loot_money",
        form: Form::Same,
        opcodes: &[opcode::CMSG_LOOT_MONEY],
    },
    VerbForm {
        verb: "loot_release",
        form: Form::Same,
        opcodes: &[opcode::CMSG_LOOT_RELEASE],
    },
    VerbForm {
        verb: "loot_roll",
        form: Form::Same,
        opcodes: &[opcode::CMSG_LOOT_ROLL],
    },
    VerbForm {
        verb: "mail_create_text_item",
        form: Form::Same,
        opcodes: &[opcode::CMSG_MAIL_CREATE_TEXT_ITEM],
    },
    VerbForm {
        verb: "mail_delete",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_MAIL_DELETE],
    },
    VerbForm {
        verb: "mail_mark_as_read",
        form: Form::Same,
        opcodes: &[opcode::CMSG_MAIL_MARK_AS_READ],
    },
    VerbForm {
        verb: "mail_return_to_sender",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_MAIL_RETURN_TO_SENDER],
    },
    VerbForm {
        verb: "mail_take_item",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_MAIL_TAKE_ITEM],
    },
    VerbForm {
        verb: "mail_take_money",
        form: Form::Same,
        opcodes: &[opcode::CMSG_MAIL_TAKE_MONEY],
    },
    VerbForm {
        verb: "meeting_stone_join",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_MEETINGSTONE_JOIN],
    },
    VerbForm {
        verb: "meeting_stone_leave",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_MEETINGSTONE_LEAVE],
    },
    VerbForm {
        verb: "meeting_stone_status_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_MEETINGSTONE_STATUS_QUERY],
    },
    VerbForm {
        verb: "minimap_ping",
        form: Form::Same,
        opcodes: &[opcode::MSG_MINIMAP_PING],
    },
    VerbForm {
        verb: "mount_special",
        form: Form::Same,
        opcodes: &[opcode::CMSG_MOUNTSPECIAL_ANIM],
    },
    VerbForm {
        verb: "move_mode_ack",
        form: Form::Has243Form,
        opcodes: &[],
    },
    VerbForm {
        verb: "move_not_active_mover",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_MOVE_NOT_ACTIVE_MOVER],
    },
    VerbForm {
        verb: "move_spline_done",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_MOVE_SPLINE_DONE],
    },
    VerbForm {
        verb: "move_time_skipped",
        form: Form::Same,
        opcodes: &[opcode::CMSG_MOVE_TIME_SKIPPED],
    },
    VerbForm {
        verb: "name_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_NAME_QUERY],
    },
    VerbForm {
        verb: "next_cinematic_camera",
        form: Form::Same,
        opcodes: &[opcode::CMSG_NEXT_CINEMATIC_CAMERA],
    },
    VerbForm {
        verb: "npc_text_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_NPC_TEXT_QUERY],
    },
    VerbForm {
        verb: "offer_petition",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_OFFER_PETITION],
    },
    VerbForm {
        verb: "open_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_OPEN_ITEM],
    },
    VerbForm {
        verb: "opening_cinematic",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_OPENING_CINEMATIC],
    },
    VerbForm {
        verb: "page_text_query",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_PAGE_TEXT_QUERY],
    },
    VerbForm {
        verb: "pet_abandon",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PET_ABANDON],
    },
    VerbForm {
        verb: "pet_action",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PET_ACTION],
    },
    VerbForm {
        verb: "pet_cancel_aura",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PET_CANCEL_AURA],
    },
    VerbForm {
        verb: "pet_name_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PET_NAME_QUERY],
    },
    VerbForm {
        verb: "pet_rename",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_PET_RENAME],
    },
    VerbForm {
        verb: "pet_set_action",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PET_SET_ACTION],
    },
    VerbForm {
        verb: "pet_spell_autocast",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PET_SPELL_AUTOCAST],
    },
    VerbForm {
        verb: "pet_stop_attack",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PET_STOP_ATTACK],
    },
    VerbForm {
        verb: "pet_unlearn",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PET_UNLEARN],
    },
    VerbForm {
        verb: "petition_buy",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_PETITION_BUY],
    },
    VerbForm {
        verb: "petition_decline",
        form: Form::Same,
        opcodes: &[opcode::MSG_PETITION_DECLINE],
    },
    VerbForm {
        verb: "petition_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PETITION_QUERY],
    },
    VerbForm {
        verb: "petition_rename",
        form: Form::Same,
        opcodes: &[opcode::MSG_PETITION_RENAME],
    },
    VerbForm {
        verb: "petition_show_list",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PETITION_SHOWLIST],
    },
    VerbForm {
        verb: "petition_show_signatures",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PETITION_SHOW_SIGNATURES],
    },
    VerbForm {
        verb: "petition_sign",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PETITION_SIGN],
    },
    VerbForm {
        verb: "ping",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PING],
    },
    VerbForm {
        verb: "played_time",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PLAYED_TIME],
    },
    VerbForm {
        verb: "player_logout",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PLAYER_LOGOUT],
    },
    VerbForm {
        verb: "push_quest_to_party",
        form: Form::Same,
        opcodes: &[opcode::CMSG_PUSHQUESTTOPARTY],
    },
    VerbForm {
        verb: "query_next_mail_time",
        form: Form::NotEstablished,
        opcodes: &[opcode::MSG_QUERY_NEXT_MAIL_TIME],
    },
    VerbForm {
        verb: "query_time",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUERY_TIME],
    },
    VerbForm {
        verb: "quest_confirm_accept",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUEST_CONFIRM_ACCEPT],
    },
    VerbForm {
        verb: "quest_push_result",
        form: Form::Same,
        opcodes: &[opcode::MSG_QUEST_PUSH_RESULT],
    },
    VerbForm {
        verb: "quest_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUEST_QUERY],
    },
    VerbForm {
        verb: "questgiver_accept_quest",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUESTGIVER_ACCEPT_QUEST],
    },
    VerbForm {
        verb: "questgiver_choose_reward",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUESTGIVER_CHOOSE_REWARD],
    },
    VerbForm {
        verb: "questgiver_complete_quest",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUESTGIVER_COMPLETE_QUEST],
    },
    VerbForm {
        verb: "questgiver_hello",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUESTGIVER_HELLO],
    },
    VerbForm {
        verb: "questgiver_query_quest",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUESTGIVER_QUERY_QUEST],
    },
    VerbForm {
        verb: "questgiver_request_reward",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUESTGIVER_REQUEST_REWARD],
    },
    VerbForm {
        verb: "questgiver_status_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUESTGIVER_STATUS_QUERY],
    },
    VerbForm {
        verb: "questlog_remove_quest",
        form: Form::Same,
        opcodes: &[opcode::CMSG_QUESTLOG_REMOVE_QUEST],
    },
    VerbForm {
        verb: "raid_target_request",
        form: Form::NotEstablished,
        opcodes: &[opcode::MSG_RAID_TARGET_UPDATE],
    },
    VerbForm {
        verb: "raid_target_set",
        form: Form::NotEstablished,
        opcodes: &[opcode::MSG_RAID_TARGET_UPDATE],
    },
    VerbForm {
        verb: "random_roll",
        form: Form::NotEstablished,
        opcodes: &[opcode::MSG_RANDOM_ROLL],
    },
    VerbForm {
        verb: "ready_check_answer",
        form: Form::NotEstablished,
        opcodes: &[opcode::MSG_RAID_READY_CHECK],
    },
    VerbForm {
        verb: "ready_check_start",
        form: Form::NotEstablished,
        opcodes: &[opcode::MSG_RAID_READY_CHECK],
    },
    VerbForm {
        verb: "reclaim_corpse",
        form: Form::Same,
        opcodes: &[opcode::CMSG_RECLAIM_CORPSE],
    },
    VerbForm {
        verb: "repair_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_REPAIR_ITEM],
    },
    VerbForm {
        verb: "repop_request",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_REPOP_REQUEST],
    },
    VerbForm {
        verb: "request_battlefield_positions",
        form: Form::NotEstablished,
        opcodes: &[opcode::MSG_BATTLEGROUND_PLAYER_POSITIONS],
    },
    VerbForm {
        verb: "request_battlefield_score_data",
        form: Form::NotEstablished,
        opcodes: &[opcode::MSG_PVP_LOG_DATA],
    },
    VerbForm {
        verb: "request_party_member_stats",
        form: Form::Same,
        opcodes: &[opcode::CMSG_REQUEST_PARTY_MEMBER_STATS],
    },
    VerbForm {
        verb: "request_raid_info",
        form: Form::Same,
        opcodes: &[opcode::CMSG_REQUEST_RAID_INFO],
    },
    VerbForm {
        verb: "reset_instances",
        form: Form::Same,
        opcodes: &[opcode::CMSG_RESET_INSTANCES],
    },
    VerbForm {
        verb: "resurrect_response",
        form: Form::Same,
        opcodes: &[opcode::CMSG_RESURRECT_RESPONSE],
    },
    VerbForm {
        verb: "save_guild_emblem",
        form: Form::NotEstablished,
        opcodes: &[opcode::MSG_SAVE_GUILD_EMBLEM],
    },
    VerbForm {
        verb: "self_res",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SELF_RES],
    },
    VerbForm {
        verb: "sell_item",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SELL_ITEM],
    },
    VerbForm {
        verb: "send_addon_message",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_MESSAGECHAT],
    },
    VerbForm {
        verb: "send_chat",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_MESSAGECHAT],
    },
    VerbForm {
        verb: "send_mail",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_SEND_MAIL],
    },
    VerbForm {
        verb: "send_message_chat",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_MESSAGECHAT],
    },
    VerbForm {
        verb: "send_movement",
        form: Form::Has243Form,
        opcodes: &[],
    },
    VerbForm {
        verb: "set_action_button",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SET_ACTION_BUTTON],
    },
    VerbForm {
        verb: "set_actionbar_toggles",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SET_ACTIONBAR_TOGGLES],
    },
    VerbForm {
        verb: "set_active_mover",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SET_ACTIVE_MOVER],
    },
    VerbForm {
        verb: "set_ammo",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SET_AMMO],
    },
    VerbForm {
        verb: "set_faction_at_war",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SET_FACTION_ATWAR],
    },
    VerbForm {
        verb: "set_faction_inactive",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SET_FACTION_INACTIVE],
    },
    VerbForm {
        verb: "set_looking_for_group",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_SET_LOOKING_FOR_GROUP],
    },
    VerbForm {
        verb: "set_selection",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SET_SELECTION],
    },
    VerbForm {
        verb: "set_sheathed",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SETSHEATHED],
    },
    VerbForm {
        verb: "set_trade_gold",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SET_TRADE_GOLD],
    },
    VerbForm {
        verb: "set_trade_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_SET_TRADE_ITEM],
    },
    VerbForm {
        verb: "set_watched_faction",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SET_WATCHED_FACTION],
    },
    VerbForm {
        verb: "spirit_healer_activate",
        form: Form::Same,
        opcodes: &[opcode::CMSG_SPIRIT_HEALER_ACTIVATE],
    },
    VerbForm {
        verb: "split_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_SPLIT_ITEM],
    },
    VerbForm {
        verb: "stable_pet",
        form: Form::Same,
        opcodes: &[opcode::CMSG_STABLE_PET],
    },
    VerbForm {
        verb: "stable_swap_pet",
        form: Form::Same,
        opcodes: &[opcode::CMSG_STABLE_SWAP_PET],
    },
    VerbForm {
        verb: "stand_state_change",
        form: Form::Same,
        opcodes: &[opcode::CMSG_STANDSTATECHANGE],
    },
    VerbForm {
        verb: "summon_response",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_SUMMON_RESPONSE],
    },
    VerbForm {
        verb: "swap_inv_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_SWAP_INV_ITEM],
    },
    VerbForm {
        verb: "swap_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_SWAP_ITEM],
    },
    VerbForm {
        verb: "tabard_vendor_activate",
        form: Form::Same,
        opcodes: &[opcode::MSG_TABARDVENDOR_ACTIVATE],
    },
    VerbForm {
        verb: "talent_wipe_confirm",
        form: Form::Same,
        opcodes: &[opcode::MSG_TALENT_WIPE_CONFIRM],
    },
    VerbForm {
        verb: "taxi_node_status_query",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TAXINODE_STATUS_QUERY],
    },
    VerbForm {
        verb: "taxi_query_available_nodes",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TAXIQUERYAVAILABLENODES],
    },
    VerbForm {
        verb: "teleport_ack",
        form: Form::Same,
        opcodes: &[opcode::MSG_MOVE_TELEPORT_ACK],
    },
    VerbForm {
        verb: "text_emote",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TEXT_EMOTE],
    },
    VerbForm {
        verb: "toggle_cloak",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TOGGLE_CLOAK],
    },
    VerbForm {
        verb: "toggle_helm",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TOGGLE_HELM],
    },
    VerbForm {
        verb: "toggle_pvp",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TOGGLE_PVP],
    },
    VerbForm {
        verb: "trainer_buy_spell",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TRAINER_BUY_SPELL],
    },
    VerbForm {
        verb: "trainer_list",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TRAINER_LIST],
    },
    VerbForm {
        verb: "turn_in_petition",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_TURN_IN_PETITION],
    },
    VerbForm {
        verb: "tutorial_clear",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TUTORIAL_CLEAR],
    },
    VerbForm {
        verb: "tutorial_flag",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TUTORIAL_FLAG],
    },
    VerbForm {
        verb: "tutorial_reset",
        form: Form::Same,
        opcodes: &[opcode::CMSG_TUTORIAL_RESET],
    },
    VerbForm {
        verb: "unaccept_trade",
        form: Form::Same,
        opcodes: &[opcode::CMSG_UNACCEPT_TRADE],
    },
    VerbForm {
        verb: "unlearn_skill",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_UNLEARN_SKILL],
    },
    VerbForm {
        verb: "unstable_pet",
        form: Form::Same,
        opcodes: &[opcode::CMSG_UNSTABLE_PET],
    },
    VerbForm {
        verb: "use_item",
        form: Form::NotEstablished,
        opcodes: &[opcode::CMSG_USE_ITEM],
    },
    VerbForm {
        verb: "who",
        form: Form::Same,
        opcodes: &[opcode::CMSG_WHO],
    },
    VerbForm {
        verb: "worldport_ack",
        form: Form::Same,
        opcodes: &[opcode::MSG_MOVE_WORLDPORT_ACK],
    },
    VerbForm {
        verb: "wrap_item",
        form: Form::Has243Form,
        opcodes: &[opcode::CMSG_WRAP_ITEM],
    },
];
