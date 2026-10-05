//! `census`: the flags of every block of the given archives, the codec masks of the compressed
//! ones, and the content check of every single-unit file the reader reads.
//!
//! ```text
//! cargo run -q -p benilla-mpq --example census -- <archive.MPQ>...
//! ```
//!
//! A block has no name in an archive that carries no listfile, so a file's type is told by its
//! magic and checked by what the format itself promises: a TrueType table directory's checksums,
//! a DBC's header arithmetic, a BLP's mip table, an M2's name span, a chunked file's chunks ending
//! exactly at its end, a zlib stream ending exactly at its unit's end.
use benilla_mpq::{Archive, BlockInfo};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn ttf(b: &[u8]) -> Result<(), String> {
    let n = b
        .get(4..6)
        .map(|x| u16::from_be_bytes([x[0], x[1]]))
        .ok_or("short")? as usize;
    for i in 0..n {
        let r = 12 + 16 * i;
        let (Some(sum), Some(off), Some(len)) = (be32(b, r + 4), be32(b, r + 8), be32(b, r + 12))
        else {
            return Err("directory past the end".into());
        };
        let (off, len) = (off as usize, len as usize);
        let t = b
            .get(off..off.saturating_add(len))
            .ok_or("table past the end")?;
        let mut acc = 0u32;
        let tag = &b[r..r + 4];
        for (k, c) in t.chunks(4).enumerate() {
            let mut w = [0u8; 4];
            w[..c.len()].copy_from_slice(c);
            if tag == b"head" && k == 2 {
                continue; // checkSumAdjustment is summed as zero
            }
            acc = acc.wrapping_add(u32::from_be_bytes(w));
        }
        if acc != sum {
            return Err(format!("table {} checksum", String::from_utf8_lossy(tag)));
        }
    }
    Ok(())
}

fn dbc(b: &[u8]) -> Result<(), String> {
    let (Some(r), Some(rs), Some(ss)) = (le32(b, 4), le32(b, 12), le32(b, 16)) else {
        return Err("short".into());
    };
    if 20 + r as usize * rs as usize + ss as usize == b.len() {
        Ok(())
    } else {
        Err("header arithmetic".into())
    }
}

fn blp(b: &[u8]) -> Result<(), String> {
    let (offs, sizes) = if &b[..4] == b"BLP2" {
        (20, 84)
    } else {
        (28, 92)
    };
    let mut any = false;
    for i in 0..16 {
        let (Some(o), Some(s)) = (le32(b, offs + 4 * i), le32(b, sizes + 4 * i)) else {
            return Err("short header".into());
        };
        if s == 0 {
            continue;
        }
        any = true;
        if (o as usize).saturating_add(s as usize) > b.len() {
            return Err(format!("mip {i} past the end"));
        }
    }
    if any {
        Ok(())
    } else {
        Err("no mip".into())
    }
}

fn m2(b: &[u8]) -> Result<(), String> {
    let (Some(v), Some(l), Some(o)) = (le32(b, 4), le32(b, 8), le32(b, 12)) else {
        return Err("short".into());
    };
    if !(256..=263).contains(&v) {
        return Err(format!("version {v}"));
    }
    if (o as usize).saturating_add(l as usize) > b.len() {
        return Err("name past the end".into());
    }
    Ok(())
}

fn chunks(b: &[u8]) -> Result<(), String> {
    let mut at = 0usize;
    while at < b.len() {
        let size = le32(b, at + 4).ok_or("chunk header past the end")? as usize;
        at = at.saturating_add(8).saturating_add(size);
    }
    if at == b.len() {
        Ok(())
    } else {
        Err("chunks overrun".into())
    }
}

fn zlib_exact(raw: &[u8]) -> Result<(), String> {
    let mut d = flate2::bufread::ZlibDecoder::new(raw);
    std::io::copy(&mut d, &mut std::io::sink()).map_err(|e| e.to_string())?;
    if d.total_in() as usize == raw.len() {
        Ok(())
    } else {
        Err("stream ends early".into())
    }
}

fn classify(b: &[u8]) -> (&'static str, Result<(), String>) {
    match b.get(..4) {
        Some([0, 1, 0, 0]) | Some(b"true") => ("ttf", ttf(b)),
        Some(b"WDBC") => ("dbc", dbc(b)),
        Some(b"BLP2") | Some(b"BLP1") => ("blp", blp(b)),
        Some(b"MD20") => ("m2", m2(b)),
        Some(b"REVM") => ("chunked(wmo/adt/wdt)", chunks(b)),
        Some(b"RIFF") => (
            "wav",
            if le32(b, 4).map(|n| n as usize + 8 == b.len()) == Some(true) {
                Ok(())
            } else {
                Err("riff size".into())
            },
        ),
        Some([0xFF, 0xD8, ..]) => (
            "jpeg",
            if b.ends_with(&[0xFF, 0xD9]) {
                Ok(())
            } else {
                Err("no EOI".into())
            },
        ),
        Some(b"GIF8") => (
            "gif",
            if b.last() == Some(&0x3B) {
                Ok(())
            } else {
                Err("no trailer".into())
            },
        ),
        Some(b"%PDF") => (
            "pdf",
            if b[b.len().saturating_sub(64)..]
                .windows(5)
                .any(|w| w == b"%%EOF")
            {
                Ok(())
            } else {
                Err("no %%EOF".into())
            },
        ),
        Some(b"icns") => (
            "icns",
            if be32(b, 4).map(|n| n as usize == b.len()) == Some(true) {
                Ok(())
            } else {
                Err("size".into())
            },
        ),
        Some([b'M', b'Z', ..]) => (
            "exe(MZ)",
            match le32(b, 0x3C).and_then(|o| b.get(o as usize..o as usize + 4)) {
                Some(b"PE\0\0") => Ok(()),
                _ => Err("no PE header".into()),
            },
        ),
        _ if !b.contains(&0) && std::str::from_utf8(b).is_ok() => ("text(utf-8)", Ok(())),
        _ => ("other(length only)", Ok(())),
    }
}

fn mask_of(b: &BlockInfo, f: &mut std::fs::File, sector: usize) -> Option<u8> {
    if !b.is_compressed() || b.is_encrypted() {
        return None;
    }
    f.seek(SeekFrom::Start(b.file_pos)).ok()?;
    let mut m = [0u8; 1];
    if b.is_single_unit() {
        if b.comp_size >= b.file_size {
            return None; // stored: no mask byte
        }
    } else {
        let mut o = [0u8; 8];
        f.read_exact(&mut o).ok()?;
        let (s0, s1) = (
            u32::from_le_bytes(o[..4].try_into().ok()?),
            u32::from_le_bytes(o[4..].try_into().ok()?),
        );
        // a sector no shorter than its share is stored: its first byte is data, not a mask
        if (s1.saturating_sub(s0) as usize) >= (b.file_size as usize).min(sector) {
            return None;
        }
        f.seek(SeekFrom::Start(b.file_pos + u64::from(s0))).ok()?;
    }
    f.read_exact(&mut m).ok()?;
    Some(m[0])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut flags: BTreeMap<&str, usize> = BTreeMap::new();
    let mut su_masks: BTreeMap<String, usize> = BTreeMap::new();
    let mut sec_masks: BTreeMap<String, usize> = BTreeMap::new();
    let mut types: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new(); // total, ok, bad
    let mut others: BTreeMap<String, usize> = BTreeMap::new();
    let mut errors: BTreeMap<String, usize> = BTreeMap::new();
    let (mut blocks, mut su_read, mut su_unread, mut su_zlib_exact) =
        (0usize, 0usize, 0usize, 0usize);
    for path in std::env::args().skip(1) {
        let a = match Archive::open(&path) {
            Ok(a) => a,
            Err(e) => {
                println!("not read: {path}: {e}");
                continue;
            }
        };
        let name = std::path::Path::new(&path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut f = std::fs::File::open(&path)?;
        let su = a.blocks().iter().filter(|b| b.is_single_unit()).count();
        println!(
            "archive {name}: {} blocks, {su} single-unit",
            a.blocks().len()
        );
        for b in a.blocks() {
            blocks += 1;
            let bf = [
                ("exists", b.exists()),
                ("single_unit", b.is_single_unit()),
                ("compressed", b.is_compressed()),
                ("imploded", b.is_imploded()),
                ("encrypted", b.is_encrypted()),
                ("fix_key", b.is_fix_key()),
                ("patch", b.is_patch()),
                ("delete_marker", b.is_delete_marker()),
                ("sector_crc", b.has_sector_crc()),
            ];
            for (n, on) in bf {
                if on {
                    *flags.entry(n).or_default() += 1;
                }
            }
            if !b.exists() || b.is_delete_marker() {
                continue;
            }
            if let Some(m) = mask_of(&b, &mut f, a.sector_size()) {
                let map = if b.is_single_unit() {
                    &mut su_masks
                } else {
                    &mut sec_masks
                };
                *map.entry(format!("0x{m:02X}")).or_default() += 1;
            }
            if !b.is_single_unit() {
                continue;
            }
            match a.read_block(b.index) {
                Ok(bytes) => {
                    su_read += 1;
                    let (ty, mut res) = classify(&bytes);
                    if ty.starts_with("other") {
                        let head: String =
                            bytes.iter().take(4).map(|c| format!("{c:02x}")).collect();
                        *others.entry(format!("{name} {head}")).or_default() += 1;
                    }
                    if bytes.len() != b.file_size as usize {
                        res = Err("length != block table size".into());
                    }
                    // the zlib stream must end exactly at the unit's end
                    if b.is_compressed() && b.comp_size < b.file_size && b.comp_size > 1 {
                        let mut raw = vec![0u8; b.comp_size as usize - 1];
                        f.seek(SeekFrom::Start(b.file_pos + 1))?;
                        f.read_exact(&mut raw)?;
                        match zlib_exact(&raw) {
                            Ok(()) => su_zlib_exact += 1,
                            Err(e) => res = Err(format!("zlib: {e}")),
                        }
                    }
                    let e = types.entry(ty).or_default();
                    e.0 += 1;
                    match res {
                        Ok(()) => e.1 += 1,
                        Err(m) => {
                            e.2 += 1;
                            *errors.entry(format!("{ty}: {m}")).or_default() += 1;
                        }
                    }
                }
                Err(e) => {
                    su_unread += 1;
                    let m = e.to_string().replace(|c: char| c.is_ascii_digit(), "#");
                    *errors.entry(format!("UNREAD {name} {m}")).or_default() += 1;
                    println!(
                        "unread: {name} block {} comp {} size {}",
                        b.index, b.comp_size, b.file_size
                    );
                }
            }
        }
    }
    println!("blocks {blocks}");
    println!("flags {flags:?}");
    println!("single-unit codec masks {su_masks:?}");
    println!("sectored codec masks (first sector) {sec_masks:?}");
    println!(
        "single-unit read {su_read}, unread {su_unread}, zlib streams ending exactly {su_zlib_exact}"
    );
    for (t, (n, ok, bad)) in &types {
        println!("  {t}: {n} read, {ok} pass, {bad} fail");
    }
    if std::env::var_os("CENSUS_OTHERS").is_some() {
        for (e, n) in &others {
            println!("  other {n} x {e}");
        }
    }
    for (e, n) in &errors {
        println!("  {n} x {e}");
    }
    Ok(())
}
