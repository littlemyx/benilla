//! The world session's write half: [`WorldWriter`] and one method per thing the player can do.
//! Each family module mirrors its namesake in `crate::messages`, which builds the bodies.

use std::net::TcpStream;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use anyhow::Result;
use benilla_srp::vanilla_header::EncrypterHalf;

use crate::messages;

use super::send_packet;

pub use disposition::{form_of, refusal_on_tbc, Form, VerbForm, VerbRefused, VERBS};

mod action_bar;
mod area_trigger;
mod attack;
mod auction;
mod bank;
mod battlefield;
mod binder;
mod channel;
mod chat;
mod death;
mod disposition;
#[cfg(test)]
mod disposition_tests;
mod duel;
mod gameobject;
mod gm_ticket;
mod gossip;
mod group;
mod guild;
mod instance;
mod items;
mod lifecycle;
mod loot;
mod mail;
mod meeting_stone;
mod names;
mod pet;
mod petition;
mod player_flags;
mod pose;
mod progression;
mod pvp;
mod quest;
mod reputation;
mod selection;
mod self_movement;
mod skills;
mod social;
mod spells;
mod stable;
mod summon;
mod tabard;
mod taxi;
#[cfg(test)]
mod tbc_forms_tests;
mod trade;
mod trainer;
mod tutorial;
mod vendor;

/// Write half of a split [`WorldSession`](super::WorldSession): a cloned socket and the encrypter.
/// Movement sends need the active player confirmed as mover first
/// ([`WorldSession::set_active_mover`](super::WorldSession::set_active_mover)). Positions are raw
/// WoW yards; `orientation` is radians.
pub struct WorldWriter {
    pub(super) stream: TcpStream,
    pub(super) encrypter: EncrypterHalf,
    /// Every packet that reached the socket since the last drain, as `(opcode, body length)`;
    /// `None` until [`Self::watch_sends`]. Pushed only after a successful write.
    pub(super) sent: Option<Vec<(u16, usize)>>,
    /// The character's faction language, set at login from its race, which a chat send naming no
    /// language carries. vmangos drops the whole message, dot-commands included, when the
    /// character does not know the language.
    pub(super) chat_language: u32,
    /// Whether the session is on 2.4.3, which picks the layout of the movement bodies.
    pub(super) tbc: bool,
    /// What a 2.4.3 send carries that only the session knows.
    pub(super) tbc_state: TbcSendState,
}

/// The per-session values 2.4.3 bodies carry: the cast counter and the menu id of the gossip window
/// on screen, which the reader records off `SMSG_GOSSIP_MESSAGE`; and the sent bodies, kept for the
/// session record once [`WorldWriter::watch_bodies`] asks.
#[derive(Default)]
pub(super) struct TbcSendState {
    pub(super) cast_count: u8,
    pub(super) gossip_menu: Arc<AtomicU32>,
    pub(super) bodies: Option<Vec<(u16, Vec<u8>)>>,
}

impl WorldWriter {
    /// Frame, encrypt and write one packet: the sole write path every verb goes through.
    fn send(&mut self, opcode: u16, body: &[u8]) -> Result<()> {
        // A verb with no established 2.4.3 form never puts its 1.12.1 bytes on a 2.4.3 socket.
        if self.tbc {
            if let Some(refused) = refusal_on_tbc(opcode) {
                return Err(refused.into());
            }
        }
        let sent = send_packet(&mut self.stream, Some(&mut self.encrypter), opcode, body);
        if sent.is_ok() {
            if let Some(log) = &mut self.sent {
                log.push((opcode, body.len()));
            }
            if let Some(bodies) = &mut self.tbc_state.bodies {
                bodies.push((opcode, body.to_vec()));
            }
        }
        sent
    }

    /// A 1.12.1 inventory slot as this session's build numbers it: 2.4.3 moves the bank bag, buyback
    /// and keyring slots (`messages::slot_to_tbc`), and a slot with no 2.4.3 number is an error.
    fn slot(&self, slot: u8) -> Result<u8> {
        if !self.tbc {
            return Ok(slot);
        }
        messages::slot_to_tbc(slot)
            .ok_or_else(|| anyhow::anyhow!("inventory slot {slot} has no 2.4.3 number"))
    }

    /// Start recording what reaches the socket; a second call keeps what is not yet drained.
    pub fn watch_sends(&mut self) {
        self.sent.get_or_insert_with(Vec::new);
    }

    /// Start keeping the body of every packet that reaches the socket, for the session record.
    pub fn watch_bodies(&mut self) {
        self.tbc_state.bodies.get_or_insert_with(Vec::new);
    }

    /// Hand over and clear the kept `(opcode, body)` pairs, in send order.
    pub fn drain_bodies(&mut self, mut each: impl FnMut(u16, &[u8])) {
        if let Some(log) = &mut self.tbc_state.bodies {
            for (opcode, body) in log.drain(..) {
                each(opcode, &body);
            }
        }
    }

    /// Hand over and clear the recorded `(opcode, body length)` pairs, in send order.
    pub fn drain_sent(&mut self, mut each: impl FnMut(u16, usize)) {
        if let Some(log) = &mut self.sent {
            for (opcode, len) in log.drain(..) {
                each(opcode, len);
            }
        }
    }
}
