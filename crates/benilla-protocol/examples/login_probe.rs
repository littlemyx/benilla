//! Live probe: log in as one build, list the realms, connect to the first, request the character
//! list and print it. Works for 1.12.1 (`WOW_BUILD=5875`, the default) and 2.4.3 (`8606`).
//!
//! With `WOW_CREATE=<Name>` set and no character of that name on the list, it creates one (human
//! male warrior, appearance bytes 0), prints the result, reads the list again, and prints every
//! character in full.
//!
//! With `WOW_ENTER=1` and `WOW_CHAR=<Name>` set, it then logs that character in, reads the world stream
//! until the own player's create has arrived and 5 s more, prints the own player through the typed
//! accessors (and which of them found their member absent for the build), the objects and update
//! blocks seen and every opcode read as `Other`. Every packet other than an update is counted by
//! the build's own opcode name, decoded or `Other`, with its raw bytes (`RAWP`), its decoded value
//! (`PARSED`) and any bytes the parse left unread (`TAIL`) printed for a script to check. It then
//! sends one read-only query per player and per distinct creature, game-object and item entry it
//! saw, keeps reading for 20 s answering 2.4.3 time-sync requests, requests a logout and waits for
//! it. It sends nothing else: no chat, combat or interaction, and no movement unless `WOW_WALK` is
//! set: `1` walks 4 yd along heading 0 and back, `out` only the first leg, `back` only the return
//! (heading pi), each at the server's run speed with a heartbeat every 500 ms, after checking that
//! no unit or object lies within 12 yd of the segment. It answers every order the server sends and
//! prints, as a `WALK finding` line, each correction or order. It stops and logs out if the own
//! player's health drops or an attack names it.
//! It also prints the groups 2.4.3 lays out differently (inventory and visible items with their
//! enchantments, the item objects, skills, quest log, explored zones, bytes fields, and the auras
//! and virtual items of every unit in range) as `RAW` lines that a script can check against
//! the DBCs, and the cross-checks that need no outside fact (the counts after `CHECKS`).
//!
//! `WOW_HOST` (default `localhost`, an optional `:port` for realmd), `WOW_USER` and `WOW_PASS` name
//! the server and account; the password is never printed. Refuses to run without
//! `WOW_UNATTENDED=1`, since a login kicks whoever holds the account.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use benilla_build::ClientBuild;
use benilla_protocol::messages::{
    self, Character, FieldTable, Object, ObjectFields, ObjectType, ServerPacket, TbcPacket,
};
use benilla_protocol::wire::Vector3d;
use benilla_protocol::{logon_as, CharCreateReq, CharRecord, MoverPose, PacketRead, WorldSession};

fn env(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| anyhow!("set {name}"))
}

fn main() -> Result<()> {
    if std::env::var("WOW_UNATTENDED").as_deref() != Ok("1") {
        bail!("refusing to log in without WOW_UNATTENDED=1");
    }
    let host = std::env::var("WOW_HOST").unwrap_or_else(|_| "localhost".into());
    let user = env("WOW_USER")?;
    let pass = env("WOW_PASS")?;
    let number: u16 = match std::env::var("WOW_BUILD") {
        Ok(v) => v.parse().context("WOW_BUILD is not a build number")?,
        Err(_) => benilla_build::VANILLA_1_12_1.build,
    };
    let build = ClientBuild::from_build(number)
        .ok_or_else(|| anyhow!("WOW_BUILD {number} is not a known build"))?;
    println!(
        "build {} ({}.{}.{})",
        build.build, build.version[0], build.version[1], build.version[2]
    );
    TBC.store(
        matches!(build.expansion, benilla_build::Expansion::Tbc),
        std::sync::atomic::Ordering::Relaxed,
    );

    let logon = logon_as(&build, &host, &user, &pass).context("realm logon")?;
    println!("realms: {}", logon.realms.len());
    for r in &logon.realms {
        println!(
            "  realm {:?} at {} type {} flags {:#04x} characters {}",
            r.name, r.address, r.realm_type, r.flags, r.characters
        );
    }
    let realm = logon
        .realms
        .first()
        .ok_or_else(|| anyhow!("the server lists no realm"))?;

    let mut world = WorldSession::connect_queued_as(
        &build,
        realm.address.as_str(),
        &user,
        logon.session_key,
        &mut |position| {
            println!("  queued at {position:?}");
            true
        },
    )
    .context("world auth")?;
    println!("world auth: ok, expansion byte {:?}", world.expansion());

    let mut characters = world.char_enum().context("character list")?;
    if let Ok(name) = std::env::var("WOW_CREATE") {
        if characters
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&name))
        {
            println!("create: skipped, {name} is already on the list");
        } else {
            let req = CharCreateReq {
                name,
                race: benilla_protocol::messages::RACE_HUMAN,
                class: benilla_protocol::messages::CLASS_WARRIOR,
                gender: benilla_protocol::messages::GENDER_MALE,
                skin: 0,
                face: 0,
                hair_style: 0,
                hair_color: 0,
                facial_hair: 0,
            };
            let result = world.create_character(&req).context("character create")?;
            println!(
                "create: result {result:#04x} in 1.12.1 numbering, server sent {:#04x}",
                world.last_char_create_code().unwrap_or(0)
            );
            characters = world.char_enum().context("character list after create")?;
        }
    }
    println!("characters: {}", characters.len());
    for record in &world.last_char_enum_records()? {
        print_record(record);
    }
    if std::env::var("WOW_ENTER").as_deref() == Ok("1") {
        let name = env("WOW_CHAR")?;
        enter_world(&mut world, &characters, &name)?;
    }
    drop(world);
    println!("disconnected");
    Ok(())
}

fn print_record(r: &CharRecord) {
    let c = &r.character;
    println!(
        "  {} guid {:#x} level {} race {} class {} gender {} appearance {}/{}/{}/{}/{}",
        c.name,
        c.guid,
        c.level,
        c.race,
        c.class,
        c.gender,
        c.skin,
        c.face,
        c.hair_style,
        c.hair_color,
        c.facial_hair
    );
    println!(
        "    zone {} map {} position ({:.2}, {:.2}, {:.2}) guild {} flags {:#x} first_login {}",
        c.zone, c.map, c.position.x, c.position.y, c.position.z, r.guild_id, c.flags, r.first_login
    );
    println!(
        "    pet display {} level {} family {}",
        c.pet_display_id, c.pet_level, c.pet_family
    );
    let slots: Vec<String> = r
        .slots
        .iter()
        .enumerate()
        .map(|(i, s)| match s.enchant_aura_id {
            Some(e) => format!("{i}:{}/{}/{e}", s.display_id, s.inventory_type),
            None => format!("{i}:{}/{}", s.display_id, s.inventory_type),
        })
        .collect();
    println!("    slots (display/type/enchant): {}", slots.join(" "));
}

/// What the entry run saw of the own player: the create's fields and its movement block.
struct OwnPlayer {
    fields: ObjectFields,
    position: Option<(Vector3d, f32)>,
    flags: Option<u32>,
    speeds: Option<[f32; 6]>,
    flight: Option<[f32; 2]>,
}

/// One create the run saw: what it is and its field snapshot.
struct Seen {
    guid: u64,
    ty: ObjectType,
    fields: ObjectFields,
    position: Option<Vector3d>,
}

/// Prints each own-player value and keeps which readers had nothing to say, and why.
#[derive(Default)]
struct Shown {
    none: Vec<String>,
    defaulted: Vec<String>,
}

impl Shown {
    /// An accessor that answers `Option`: `None` is either an absent member or an uncarried field.
    fn value<T: std::fmt::Debug>(&mut self, name: &str, member: u16, v: Option<T>) {
        match v {
            Some(x) => println!("    {name}: {x:?}"),
            None => {
                let why = if member == FieldTable::ABSENT {
                    "member ABSENT"
                } else {
                    "not carried"
                };
                println!("    {name}: None ({why})");
                self.none.push(format!("{name} [{why}]"));
            }
        }
    }

    /// An accessor with a default for a missing field: the default is not a reading when the member
    /// is absent, so it is flagged.
    fn plain<T: std::fmt::Debug>(&mut self, name: &str, member: u16, v: T) {
        if member == FieldTable::ABSENT {
            println!("    {name}: {v:?} (member ABSENT, a default)");
            self.defaulted.push(name.to_string());
        } else {
            println!("    {name}: {v:?}");
        }
    }
}

const POWER_NAMES: [&str; 5] = ["mana", "rage", "focus", "energy", "happiness"];

fn print_own(own: &OwnPlayer, character: &Character, verify: Option<(u32, Vector3d, f32)>) {
    let f = &own.fields;
    let t = f.table();
    let mut shown = Shown::default();
    println!("own player (typed accessors only):");
    println!("    object type: {:?}", f.object_type());
    println!("    created as: {:?}", f.created_as());
    shown.value("scale", t.object_scale_x, f.object_scale_x());
    shown.value("level", t.unit_level, f.unit_level());
    shown.value("race", t.unit_bytes_0, f.unit_race());
    shown.value("class", t.unit_bytes_0, f.unit_class());
    shown.value("gender", t.unit_bytes_0, f.unit_gender());
    shown.plain("power type", t.unit_bytes_0, f.unit_power_type());
    shown.value("health", t.unit_health, f.unit_health());
    shown.value("max health", t.unit_maxhealth, f.unit_max_health());
    for ty in 0..5u8 {
        let base = t.unit_power1;
        shown.value(
            &format!("power {ty} ({}) raw", POWER_NAMES[usize::from(ty)]),
            base,
            f.unit_power(ty),
        );
        shown.value(
            &format!("max power {ty} ({}) raw", POWER_NAMES[usize::from(ty)]),
            t.unit_maxpower1,
            f.unit_max_power(ty),
        );
    }
    shown.value(
        "faction template",
        t.unit_factiontemplate,
        f.unit_faction_template(),
    );
    shown.value("display id", t.unit_displayid, f.unit_displayid());
    shown.value(
        "native display id",
        t.unit_nativedisplayid,
        f.unit_native_displayid(),
    );
    shown.plain("unit flags", t.unit_flags, format!("{:#x}", f.unit_flags()));
    shown.value("target", t.unit_target, f.unit_target());
    shown.plain(
        "mount display id",
        t.unit_mountdisplayid,
        f.unit_mount_display_id(),
    );
    shown.value("base health", t.unit_base_health, f.unit_base_health());
    shown.plain("aura state", t.unit_aurastate, f.unit_aura_state());
    shown.plain(
        "bounding radius",
        t.unit_boundingradius,
        f.unit_bounding_radius(),
    );
    shown.plain("combat reach", t.unit_combatreach, f.unit_combat_reach());
    shown.plain("npc flags", t.unit_npc_flags, f.unit_npc_flags());
    shown.plain(
        "stand state",
        t.shape.stand_state.field,
        f.unit_stand_state(),
    );
    shown.plain(
        "shapeshift form",
        t.shape.shapeshift_form.field,
        f.unit_shapeshift_form(),
    );
    shown.value("sheath state", t.unit_bytes_2, f.unit_sheath_state());
    shown.value("strength", t.unit_stat0, f.unit_stat(0));
    shown.value("armor", t.unit_resistances, f.unit_resistance(0));
    shown.value("attack power", t.unit_attack_power, f.unit_attack_power());
    shown.value("min damage", t.unit_mindamage, f.unit_min_damage());
    shown.value("max damage", t.unit_maxdamage, f.unit_max_damage());
    shown.value(
        "main-hand attack time",
        t.unit_baseattacktime,
        f.unit_base_attack_time(0),
    );
    shown.value("aura 0", t.unit_aura, f.unit_aura(0));
    shown.value("skin", t.player_bytes, f.player_skin());
    shown.value("face", t.player_bytes, f.player_face());
    shown.value("hair style", t.player_bytes, f.player_hair_style());
    shown.value("hair color", t.player_bytes, f.player_hair_color());
    shown.value("facial hair", t.player_bytes_2, f.player_facial_hair());
    shown.value("rest state", t.player_bytes_2, f.player_rest_state());
    shown.plain(
        "player flags",
        t.player_flags,
        format!("{:#x}", f.player_flags()),
    );
    shown.value("money (copper)", t.player_field_coinage, f.player_money());
    shown.value("xp", t.player_xp, f.player_xp());
    shown.value(
        "next level xp",
        t.player_next_level_xp,
        f.player_next_level_xp(),
    );
    shown.value(
        "dodge %",
        t.player_dodge_percentage,
        f.player_dodge_percentage(),
    );
    shown.value("skill slot 0", t.player_skill_info_1_1, f.player_skill(0));
    shown.value(
        "inventory slot 3 (shirt) guid",
        t.player_inv_slot_head,
        f.player_inv_slot(3),
    );
    shown.value(
        "quest log slot 0",
        t.player_quest_log_1_1,
        f.player_quest_log(0),
    );
    shown.value(
        "combo points",
        t.shape.combo_points.field,
        f.player_combo_points(),
    );
    shown.value(
        "visible item 0",
        t.player_visible_item_1_creator,
        f.player_visible_item_entry(0),
    );
    println!("movement block:");
    match own.position {
        Some((p, o)) => println!(
            "    position ({:.2}, {:.2}, {:.2}) facing {:.5}",
            p.x, p.y, p.z, o
        ),
        None => println!("    position: none"),
    }
    println!(
        "    movement flags: {:?}",
        own.flags.map(|v| format!("{v:#x}"))
    );
    match own.speeds {
        Some([walk, run, run_back, swim, swim_back, turn]) => {
            println!("    walk {walk}  run {run}  run_back {run_back}  swim {swim}  swim_back {swim_back}  turn_rate {turn}");
        }
        None => println!("    speeds: none"),
    }
    match own.flight {
        Some([fly, fly_back]) => println!("    flight {fly}  flight_back {fly_back}"),
        None => println!("    flight speeds: none"),
    }
    if let Some((map, p, o)) = verify {
        println!(
            "compare: verify-world map {map} ({:.2}, {:.2}, {:.2}) facing {:.5}; list map {} zone {} ({:.2}, {:.2}, {:.2})",
            p.x, p.y, p.z, o, character.map, character.zone,
            character.position.x, character.position.y, character.position.z
        );
    }
    println!(
        "accessors that returned None: {} {:?}",
        shown.none.len(),
        shown.none
    );
    println!(
        "accessors that fell back to a default on an ABSENT member: {} {:?}",
        shown.defaulted.len(),
        shown.defaulted
    );
    let absent = t.absent_members();
    println!(
        "table members ABSENT for this build: {} {:?}",
        absent.len(),
        absent
    );
}

fn is_timeout(error: &str) -> bool {
    error.contains("os error 35") || error.contains("os error 11") || error.contains("timed out")
}

/// Log the named character in, read until its create block has arrived and 5 s more, report, and
/// log out. Sends nothing but the login and the logout request.
fn enter_world(world: &mut WorldSession, characters: &[Character], name: &str) -> Result<()> {
    let character = characters
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| anyhow!("WOW_CHAR {name} is not on the character list"))?
        .clone();
    println!(
        "entering the world as {} ({:#x})",
        character.name, character.guid
    );
    world.set_read_timeout(Some(Duration::from_secs(1)))?;
    world.player_login(character.guid)?;

    let begin = Instant::now();
    let mut own: Option<OwnPlayer> = None;
    let mut own_at: Option<Instant> = None;
    let mut verify: Option<(u32, Vector3d, f32)> = None;
    let mut creates: BTreeMap<String, u32> = BTreeMap::new();
    let mut by_update_type: BTreeMap<&str, u32> = BTreeMap::new();
    let mut out_of_range_guids = 0usize;
    let mut near_guids = 0usize;
    let mut move_flags: BTreeMap<String, u32> = BTreeMap::new();
    let mut splines = 0u32;
    let mut other: BTreeMap<u16, u32> = BTreeMap::new();
    let mut typed: BTreeMap<String, u32> = BTreeMap::new();
    let mut parse_errors: Vec<String> = Vec::new();
    let mut tally = Tally::default();
    let mut seen: Vec<Seen> = Vec::new();
    let mut first_own_at = None;
    loop {
        if own_at.is_some_and(|t| t.elapsed() >= Duration::from_secs(5)) {
            break;
        }
        if begin.elapsed() >= Duration::from_secs(60) {
            println!("giving up: no own-player create within 60 s");
            break;
        }
        let read = match world.recv_detailed() {
            Ok(p) => p,
            Err(e) => {
                let text = format!("{e:#}");
                if is_timeout(&text) {
                    continue;
                }
                if text.contains("parsing opcode") {
                    println!("  parse error: {text}");
                    parse_errors.push(text);
                    continue;
                }
                return Err(e.context("reading the world stream"));
            }
        };
        tally.note(&read, begin.elapsed());
        answer_time_sync(world, &mut tally)?;
        match read.packet {
            ServerPacket::LoginVerifyWorld {
                map,
                position,
                orientation,
            } => {
                println!(
                    "verify-world: map {map} position ({:.2}, {:.2}, {:.2}) facing {orientation:.5}",
                    position.x, position.y, position.z
                );
                verify = Some((map, position, orientation));
            }
            ServerPacket::CharacterLoginFailed { result } => {
                bail!("the server refused the login: reason {result}")
            }
            ServerPacket::UpdateObject { objects } => {
                for object in objects {
                    match object {
                        Object::Create {
                            guid,
                            object_type,
                            movement,
                            mask,
                        } => {
                            *by_update_type.entry("create").or_default() += 1;
                            *creates.entry(format!("{object_type:?}")).or_default() += 1;
                            if let Some(m) = movement.mover {
                                *move_flags.entry(format!("{:#x}", m.flags)).or_default() += 1;
                            }
                            splines += u32::from(movement.spline.is_some());
                            seen.push(Seen {
                                guid,
                                ty: object_type,
                                fields: mask.clone(),
                                position: movement.position.map(|(p, _)| p),
                            });
                            if guid == character.guid && own.is_none() {
                                own = Some(OwnPlayer {
                                    fields: mask,
                                    position: movement.position,
                                    flags: movement.mover.map(|m| m.flags),
                                    speeds: movement.speeds,
                                    flight: movement.flight_speeds,
                                });
                                own_at = Some(Instant::now());
                                first_own_at = Some(begin.elapsed());
                            }
                        }
                        Object::Values { .. } => *by_update_type.entry("values").or_default() += 1,
                        Object::Movement { .. } => {
                            *by_update_type.entry("movement").or_default() += 1
                        }
                        Object::OutOfRange { guids } => {
                            *by_update_type.entry("out of range").or_default() += 1;
                            out_of_range_guids += guids.len();
                        }
                        Object::Near { guids } => {
                            *by_update_type.entry("near").or_default() += 1;
                            near_guids += guids.len();
                        }
                    }
                }
            }
            ServerPacket::Other { opcode } => *other.entry(opcode).or_default() += 1,
            other_packet => *typed.entry(other_packet.name()).or_default() += 1,
        }
    }

    match &own {
        Some(own) => {
            println!(
                "own player create arrived {:.2} s after the login request",
                first_own_at.map_or(0.0, |d| d.as_secs_f64())
            );
            print_own(own, &character, verify);
            report_groups(own, character.guid, &seen);
        }
        None => println!("the own player's create never arrived"),
    }
    println!("objects created, by type: {creates:?}");
    println!(
        "movement flags on living creates: {move_flags:?}; creates riding a spline: {splines}"
    );
    println!("update blocks, by update type: {by_update_type:?} (out-of-range guids {out_of_range_guids}, near guids {near_guids})");
    println!("typed packets other than updates: {typed:?}");
    for (id, s) in seen.iter().enumerate() {
        println!(
            "RAW create {id} guid {} type {:?} entry {:?} position {:?}",
            hex_guid(s.guid),
            s.ty,
            s.fields.object_entry(),
            s.position.map(|p| (p.x, p.y, p.z))
        );
    }
    println!("parse errors: {}", parse_errors.len());
    let _ = &other;

    if let Ok(mode) = std::env::var("WOW_WALK") {
        match &own {
            Some(o) => {
                if let Err(e) = walk(world, &mut tally, character.guid, o, &seen, &mode) {
                    println!("WALK failed: {e:#}");
                }
            }
            None => println!("WALK: no own-player create, not walking"),
        }
    }

    // The queries: one per distinct entry seen (and every player), then 20 s answering time sync.
    let sent = send_queries(world, &seen)?;
    let waiting = Instant::now();
    while waiting.elapsed() < Duration::from_secs(20) {
        match world.recv_detailed() {
            Ok(read) => {
                tally.note(&read, begin.elapsed());
                answer_time_sync(world, &mut tally)?;
            }
            Err(e) => {
                let text = format!("{e:#}");
                if is_timeout(&text) {
                    continue;
                }
                if text.contains("parsing opcode") {
                    println!("  parse error: {text}");
                    parse_errors.push(text);
                    continue;
                }
                return Err(e.context("reading the world stream after the queries"));
            }
        }
    }
    println!(
        "session alive {:.1} s after the queries went out",
        waiting.elapsed().as_secs_f64()
    );
    println!("queries sent {sent:?}");
    println!("query replies (found, missed) {:?}", tally.replies);
    println!(
        "time sync: requests {:?}, replies sent {}",
        tally.sync_requests, tally.sync_replied
    );
    println!("parse errors in all: {}", parse_errors.len());
    println!("unread tails: {}", tally.tails.len());
    println!("decoded packets other than updates, by opcode:");
    for (op, n) in &tally.parsed {
        println!("    {op:#06x} x{n}  {}", opcode_label(*op));
    }
    let mut ranked: Vec<(u16, u32)> = tally.other.iter().map(|(&op, &n)| (op, n)).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!(
        "`Other` opcodes ({} distinct, by count; names from the build's own table):",
        ranked.len()
    );
    for (op, n) in ranked {
        println!("    {op:#06x} x{n}  {}", opcode_label(op));
    }

    let logout_began = Instant::now();
    let result = world.logout(Duration::from_secs(25));
    match result {
        Ok(()) => println!(
            "logout: SMSG_LOGOUT_COMPLETE after {:.1} s",
            logout_began.elapsed().as_secs_f64()
        ),
        Err(e) => println!("logout: {e:#}"),
    }
    Ok(())
}

/// Whether the run is on 2.4.3, which picks the opcode table the labels come from.
static TBC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The name of a server opcode for this build: on 2.4.3 its own table first, then (flagged) the
/// 1.12.1 one, which names several 2.4.3 numbers otherwise.
fn opcode_label(opcode: u16) -> String {
    let tbc = TBC.load(std::sync::atomic::Ordering::Relaxed);
    let own = if tbc {
        messages::tbc_opcode_name(opcode)
    } else {
        messages::opcode_name(opcode)
    };
    own.map(str::to_string)
        .or_else(|| {
            tbc.then(|| messages::opcode_name(opcode))
                .flatten()
                .map(|n| format!("(1.12.1 table only: {n})"))
        })
        .unwrap_or_else(|| "(no name known)".to_string())
}

/// Send one query per distinct entry the run saw (creature, game object, item) and one name query
/// per player; returns the counts by kind.
fn send_queries(world: &mut WorldSession, seen: &[Seen]) -> Result<BTreeMap<&'static str, u32>> {
    let mut sent: BTreeMap<&'static str, u32> = BTreeMap::new();
    let mut done: std::collections::BTreeSet<(&'static str, u32)> = Default::default();
    for s in seen {
        let entry = s.fields.object_entry().unwrap_or(0);
        match s.ty {
            ObjectType::Player => {
                world.name_query(s.guid)?;
                *sent.entry("name").or_default() += 1;
            }
            ObjectType::Unit if done.insert(("creature", entry)) => {
                world.creature_query(entry, s.guid)?;
                *sent.entry("creature").or_default() += 1;
            }
            ObjectType::GameObject if done.insert(("game object", entry)) => {
                world.gameobject_query(entry, s.guid)?;
                *sent.entry("game object").or_default() += 1;
            }
            ObjectType::Item | ObjectType::Container if done.insert(("item", entry)) => {
                world.item_query(entry, s.guid)?;
                *sent.entry("item").or_default() += 1;
            }
            _ => {}
        }
    }
    Ok(sent)
}

/// A hex string of `bytes`.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What the run read, by packet: counts, unread tails, raw bytes for the checker, the time-sync
/// exchange and the query replies.
#[derive(Default)]
struct Tally {
    /// Opcode to count, for packets the dispatch decoded.
    parsed: BTreeMap<u16, u32>,
    /// Opcode to count, for packets left as `Other`.
    other: BTreeMap<u16, u32>,
    /// `(opcode, tail, hex)` of every parse that left bytes unread.
    tails: Vec<String>,
    /// Time-sync counters asked for and not yet answered.
    pending_sync: Vec<u32>,
    /// `(counter, seconds since the login request)` of each request.
    sync_requests: Vec<(u32, f64)>,
    sync_replied: u32,
    /// Replies by query kind: `(found, missed)`.
    replies: BTreeMap<&'static str, (u32, u32)>,
}

impl Tally {
    /// Count one packet, print its raw bytes and parsed value for the checker, and queue any
    /// time-sync reply.
    fn note(&mut self, read: &PacketRead, since: Duration) {
        let op = read.opcode;
        let label = opcode_label(op);
        let is_other = matches!(read.packet, ServerPacket::Other { .. });
        *if is_other {
            self.other.entry(op).or_default()
        } else {
            self.parsed.entry(op).or_default()
        } += 1;
        if read.tail != 0 {
            let line = format!(
                "{op:#06x} {label} left {} of {} bytes unread: {}",
                read.tail,
                read.body.len(),
                hex(&read.body)
            );
            println!("TAIL {line}");
            self.tails.push(line);
        }
        let update = op == messages::opcode::SMSG_UPDATE_OBJECT
            || op == messages::opcode::SMSG_COMPRESSED_UPDATE_OBJECT;
        if !update {
            println!(
                "RAWP {op:#06x} {label} {} {}",
                read.body.len(),
                hex(&read.body)
            );
        }
        match &read.packet {
            ServerPacket::Tbc(p) => {
                println!("PARSED {} {p:?}", p.name());
                if let TbcPacket::TimeSyncRequest { counter } = p {
                    self.pending_sync.push(*counter);
                    self.sync_requests.push((*counter, since.as_secs_f64()));
                }
            }
            ServerPacket::TimeSpeed {
                hours,
                minutes,
                day_serial,
                timescale,
            } => println!("PARSED {label} {hours}:{minutes:02} day_serial {day_serial} speed {timescale}"),
            ServerPacket::TutorialFlags(t) => println!("PARSED {label} {:?}", t.bytes),
            ServerPacket::BindPoint {
                position,
                map,
                area,
            } => println!(
                "PARSED {label} ({}, {}, {}) map {map} area {area}",
                position.x, position.y, position.z
            ),
            ServerPacket::InitialSpells {
                spell_ids,
                cooldowns,
            } => println!("PARSED {label} spells {spell_ids:?} cooldowns {cooldowns:?}"),
            ServerPacket::ActionButtons { buttons } => {
                println!("PARSED {label} {} occupied {buttons:?}", buttons.len())
            }
            ServerPacket::InitializeFactions { standings } => {
                let listed: Vec<_> = standings
                    .iter()
                    .enumerate()
                    .filter(|(_, (f, s))| *f != 0 || *s != 0)
                    .map(|(i, (f, s))| format!("{i}:{f}:{s}"))
                    .collect();
                println!(
                    "PARSED {label} count {} nonzero {}",
                    standings.len(),
                    listed.join(",")
                );
            }
            ServerPacket::SetProficiency {
                item_class,
                subclass_mask,
            } => println!("PARSED {label} class {item_class} mask {subclass_mask:#x}"),
            ServerPacket::MessageChat(m) => println!("PARSED {label} {m:?}"),
            ServerPacket::Notification { text } => println!("PARSED {label} {text:?}"),
            ServerPacket::Weather {
                weather_type,
                grade,
                sound_id,
                instant,
            } => println!("PARSED {label} type {weather_type} grade {grade} sound {sound_id} instant {instant}"),
            ServerPacket::InitWorldStates(w) => println!("PARSED {label} {w:?}"),
            ServerPacket::UpdateAuraDuration { slot, remaining_ms } => {
                println!("PARSED {label} slot {slot} remaining {remaining_ms}")
            }
            ServerPacket::SpellStart(s) => println!("PARSED {label} {s:?}"),
            ServerPacket::SpellGo(s) => println!("PARSED {label} {s:?}"),
            ServerPacket::MonsterMove {
                guid,
                transport,
                start,
                spline_id,
                path,
                facing,
                stop,
                duration_ms,
                flying,
                run_mode,
            } => {
                let points: Vec<String> = path
                    .iter()
                    .map(|p| format!("({:.2},{:.2},{:.2})", p.x, p.y, p.z))
                    .collect();
                println!(
                    "PARSED {label} guid {guid:#x} transport {transport:?} start ({:.2},{:.2},{:.2}) id {spline_id} stop {stop} duration {duration_ms} flying {flying} run {run_mode} facing {facing:?} path [{}]",
                    start.x, start.y, start.z, points.join(" ")
                );
            }
            ServerPacket::DestroyObject { guid } => println!("PARSED {label} {guid:#x}"),
            ServerPacket::DefenseMessage { zone_id, text } => {
                println!("PARSED {label} zone {zone_id} {text:?}")
            }
            ServerPacket::NameQueryResponse {
                guid,
                name,
                race,
                gender,
                class,
            } => {
                self.replies.entry("name").or_default().0 += 1;
                println!("PARSED {label} guid {guid:#x} name {name:?} race {race} gender {gender} class {class}");
            }
            ServerPacket::CreatureQueryResponse { entry, info } => {
                let slot = self.replies.entry("creature").or_default();
                match info {
                    Some(i) => {
                        slot.0 += 1;
                        println!("PARSED {label} entry {entry} {i:?}");
                    }
                    None => {
                        slot.1 += 1;
                        println!("PARSED {label} entry {entry} MISS");
                    }
                }
            }
            ServerPacket::GameObjectQueryResponse { entry, info } => {
                let slot = self.replies.entry("game object").or_default();
                match info {
                    Some(i) => {
                        slot.0 += 1;
                        println!(
                            "PARSED {label} entry {entry} type {} display {} name {:?} data0..5 {:?}",
                            i.type_id,
                            i.display_id,
                            i.name,
                            &i.data[..6]
                        );
                    }
                    None => {
                        slot.1 += 1;
                        println!("PARSED {label} entry {entry} MISS");
                    }
                }
            }
            ServerPacket::ItemQueryResponse { entry, info } => {
                let slot = self.replies.entry("item").or_default();
                match info {
                    Some(i) => {
                        slot.0 += 1;
                        println!(
                            "PARSED {label} entry {entry} name {:?} class {} subclass {} display {} quality {} inventory {} stack {} flags {:#x} bag_family {} buy {} sell {} level {} required {}",
                            i.name, i.class, i.subclass, i.display_info_id, i.quality,
                            i.inventory_type, i.stackable, i.flags, i.bag_family, i.buy_price,
                            i.sell_price, i.item_level, i.required_level
                        );
                    }
                    None => {
                        slot.1 += 1;
                        println!("PARSED {label} entry {entry} MISS");
                    }
                }
            }
            _ => {}
        }
    }
}

/// Answer every queued time-sync request, as the 2.4.3 client does.
fn answer_time_sync(world: &mut WorldSession, tally: &mut Tally) -> Result<()> {
    for counter in std::mem::take(&mut tally.pending_sync) {
        world.time_sync_response(counter)?;
        tally.sync_replied += 1;
        println!("sent CMSG_TIME_SYNC_RESP for counter {counter}");
    }
    Ok(())
}

/// The values `report_groups` cross-checks internally, counted over the whole run.
#[derive(Default)]
struct Checks {
    visible_entries: u32,
    visible_entries_matching_items: u32,
    equipped_items: u32,
    items_owned_by_player: u32,
    items_contained_by_player: u32,
    durability_equals_max: u32,
    enchanted_items: u32,
    units: u32,
    units_with_auras: u32,
    auras: u32,
    aura_flag_anomalies: u32,
    units_with_virtual_items: u32,
    virtual_items: u32,
}

fn hex_guid(g: u64) -> String {
    format!("{g:#x}")
}

/// Prints the groups 2.4.3 lays out differently, through the typed accessors only, and the
/// internal cross-checks: `RAW` lines are for a script to check against the DBCs.
fn report_groups(own: &OwnPlayer, player_guid: u64, seen: &[Seen]) {
    let f = &own.fields;
    let t = f.table();
    let mut c = Checks::default();
    println!("groups report (build shape: aura slots {}, quest slots {}, visible stride {}, enchant slots {}, bank {}+{}, explored words {}):",
        f.unit_aura_slot_count(), f.player_quest_log_slot_count(), t.shape.visible_stride,
        f.item_enchant_slot_count(), f.player_bank_slot_count(), f.player_bank_bag_slot_count(),
        f.player_explored_zone_count());

    // The item objects by guid.
    let items: BTreeMap<u64, &ObjectFields> = seen
        .iter()
        .filter(|s| matches!(s.ty, ObjectType::Item | ObjectType::Container))
        .map(|s| (s.guid, &s.fields))
        .collect();
    println!("item objects created: {}", items.len());
    for (&guid, it) in &items {
        let enchants: Vec<(u8, i32)> = (0..it.item_enchant_slot_count())
            .filter_map(|slot| it.item_enchant(slot).map(|id| (slot, id)))
            .collect();
        println!(
            "RAW item guid {} entry {:?} stack {:?} durability {:?}/{:?} owner {:?} contained {:?} enchants {:?} container slots {:?}",
            hex_guid(guid),
            it.object_entry(),
            it.item_stack_count(),
            it.item_durability(),
            it.item_max_durability(),
            it.item_owner().map(hex_guid),
            it.item_contained().map(hex_guid),
            enchants,
            it.container_num_slots(),
        );
        c.items_owned_by_player += u32::from(it.item_owner() == Some(player_guid));
        c.items_contained_by_player += u32::from(it.item_contained() == Some(player_guid));
        c.durability_equals_max += u32::from(it.item_durability() == it.item_max_durability());
        c.enchanted_items += u32::from(!enchants.is_empty());
    }

    // Equipment and bag slots, with the visible item of each equipped slot.
    for i in 0..23u8 {
        let Some(guid) = f.player_inv_slot(i).filter(|&g| g != 0) else {
            continue;
        };
        let item_entry = items.get(&guid).and_then(|it| it.object_entry());
        let visible = (i < 19).then(|| f.player_visible_item_entry(i)).flatten();
        let venchants: Vec<(u8, u32)> = (0..f.player_visible_item_enchant_slot_count())
            .filter_map(|j| f.player_visible_item_enchant(i, j).map(|e| (j, e)))
            .collect();
        println!(
            "RAW equip slot {i} guid {} item entry {:?} visible entry {:?} visible enchants {:?} properties {}",
            hex_guid(guid), item_entry, visible, venchants, f.player_visible_item_properties(i)
        );
        if i < 19 {
            c.equipped_items += 1;
            c.visible_entries += u32::from(visible.is_some());
            c.visible_entries_matching_items +=
                u32::from(visible.is_some() && visible == item_entry);
        }
    }
    let visible_without_item: Vec<u8> = (0..19u8)
        .filter(|&i| {
            f.player_visible_item_entry(i).is_some()
                && f.player_inv_slot(i).filter(|&g| g != 0).is_none()
        })
        .collect();
    println!("visible items with no item in the slot: {visible_without_item:?}");
    for i in 0..16u8 {
        if let Some(guid) = f.player_pack_slot(i).filter(|&g| g != 0) {
            println!(
                "RAW pack slot {i} guid {} item entry {:?} stack {:?}",
                hex_guid(guid),
                items.get(&guid).and_then(|it| it.object_entry()),
                items.get(&guid).and_then(|it| it.item_stack_count())
            );
        }
    }
    let bank: Vec<u8> = (0..f.player_bank_slot_count())
        .filter(|&i| f.player_bank_slot(i).is_some_and(|g| g != 0))
        .collect();
    let bank_bags: Vec<u8> = (0..f.player_bank_bag_slot_count())
        .filter(|&i| f.player_bank_bag_slot(i).is_some_and(|g| g != 0))
        .collect();
    println!(
        "bank: {} slots read, occupied {bank:?}; bank bags {} slots, occupied {bank_bags:?}; bank bag slots purchased {:?}",
        f.player_bank_slot_count(),
        f.player_bank_bag_slot_count(),
        f.player_bank_bag_slots_purchased()
    );

    for slot in 0..benilla_protocol::messages::PLAYER_SKILL_SLOTS {
        if let Some(sk) = f.player_skill(slot).filter(|k| k.skill_id != 0) {
            println!(
                "RAW skill slot {slot} id {} value {} max {} step {} temp {} perm {}",
                sk.skill_id, sk.value, sk.max, sk.step, sk.temp_bonus, sk.perm_bonus
            );
        }
    }

    let log: Vec<_> = (0..f.player_quest_log_slot_count())
        .filter_map(|s| f.player_quest_log(s).map(|q| (s, q)))
        .collect();
    let occupied: Vec<_> = log.iter().filter(|(_, q)| q.quest_id != 0).collect();
    println!(
        "quest log: {} slots read, {} occupied {occupied:?}",
        log.len(),
        occupied.len()
    );

    for i in 0..f.player_explored_zone_count() {
        let w = f.player_explored_zone_slot(i);
        if w != 0 {
            println!("RAW explored word {i} = {w:#010x}");
        }
    }

    println!(
        "bytes: stand {} sheath {:?} form {} drunk {:?} rest {:?} skin/face/hair/color/facial {:?}/{:?}/{:?}/{:?}/{:?}",
        f.unit_stand_state(), f.unit_sheath_state(), f.unit_shapeshift_form(), f.player_drunk_byte(),
        f.player_rest_state(), f.player_skin(), f.player_face(), f.player_hair_style(),
        f.player_hair_color(), f.player_facial_hair()
    );
    println!(
        "honor and PvP: honor rank {:?} pvp rank {:?} medal {:?} rank bar {:?} yesterday contribution {:?} lifetime honorable {:?} session kills {:?}; combo points {:?} combo target {:?} track stealthed {} release timer {} action bars {:?}",
        f.player_honor_rank(), f.player_pvp_rank(), f.player_pvp_medal(), f.player_honor_rank_bar(),
        f.player_yesterday_contribution(), f.player_lifetime_honorable_kills(), f.player_session_kills(),
        f.player_combo_points(), f.player_combo_target_carried(), f.player_track_stealthed(),
        f.player_release_timer_running(), f.player_action_bar_toggles()
    );
    println!(
        "pet training points carried {:?}",
        f.unit_training_points_carried()
    );

    // Auras and virtual items of every unit and player, the own player included.
    let mut limits: BTreeMap<Option<u8>, u32> = BTreeMap::new();
    let mut stand: BTreeMap<u8, u32> = BTreeMap::new();
    let mut forms: BTreeMap<u8, u32> = BTreeMap::new();
    let mut with_npc_flags = 0u32;
    let mut examples: Vec<String> = Vec::new();
    for s in seen
        .iter()
        .filter(|s| matches!(s.ty, ObjectType::Unit | ObjectType::Player))
    {
        let u = &s.fields;
        c.units += 1;
        *limits.entry(u.unit_aura_positive_limit()).or_default() += 1;
        *stand.entry(u.unit_stand_state()).or_default() += 1;
        *forms.entry(u.unit_shapeshift_form()).or_default() += 1;
        with_npc_flags += u32::from(u.unit_npc_flags() != 0);
        let auras: Vec<_> = u.unit_auras().collect();
        if !auras.is_empty() {
            c.units_with_auras += 1;
        }
        for a in &auras {
            c.auras += 1;
            let exactly_one_cancel_bit =
                (a.flags & (t.shape.aura_cancelable | (t.shape.aura_cancelable << 1))).count_ones()
                    == 1;
            if t.shape.aura_flag_bits == 8 && !exactly_one_cancel_bit {
                c.aura_flag_anomalies += 1;
            }
            println!(
                "RAW aura unit {} ({:?}) slot {} spell {} flags {:#04x} level {} stacks {} helpful {} cancelable {}",
                hex_guid(s.guid), s.ty, a.slot, a.spell_id, a.flags, a.level, a.stacks,
                u.unit_aura_is_helpful(a), u.unit_aura_is_cancelable(a)
            );
            if examples.len() < 3 && s.guid != player_guid {
                examples.push(format!(
                    "unit {} slot {} spell {} level {} stacks {}",
                    hex_guid(s.guid),
                    a.slot,
                    a.spell_id,
                    a.level,
                    a.stacks
                ));
            }
        }
        let mut any_virtual = false;
        for slot in 0..3u8 {
            let display = u.unit_virtual_item_display(slot).filter(|&d| d != 0);
            let info = u.unit_virtual_item_info(slot);
            if display.is_some() {
                any_virtual = true;
                c.virtual_items += 1;
                println!(
                    "RAW vitem unit {} slot {slot} display {:?} info {:?} sheath {:?}",
                    hex_guid(s.guid),
                    display,
                    info,
                    u.unit_virtual_item_sheath(slot)
                );
            }
        }
        c.units_with_virtual_items += u32::from(any_virtual);
    }
    println!(
        "units and players created: {}, auras live: {}",
        c.units, c.auras
    );
    println!("debuff limit by unit count: {limits:?}");
    println!("stand state by unit count: {stand:?}; shapeshift form by unit count: {forms:?}; units with npc flags: {with_npc_flags}");
    println!("aura examples (other units): {examples:?}");
    println!(
        "CHECKS equipped items {} | visible entries {} | visible entries equal to the item object's entry {} | items owned by the player {} of {} | items contained by the player {} | durability equals max {} of {} | enchanted items {} | units with auras {} of {} | aura flag anomalies {} | units with virtual items {} ({} items)",
        c.equipped_items, c.visible_entries, c.visible_entries_matching_items,
        c.items_owned_by_player, items.len(), c.items_contained_by_player,
        c.durability_equals_max, items.len(), c.enchanted_items,
        c.units_with_auras, c.units, c.aura_flag_anomalies, c.units_with_virtual_items, c.virtual_items
    );
}

/// Yards of one leg of the walk.
const WALK_YARDS: f32 = 4.0;
/// The clearance every unit and object must keep from the walked segment.
const WALK_CLEARANCE: f32 = 12.0;
/// The client's movement heartbeat cadence while moving.
const WALK_HEARTBEAT: Duration = Duration::from_millis(500);

/// The state a walk pump needs.
struct Walker {
    guid: u64,
    mover: MoverPose,
    /// The own player's health at entry; any lower value ends the walk.
    health: Option<u32>,
    /// Set by a health drop or an attack naming the own player.
    abort: bool,
    /// Packets that were a correction, an order, or unexpected, as printed.
    findings: u32,
    /// Relayed `MSG_MOVE_*` of other movers.
    relays: u32,
    /// Packets read during the walk.
    read: u32,
    /// Orders answered, by what they were.
    answered: BTreeMap<&'static str, u32>,
}

/// Distance from `p` to the segment `a`..`b`.
fn segment_distance(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    ((a.0 + t * dx - p.0).powi(2) + (a.1 + t * dy - p.1).powi(2)).sqrt()
}

/// Read one packet for at most the socket's short timeout, account for it, answer what it asks.
fn walk_pump(
    world: &mut WorldSession,
    tally: &mut Tally,
    w: &mut Walker,
    begin: Instant,
) -> Result<()> {
    let read = match world.recv_detailed() {
        Ok(r) => r,
        Err(e) => {
            let text = format!("{e:#}");
            if is_timeout(&text) {
                return Ok(());
            }
            if text.contains("parsing opcode") {
                println!("WALK parse error: {text}");
                w.findings += 1;
                return Ok(());
            }
            return Err(e.context("reading the world stream during the walk"));
        }
    };
    w.read += 1;
    tally.note(&read, begin.elapsed());
    answer_time_sync(world, tally)?;
    let is_sync = matches!(
        read.packet,
        ServerPacket::Tbc(TbcPacket::TimeSyncRequest { .. })
    );
    if !is_sync {
        if let Some(what) = world.answer_movement(&read.packet, &w.mover)? {
            *w.answered.entry(what).or_default() += 1;
        }
    }
    let label = opcode_label(read.opcode);
    let mine = |g: u64| g == w.guid;
    let finding = match &read.packet {
        ServerPacket::Tbc(TbcPacket::MoveRelay { .. }) => {
            w.relays += 1;
            None
        }
        ServerPacket::Tbc(
            TbcPacket::ForceFlightSpeedChange { .. } | TbcPacket::MoveSetCanFly { .. },
        )
        | ServerPacket::Teleport { .. }
        | ServerPacket::ForceSpeedChange { .. }
        | ServerPacket::MoveMode { .. }
        | ServerPacket::KnockBack { .. }
        | ServerPacket::ClientControlUpdate { .. }
        | ServerPacket::NewWorld { .. }
        | ServerPacket::TransferPending { .. } => Some("an order or correction"),
        ServerPacket::SplineMoveMode { guid, .. } if mine(*guid) => Some("a mode change"),
        ServerPacket::SplineSpeedChange { guid, .. } if mine(*guid) => Some("a speed change"),
        ServerPacket::LogoutComplete => Some("the logout completing"),
        ServerPacket::AttackStart { attacker, victim }
        | ServerPacket::AttackStop { attacker, victim }
            if mine(*attacker) || mine(*victim) =>
        {
            w.abort = true;
            Some("an attack naming us")
        }
        ServerPacket::UpdateObject { objects } => {
            for o in objects {
                if let Object::Values { guid, mask } = o {
                    if mine(*guid) {
                        if let (Some(now), Some(then)) = (mask.unit_health(), w.health) {
                            if now < then {
                                w.abort = true;
                                println!("WALK own health {then} -> {now}");
                            }
                        }
                    }
                }
            }
            None
        }
        _ => None,
    };
    if let Some(kind) = finding {
        w.findings += 1;
        println!("WALK finding: {label} ({kind})");
    }
    Ok(())
}

/// Read and answer for `span`.
fn walk_read_for(
    world: &mut WorldSession,
    tally: &mut Tally,
    w: &mut Walker,
    begin: Instant,
    span: Duration,
) -> Result<()> {
    let until = Instant::now() + span;
    while Instant::now() < until && !w.abort {
        walk_pump(world, tally, w, begin)?;
    }
    Ok(())
}

/// One leg: set the facing to `heading`, start forward, heartbeat every 500 ms at `speed` along the
/// heading, stop `WALK_YARDS` on, with z kept as the server gave it.
fn walk_leg(
    world: &mut WorldSession,
    tally: &mut Tally,
    w: &mut Walker,
    begin: Instant,
    heading: f32,
    speed: f32,
) -> Result<()> {
    use benilla_protocol::messages::{opcode, tbc_flag};
    let start = w.mover.position;
    let (dx, dy) = (heading.cos(), heading.sin());
    let at = |travelled: f32| {
        [
            start[0] + dx * travelled,
            start[1] + dy * travelled,
            start[2],
        ]
    };
    world.send_movement(
        opcode::MSG_MOVE_SET_FACING,
        0,
        start,
        heading,
        0.0,
        0,
        None,
        None,
    )?;
    w.mover.orientation = heading;
    let began = Instant::now();
    world.send_movement(
        opcode::MSG_MOVE_START_FORWARD,
        tbc_flag::FORWARD,
        start,
        heading,
        0.0,
        0,
        None,
        None,
    )?;
    w.mover.flags = tbc_flag::FORWARD;
    println!(
        "WALK start forward heading {heading:.5} from ({:.3}, {:.3}, {:.3}) at {speed} yd/s",
        start[0], start[1], start[2]
    );
    let duration = Duration::from_secs_f32(WALK_YARDS / speed);
    let mut next_beat = WALK_HEARTBEAT;
    loop {
        let elapsed = began.elapsed();
        if elapsed >= duration || w.abort {
            break;
        }
        if elapsed >= next_beat {
            let pos = at(speed * elapsed.as_secs_f32());
            world.send_movement(
                opcode::MSG_MOVE_HEARTBEAT,
                tbc_flag::FORWARD,
                pos,
                heading,
                0.0,
                0,
                None,
                None,
            )?;
            w.mover.position = pos;
            println!(
                "WALK heartbeat at {:.0} ms ({:.3}, {:.3})",
                elapsed.as_secs_f32() * 1000.0,
                pos[0],
                pos[1]
            );
            next_beat += WALK_HEARTBEAT;
        }
        walk_pump(world, tally, w, begin)?;
    }
    // Stop where the leg ends, or where an abort caught us.
    let end = if w.abort {
        w.mover.position
    } else {
        at(WALK_YARDS)
    };
    world.send_movement(opcode::MSG_MOVE_STOP, 0, end, heading, 0.0, 0, None, None)?;
    w.mover = MoverPose {
        position: end,
        flags: 0,
        ..w.mover
    };
    println!(
        "WALK stop at ({:.3}, {:.3}, {:.3}) after {:.0} ms",
        end[0],
        end[1],
        end[2],
        began.elapsed().as_secs_f32() * 1000.0
    );
    Ok(())
}

/// The walk: `1` out and back, `out`, or `back`.
fn walk(
    world: &mut WorldSession,
    tally: &mut Tally,
    guid: u64,
    own: &OwnPlayer,
    seen: &[Seen],
    mode: &str,
) -> Result<()> {
    use benilla_protocol::messages::opcode;
    let (out, back) = match mode {
        "1" => (true, true),
        "out" => (true, false),
        "back" => (false, true),
        other => bail!("WOW_WALK {other:?} is not 1, out or back"),
    };
    let (p0, facing0) = own.position.ok_or_else(|| anyhow!("no own position"))?;
    let speed = own
        .speeds
        .map(|s| s[1])
        .ok_or_else(|| anyhow!("no run speed"))?;
    let start = [p0.x, p0.y, p0.z];
    // The segment both legs cover: from the start along the heading of the first leg.
    let first_heading = if out { 0.0f32 } else { std::f32::consts::PI };
    let end = (
        start[0] + first_heading.cos() * WALK_YARDS,
        start[1] + first_heading.sin() * WALK_YARDS,
    );
    let mut nearest: Option<(f32, u64)> = None;
    for s in seen.iter().filter(|s| s.guid != guid) {
        if let Some(p) = s.position {
            let d = segment_distance((start[0], start[1]), end, (p.x, p.y));
            if nearest.is_none_or(|(n, _)| d < n) {
                nearest = Some((d, s.guid));
            }
        }
    }
    println!(
        "WALK mode {mode}: start ({:.3}, {:.3}, {:.3}) facing {facing0:.5}, run speed {speed}, nearest object to the segment {:?}",
        start[0],
        start[1],
        start[2],
        nearest.map(|(d, g)| (d, hex_guid(g)))
    );
    if nearest.is_some_and(|(d, _)| d < WALK_CLEARANCE) {
        bail!("an object lies within {WALK_CLEARANCE} yd of the segment, not walking");
    }
    let mut w = Walker {
        guid,
        mover: MoverPose {
            guid,
            position: start,
            orientation: facing0,
            flags: 0,
        },
        health: own.fields.unit_health(),
        abort: false,
        findings: 0,
        relays: 0,
        read: 0,
        answered: BTreeMap::new(),
    };
    let begin = Instant::now();
    world.set_read_timeout(Some(Duration::from_millis(50)))?;
    world.set_active_mover(guid)?;
    if out {
        walk_leg(world, tally, &mut w, begin, 0.0, speed)?;
        walk_read_for(world, tally, &mut w, begin, Duration::from_secs(3))?;
    }
    if back && !w.abort {
        walk_leg(world, tally, &mut w, begin, std::f32::consts::PI, speed)?;
        walk_read_for(world, tally, &mut w, begin, Duration::from_secs(3))?;
    }
    // Restore the facing the character had at entry.
    let here = w.mover.position;
    world.send_movement(
        opcode::MSG_MOVE_SET_FACING,
        0,
        here,
        facing0,
        0.0,
        0,
        None,
        None,
    )?;
    w.mover.orientation = facing0;
    walk_read_for(world, tally, &mut w, begin, Duration::from_secs(1))?;
    world.set_read_timeout(Some(Duration::from_secs(1)))?;
    println!(
        "WALK done at ({:.3}, {:.3}, {:.3}) facing {facing0:.5}: {} packets read, {} findings, {} relays of other movers, answered {:?}, abort {}",
        here[0], here[1], here[2], w.read, w.findings, w.relays, w.answered, w.abort
    );
    Ok(())
}
