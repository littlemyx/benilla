//! Single-unit files of a real 2.4.3 install (`$WOW_DATA_TBC`): read whole, to the block table's
//! size, and the TrueType fonts pass their own table-directory checksums.
use benilla_mpq::{Archive, Error};

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}

/// Every table of an sfnt sums to the checksum its directory entry holds (`head` with its
/// checkSumAdjustment counted as zero).
fn ttf_checksums_hold(b: &[u8]) -> bool {
    let n = u16::from_be_bytes([b[4], b[5]]) as usize;
    (0..n).all(|i| {
        let r = 12 + 16 * i;
        let (off, len) = (be32(b, r + 8) as usize, be32(b, r + 12) as usize);
        let sum = b[off..off + len]
            .chunks(4)
            .enumerate()
            .fold(0u32, |acc, (k, c)| {
                let mut w = [0u8; 4];
                w[..c.len()].copy_from_slice(c);
                if &b[r..r + 4] == b"head" && k == 2 {
                    acc
                } else {
                    acc.wrapping_add(u32::from_be_bytes(w))
                }
            });
        sum == be32(b, r + 4)
    })
}

fn tbc_data() -> Option<std::path::PathBuf> {
    let Some(d) = std::env::var_os("WOW_DATA_TBC").filter(|v| !v.is_empty()) else {
        eprintln!("skipping: WOW_DATA_TBC not set");
        return None;
    };
    Some(d.into())
}

#[test]
fn the_single_unit_font_of_the_locale_archive_reads_and_checks() {
    let Some(data) = tbc_data() else { return };
    let a = Archive::open(data.join("enGB/locale-enGB.MPQ")).unwrap();
    let blocks = a.blocks();
    let single: Vec<_> = blocks.iter().filter(|b| b.is_single_unit()).collect();
    assert_eq!(
        single.len(),
        1,
        "the locale archive holds one single-unit file"
    );
    let bytes = a.read_block(single[0].index).unwrap();
    assert_eq!(bytes.len(), single[0].file_size as usize);
    assert!(ttf_checksums_hold(&bytes));
    // By name, the way the font loader asks: the single-unit one and the sectored ones.
    for name in ["FRIZQT__.TTF", "ARIALN.TTF", "skurri.ttf", "MORPHEUS.TTF"] {
        let path = format!("Fonts\\{name}");
        let bytes = a.read_file(&path).unwrap();
        assert_eq!(Some(bytes.len() as u32), a.file_size(&path), "{path}");
        // skurri.ttf's own directory checksums do not hold (a sectored file: the 1.12.1 reader
        // returns the same bytes for such files), so it is held to its length alone.
        assert!(name == "skurri.ttf" || ttf_checksums_hold(&bytes), "{path}");
    }
}

#[test]
fn every_single_unit_file_reads_to_its_size_or_names_the_codec_it_lacks() {
    let Some(data) = tbc_data() else { return };
    let (mut read, mut lacking) = (0, 0);
    for archive in [
        "enGB/base-enGB.MPQ",
        "enGB/backup-enGB.MPQ",
        "enGB/locale-enGB.MPQ",
    ] {
        let a = Archive::open(data.join(archive)).unwrap();
        for b in a.blocks().iter().filter(|b| b.is_single_unit()) {
            match a.read_block(b.index) {
                Ok(bytes) => {
                    assert_eq!(
                        bytes.len(),
                        b.file_size as usize,
                        "{archive} block {}",
                        b.index
                    );
                    read += 1;
                }
                Err(Error::Unsupported(m)) => {
                    assert!(m.contains("codec 0x10"), "{m}");
                    lacking += 1;
                }
                Err(e) => panic!("{archive} block {}: {e}", b.index),
            }
        }
    }
    assert_eq!(
        (read, lacking),
        (219, 5),
        "the installer archives' bzip2 units stay refused"
    );
}
