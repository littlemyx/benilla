//! Which client build an install is: the `## Interface:` number its stock `FrameXML.toc` states.

use anyhow::{Context, Result};
use benilla_build::ClientBuild;

use crate::Chain;

/// The manifest the client's interface loader opens first, as the patch chain resolves it.
const FRAMEXML_TOC: &str = "Interface\\FrameXML\\FrameXML.toc";

/// The build `chain` was installed as, from its `FrameXML.toc` (the last `## Interface:` line
/// wins, as the reference's TOC parse replaces an earlier value). A number no known build
/// states is an error, never a fallback; a known build the client cannot play still detects.
pub fn detect_build(chain: &Chain) -> Result<ClientBuild> {
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
}
