//! Which client build an install is: the `## Interface:` number its stock `FrameXML.toc` states.

use anyhow::{Context, Result};
use benilla_build::{ClientBuild, UnknownBuild};

use crate::Chain;

/// The manifest the client's interface loader opens first, as the patch chain resolves it.
const FRAMEXML_TOC: &str = "Interface\\FrameXML\\FrameXML.toc";

/// What a chain's stock `FrameXML.toc` says about its build; the chain caches it
/// ([`Chain::build`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Probe {
    Build(ClientBuild),
    /// No readable TOC, or one without an `## Interface:` number: not an install (a single
    /// archive, a test fixture).
    NotAnInstall,
    /// An `## Interface:` number no known build states.
    Unknown(UnknownBuild),
}

/// Read `chain`'s `FrameXML.toc` once and classify it.
pub(crate) fn probe(chain: &Chain) -> Probe {
    let Ok(toc) = chain.read(FRAMEXML_TOC) else {
        return Probe::NotAnInstall;
    };
    let Some(interface) = interface_number(&String::from_utf8_lossy(&toc)) else {
        return Probe::NotAnInstall;
    };
    match ClientBuild::from_interface(interface) {
        Ok(build) => Probe::Build(build),
        Err(unknown) => Probe::Unknown(unknown),
    }
}

/// The build `chain` was installed as, from its `FrameXML.toc` (the last `## Interface:` line
/// wins, as the reference's TOC parse replaces an earlier value). A number no known build
/// states is an error, never a fallback; a known build the client cannot play still detects.
/// A known build comes from the chain's cached probe; an error re-reads for its message.
pub fn detect_build(chain: &Chain) -> Result<ClientBuild> {
    if let Probe::Build(build) = chain.probe() {
        return Ok(*build);
    }
    let toc = chain
        .read(FRAMEXML_TOC)
        .with_context(|| format!("reading {FRAMEXML_TOC} to learn the install's build"))?;
    let interface = interface_number(&String::from_utf8_lossy(&toc))
        .with_context(|| format!("{FRAMEXML_TOC} states no `## Interface:` number"))?;
    Ok(ClientBuild::from_interface(interface)?)
}

/// The leading digits of the last `## Interface:` value (`11507, 11508` reads `11507`).
fn interface_number(toc: &str) -> Option<u32> {
    toc.lines()
        .filter_map(|line| {
            let (key, value) = line
                .trim_start_matches('\u{feff}')
                .strip_prefix("##")?
                .split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("Interface")
                .then_some(value)
        })
        .next_back()
        .and_then(|v| {
            let digits: String = v.trim().chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_build::{TBC_2_4_3, VANILLA_1_12_1};

    #[test]
    fn the_interface_directive_reads_as_the_toc_parse_reads_it() {
        assert_eq!(
            interface_number("## Interface: 11200\r\nFile.xml\r\n"),
            Some(11200)
        );
        assert_eq!(
            interface_number("\u{feff}## interface:11507, 11508"),
            Some(11507)
        );
        assert_eq!(
            interface_number("## Interface: 1\n## Interface: 2"),
            Some(2)
        );
        assert_eq!(interface_number("## Title: x\nFile.xml"), None);
        assert_eq!(interface_number("## Interface: none"), None);
    }

    #[test]
    fn the_install_is_1_12_1() {
        let data = crate::wow_data_or_skip!();
        let chain = Chain::open(&data).expect("open the chain");
        assert_eq!(detect_build(&chain).expect("detect"), VANILLA_1_12_1);
    }

    #[test]
    fn the_tbc_install_is_2_4_3() {
        let data = crate::wow_data_tbc_or_skip!();
        let chain = Chain::open(&data).expect("open the chain");
        assert_eq!(detect_build(&chain).expect("detect"), TBC_2_4_3);
    }

    #[test]
    fn a_chain_knows_its_build_and_its_dbc_layout() {
        let data = crate::wow_data_or_skip!();
        let chain = Chain::open(&data).expect("open the chain");
        assert_eq!(chain.build(), Some(VANILLA_1_12_1));
        assert_eq!(chain.build_error(), None);
        assert_eq!(chain.dbc_layout(), crate::DbcLayout::VANILLA_1_12_1);
        assert_eq!(chain.dbc_layout().loc_slots, 9);
    }

    #[test]
    fn a_tbc_chain_reads_the_wide_layout() {
        let data = crate::wow_data_tbc_or_skip!();
        let chain = Chain::open(&data).expect("open the chain");
        assert_eq!(chain.build(), Some(TBC_2_4_3));
        assert_eq!(chain.dbc_layout(), crate::DbcLayout::TBC_2_4_3);
        assert_eq!(chain.dbc_layout().loc_slots, 17);
    }

    /// A single archive holds no stock `FrameXML.toc`: no build, and the 1.12.1 layout.
    #[test]
    fn a_chain_without_the_stock_interface_reads_as_a_1_12_1_fixture() {
        let data = crate::wow_data_or_skip!();
        let Some(archive) = ["dbc.MPQ", "dbc.mpq"]
            .iter()
            .map(|n| data.join(n))
            .find(|p| p.is_file())
        else {
            return;
        };
        let chain = Chain::open(&archive).expect("open one archive");
        assert_eq!(chain.build(), None);
        assert_eq!(chain.build_error(), None);
        assert_eq!(chain.dbc_layout(), crate::DbcLayout::VANILLA_1_12_1);
    }

    #[test]
    fn every_known_build_has_a_layout() {
        for b in benilla_build::KNOWN {
            assert!(crate::DbcLayout::of(b).is_some(), "{b:?}");
        }
        assert_eq!(crate::DbcLayout::of(&VANILLA_1_12_1).unwrap().loc_slots, 9);
        assert_eq!(crate::DbcLayout::of(&TBC_2_4_3).unwrap().loc_slots, 17);
    }
}
