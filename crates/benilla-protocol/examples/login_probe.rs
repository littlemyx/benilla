//! Live probe: log in as one build, list the realms, connect to the first, request the character
//! list and print it. Works for 1.12.1 (`WOW_BUILD=5875`, the default) and 2.4.3 (`8606`).
//!
//! `WOW_HOST` (default `localhost`, an optional `:port` for realmd), `WOW_USER` and `WOW_PASS` name
//! the server and account; the password is never printed. Refuses to run without
//! `WOW_UNATTENDED=1`, since a login kicks whoever holds the account.

use anyhow::{anyhow, bail, Context, Result};
use benilla_build::ClientBuild;
use benilla_protocol::{logon_as, WorldSession};

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

    let characters = world.char_enum().context("character list")?;
    println!("characters: {}", characters.len());
    for c in &characters {
        println!(
            "  {} level {} race {} class {} gender {}",
            c.name, c.level, c.race, c.class, c.gender
        );
    }
    drop(world);
    println!("disconnected");
    Ok(())
}
