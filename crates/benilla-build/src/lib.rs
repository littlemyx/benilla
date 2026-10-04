//! The build profile: which frozen client build benilla is playing as. Each build is the last
//! one of an expansion, and a fact that differs between builds (the number the server sees, the
//! version the logon names, the `## Interface:` the stock FrameXML states) lives here, never as a
//! bare constant at the use site.

use std::fmt;

/// The expansion a build belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Expansion {
    Vanilla,
}

/// One frozen client build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientBuild {
    /// The build number the client sends in `CMD_AUTH_LOGON_CHALLENGE` and `CMSG_AUTH_SESSION`.
    pub build: u16,
    /// The version the logon challenge names: major, minor, patch.
    pub version: [u8; 3],
    /// The `## Interface:` number the build's stock `FrameXML.toc` states.
    pub interface: u32,
    pub expansion: Expansion,
}

/// 1.12.1 (5875), the vanilla client's last build.
pub const VANILLA_1_12_1: ClientBuild = ClientBuild {
    build: 5875,
    version: [1, 12, 1],
    interface: 11200,
    expansion: Expansion::Vanilla,
};

/// Every build this client can play as.
pub const SUPPORTED: &[ClientBuild] = &[VANILLA_1_12_1];

/// An `## Interface:` number that names no supported build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownBuild {
    pub interface: u32,
}

impl fmt::Display for UnknownBuild {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the install states Interface {}, which is no supported build (supported:",
            self.interface
        )?;
        for (i, b) in SUPPORTED.iter().enumerate() {
            let [major, minor, patch] = b.version;
            let sep = if i == 0 { " " } else { ", " };
            write!(
                f,
                "{sep}{major}.{minor}.{patch} (build {}, Interface {})",
                b.build, b.interface
            )?;
        }
        write!(f, ")")
    }
}

impl std::error::Error for UnknownBuild {}

impl ClientBuild {
    /// The supported build whose stock `FrameXML.toc` states `interface`.
    pub fn from_interface(interface: u32) -> Result<ClientBuild, UnknownBuild> {
        SUPPORTED
            .iter()
            .find(|b| b.interface == interface)
            .copied()
            .ok_or(UnknownBuild { interface })
    }

    /// The supported build with this build number.
    pub fn from_build(build: u16) -> Option<ClientBuild> {
        SUPPORTED.iter().find(|b| b.build == build).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_known_interface_names_its_build() {
        assert_eq!(
            ClientBuild::from_interface(VANILLA_1_12_1.interface),
            Ok(VANILLA_1_12_1)
        );
        assert_eq!(ClientBuild::from_build(5875), Some(VANILLA_1_12_1));
    }

    #[test]
    fn an_unknown_number_is_an_error_that_names_it_and_the_supported_builds() {
        let err = ClientBuild::from_interface(30300).unwrap_err();
        assert_eq!(err, UnknownBuild { interface: 30300 });
        let text = err.to_string();
        assert!(text.contains("30300"), "{text}");
        assert!(text.contains("1.12.1"), "{text}");
        assert_eq!(ClientBuild::from_build(1), None);
    }

    #[test]
    fn every_supported_build_round_trips_and_none_repeats() {
        for b in SUPPORTED {
            assert_eq!(ClientBuild::from_interface(b.interface), Ok(*b));
            assert_eq!(ClientBuild::from_build(b.build), Some(*b));
        }
        for (i, a) in SUPPORTED.iter().enumerate() {
            for b in &SUPPORTED[i + 1..] {
                assert_ne!(a.build, b.build);
                assert_ne!(a.interface, b.interface);
            }
        }
    }
}
