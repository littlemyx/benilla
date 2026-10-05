use std::io::Read;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use benilla_srp::vanilla_header::DecrypterHalf;

use benilla_build::ClientBuild;

use crate::messages::{self, FieldTable, ServerPacket};

use super::recv_packet;

/// The read half of a split [`WorldSession`](super::WorldSession): cloned socket and decrypter.
pub struct WorldReader {
    pub(super) stream: TcpStream,
    pub(super) decrypter: DecrypterHalf,
    pub(super) build: ClientBuild,
    /// The build's update-field indices, which every update object is read through; `None` for a
    /// build without a table, whose parser never takes one.
    pub(super) fields: Option<&'static FieldTable>,
    /// The menu id of the last 2.4.3 `SMSG_GOSSIP_MESSAGE`, shared with the writer, whose
    /// `CMSG_GOSSIP_SELECT_OPTION` echoes it.
    pub(super) gossip_menu: Arc<AtomicU32>,
    /// Whether [`Self::poll`] keeps each body for [`Self::last_body`].
    pub(super) keep_body: bool,
    pub(super) last_body: Vec<u8>,
}

impl WorldReader {
    /// Keep the body of every polled packet, for the session record.
    pub fn keep_bodies(&mut self) {
        self.keep_body = true;
    }

    /// The body of the packet the last [`Self::poll`] read; empty unless [`Self::keep_bodies`].
    pub fn last_body(&self) -> &[u8] {
        &self.last_body
    }

    /// Read + decrypt one server packet (blocking).
    pub fn recv(&mut self) -> Result<ServerPacket> {
        recv_packet(
            &mut self.stream,
            Some(&mut self.decrypter),
            &self.build,
            self.fields,
        )
    }

    /// Read one packet and decode it into a [`crate::Poll`]; errors only when the socket fails. The
    /// whole body is read before parsing, so an unparseable packet is skipped, the stream aligned.
    pub fn poll(&mut self) -> Result<crate::Poll> {
        let mut header = [0u8; 4];
        if let Err(e) = self.stream.read_exact(&mut header) {
            return Err(anyhow!("world stream closed: {e}"));
        }
        self.decrypter.decrypt(&mut header);
        let size = u16::from_be_bytes([header[0], header[1]]);
        let opcode = u16::from_le_bytes([header[2], header[3]]);
        let body_len = size.saturating_sub(2) as usize;
        let mut body = vec![0u8; body_len];
        if let Err(e) = self.stream.read_exact(&mut body) {
            return Err(anyhow!("world stream closed: {e}"));
        }
        if self.keep_body {
            self.last_body.clone_from(&body);
        }
        if opcode == messages::tbc_opcode::SMSG_GOSSIP_MESSAGE
            && matches!(self.build.expansion, benilla_build::Expansion::Tbc)
        {
            if let Some(menu) = body.get(8..12) {
                let menu = u32::from_le_bytes([menu[0], menu[1], menu[2], menu[3]]);
                self.gossip_menu.store(menu, Ordering::Relaxed);
            }
        }
        match messages::parse_server_with_tail_for(&self.build, self.fields, opcode, &body) {
            Ok((packet, tail)) => Ok(crate::Poll::Events {
                opcode,
                events: crate::decode(packet),
                tail,
            }),
            Err(e) => Ok(crate::Poll::Skipped {
                opcode,
                reason: format!(
                    "opcode {opcode:#06x} ({}): {e} [{body_len}B: {}]",
                    messages::opcode_name(opcode).unwrap_or("?"),
                    hex_preview(&body, 64)
                ),
            }),
        }
    }
}

/// Hex of the first `max` bytes of `body`, `…` when truncated, for decoding a packet by hand.
fn hex_preview(body: &[u8], max: usize) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for b in body.iter().take(max) {
        let _ = write!(s, "{b:02x} ");
    }
    if body.len() > max {
        s.push('…');
    }
    s.trim_end().to_string()
}
