//! Integration tests over the patch chain of a real 2.4.3 install (`$WOW_DATA_TBC`).

use benilla_build::TBC_2_4_3;
use benilla_formats::{detect_build, Chain};

fn archive_name(chain: &Chain, name: &str) -> String {
    chain
        .find_file_archive(name)
        .unwrap_or_else(|| panic!("{name} resolves nowhere"))
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn the_tbc_chain_opens_in_the_enus_else_engb_locale_and_names_its_build() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let chain = Chain::open(&data).expect("open the 2.4.3 chain");
    // The locale the test install is read as: enUS when it has the folder, else enGB.
    let want = if data.join("enUS").is_dir() {
        "enUS"
    } else {
        "enGB"
    };
    assert_eq!(chain.locale(), Some(want));
    assert_eq!(detect_build(&chain).expect("detect"), TBC_2_4_3);
}

#[test]
fn the_stock_toc_and_spell_dbc_resolve_to_the_highest_priority_locale_patch() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let chain = Chain::open(&data).expect("open the 2.4.3 chain");
    assert_eq!(chain.locale(), Some("enGB"), "the facts below are enGB's");
    // locale-enGB states Interface 20000; patch-enGB's 2200-byte TOC (20400) wins over it.
    let toc = "Interface\\FrameXML\\FrameXML.toc";
    assert_eq!(archive_name(&chain, toc), "patch-enGB.MPQ");
    assert_eq!(chain.read(toc).expect("read the toc").len(), 2200);
    // Spell.dbc is in locale-enGB, patch-enGB and patch-enGB-2; the last wins.
    let spell = "DBFilesClient\\Spell.dbc";
    assert_eq!(archive_name(&chain, spell), "patch-enGB-2.MPQ");
    assert_eq!(&chain.read(spell).expect("read Spell.dbc")[..4], b"WDBC");
}
