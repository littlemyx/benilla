//! The Sunwell5ManFix tiles (WDT `MPHD` bit 2, big alpha) decode with 8-bit uncompressed layers
//! and RLE layers (`MCLY` flag 0x200), pinned on one named tile. Each number was read from the
//! bytes with an independent python reader (`py_alpha.py`: 54,384 chunks agree with the Rust map).

use benilla_adt::{parse_adt, CombinedAlphaMap, ParsedAdt};
use benilla_formats::{adt_to_tile_mesh, adt_to_tile_mesh_with_alpha, Chain, MapTiles};
use std::io::Cursor;

const TILE: &str = "World\\Maps\\Sunwell5ManFix\\Sunwell5ManFix_31_32.adt";

#[test]
fn the_sunwell_map_reports_big_alpha_through_its_tile_index() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let mut chain = Chain::open(&data).expect("open the 2.4.3 chain");
    assert!(MapTiles::load(&mut chain, "Sunwell5ManFix")
        .expect("wdt")
        .has_big_alpha());
    assert!(!MapTiles::load(&mut chain, "Expansion01")
        .expect("wdt")
        .has_big_alpha());
}

#[test]
fn a_sunwell_tile_decodes_big_and_rle_layers() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let chain = Chain::open(&data).expect("open the 2.4.3 chain");
    let bytes = chain.read(TILE).expect("read the tile");
    let ParsedAdt::Root(root) = parse_adt(&mut Cursor::new(&bytes[..])).expect("parse");

    // 152 chunks have texture layers; 268 of their layers are RLE (flag 0x200) and 23 raw.
    let (mut with_layers, mut rle, mut raw) = (0, 0, 0);
    for c in &root.mcnk_chunks {
        let Some(l) = c.layers.as_ref().filter(|l| l.layers.len() > 1) else {
            continue;
        };
        with_layers += 1;
        for ly in l.layers.iter().skip(1) {
            if ly.flags.alpha_map_compressed() {
                rle += 1;
            } else {
                raw += 1;
            }
        }
    }
    assert_eq!((with_layers, rle, raw), (152, 268, 23));

    // Chunk 14: layer 1 raw 4096 bytes at 0, layer 2 RLE at 4096, layer 3 raw 4096 bytes at
    // 4831. Texel (20, 10): big alpha reads the raw layers as 8 bits, the 4-bit read does not.
    let chunk = &root.mcnk_chunks[14];
    let at = |m: &CombinedAlphaMap| {
        let i = (10 * 64 + 20) * 4;
        [m.as_slice()[i], m.as_slice()[i + 1], m.as_slice()[i + 2]]
    };
    assert_eq!(
        at(&CombinedAlphaMap::new(chunk, true, true)),
        [0x9f, 0x00, 0x5f]
    );
    assert_eq!(
        at(&CombinedAlphaMap::new(chunk, false, true)),
        [0xff, 0x00, 0xff]
    );

    // Through the mesh path: 16 chunks of the tile decode differently with the map's flag.
    let big = adt_to_tile_mesh_with_alpha(&bytes, true).expect("mesh");
    let small = adt_to_tile_mesh(&bytes).expect("mesh");
    assert_eq!(big.chunks.len(), small.chunks.len());
    let differing = big
        .chunks
        .iter()
        .zip(&small.chunks)
        .filter(|(a, b)| a.alpha_map != b.alpha_map)
        .count();
    assert_eq!(differing, 16);
}
