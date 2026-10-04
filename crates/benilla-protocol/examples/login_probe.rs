//! Live probe: log in as one build, list the realms, connect to the first, request the character
//! list and print it. Works for 1.12.1 (`WOW_BUILD=5875`, the default) and 2.4.3 (`8606`).
//!
//! With `WOW_CREATE=<Name>` set and no character of that name on the list, it creates one (human
//! male warrior, appearance bytes 0), prints the result, reads the list again, and prints every
//! character in full.
//!
//! `WOW_HOST` (default `localhost`, an optional `:port` for realmd), `WOW_USER` and `WOW_PASS` name
//! the server and account; the password is never printed. Refuses to run without
//! `WOW_UNATTENDED=1`, since a login kicks whoever holds the account.

use anyhow::{anyhow, bail, Context, Result};
use benilla_build::ClientBuild;
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
