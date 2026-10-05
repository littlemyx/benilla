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
//! blocks seen and every opcode read as `Other`, then requests a logout and waits for it. It sends
//! nothing but the login and the logout request: no movement, chat, combat or interaction.
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
    self, Character, FieldTable, Object, ObjectFields, ObjectType, ServerPacket,
};
use benilla_protocol::wire::Vector3d;
use benilla_protocol::{logon_as, CharCreateReq, CharRecord, WorldSession};

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
        let packet = match world.recv() {
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
        match packet {
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
    let mut ranked: Vec<(u16, u32)> = other.iter().map(|(&op, &n)| (op, n)).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!(
        "`Other` opcodes ({} distinct, by count; names from cmangos-tbc, else the 1.12.1 table):",
        ranked.len()
    );
    for (op, n) in ranked {
        println!(
            "    {op:#06x} x{n}  {}",
            tbc_opcode_name(op)
                .map(str::to_string)
                .or_else(|| {
                    messages::opcode_name(op).map(|n| format!("(1.12.1 table only: {n})"))
                })
                .unwrap_or_else(|| "(no name known)".to_string())
        );
    }
    println!("parse errors: {}", parse_errors.len());

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

/// Names of the 2.4.3 opcodes the entry run reads, from cmangos-tbc `Opcodes.h`: the 1.12.1 table
/// names several of these numbers otherwise or not at all (0x209, 0x33B), so it is only a fallback.
fn tbc_opcode_name(opcode: u16) -> Option<&'static str> {
    Some(match opcode {
        0x042 => "SMSG_LOGIN_SETTIMESPEED",
        0x067 => "SMSG_CONTACT_LIST",
        0x096 => "SMSG_MESSAGECHAT",
        0x0DD => "SMSG_MONSTER_MOVE",
        0x0FA => "SMSG_TRIGGER_CINEMATIC",
        0x0FD => "SMSG_TUTORIAL_FLAGS",
        0x122 => "SMSG_INITIALIZE_FACTIONS",
        0x127 => "SMSG_SET_PROFICIENCY",
        0x129 => "SMSG_ACTION_BUTTONS",
        0x12A => "SMSG_INITIAL_SPELLS",
        0x131 => "SMSG_SPELL_START",
        0x132 => "SMSG_SPELL_GO",
        0x137 => "SMSG_UPDATE_AURA_DURATION",
        0x14F => "SMSG_SPELLBREAKLOG",
        0x155 => "SMSG_BINDPOINTUPDATE",
        0x1CB => "SMSG_NOTIFICATION",
        0x209 => "SMSG_ACCOUNT_DATA_TIMES",
        0x21E => "SMSG_SET_REST_START",
        0x24C => "SMSG_SPELLLOGEXECUTE",
        0x293 => "SMSG_MEETINGSTONE_LEAVE",
        0x2C2 => "SMSG_INIT_WORLD_STATES",
        0x2F4 => "SMSG_WEATHER",
        0x329 => "MSG_SET_DUNGEON_DIFFICULTY",
        0x332 => "SMSG_EXPECTED_SPAM_RECORDS",
        0x33A => "SMSG_DEFENSE_MESSAGE",
        0x33B => "SMSG_INSTANCE_DIFFICULTY",
        0x33D => "SMSG_MOTD",
        0x36C => "SMSG_LFG_UPDATE",
        0x390 => "SMSG_TIME_SYNC_REQ",
        0x3A3 => "SMSG_INIT_EXTRA_AURA_INFO",
        0x3A4 => "SMSG_SET_EXTRA_AURA_INFO",
        0x3A6 => "SMSG_CLEAR_EXTRA_AURA_INFO",
        0x3C8 => "SMSG_FEATURE_SYSTEM_STATUS",
        0x41D => "SMSG_SEND_UNLEARN_SPELLS",
        _ => return None,
    })
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
