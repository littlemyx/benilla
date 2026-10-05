//! The session record, a dev instrument: `WOW_SESSION_RECORD=<dir>` writes `<dir>/record.log`, the
//! whole play session as one text file a person can leave behind and a script can read. Every
//! world packet in both directions with its parse outcome and raw bytes, every writer verb with
//! sent or refused, every Lua error, interface event and missing verb, the census at exit and a
//! marker line per press of the marker chord; screenshots go into the same directory. Writes are
//! queued to one thread that buffers them, so no game thread waits on the disk. The auth exchange
//! never passes here (it runs before the world session splits). Compiles out of the player build.
//!
//! Format: line 1 is `# benilla session record v1 t0=<unix seconds>`; every other line is
//! `t=<ms since start> <kind> <fields>`, one line per record, newlines in a field written as ` | `.
//! `pkt in|out <opcode> <NAME> len=<n> [parse=<outcome> tail=<n>] hex=<bytes>`; `verb <Command>
//! sent|refused:<verb>|failed`; `lua <kind> x<count> <message>`; `missing-verb <name>`;
//! `event <name> args=<n>`; `marker <n>`; `census <line>`; `stage <name> <detail>`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use bevy::prelude::*;

use crate::ui_chat::{ChatEvent, ChatEventKind, ChatLog};

/// The chord that drops a marker into the record, printed at startup.
pub(crate) const MARKER_CHORD: &str = "Ctrl+Shift+K";

enum Msg {
    Line(String),
    Flush(mpsc::Sender<()>),
}

struct Recorder {
    dir: PathBuf,
    t0: Instant,
    tx: Mutex<mpsc::Sender<Msg>>,
}

static RECORDER: OnceLock<Option<Recorder>> = OnceLock::new();

fn recorder() -> Option<&'static Recorder> {
    RECORDER
        .get_or_init(|| {
            if !crate::run_mode::dev_affordances() {
                return None;
            }
            let dir =
                PathBuf::from(std::env::var_os("WOW_SESSION_RECORD").filter(|v| !v.is_empty())?);
            std::fs::create_dir_all(&dir)
                .and_then(|()| std::fs::File::create(dir.join("record.log")))
                .map_err(|e| eprintln!("session-record: cannot create {}: {e}", dir.display()))
                .ok()
                .map(|file| start(dir, file))
        })
        .as_ref()
}

fn start(dir: PathBuf, file: std::fs::File) -> Recorder {
    let (tx, rx) = mpsc::channel::<Msg>();
    let t0_wall = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    std::thread::Builder::new()
        .name("session-record".into())
        .spawn(move || {
            let mut out = std::io::BufWriter::with_capacity(1 << 16, file);
            let _ = writeln!(out, "# benilla session record v1 t0={t0_wall:.3}");
            loop {
                match rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(Msg::Line(l)) => {
                        let _ = writeln!(out, "{l}");
                    }
                    Ok(Msg::Flush(ack)) => {
                        let _ = out.flush();
                        let _ = ack.send(());
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        let _ = out.flush();
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        let _ = out.flush();
                        return;
                    }
                }
            }
        })
        .expect("spawn session-record thread");
    Recorder {
        dir,
        t0: Instant::now(),
        tx: Mutex::new(tx),
    }
}

/// Whether the record is on.
pub(crate) fn enabled() -> bool {
    recorder().is_some()
}

/// The record's directory, where screenshots go too.
pub(crate) fn dir() -> Option<&'static Path> {
    recorder().map(|r| r.dir.as_path())
}

/// Append `<kind> <msg>` stamped with the time since start; a newline in `msg` becomes ` | `.
pub(crate) fn line(kind: &str, msg: &str) {
    let Some(r) = recorder() else { return };
    let t = r.t0.elapsed().as_millis();
    let msg = msg.replace('\n', " | ");
    if let Ok(tx) = r.tx.lock() {
        let _ = tx.send(Msg::Line(format!("t={t} {kind} {msg}")));
    }
}

fn hex(body: &[u8]) -> String {
    use std::fmt::Write as _;
    body.iter()
        .fold(String::with_capacity(body.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// One packet off the world socket: how the decode took it, and its bytes.
pub(crate) fn packet_in(opcode: u16, name: &str, parse: &str, tail: usize, body: &[u8]) {
    if enabled() {
        line(
            "pkt",
            &format!(
                "in {opcode:#06x} {name} len={} parse={parse} tail={tail} hex={}",
                body.len(),
                hex(body)
            ),
        );
    }
}

/// One packet that reached the world socket.
pub(crate) fn packet_out(opcode: u16, name: &str, body: &[u8]) {
    if enabled() {
        line(
            "pkt",
            &format!(
                "out {opcode:#06x} {name} len={} hex={}",
                body.len(),
                hex(body)
            ),
        );
    }
}

/// An inbound opcode's name in the session's build.
pub(crate) fn in_name(build: &benilla_build::ClientBuild, opcode: u16) -> &'static str {
    let names = if matches!(build.expansion, benilla_build::Expansion::Tbc) {
        benilla_protocol::messages::tbc_opcode_name(opcode)
    } else {
        benilla_protocol::messages::opcode_name(opcode)
    };
    names.unwrap_or("?")
}

/// An outbound opcode's name; the record is read against 2.4.3, so its table goes first.
pub(crate) fn out_name(opcode: u16) -> &'static str {
    benilla_protocol::messages::tbc_opcode_name(opcode)
        .or_else(|| benilla_protocol::messages::opcode_name(opcode))
        .unwrap_or("?")
}

/// How the decode took a packet, as the census counts it: `no-event`, `other` (known opcode, no
/// reader) or the kinds of the events it made.
pub(crate) fn parse_outcome(events: &[benilla_protocol::SessionEvent]) -> String {
    use benilla_protocol::{SessionEvent, SessionEventKind};
    if events.is_empty() {
        return "no-event".into();
    }
    if events.iter().all(|e| {
        matches!(
            e,
            SessionEvent::PacketDropped {
                unparseable: false,
                ..
            }
        )
    }) {
        return "other".into();
    }
    let kinds: Vec<&'static str> = events
        .iter()
        .map(|e| SessionEventKind::from(e).into())
        .collect();
    kinds.join("+")
}

/// The variant name of a command: its `Debug` text up to the first space, brace or parenthesis.
pub(crate) fn command_name(command: &impl std::fmt::Debug) -> String {
    let text = format!("{command:?}");
    let end = text.find([' ', '{', '(']).unwrap_or(text.len());
    text[..end].to_string()
}

/// One writer verb called: `outcome` is `sent`, `refused:<verb>` or `failed`.
pub(crate) fn verb(command: &str, outcome: &str) {
    if enabled() {
        line("verb", &format!("{command} {outcome}"));
    }
}

/// Flush the buffer to disk and wait for it, at exit.
pub(crate) fn finish() {
    let Some(r) = recorder() else { return };
    let (ack_tx, ack_rx) = mpsc::channel();
    if let Ok(tx) = r.tx.lock() {
        if tx.send(Msg::Flush(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(Duration::from_secs(2));
        }
    }
}

/// Lines for the chat frame that it has not yet shown.
static REFUSALS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn announce(line: String) {
    if crate::run_mode::dev_affordances() {
        if let Ok(mut q) = REFUSALS.lock() {
            q.push(line);
        }
    }
}

/// A verb the writer refused on 2.4.3, once per verb (the caller dedups): queued for the chat line.
pub(crate) fn note_refused(verb: &str) {
    announce(format!("benilla: not sent on 2.4.3 yet: {verb}"));
}

/// A server packet no 2.4.3 reader is enabled for, once per opcode: queued for the chat line.
pub(crate) fn note_unread(name: &str) {
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    if let Ok(mut seen) = SEEN.lock() {
        if seen.iter().any(|n| n == name) {
            return;
        }
        seen.push(name.to_string());
    }
    announce(format!("benilla: not read on 2.4.3 yet: {name}"));
}

/// Says in the chat frame, as a system line, why a click or a window did nothing: the verb has no
/// 2.4.3 form, or the packet no 2.4.3 reader.
fn announce_refusals(mut chat: Option<ResMut<ChatLog>>) {
    let Ok(mut q) = REFUSALS.lock() else { return };
    if q.is_empty() {
        return;
    }
    let Some(chat) = chat.as_mut() else { return };
    for line in q.drain(..) {
        chat.push_event(ChatEvent::text_only(ChatEventKind::System, line));
    }
}

/// The marker chord: a numbered line in the record and one in the chat frame.
fn markers(
    keys: Res<ButtonInput<KeyCode>>,
    mut chat: Option<ResMut<ChatLog>>,
    mut count: Local<u32>,
) {
    if !crate::run_mode::dev_chord(&keys, KeyCode::KeyK) {
        return;
    }
    *count += 1;
    line("marker", &count.to_string());
    if let Some(chat) = chat.as_mut() {
        chat.push_event(ChatEvent::text_only(
            ChatEventKind::System,
            format!("benilla: marker {} recorded", *count),
        ));
    }
}

/// Opens the record at startup and carries the refusal line and the marker chord.
pub(crate) struct SessionRecordPlugin;

impl Plugin for SessionRecordPlugin {
    fn build(&self, app: &mut App) {
        if let Some(dir) = dir() {
            println!(
                "session-record: writing {}/record.log; {MARKER_CHORD} drops a marker; screenshots go there too",
                dir.display()
            );
            line("start", &format!("marker-chord={MARKER_CHORD}"));
        }
        app.add_systems(Update, (announce_refusals, markers));
        app.add_systems(
            Last,
            finish_on_exit.after(crate::ui_script::flow_probe::summary_on_exit),
        );
    }
}

fn finish_on_exit(mut exits: MessageReader<AppExit>) {
    if exits.read().next().is_some() {
        finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_named_by_its_variant() {
        #[derive(Debug)]
        #[allow(dead_code)]
        enum C {
            Plain,
            Tuple(u8),
            Fields { _a: u8 },
        }
        assert_eq!(command_name(&C::Plain), "Plain");
        assert_eq!(command_name(&C::Tuple(1)), "Tuple");
        assert_eq!(command_name(&C::Fields { _a: 1 }), "Fields");
    }

    #[test]
    fn a_refused_verb_and_an_unread_packet_are_queued_for_the_chat_frame_once() {
        if !crate::run_mode::dev_affordances() {
            return;
        }
        note_refused("zz_test_verb");
        note_unread("ZZ_TEST_PACKET");
        note_unread("ZZ_TEST_PACKET");
        let q = REFUSALS.lock().unwrap();
        assert!(q.contains(&"benilla: not sent on 2.4.3 yet: zz_test_verb".to_string()));
        let unread = "benilla: not read on 2.4.3 yet: ZZ_TEST_PACKET";
        assert_eq!(q.iter().filter(|l| *l == unread).count(), 1);
    }

    #[test]
    fn bodies_are_written_as_contiguous_lowercase_hex() {
        assert_eq!(hex(&[0x00, 0xab, 0x10]), "00ab10");
        assert_eq!(hex(&[]), "");
    }

    #[test]
    fn without_the_switch_nothing_is_recorded() {
        if std::env::var_os("WOW_SESSION_RECORD").is_none() {
            assert!(!enabled());
            assert!(dir().is_none());
        }
    }
}
