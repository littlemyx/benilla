//! The realmd (login) wire protocol, version 3 in 1.12.1 and 8 in 2.4.3: logon challenge, logon
//! proof and realm list, the three exchanges [`crate::logon`] performs. Login packets are not header-encrypted and
//! each is self-delimiting, so they are read in request/response order.

use std::io::{Read, Write};
use std::net::Ipv4Addr;

use anyhow::{bail, Result};
use benilla_build::{ClientBuild, Expansion};
use sha1::{Digest, Sha1};

use crate::wire::{read_array, read_cstring, read_f32_le, read_u16_le, read_u32_le, read_u8};
use crate::RealmInfo;

const CMD_AUTH_LOGON_CHALLENGE: u8 = 0x00;
const CMD_AUTH_LOGON_PROOF: u8 = 0x01;
const CMD_REALM_LIST: u8 = 0x10;

const PROTOCOL_VERSION_THREE: u8 = 3;
/// The login protocol byte of 2.4.x (one source: wow_messages `ProtocolVersion`).
const PROTOCOL_VERSION_EIGHT: u8 = 8;

/// The protocol byte `build`'s logon challenge carries.
fn protocol_version(build: &ClientBuild) -> u8 {
    match build.expansion {
        Expansion::Tbc => PROTOCOL_VERSION_EIGHT,
        _ => PROTOCOL_VERSION_THREE,
    }
}

/// Whether `build`'s realmd replies use the 2.4.3 layouts (security-flag bitmask, 32-byte proof
/// reply, 2.4.3 realm list) rather than 1.12.1's.
fn is_tbc(build: &ClientBuild) -> bool {
    matches!(build.expansion, Expansion::Tbc)
}
const GAME_NAME_WOW: u32 = 0x0057_6f57; // "WoW\0" little-endian
const PLATFORM_X86: u32 = 0x0078_3836; // "x86\0"
                                       // Tags are little-endian u32s, so they reach the
                                       // wire reversed (`0x0057696e` is `n i W \0`) and
                                       // realmd reverses them back. vmangos fails a session
                                       // on anything but "Win" or "OSX".
const OS_WINDOWS: u32 = 0x0057_696e; // "Win\0"
const OS_MACOS: u32 = 0x004F_5358; // "OSX\0"

/// The host's OS tag: `OSX` on macOS, else `Win`, as there was no 1.12 Linux client. Servers act
/// on it: vmangos picks `WardenWin` or `WardenMac`, and realmd checks the build (`FindBuildInfo`).
const fn client_os() -> u32 {
    if cfg!(target_os = "macos") {
        OS_MACOS
    } else {
        OS_WINDOWS
    }
}
const LOCALE_EN_US: u32 = 0x656e_5553; // "enUS"

/// A non-success auth result byte, typed so the app can map it to its `AUTH_*` glue string; it
/// survives [`crate::logon`]'s `anyhow` contexts via `downcast_ref`. vmangos answers an unknown
/// account and a wrong password alike with 0x04 (`WOW_FAIL_UNKNOWN_ACCOUNT`, `AuthCodes.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthReject {
    /// The grunt result byte (`WOW_FAIL_*`).
    pub code: u8,
}

impl std::fmt::Display for AuthReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "server rejected logon: result {:#04x}", self.code)
    }
}

impl std::error::Error for AuthReject {}

/// The SRP6 inputs from a successful `CMD_AUTH_LOGON_CHALLENGE_Server`.
pub struct ChallengeReply {
    pub server_public_key: [u8; 32],
    pub generator: u8,
    pub large_safe_prime: [u8; 32],
    pub salt: [u8; 32],
    /// The version challenge answered by `crc_hash` ([`version_proof`]); not an SRP6 input.
    pub crc_salt: [u8; 16],
}

/// Send `CMD_AUTH_LOGON_CHALLENGE_Client`. `account_name` must be uppercased, as the SRP6 hashes
/// use it; `build` names the version and build number the client reports.
pub fn write_logon_challenge(
    w: &mut impl Write,
    account_name: &str,
    build: &ClientBuild,
) -> std::io::Result<()> {
    // Everything after the 2-byte size field, assembled first so we can prefix its length.
    let mut body = Vec::with_capacity(34 + account_name.len());
    body.extend_from_slice(&GAME_NAME_WOW.to_le_bytes());
    body.extend_from_slice(&build.version); // major, minor, patch
    body.extend_from_slice(&build.build.to_le_bytes());
    body.extend_from_slice(&PLATFORM_X86.to_le_bytes());
    body.extend_from_slice(&client_os().to_le_bytes());
    body.extend_from_slice(&LOCALE_EN_US.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes()); // utc_timezone_offset
    body.extend_from_slice(&Ipv4Addr::LOCALHOST.octets()); // client_ip_address (network order)
    body.push(account_name.len() as u8);
    body.extend_from_slice(account_name.as_bytes());

    let mut packet = Vec::with_capacity(4 + body.len());
    packet.push(CMD_AUTH_LOGON_CHALLENGE);
    packet.push(protocol_version(build));
    packet.extend_from_slice(&(body.len() as u16).to_le_bytes());
    packet.extend_from_slice(&body);
    w.write_all(&packet)
}

/// Read the 1.12.1 challenge reply; a non-success result is an [`AuthReject`].
pub fn read_challenge_reply(r: &mut impl Read) -> Result<ChallengeReply> {
    read_challenge_reply_as(r, &benilla_build::VANILLA_1_12_1)
}

/// [`read_challenge_reply`] for `build`'s layout: 2.4.3 reads `security_flag` as a bitmask (PIN
/// `0x01`, matrix card `0x02`, authenticator `0x04`), each with its block, and an answer the
/// client cannot give is an error naming the flag.
pub fn read_challenge_reply_as(r: &mut impl Read, build: &ClientBuild) -> Result<ChallengeReply> {
    let opcode = read_u8(r)?;
    if opcode != CMD_AUTH_LOGON_CHALLENGE {
        bail!("expected CMD_AUTH_LOGON_CHALLENGE (0x00), got {opcode:#x}");
    }
    let _protocol_version = read_u8(r)?;
    let result = read_u8(r)?;
    if result != 0 {
        return Err(AuthReject { code: result }.into());
    }
    let server_public_key = read_array::<32>(r)?;
    let generator_len = read_u8(r)?;
    let mut generator = vec![0u8; generator_len as usize];
    r.read_exact(&mut generator)?;
    let generator = match generator.first() {
        Some(&g) => g,
        None => bail!("server sent an empty generator"),
    };
    let prime_len = read_u8(r)? as usize;
    let mut prime = vec![0u8; prime_len];
    r.read_exact(&mut prime)?;
    let large_safe_prime: [u8; 32] = prime
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("large safe prime was {prime_len} bytes, expected 32"))?;
    let salt = read_array::<32>(r)?;
    // Read to the end: login packets are not length-framed, so leftover bytes desync the next
    // read. `security_flag` and the PIN block are unused but must be consumed.
    let crc_salt = read_array::<16>(r)?;
    let security_flag = read_u8(r)?;
    if is_tbc(build) {
        refuse_security_flags(r, security_flag)?;
    } else if security_flag & 0x01 != 0 {
        // PIN: pin_grid_seed (u32) + pin_salt[16]; vmangos always sends security_flag 0.
        let _pin_grid_seed = read_u32_le(r)?;
        let _pin_salt = read_array::<16>(r)?;
    }
    Ok(ChallengeReply {
        server_public_key,
        generator,
        large_safe_prime,
        salt,
        crc_salt,
    })
}

/// The 2.4.3 `security_flag` bits and what follows each in the challenge reply (two sources:
/// wow_messages, cmangos-tbc): PIN `u32` grid seed + 16-byte salt; matrix card `u8` width, height,
/// digit count, challenge count + `u64` seed; authenticator `u8` required.
const SECURITY_PIN: u8 = 0x01;
const SECURITY_MATRIX: u8 = 0x02;
const SECURITY_AUTHENTICATOR: u8 = 0x04;

/// A set 2.4.3 flag asks for an answer the proof does not carry (it sends `security_flag` 0), so
/// each block is consumed, keeping the stream aligned, and the logon fails naming the flag.
fn refuse_security_flags(r: &mut impl Read, flags: u8) -> Result<()> {
    if flags == 0 {
        return Ok(());
    }
    let mut named = Vec::new();
    if flags & SECURITY_PIN != 0 {
        let _grid_seed = read_u32_le(r)?;
        let _salt = read_array::<16>(r)?;
        named.push("PIN (0x01)");
    }
    if flags & SECURITY_MATRIX != 0 {
        let _width_height_digits_challenges = read_array::<4>(r)?;
        let _seed = read_array::<8>(r)?;
        named.push("matrix card (0x02)");
    }
    if flags & SECURITY_AUTHENTICATOR != 0 {
        let _required = read_u8(r)?;
        named.push("authenticator (0x04)");
    }
    let unknown = flags & !(SECURITY_PIN | SECURITY_MATRIX | SECURITY_AUTHENTICATOR);
    if unknown != 0 {
        bail!("the server asks for an unknown security check (flag bits {unknown:#04x})");
    }
    bail!(
        "the server asks for a security check benilla cannot answer: {}",
        named.join(", ")
    )
}

// --- the version (client-integrity) proof --------------------------------------------------------
//
// The proof's `crc_hash` answers the challenge's `crc_salt`: the 1.12 client hashes its own
// binaries under that salt into a 20-byte digest `H` and sends `SHA1(A ‖ H)`. realmd recomputes it
// from a stored `H` (`AuthSocket::VerifyVersion`) and, under `StrictVersionCheck = 1` (the shipped
// `realmd.conf.example` value), refuses a mismatch with `WOW_FAIL_VERSION_INVALID` (0x09).

/// The `crc_salt` every mangos-family realmd sends: a constant, not a nonce (vmangos
/// `AuthSocket.cpp:65`, cmangos `AuthSocket.cpp:187`), so the answer is a per-build constant.
const MANGOS_VERSION_CHALLENGE: [u8; 16] = [
    0xba, 0xa3, 0x1e, 0x99, 0xa0, 0x0b, 0x21, 0x57, 0xfc, 0x37, 0x3f, 0xb3, 0x69, 0xcd, 0xd2, 0xf1,
];

/// `H` for the build 5875 Windows client under [`MANGOS_VERSION_CHALLENGE`]: vmangos's
/// `allowed_clients` row (`20221117065844_logon.sql`), identical to cmangos's `RealmList.cpp`.
const INTEGRITY_HASH_5875_WINDOWS: [u8; 20] = [
    0x95, 0xed, 0xb2, 0x7c, 0x78, 0x23, 0xb3, 0x63, 0xcb, 0xdd, 0xab, 0x56, 0xa3, 0x92, 0xe7, 0xcb,
    0x73, 0xfc, 0xca, 0x20,
];

/// The same for the Mac client, sent on macOS because servers pick `H` by the OS tag.
const INTEGRITY_HASH_5875_MACOS: [u8; 20] = [
    0x8d, 0x17, 0x3c, 0xc3, 0x81, 0x96, 0x1e, 0xeb, 0xab, 0xf3, 0x36, 0xf5, 0xe6, 0x67, 0x5b, 0x10,
    0x1b, 0xb5, 0x13, 0xe5,
];

/// The integrity digest `H` of `build` for `crc_salt`, known only for [`MANGOS_VERSION_CHALLENGE`]
/// and a build with a stored row (today 5875). The 8606 digest is not known; cmangos-tbc checks it
/// only under `StrictVersionCheck`, which its shipped `realmd.conf` leaves off.
/// Deviation: a stored per-OS constant, not the reference's HMAC over its own executables
/// (`0x5b1170`), because every mangos-family realmd issues this one salt.
fn integrity_hash(build: &ClientBuild, crc_salt: &[u8; 16]) -> Option<[u8; 20]> {
    if *crc_salt != MANGOS_VERSION_CHALLENGE {
        return None;
    }
    match build.build {
        5875 => Some(if client_os() == OS_MACOS {
            INTEGRITY_HASH_5875_MACOS
        } else {
            INTEGRITY_HASH_5875_WINDOWS
        }),
        _ => None,
    }
}

/// The proof's `crc_hash`: `SHA1(A ‖ H)` over `A`'s wire bytes (realmd hashes `lp->A` as
/// received), or twenty zeros for an unknown salt, which only a strict server refuses.
pub fn version_proof(
    build: &ClientBuild,
    crc_salt: &[u8; 16],
    client_public_key: &[u8; 32],
) -> [u8; 20] {
    match integrity_hash(build, crc_salt) {
        Some(h) => {
            let mut sha = Sha1::new();
            sha.update(client_public_key);
            sha.update(h);
            sha.finalize().into()
        }
        None => [0u8; 20],
    }
}

/// Send `CMD_AUTH_LOGON_PROOF_Client`, computing `crc_hash` from the challenge's `crc_salt`.
pub fn write_logon_proof(
    w: &mut impl Write,
    build: &ClientBuild,
    client_public_key: &[u8; 32],
    client_proof: &[u8; 20],
    crc_salt: &[u8; 16],
) -> std::io::Result<()> {
    let mut packet = Vec::with_capacity(1 + 32 + 20 + 20 + 1 + 1);
    packet.push(CMD_AUTH_LOGON_PROOF);
    packet.extend_from_slice(client_public_key);
    packet.extend_from_slice(client_proof);
    packet.extend_from_slice(&version_proof(build, crc_salt, client_public_key));
    packet.push(0); // number_of_telemetry_keys
    packet.push(0); // security_flag = None
    w.write_all(&packet)
}

/// Read the 1.12.1 proof reply: the server's `M2`, or an [`AuthReject`].
pub fn read_proof_reply(r: &mut impl Read) -> Result<[u8; 20]> {
    read_proof_reply_as(r, &benilla_build::VANILLA_1_12_1)
}

/// [`read_proof_reply`] for `build`'s layout: after `M2` 1.12.1 has `u32` survey id; 2.4.3 has `u32`
/// account flags, `u32` survey id, `u16` unknown (32 bytes in all; two sources). A 2.4.3 failure
/// carries a trailing `u16` that is not read, as the socket is dropped on the error.
pub fn read_proof_reply_as(r: &mut impl Read, build: &ClientBuild) -> Result<[u8; 20]> {
    let opcode = read_u8(r)?;
    if opcode != CMD_AUTH_LOGON_PROOF {
        bail!("expected CMD_AUTH_LOGON_PROOF (0x01), got {opcode:#x}");
    }
    let result = read_u8(r)?;
    if result != 0 {
        // A wrong password fails here, where `M1` does not verify: vmangos answers 0x04.
        return Err(AuthReject { code: result }.into());
    }
    let server_proof = read_array::<20>(r)?;
    if is_tbc(build) {
        let _account_flags = read_u32_le(r)?;
        let _survey_id = read_u32_le(r)?;
        let _unknown = read_u16_le(r)?;
    } else {
        let _hardware_survey_id = read_u32_le(r)?;
    }
    Ok(server_proof)
}

/// Send `CMD_REALM_LIST_Client` (opcode + a `u32` padding of 0).
pub fn write_realm_list_request(w: &mut impl Write) -> std::io::Result<()> {
    let mut packet = [0u8; 5];
    packet[0] = CMD_REALM_LIST;
    w.write_all(&packet)
}

/// The magic populations the 1.12 realm-list parser (`0x5b2230`) rewrites, as `(population sent,
/// population rewritten, flag OR'd)`: Recommended, New and Full travel as these exact float bit
/// patterns, not as wire flags.
pub(crate) const MAGIC_POPULATIONS: [(u32, u32, u8); 3] = [
    (0x4416_0000, 0x0000_0000, 0x20), // 600.0 → 0.0,   Recommended
    (0x4348_0000, 0x3a83_126f, 0x40), // 200.0 → 0.001, New
    (0x43c8_0000, 0x4100_0000, 0x80), // 400.0 → 8.0,   Full
];

/// Read the 1.12.1 `CMD_REALM_LIST_Server` into the advertised realms, rewriting
/// [`MAGIC_POPULATIONS`] here as the reference parser does: the realm-list screen averages every
/// realm's population, and an unswapped Recommended `600.0` would skew that mean for every row.
pub fn read_realm_list(r: &mut impl Read) -> Result<Vec<RealmInfo>> {
    read_realm_list_as(r, &benilla_build::VANILLA_1_12_1)
}

/// [`read_realm_list`] for `build`'s layout.
pub fn read_realm_list_as(r: &mut impl Read, build: &ClientBuild) -> Result<Vec<RealmInfo>> {
    if is_tbc(build) {
        read_realm_list_tbc(r)
    } else {
        read_realm_list_vanilla(r)
    }
}

/// The 2.4.3 realm list (two sources): `u16` count; per realm `u8` type, `u8` locked, `u8` flags,
/// name, address, `f32` population, `u8` characters, `u8` category, `u8` id, and a `u8 u8 u8 u16`
/// version when flag `0x04` is set; then a `u16` trailer. The lock byte and the version are read
/// and dropped (`RealmInfo` has no field for them). The 1.12 magic populations are not rewritten:
/// whether the 2.4.3 client does so is not established.
fn read_realm_list_tbc(r: &mut impl Read) -> Result<Vec<RealmInfo>> {
    const FLAG_SPECIFY_BUILD: u8 = 0x04;
    let opcode = read_u8(r)?;
    if opcode != CMD_REALM_LIST {
        bail!("expected CMD_REALM_LIST (0x10), got {opcode:#x}");
    }
    let _size = read_u16_le(r)?;
    let _header_padding = read_u32_le(r)?;
    let number_of_realms = read_u16_le(r)?;
    let mut realms = Vec::with_capacity(usize::from(number_of_realms).min(64));
    for _ in 0..number_of_realms {
        let realm_type = read_u8(r)?;
        let _locked = read_u8(r)?;
        let flags = read_u8(r)?;
        let name = read_cstring(r)?;
        let address = read_cstring(r)?;
        let population = read_f32_le(r)?;
        let characters = read_u8(r)?;
        let category = read_u8(r)?;
        let id = read_u8(r)?;
        if flags & FLAG_SPECIFY_BUILD != 0 {
            let _version = read_array::<3>(r)?;
            let _build = read_u16_le(r)?;
        }
        realms.push(RealmInfo {
            name,
            address,
            population,
            characters,
            realm_type: u32::from(realm_type),
            flags,
            category,
            id,
        });
    }
    let _footer_padding = read_u16_le(r)?;
    Ok(realms)
}

fn read_realm_list_vanilla(r: &mut impl Read) -> Result<Vec<RealmInfo>> {
    let opcode = read_u8(r)?;
    if opcode != CMD_REALM_LIST {
        bail!("expected CMD_REALM_LIST (0x10), got {opcode:#x}");
    }
    let _size = read_u16_le(r)?;
    let _header_padding = read_u32_le(r)?;
    let number_of_realms = read_u8(r)?;
    let mut realms = Vec::with_capacity(number_of_realms as usize);
    for _ in 0..number_of_realms {
        let realm_type = read_u32_le(r)?;
        let mut flags = read_u8(r)?;
        let name = read_cstring(r)?;
        let address = read_cstring(r)?;
        let mut population = read_f32_le(r)?;
        let characters = read_u8(r)?;
        let category = read_u8(r)?;
        let id = read_u8(r)?;
        // The sentinel swap, before anyone can average this number.
        if let Some(&(_, rewritten, bit)) = MAGIC_POPULATIONS
            .iter()
            .find(|(magic, _, _)| *magic == population.to_bits())
        {
            population = f32::from_bits(rewritten);
            flags |= bit;
        }
        realms.push(RealmInfo {
            name,
            address,
            population,
            characters,
            realm_type,
            flags,
            category,
            id,
        });
    }
    let _footer_padding = read_u16_le(r)?; // consume so the stream stays aligned
    Ok(realms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_build::{TBC_2_4_3, VANILLA_1_12_1};

    /// The 1.12.1 challenge as it went out before the version came from the build profile.
    #[test]
    fn the_1_12_1_logon_challenge_bytes_are_unchanged() {
        let mut got = Vec::new();
        write_logon_challenge(&mut got, "TEST", &VANILLA_1_12_1).unwrap();
        let mut want = vec![CMD_AUTH_LOGON_CHALLENGE, PROTOCOL_VERSION_THREE, 34, 0];
        want.extend_from_slice(&0x0057_6f57u32.to_le_bytes()); // "WoW\0"
        want.extend_from_slice(&[1, 12, 1]); // version
        want.extend_from_slice(&5875u16.to_le_bytes()); // build
        want.extend_from_slice(&0x0078_3836u32.to_le_bytes()); // "x86\0"
        want.extend_from_slice(&client_os().to_le_bytes());
        want.extend_from_slice(&0x656e_5553u32.to_le_bytes()); // "enUS"
        want.extend_from_slice(&0u32.to_le_bytes()); // timezone
        want.extend_from_slice(&[127, 0, 0, 1]); // client ip
        want.push(4);
        want.extend_from_slice(b"TEST");
        assert_eq!(got, want);
    }

    #[test]
    fn the_2_4_3_logon_challenge_differs_from_1_12_1_in_the_protocol_byte_and_build() {
        let mut got = Vec::new();
        write_logon_challenge(&mut got, "TEST", &TBC_2_4_3).unwrap();
        let mut vanilla = Vec::new();
        write_logon_challenge(&mut vanilla, "TEST", &VANILLA_1_12_1).unwrap();
        assert_eq!(got.len(), vanilla.len());
        assert_eq!(got[..2], [CMD_AUTH_LOGON_CHALLENGE, 8]);
        assert_eq!(vanilla[..2], [CMD_AUTH_LOGON_CHALLENGE, 3]);
        assert_eq!(got[8..11], [2, 4, 3], "version");
        assert_eq!(got[11..13], 8606u16.to_le_bytes(), "build");
        // Everything else is identical.
        let (mut a, mut b) = (got.clone(), vanilla.clone());
        for v in [&mut a, &mut b] {
            v[1] = 0;
            v[8..13].fill(0);
        }
        assert_eq!(a, b);
    }

    /// A challenge reply up to and including `security_flag`, then `tail`.
    fn challenge_reply(flag: u8, tail: &[u8]) -> Vec<u8> {
        let mut p = vec![CMD_AUTH_LOGON_CHALLENGE, 0, 0];
        p.extend_from_slice(&[0xB0; 32]); // B
        p.extend_from_slice(&[1, 7]); // g
        p.push(32);
        p.extend_from_slice(&[0x4E; 32]); // N
        p.extend_from_slice(&[0x5A; 32]); // salt
        p.extend_from_slice(&MANGOS_VERSION_CHALLENGE);
        p.push(flag);
        p.extend_from_slice(tail);
        p
    }

    #[test]
    fn a_2_4_3_challenge_reply_with_no_security_flags_reads_through() {
        let packet = challenge_reply(0, &[]);
        let mut r = packet.as_slice();
        let reply = read_challenge_reply_as(&mut r, &TBC_2_4_3).unwrap();
        assert!(r.is_empty());
        assert_eq!(reply.generator, 7);
        assert_eq!(reply.crc_salt, MANGOS_VERSION_CHALLENGE);
    }

    #[test]
    fn each_2_4_3_security_flag_is_consumed_and_refused_by_name() {
        // flag, its block, the name expected in the error
        let cases: [(u8, Vec<u8>, &str); 3] = [
            (0x01, vec![0; 20], "PIN"),
            (0x02, vec![0; 12], "matrix"),
            (0x04, vec![1], "authenticator"),
        ];
        for (flag, block, name) in cases {
            let mut packet = challenge_reply(flag, &block);
            packet.push(0xEE); // the next packet's first byte must stay unread
            let mut r = packet.as_slice();
            let err = read_challenge_reply_as(&mut r, &TBC_2_4_3)
                .err()
                .expect("an error");
            assert!(err.to_string().contains(name), "{err}");
            assert_eq!(r, [0xEE], "the {name} block is consumed");
        }
        let mut both = vec![0; 12];
        both.push(1);
        let packet = challenge_reply(0x06, &both);
        let err = read_challenge_reply_as(&mut packet.as_slice(), &TBC_2_4_3)
            .err()
            .expect("an error");
        let text = err.to_string();
        assert!(
            text.contains("matrix") && text.contains("authenticator"),
            "{text}"
        );
    }

    #[test]
    fn the_1_12_1_challenge_reply_still_reads_a_pin_block() {
        let packet = challenge_reply(0x01, &[0; 20]);
        let mut r = packet.as_slice();
        read_challenge_reply(&mut r).unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn the_2_4_3_proof_reply_is_32_bytes_and_the_1_12_1_one_26() {
        let m2: [u8; 20] = std::array::from_fn(|i| i as u8);
        let mut tbc = vec![CMD_AUTH_LOGON_PROOF, 0];
        tbc.extend_from_slice(&m2);
        tbc.extend_from_slice(&0x0080_0000u32.to_le_bytes()); // account flags
        tbc.extend_from_slice(&0u32.to_le_bytes()); // survey id
        tbc.extend_from_slice(&0u16.to_le_bytes());
        assert_eq!(tbc.len(), 32);
        let mut r = tbc.as_slice();
        assert_eq!(read_proof_reply_as(&mut r, &TBC_2_4_3).unwrap(), m2);
        assert!(r.is_empty());

        let mut vanilla = vec![CMD_AUTH_LOGON_PROOF, 0];
        vanilla.extend_from_slice(&m2);
        vanilla.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(vanilla.len(), 26);
        let mut r = vanilla.as_slice();
        assert_eq!(read_proof_reply(&mut r).unwrap(), m2);
        assert!(r.is_empty());
    }

    #[test]
    fn a_2_4_3_proof_failure_is_a_typed_reject() {
        let packet = [CMD_AUTH_LOGON_PROOF, 0x04, 0, 0];
        let err = read_proof_reply_as(&mut packet.as_slice(), &TBC_2_4_3).unwrap_err();
        assert_eq!(
            err.downcast_ref::<AuthReject>(),
            Some(&AuthReject { code: 4 })
        );
    }

    #[test]
    fn the_unknown_8606_integrity_hash_is_answered_with_zeros() {
        assert_eq!(
            version_proof(&TBC_2_4_3, &MANGOS_VERSION_CHALLENGE, &test_public_key()),
            [0u8; 20]
        );
    }

    /// One 2.4.3 realm record: `u8 type, u8 locked, u8 flags, name, address, f32 population,
    /// u8 characters, u8 category, u8 id`, and the version when `flags & 4`.
    fn tbc_realm(flags: u8, population: f32) -> Vec<u8> {
        let mut b = vec![1, 0, flags];
        b.extend_from_slice(b"CMaNGOS TBC\0");
        b.extend_from_slice(b"192.168.2.87:8086\0");
        b.extend_from_slice(&population.to_le_bytes());
        b.extend_from_slice(&[2, 1, 0x2C]);
        if flags & 0x04 != 0 {
            b.extend_from_slice(&[2, 4, 3]);
            b.extend_from_slice(&8606u16.to_le_bytes());
        }
        b
    }

    fn tbc_realm_list(realms: &[Vec<u8>]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&0u32.to_le_bytes()); // header padding
        body.extend_from_slice(&(realms.len() as u16).to_le_bytes());
        for r in realms {
            body.extend_from_slice(r);
        }
        body.extend_from_slice(&0x0010u16.to_le_bytes()); // trailer
        let mut packet = vec![CMD_REALM_LIST];
        packet.extend_from_slice(&(body.len() as u16).to_le_bytes());
        packet.extend_from_slice(&body);
        packet
    }

    #[test]
    fn the_2_4_3_realm_list_reads_with_and_without_the_version_tuple() {
        let packet = tbc_realm_list(&[
            tbc_realm(0x00, 0.5),
            tbc_realm(0x04, 400.0),
            tbc_realm(0, 1.0),
        ]);
        let mut r = packet.as_slice();
        let realms = read_realm_list_as(&mut r, &TBC_2_4_3).unwrap();
        assert!(
            r.is_empty(),
            "the stream stays aligned across the version tuple"
        );
        assert_eq!(realms.len(), 3);
        let a = &realms[0];
        assert_eq!(a.name, "CMaNGOS TBC");
        assert_eq!(a.address, "192.168.2.87:8086");
        assert_eq!(
            (a.realm_type, a.flags, a.characters, a.category, a.id),
            (1, 0, 2, 1, 0x2C)
        );
        assert_eq!(a.population, 0.5);
        assert_eq!(realms[1].flags, 0x04);
        // The 1.12 magic populations are not applied to 2.4.3.
        assert_eq!(realms[1].population, 400.0);
        assert_eq!(realms[2].name, "CMaNGOS TBC");
    }

    #[test]
    fn an_empty_2_4_3_realm_list_is_a_zero_count() {
        let packet = tbc_realm_list(&[]);
        let mut r = packet.as_slice();
        assert!(read_realm_list_as(&mut r, &TBC_2_4_3).unwrap().is_empty());
        assert!(r.is_empty());
    }

    #[test]
    fn os_tags_reverse_to_the_names_servers_match_on() {
        let spelled = |tag: u32| {
            let mut b = tag.to_le_bytes();
            b.reverse();
            String::from_utf8(b.iter().copied().filter(|&c| c != 0).collect()).unwrap()
        };
        assert_eq!(spelled(OS_WINDOWS), "Win");
        assert_eq!(spelled(OS_MACOS), "OSX");
        assert_eq!(spelled(PLATFORM_X86), "x86");
    }

    #[test]
    fn client_os_follows_the_host() {
        if cfg!(target_os = "macos") {
            assert_eq!(client_os(), OS_MACOS);
        } else {
            assert_eq!(client_os(), OS_WINDOWS);
        }
    }

    /// An arbitrary but fixed `A` for the version-proof vectors below.
    fn test_public_key() -> [u8; 32] {
        std::array::from_fn(|i| (i as u8).wrapping_mul(7).wrapping_add(9))
    }

    /// Expected digests computed outside this crate (Python `hashlib`) as `SHA1(A ‖ H)`.
    #[test]
    fn version_proof_matches_the_realmd_expression() {
        let expected = if cfg!(target_os = "macos") {
            "dccd0e67946fae221451513b1a59724090ff6096"
        } else {
            "9b95cd41edd719fddf237294b8aca17010e71703"
        };
        let got = version_proof(
            &VANILLA_1_12_1,
            &MANGOS_VERSION_CHALLENGE,
            &test_public_key(),
        );
        assert_eq!(
            got.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            expected
        );
    }

    #[test]
    fn an_unknown_version_challenge_is_answered_with_zeros() {
        let mut salt = MANGOS_VERSION_CHALLENGE;
        salt[0] ^= 0xff;
        assert_eq!(
            version_proof(&VANILLA_1_12_1, &salt, &test_public_key()),
            [0u8; 20]
        );
    }

    /// `opcode · A[32] · M1[20] · crc_hash[20] · num_keys · security_flag`, 75 bytes.
    #[test]
    fn the_proof_packet_carries_the_version_proof() {
        let a = test_public_key();
        let m1: [u8; 20] = std::array::from_fn(|i| (i as u8).wrapping_mul(13).wrapping_add(4));
        let mut packet = Vec::new();
        write_logon_proof(
            &mut packet,
            &VANILLA_1_12_1,
            &a,
            &m1,
            &MANGOS_VERSION_CHALLENGE,
        )
        .unwrap();

        assert_eq!(packet.len(), 1 + 32 + 20 + 20 + 1 + 1);
        assert_eq!(packet[0], CMD_AUTH_LOGON_PROOF);
        assert_eq!(&packet[1..33], &a);
        assert_eq!(&packet[33..53], &m1);
        assert_eq!(
            &packet[53..73],
            &version_proof(&VANILLA_1_12_1, &MANGOS_VERSION_CHALLENGE, &a)
        );
        assert_eq!(&packet[73..], &[0, 0]);
    }

    #[test]
    fn the_realm_list_keeps_every_field_the_wire_carries() {
        let mut body = Vec::new();
        body.extend_from_slice(&8u32.to_le_bytes()); // realm_type: RPPVP
        body.push(0x42); // flags: 0x02 offline | 0x40 sentinel
        body.extend_from_slice(b"Onyxia\0");
        body.extend_from_slice(b"127.0.0.1:8085\0");
        body.extend_from_slice(&1.75f32.to_le_bytes()); // population
        body.push(3); // characters
        body.push(2); // category
        body.push(9); // realm id

        let mut packet = vec![CMD_REALM_LIST];
        packet.extend_from_slice(&(body.len() as u16 + 3).to_le_bytes()); // size
        packet.extend_from_slice(&0u32.to_le_bytes()); // header padding
        packet.push(1); // number_of_realms
        packet.extend_from_slice(&body);
        packet.extend_from_slice(&0u16.to_le_bytes()); // footer padding

        let realms = read_realm_list(&mut packet.as_slice()).unwrap();
        assert_eq!(realms.len(), 1);
        let r = &realms[0];
        assert_eq!(r.name, "Onyxia");
        assert_eq!(r.address, "127.0.0.1:8085");
        assert_eq!(r.realm_type, 8);
        assert_eq!(r.flags, 0x42);
        assert_eq!(r.population, 1.75);
        assert_eq!(r.characters, 3);
        assert_eq!(r.category, 2);
        assert_eq!(r.id, 9);
    }

    /// The rewritten values are the reference parser's own, bit for bit.
    #[test]
    fn the_three_magic_populations_become_flags_and_are_rewritten() {
        let one = |pop: f32| {
            let mut body = Vec::new();
            body.extend_from_slice(&0u32.to_le_bytes());
            body.push(0x02); // a real wire flag, which must survive the OR
            body.extend_from_slice(b"R\0");
            body.extend_from_slice(b"h:1\0");
            body.extend_from_slice(&pop.to_le_bytes());
            body.extend_from_slice(&[0, 1, 0]);
            let mut packet = vec![CMD_REALM_LIST];
            packet.extend_from_slice(&(body.len() as u16 + 3).to_le_bytes());
            packet.extend_from_slice(&0u32.to_le_bytes());
            packet.push(1);
            packet.extend_from_slice(&body);
            packet.extend_from_slice(&0u16.to_le_bytes());
            read_realm_list(&mut packet.as_slice()).unwrap().remove(0)
        };

        let recommended = one(600.0);
        assert_eq!(recommended.flags, 0x02 | 0x20, "the wire flag survives");
        assert_eq!(recommended.population, 0.0);

        let new = one(200.0);
        assert_eq!(new.flags, 0x02 | 0x40);
        assert_eq!(new.population.to_bits(), 0x3a83_126f, "0.001f exactly");

        let full = one(400.0);
        assert_eq!(full.flags, 0x02 | 0x80);
        assert_eq!(full.population, 8.0);

        // An ordinary population is left alone, flags and all.
        let plain = one(1.5);
        assert_eq!(plain.flags, 0x02);
        assert_eq!(plain.population, 1.5);
    }
}
