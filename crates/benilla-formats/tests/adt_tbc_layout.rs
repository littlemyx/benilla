//! 2.4.3 ADT and WDT facts, pinned on named files. Each was read from the bytes with an
//! independent reader (`MCNK` flag bit 15 and `MPHD` bit 2 included).

use benilla_adt::{parse_adt, CombinedAlphaMap, ParsedAdt, MCNK_DO_NOT_FIX_ALPHA};
use benilla_formats::Chain;
use benilla_wdt::{WdtReader, WowVersion};
use std::io::Cursor;

#[test]
fn an_outland_tile_with_four_layers_keeps_the_last_alpha_column_its_chunk_flags() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let chain = Chain::open(&data).expect("open the 2.4.3 chain");
    let bytes = chain
        .read("World\\Maps\\Expansion01\\Expansion01_14_34.adt")
        .expect("read the tile");
    let ParsedAdt::Root(root) = parse_adt(&mut Cursor::new(&bytes[..])).expect("parse");
    assert_eq!(root.mcnk_chunks.len(), 256);
    let wet = |c: &&benilla_adt::McnkChunk| c.header.flags & 0x3c != 0;
    assert_eq!(root.mcnk_chunks.iter().filter(wet).count(), 45);
    assert_eq!(
        root.mcnk_chunks
            .iter()
            .filter(|c| c.layers.as_ref().is_some_and(|l| l.layers.len() == 4))
            .count(),
        190
    );
    // Chunk 17: flag 0x8000 only, four layers, layer 1 a 4-bit map whose row 0 ends 0 (col 62)
    // then 136 (col 63).
    let c = &root.mcnk_chunks[17];
    assert_eq!(c.header.flags, MCNK_DO_NOT_FIX_ALPHA);
    let m = CombinedAlphaMap::new(c, false, true);
    let texel = |x: usize, y: usize| m.as_slice()[(y * 64 + x) * 4];
    assert_eq!((texel(62, 0), texel(63, 0)), (0, 136));
}

#[test]
fn only_sunwell5manfix_sets_the_big_alpha_bit_among_the_2_4_3_wdts() {
    let data = benilla_formats::wow_data_tbc_or_skip!();
    let chain = Chain::open(&data).expect("open the 2.4.3 chain");
    let big = |name: &str| {
        let bytes = chain.read(name).expect("read the wdt");
        WdtReader::new(Cursor::new(bytes), WowVersion::Classic)
            .read()
            .expect("parse")
            .has_big_alpha()
    };
    assert!(big("World\\Maps\\Sunwell5ManFix\\Sunwell5ManFix.wdt"));
    assert!(!big("World\\Maps\\Expansion01\\Expansion01.wdt"));
    assert!(!big("World\\Maps\\Azeroth\\Azeroth.wdt"));
}
