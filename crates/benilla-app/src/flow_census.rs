//! The flow census, a dev instrument for either client build: how far a live run got (stages with
//! timestamps), what the interface raised and registered, and what the server sent that nothing
//! consumed. It prints greppable `census:` lines as it goes and a summary at exit; it changes no
//! behaviour and compiles out of the player build.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use benilla_build::{ClientBuild, Expansion};
use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

/// Process-relative zero for every stamp.
static T0: OnceLock<Instant> = OnceLock::new();
static CENSUS: Mutex<Census> = Mutex::new(Census::new());

fn ms() -> u128 {
    T0.get_or_init(Instant::now).elapsed().as_millis()
}

/// What one interface VM reported when it was last sampled.
#[derive(Default, Clone)]
pub(crate) struct VmSnap {
    /// Error / load / warning rows: (kind tag, message, count).
    pub(crate) diagnostics: Vec<(&'static str, String, u32)>,
    pub(crate) registered: BTreeMap<String, u64>,
    pub(crate) fired: BTreeMap<String, u64>,
}

/// One opcode's fate: events produced, a parse with no event, an unknown opcode, a failed parse.
#[derive(Default, Clone, Copy)]
struct PacketTally {
    consumed: u64,
    ignored: u64,
    other: u64,
    unparseable: u64,
}

struct Census {
    build: Option<ClientBuild>,
    stages: Vec<(String, u128, String)>,
    events: BTreeMap<&'static str, u64>,
    packets: BTreeMap<u16, PacketTally>,
    vms: BTreeMap<u64, VmSnap>,
}

impl Census {
    const fn new() -> Self {
        Self {
            build: None,
            stages: Vec::new(),
            events: BTreeMap::new(),
            packets: BTreeMap::new(),
            vms: BTreeMap::new(),
        }
    }
}

fn with<R>(f: impl FnOnce(&mut Census) -> R) -> R {
    let mut c = CENSUS.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut c)
}

/// Records a stage once, on first reach, and prints it.
pub(crate) fn stage(name: &str, detail: &str) {
    let t = ms();
    let fresh = with(|c| {
        if c.stages.iter().any(|(n, _, _)| n == name) {
            false
        } else {
            c.stages.push((name.to_string(), t, detail.to_string()));
            true
        }
    });
    if fresh {
        println!("census: stage {name} t={t}ms {detail}");
    }
}

/// One decoded event reaching the dispatch, by kind; the stage edges of the login flow ride it.
pub(crate) fn note_event(ev: &SessionEvent) {
    if !crate::run_mode::dev_affordances() {
        return;
    }
    let kind: &'static str = SessionEventKind::from(ev).into();
    with(|c| *c.events.entry(kind).or_default() += 1);
    match ev {
        SessionEvent::RealmList { realms } => {
            let names: Vec<_> = realms.iter().map(|r| r.name.as_str()).collect();
            stage(
                "realm_list",
                &format!("{} realm(s): {names:?}", realms.len()),
            );
        }
        SessionEvent::CharacterList { characters, .. } => {
            let line: Vec<String> = characters
                .iter()
                .map(|c| {
                    format!(
                        "{} level {} race {} class {} map {} zone {}",
                        c.name, c.level, c.race, c.class, c.map, c.zone
                    )
                })
                .collect();
            stage(
                "character_list",
                &format!("{} character(s): {line:?}", line.len()),
            );
        }
        SessionEvent::Connected { name, .. } => stage("world_entry", &format!("as {name}")),
        SessionEvent::LoginFailed { reason, .. } => {
            println!("census: login-failed t={}ms {reason}", ms());
        }
        SessionEvent::LoggedOut => stage("logged_out", ""),
        _ => {}
    }
}

/// One packet off the world socket: `events` is what the decode produced for it.
pub(crate) fn note_packet(opcode: u16, events: &[SessionEvent]) {
    if !crate::run_mode::dev_affordances() {
        return;
    }
    with(|c| {
        let t = c.packets.entry(opcode).or_default();
        if events.is_empty() {
            t.ignored += 1;
        } else if events.iter().all(|e| {
            matches!(
                e,
                SessionEvent::PacketDropped {
                    unparseable: false,
                    ..
                }
            )
        }) {
            t.other += 1;
        } else {
            t.consumed += 1;
        }
    });
}

/// A packet whose parse failed and was skipped.
pub(crate) fn note_unparseable(opcode: u16) {
    if !crate::run_mode::dev_affordances() {
        return;
    }
    with(|c| c.packets.entry(opcode).or_default().unparseable += 1);
}

/// The missing name in "attempt to call global 'X' (a nil value)", for 5.0 and 5.1 quoting.
fn missing_verb(message: &str) -> Option<String> {
    let rest = message.split("attempt to call ").nth(1)?;
    let (what, rest) = rest.split_once(' ')?;
    if !matches!(what, "global" | "method" | "field" | "local" | "upvalue") {
        return None;
    }
    let open = rest.find(['\'', '`', '"'])?;
    let rest = &rest[open + 1..];
    let close = rest.find(['\'', '"'])?;
    Some(format!("{what} {}", &rest[..close]))
}

fn top<K: Clone + Ord>(map: BTreeMap<K, u64>, n: usize) -> Vec<(K, u64)> {
    let mut v: Vec<_> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

/// The summary, one greppable line per fact.
pub(crate) fn print_summary() {
    with(|c| {
        let tbc = matches!(c.build.map(|b| b.expansion), Some(Expansion::Tbc));
        let name = |op: u16| {
            if tbc {
                benilla_protocol::messages::tbc_opcode_name(op)
            } else {
                benilla_protocol::messages::opcode_name(op)
            }
            .unwrap_or("?")
        };
        println!(
            "census: build {}",
            c.build.map_or("none".into(), |b| format!(
                "{} ({})",
                b.build,
                if tbc { "2.4.3" } else { "1.12.1" }
            ))
        );
        for (n, t, d) in &c.stages {
            println!("census: stage-reached {n} t={t}ms {d}");
        }
        // Lua errors across every VM sampled.
        let (mut errors, mut loads, mut warns) = (0u64, 0u64, 0u64);
        let mut distinct: BTreeMap<String, u64> = BTreeMap::new();
        let mut missing: BTreeMap<String, u64> = BTreeMap::new();
        for vm in c.vms.values() {
            for (kind, msg, n) in &vm.diagnostics {
                let n = u64::from(*n);
                match *kind {
                    "error" => {
                        errors += n;
                        *distinct.entry(msg.clone()).or_default() += n;
                        if let Some(v) = missing_verb(msg) {
                            *missing.entry(v).or_default() += n;
                        }
                    }
                    "load" => loads += n,
                    _ => warns += n,
                }
            }
        }
        println!(
            "census: lua-errors total={errors} distinct={} load-failures={loads} warnings={warns}",
            distinct.len()
        );
        for (msg, n) in top(distinct, 10) {
            println!("census: lua-error x{n} {}", msg.replace('\n', " | "));
        }
        println!(
            "census: missing-verbs-at-run-time distinct={}",
            missing.len()
        );
        for (v, n) in top(missing, 15) {
            println!("census: missing-verb x{n} {v}");
        }
        // Events: registered by the stock files vs fired by the engine.
        let mut registered: BTreeMap<String, u64> = BTreeMap::new();
        let mut fired: BTreeMap<String, u64> = BTreeMap::new();
        for vm in c.vms.values() {
            for (k, v) in &vm.registered {
                *registered.entry(k.clone()).or_default() += v;
            }
            for (k, v) in &vm.fired {
                *fired.entry(k.clone()).or_default() += v;
            }
        }
        let never: BTreeMap<String, u64> = registered
            .iter()
            .filter(|(k, _)| !fired.contains_key(*k))
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        println!(
            "census: ui-events registered-distinct={} registrations={} fired-distinct={} fired-total={} registered-never-fired={}",
            registered.len(),
            registered.values().sum::<u64>(),
            fired.len(),
            fired.values().sum::<u64>(),
            never.len()
        );
        for (k, v) in top(never, 10) {
            println!("census: never-fired x{v} registrations {k}");
        }
        for (k, v) in top(fired, 12) {
            println!("census: fired x{v} {k}");
        }
        // Packets.
        let (mut consumed, mut ignored, mut other, mut bad) = (0u64, 0u64, 0u64, 0u64);
        for t in c.packets.values() {
            consumed += t.consumed;
            ignored += t.ignored;
            other += t.other;
            bad += t.unparseable;
        }
        println!(
            "census: packets opcodes={} consumed={consumed} parsed-no-event={ignored} other={other} unparseable={bad}",
            c.packets.len()
        );
        let mut ign: Vec<(u16, PacketTally)> = c
            .packets
            .iter()
            .filter(|(_, t)| t.ignored > 0)
            .map(|(o, t)| (*o, *t))
            .collect();
        ign.sort_by_key(|(_, t)| std::cmp::Reverse(t.ignored));
        for (op, t) in ign {
            println!(
                "census: packet-no-event x{} {op:#06x} {}",
                t.ignored,
                name(op)
            );
        }
        let mut oth: Vec<(u16, PacketTally)> = c
            .packets
            .iter()
            .filter(|(_, t)| t.other > 0 || t.unparseable > 0)
            .map(|(o, t)| (*o, *t))
            .collect();
        oth.sort_by_key(|(_, t)| std::cmp::Reverse(t.other + t.unparseable));
        for (op, t) in oth {
            println!(
                "census: packet-other x{} unparseable x{} {op:#06x} {}",
                t.other,
                t.unparseable,
                name(op)
            );
        }
        let mut cons: Vec<(u16, u64)> = c
            .packets
            .iter()
            .filter(|(_, t)| t.consumed > 0)
            .map(|(o, t)| (*o, t.consumed))
            .collect();
        cons.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let list: Vec<String> = cons
            .iter()
            .take(15)
            .map(|(o, n)| format!("{}x{n}", name(*o)))
            .collect();
        println!("census: packets-consumed-top {}", list.join(" "));
        let evs: Vec<String> = c.events.iter().map(|(k, n)| format!("{k}x{n}")).collect();
        println!("census: session-events {}", evs.join(" "));
    });
}

/// The census as an app plugin: stage edges off the client state, the own player's first position,
/// the VM sample, a heartbeat and the exit summary.
pub(crate) struct FlowCensusPlugin;

impl Plugin for FlowCensusPlugin {
    fn build(&self, app: &mut App) {
        T0.get_or_init(Instant::now);
        let build = app
            .world()
            .get_resource::<crate::session_build::SessionBuild>()
            .copied()
            .unwrap_or_default()
            .0;
        with(|c| c.build = Some(build));
        stage("startup", &format!("build {}", build.build));
        app.add_systems(
            Update,
            (
                state_stages,
                own_player_stage,
                crate::ui_script::flow_probe::sample_ui,
                heartbeat,
            ),
        )
        .add_systems(Last, crate::ui_script::flow_probe::summary_on_exit);
    }
}

fn state_stages(state: Option<Res<State<crate::char_select::ClientState>>>) {
    if let Some(state) = state {
        stage(&format!("state_{:?}", state.get()), "");
    }
}

fn own_player_stage(
    own: Query<&Transform, With<crate::net::SelfPlayer>>,
    map: Option<Res<benilla_world::world_map::CurrentMap>>,
) {
    if let Ok(t) = own.single() {
        let [x, y, z] = benilla_assets::coords::bevy_to_wow(t.translation);
        let map = map.map_or(-1, |m| m.0 as i64);
        stage(
            "own_player",
            &format!("map {map} @ [{x:.2}, {y:.2}, {z:.2}]"),
        );
    }
}

pub(crate) static NEXT_SAMPLE_MS: AtomicU64 = AtomicU64::new(0);
static NEXT_BEAT_MS: AtomicU64 = AtomicU64::new(15_000);

/// True once per `every_ms` of process time, per timer.
pub(crate) fn due(next: &AtomicU64, every_ms: u64) -> bool {
    let now = ms() as u64;
    if now < next.load(Ordering::Relaxed) {
        return false;
    }
    next.store(now + every_ms, Ordering::Relaxed);
    true
}

fn heartbeat() {
    if !due(&NEXT_BEAT_MS, 15_000) {
        return;
    }
    let last = with(|c| {
        c.stages
            .last()
            .map(|(n, t, _)| format!("{n}@{t}ms"))
            .unwrap_or_default()
    });
    println!("census: heartbeat t={}ms last-stage={last}", ms());
}

/// Replaces one VM's earlier sample.
pub(crate) fn record_vm(session: u64, snap: VmSnap) {
    with(|c| {
        c.vms.insert(session, snap);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_global_is_named_in_either_lua_dialect() {
        assert_eq!(
            missing_verb("attempt to call global 'GetFoo' (a nil value)").as_deref(),
            Some("global GetFoo")
        );
        assert_eq!(
            missing_verb("x.lua:3: attempt to call global `GetFoo' (a nil value)").as_deref(),
            Some("global GetFoo")
        );
        assert_eq!(
            missing_verb("attempt to call method 'SetFoo' (a nil value)").as_deref(),
            Some("method SetFoo")
        );
        assert_eq!(missing_verb("attempt to index a nil value"), None);
    }

    #[test]
    fn packets_are_told_apart_by_what_the_decode_made_of_them() {
        // A player build counts nothing.
        if !crate::run_mode::dev_affordances() {
            return;
        }
        note_packet(0xFFF0, &[]);
        note_packet(
            0xFFF1,
            &[SessionEvent::PacketDropped {
                opcode: 0xFFF1,
                unparseable: false,
            }],
        );
        note_packet(0xFFF2, &[SessionEvent::LoggedOut]);
        note_unparseable(0xFFF3);
        with(|c| {
            assert_eq!(c.packets[&0xFFF0].ignored, 1);
            assert_eq!(c.packets[&0xFFF1].other, 1);
            assert_eq!(c.packets[&0xFFF2].consumed, 1);
            assert_eq!(c.packets[&0xFFF3].unparseable, 1);
        });
    }
}
