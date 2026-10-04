//! The 2.4.3 world-header key. The cipher recurrences are [`crate::vanilla_header`]'s; the key is
//! not the session key but `HMAC-SHA1(seed, K)`, 20 bytes, one key for both directions.
//! Source: cmangos-tbc `AuthCrypt::Init` only (one source); a wrong seed garbles every header at
//! `SMSG_AUTH_RESPONSE`.

use sha1::{Digest, Sha1};

use crate::SESSION_KEY_LENGTH;

/// The 16-byte HMAC key of the 2.4.3 header cipher (one source: cmangos-tbc `AuthCrypt.cpp`).
const HEADER_KEY_SEED: [u8; 16] = [
    0x38, 0xa7, 0x83, 0x15, 0xf8, 0x92, 0x25, 0x30, 0x71, 0x98, 0x67, 0xb1, 0x8c, 0x04, 0xe2, 0xaa,
];

/// The cipher key length in bytes: one SHA-1 digest.
pub const TBC_HEADER_KEY_LENGTH: usize = 20;

/// SHA-1's block size, which RFC 2104 pads the HMAC key to.
const BLOCK: usize = 64;

/// `HMAC-SHA1(key, message)` per RFC 2104.
fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; 20] {
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..20].copy_from_slice(&Sha1::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let pad = |byte: u8| block.map(|b| b ^ byte);
    let mut inner = Sha1::new();
    inner.update(pad(0x36));
    inner.update(message);
    let mut outer = Sha1::new();
    outer.update(pad(0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// The 2.4.3 header cipher key for a session key `K`.
pub fn derive_header_key(session_key: &[u8; SESSION_KEY_LENGTH]) -> [u8; TBC_HEADER_KEY_LENGTH] {
    hmac_sha1(&HEADER_KEY_SEED, session_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vanilla_header::{HeaderCrypto, ProofSeed};
    use crate::NormalizedString;

    fn hx(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
            .collect()
    }

    fn session_key() -> [u8; 40] {
        std::array::from_fn(|i| (i as u8).wrapping_mul(7).wrapping_add(3))
    }

    /// RFC 2202 test cases 1 and 6 (the latter keys with more than a block).
    #[test]
    fn hmac_sha1_matches_rfc_2202() {
        assert_eq!(
            hmac_sha1(&[0x0b; 20], b"Hi There").to_vec(),
            hx("b617318655057264e28bc0b6fb378c8ef146be00")
        );
        assert_eq!(
            hmac_sha1(
                &[0xaa; 80],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )
            .to_vec(),
            hx("aa4ae5e15272d00e95705637ce8a3b55ed402112")
        );
    }

    /// Derived outside this crate (Python `hmac`/`hashlib`).
    #[test]
    fn the_header_key_is_the_hmac_of_the_session_key_under_the_seed() {
        assert_eq!(
            derive_header_key(&session_key()).to_vec(),
            hx("51751ac2089c74ad909ba1558ca041ba822b9d9e")
        );
    }

    /// Four headers cross the 20-byte key wrap; expected bytes from an independent Python run.
    #[test]
    fn the_tbc_cipher_cycles_over_twenty_key_bytes() {
        let (_p, crypto) = ProofSeed::new().into_client_header_crypto_tbc(
            &NormalizedString::new("alice").unwrap(),
            session_key(),
            0xDEAD_BEEF,
        );
        let (mut enc, mut dec) = crypto.split();
        assert_eq!(
            enc.encrypt_client_header(12, 0x37F).to_vec(),
            hx("51ca2ff0f894")
        );
        assert_eq!(
            enc.encrypt_client_header(0x1FF, 0xC7).to_vec(),
            hx("095bb24dee43")
        );
        let mut rest = [0u8, 1, 2, 3, 4, 5, 0, 1, 2, 3, 4, 5];
        enc.encrypt(&mut rest);
        assert_eq!(rest.to_vec(), hx("cf70b36cf220bd5caf25430a"));

        let mut buf: Vec<u8> = (0..32u16).map(|i| (i as u8).wrapping_mul(13)).collect();
        dec.decrypt(&mut buf);
        assert_eq!(
            buf,
            hx("517817cf059179a09d96ac5881ad4cb78f2690935c7817cf059179a09d96ac58")
        );
    }

    #[test]
    fn the_server_half_inverts_the_client_half() {
        let mut client = HeaderCrypto::from_session_key_tbc(session_key());
        let mut server = HeaderCrypto::from_session_key_tbc(session_key());
        let mut data: Vec<u8> = (0..50u8).collect();
        let plain = data.clone();
        client.encrypter().encrypt(&mut data);
        assert_ne!(data, plain);
        server.decrypter().decrypt(&mut data);
        assert_eq!(data, plain);
    }
}
