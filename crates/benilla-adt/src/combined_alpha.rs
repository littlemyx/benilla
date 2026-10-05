//! MCAL to one 64×64 RGBA alpha map, ported from wow-adt's `CombinedAlphaMap`: R, G, B are the
//! opacity of texture layers 1, 2, 3 and A is 255. Layers go in MCLY order after the opaque base. A
//! layer is 4-bit packed (×17: the reference packs it into an RGBA4444 texel, read as `n / 15`),
//! 8-bit raw, or RLE (token bit 7 fill, else copy; the low 7 bits are the count).

use crate::McnkChunk;

/// The combined 64×64 RGBA alpha map, `y`-major.
pub struct CombinedAlphaMap {
    map: Vec<u8>, // 64*64*4, [y][x][rgba]
    x: usize,
    y: usize,
    layer: usize,
    has_big_alpha: bool,
    fix_alpha: bool,
}

const W: usize = 64;

impl CombinedAlphaMap {
    /// Decode `chunk`'s alpha layers. `has_big_alpha` makes uncompressed layers 8-bit, and
    /// `fix_alpha` fills the last row and column from their neighbours (a 63×63 source); vanilla
    /// is `false`, `true`. A chunk whose header sets [`crate::MCNK_DO_NOT_FIX_ALPHA`] keeps its own
    /// last row and column: measured on 2.4.3, those layers differ from their neighbours in 29% of
    /// cases against 80% in unflagged chunks, and the 1.12.1 chunks that set it already match, so
    /// 1.12.1 output is the same either way.
    pub fn new(chunk: &McnkChunk, has_big_alpha: bool, fix_alpha: bool) -> Self {
        let fix_alpha = fix_alpha && chunk.header.flags & crate::MCNK_DO_NOT_FIX_ALPHA == 0;
        let mut map = vec![0u8; W * W * 4];
        for px in map.as_chunks_mut::<4>().0 {
            px[3] = 255; // A is unused; opaque, so a dump shows the channels
        }
        let mut s = Self {
            map,
            x: 0,
            y: 0,
            layer: 0,
            has_big_alpha,
            fix_alpha,
        };
        s.ingest_chunk_layers(chunk);
        s
    }

    /// The combined RGBA bytes (64×64×4).
    pub fn as_slice(&self) -> &[u8] {
        &self.map
    }

    fn ingest_chunk_layers(&mut self, chunk: &McnkChunk) {
        let (Some(mcly), Some(mcal)) = (&chunk.layers, &chunk.alpha) else {
            return;
        };
        for layer in mcly.layers.iter().skip(1) {
            let offset = layer.offset_in_mcal as usize;
            if layer.flags.alpha_map_compressed() {
                self.ingest_compressed(&mcal.data, offset);
            } else if self.has_big_alpha {
                self.ingest_big(&mcal.data, offset);
            } else {
                self.ingest_small(&mcal.data, offset);
            }
        }
    }

    fn ingest_big(&mut self, raw: &[u8], offset: usize) {
        const LAYER: usize = W * W;
        if offset + LAYER <= raw.len() {
            for &a in &raw[offset..offset + LAYER] {
                if !self.set_next(a) {
                    break;
                }
            }
        }
        self.next_layer();
    }

    fn ingest_small(&mut self, raw: &[u8], offset: usize) {
        const PACKED: usize = W * W / 2;
        if offset + PACKED <= raw.len() {
            for &p in &raw[offset..offset + PACKED] {
                if !self.set_next((p & 0x0F) * 17) {
                    break;
                }
                if !self.set_next(((p >> 4) & 0x0F) * 17) {
                    break;
                }
            }
        }
        self.next_layer();
    }

    fn ingest_compressed(&mut self, raw: &[u8], mut offset: usize) {
        const TARGET: usize = W * W;
        let mut out = Vec::with_capacity(TARGET);
        while out.len() < TARGET {
            if offset >= raw.len() {
                break;
            }
            let token = raw[offset];
            offset += 1;
            let count = (token & 0x7F) as usize;
            if count == 0 {
                continue;
            }
            if token & 0x80 != 0 {
                // fill
                if offset >= raw.len() {
                    break;
                }
                let v = raw[offset];
                offset += 1;
                for _ in 0..count {
                    if out.len() >= TARGET {
                        break;
                    }
                    out.push(v);
                }
            } else {
                // copy
                for _ in 0..count {
                    if offset >= raw.len() || out.len() >= TARGET {
                        break;
                    }
                    out.push(raw[offset]);
                    offset += 1;
                }
            }
        }
        out.resize(TARGET, 0);
        for &a in &out {
            if !self.set_next(a) {
                break;
            }
        }
        self.next_layer();
    }

    fn get(&self, x: usize, y: usize, layer: usize) -> u8 {
        if x < W && y < W && layer < 4 {
            self.map[(y * W + x) * 4 + layer]
        } else {
            0
        }
    }

    fn set(&mut self, x: usize, y: usize, layer: usize, a: u8) {
        if x < W && y < W && layer < 4 {
            self.map[(y * W + x) * 4 + layer] = a;
        }
    }

    fn set_next(&mut self, mut a: u8) -> bool {
        if self.fix_alpha {
            if self.x == 63 {
                a = self.get(self.x - 1, self.y, self.layer);
            }
            if self.y == 63 {
                a = self.get(self.x, self.y - 1, self.layer);
            }
        }
        self.set(self.x, self.y, self.layer, a);
        self.x += 1;
        if self.x >= W {
            self.x = 0;
            self.y += 1;
        }
        self.y < W
    }

    fn next_layer(&mut self) {
        self.layer += 1;
        self.x = 0;
        self.y = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> CombinedAlphaMap {
        CombinedAlphaMap {
            map: vec![0; W * W * 4],
            x: 0,
            y: 0,
            layer: 0,
            has_big_alpha: false,
            fix_alpha: true,
        }
    }

    /// The reference packs a 4-bit weight into an RGBA4444 texel (64×64 `0x6b03d0`, 32×32
    /// `0x6b08d0`), read as `n / 15`, so nibble 15 is full coverage.
    /// A chunk with an opaque base layer and one 4-bit layer whose texels are all `0xF` except
    /// the last column, which is `0x0`.
    fn chunk_with_odd_last_column(flags: u32) -> McnkChunk {
        use crate::{McalChunk, MclyChunk, MclyFlags, MclyLayer, McnkHeader};
        let layer = |offset_in_mcal| MclyLayer {
            texture_id: 0,
            flags: MclyFlags { value: 0 },
            offset_in_mcal,
            effect_id: 0,
        };
        let mut data = vec![0xFFu8; W * W / 2];
        for row in 0..W {
            data[row * W / 2 + W / 2 - 1] = 0x0F; // texel 62 full, texel 63 empty
        }
        McnkChunk {
            header: McnkHeader {
                flags,
                index_x: 0,
                index_y: 0,
                area_id: 0,
                holes_low_res: 0,
                pred_tex: [0; 8],
                no_effect_doodad: [0; 8],
                unknown_8bytes: [0; 8],
                position: [0.0; 3],
            },
            heights: None,
            normals: None,
            layers: Some(MclyChunk {
                layers: vec![layer(0), layer(0)],
            }),
            alpha: Some(McalChunk { data }),
            shadow: None,
            liquids: Vec::new(),
        }
    }

    #[test]
    fn the_do_not_fix_flag_keeps_the_last_row_and_column() {
        let unflagged = CombinedAlphaMap::new(&chunk_with_odd_last_column(0), false, true);
        let flagged = CombinedAlphaMap::new(
            &chunk_with_odd_last_column(crate::MCNK_DO_NOT_FIX_ALPHA),
            false,
            true,
        );
        // The file holds 255 at column 62 and 0 at column 63; unflagged, column 63 copies 62.
        let texel = |m: &CombinedAlphaMap, x: usize, y: usize| m.as_slice()[(y * W + x) * 4];
        assert_eq!(texel(&unflagged, 62, 0), 255);
        assert_eq!(texel(&unflagged, 63, 0), 255);
        assert_eq!(texel(&flagged, 62, 0), 255);
        assert_eq!(texel(&flagged, 63, 0), 0);
    }

    #[test]
    fn four_bit_alpha_reads_as_n_over_15() {
        let mut m = blank();
        // Low nibble first: texel 0 = 0xF, texel 1 = 0x8.
        m.ingest_small(&[0x8F; W * W / 2], 0);
        assert_eq!(m.get(0, 0, 0), 255, "nibble 15 is full coverage");
        assert_eq!(m.get(1, 0, 0), 136, "nibble 8 is 8/15");
        let mut z = blank();
        z.ingest_small(&[0x00; W * W / 2], 0);
        assert_eq!(z.get(0, 0, 0), 0);
    }
}
