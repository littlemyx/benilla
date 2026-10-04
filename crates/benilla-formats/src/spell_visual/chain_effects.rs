//! `SpellChainEffects.dbc`: the beams of Chain Lightning, Drain Life, Mind Flay, Chain Heal. A kit
//! reaches it only through a CharProc slot of type 0 or 12, both of which the dispatcher
//! (`0x60d7c0`, table `0x60dc20`) routes to one case (`0x60da79`) and on to `CreateChainVisual`
//! (`0x6ecbd0`). Ids skip 14 and 16, so an id is a lookup, never an index.
//! `benilla-extract <Data> chaincensus` prints the table with every kit that draws a beam.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};

use crate::{Chain, DbcLayout};

const SPELL_CHAIN_EFFECTS: &str = "DBFilesClient\\SpellChainEffects.dbc";
const SPELL_CHAIN_EFFECTS_FIELDS: usize = 8;

/// The client's clamp on a chain proc's beam count (`0x6ecbd0`); only kit 6397, Chain Burn, asks
/// for more than one.
pub const CHAIN_MAX_BEAMS: u32 = 3;

/// One row: the shape and animation of one beam. Fields 4-6 are not what their community names
/// (`TexCoordScale`, `SegDuration`, `SegDelay`) say.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainEffect {
    /// Field 1: the target segment length; a hop gets `trunc(length / this + 2.0)` segments, so
    /// never fewer than two (`0x7af713`).
    pub avg_seg_len: f32,
    /// Field 2: the half-width; the ribbon spans twice this in yards.
    pub half_width: f32,
    /// Field 3: the jitter amplitude as a fraction of the beam's length, re-rolled every frame
    /// (`0x7b0950`) and blended 0.75 old to 0.25 new; 0.01 on most rows, 0.04 on Chain Lightning's.
    pub noise_scale: f32,
    /// Field 4: the texture's scroll period in seconds, `u = -(phase / this)` with `phase`
    /// advancing by `dt` modulo this (`0x7af9d7`). A negative period reverses the scroll: four
    /// rows, the drains among them, ship -0.5, flowing back toward the caster.
    pub scroll_period_s: f32,
    /// Field 5: how long one hop burns in ms (`0x6ec9eb`), and so the beam's expiry,
    /// `now + hops × this` (`0x6ecd30`); a channel beam ignores it and lives until swept.
    pub bolt_life_ms: u32,
    /// Field 6: the stagger between hops in ms, hop `i` lighting at `t0 + i × this` (`0x6ec9da`),
    /// so a 3-hop cast arcs outward; a channel beam bypasses it.
    pub bolt_stagger_ms: u32,
    /// Field 7: the texture path as stored (`Textures\SpellChainEffects\*.blp`).
    pub texture: String,
}

/// A kit's chain CharProc, its params read through [`super::char_proc_small_int`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainProc {
    /// `CharParamZero`, the [`ChainEffect`] id; never 0, as the client no-ops an id with no row
    /// (`0x6ecc0e`).
    pub effect_id: u32,
    /// `CharParamOne`, clamped to [`CHAIN_MAX_BEAMS`]; the client draws nothing for 0.
    pub beams: u32,
    /// `CharParamTwo` as a bool (`setne`): set on channel-stage kits, clear on cast-stage ones, and
    /// the only real difference between the two chain types.
    pub flag: bool,
    /// The stored type, 0 or 12; behaviour keys off [`Self::flag`], as the dispatcher never forks.
    pub ty: i32,
}

/// The header shape of a build's table: 8 fields in 32 bytes in 1.12.1; 47 fields in 173 bytes in
/// 2.4.3, where five colour and blend columns are bytes (39 dwords, 5 bytes, 3 dwords). The eight
/// read columns are the first eight dwords of both, so a record is read by hand off its own stride.
pub(crate) fn chain_effects_shape(layout: DbcLayout) -> (u32, u32) {
    if layout.is_tbc() {
        (47, 173)
    } else {
        (SPELL_CHAIN_EFFECTS_FIELDS as u32, 32)
    }
}

/// Read `SpellChainEffects.dbc`; a textureless row stays, as the client's constructor never reads
/// the texture.
pub(super) fn load(chain: &mut Chain) -> Result<HashMap<u32, ChainEffect>> {
    let bytes = chain
        .read_file(SPELL_CHAIN_EFFECTS)
        .with_context(|| format!("reading {SPELL_CHAIN_EFFECTS}"))?;
    let (want_fields, want_size) = chain_effects_shape(chain.dbc_layout());
    let header = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("four bytes")))
    };
    let (Some(count), Some(fields), Some(size), Some(string_size)) =
        (header(4), header(8), header(12), header(16))
    else {
        bail!("SpellChainEffects.dbc: header truncated");
    };
    if &bytes[0..4] != b"WDBC" || fields != want_fields || size != want_size {
        bail!(
            "SpellChainEffects.dbc: unexpected layout (fields {fields}, record size {size}; \
             expected {want_fields}/{want_size})"
        );
    }
    let (count, size, string_size) = (count as usize, size as usize, string_size as usize);
    let records_end = 20 + count * size;
    if bytes.len() < records_end + string_size {
        bail!("SpellChainEffects.dbc: records or string block run past the file");
    }
    let strings = &bytes[records_end..records_end + string_size];
    let mut rows = HashMap::with_capacity(count);
    for i in 0..count {
        let at = 20 + i * size;
        let word = |k: usize| {
            u32::from_le_bytes(
                bytes[at + 4 * k..at + 4 * k + 4]
                    .try_into()
                    .expect("four bytes"),
            )
        };
        let float = |k: usize| f32::from_bits(word(k));
        let texture = strings
            .get(word(7) as usize..)
            .and_then(|tail| tail.split(|&b| b == 0).next())
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default();
        rows.insert(
            word(0),
            ChainEffect {
                avg_seg_len: float(1),
                half_width: float(2),
                noise_scale: float(3),
                scroll_period_s: float(4),
                bolt_life_ms: word(5),
                bolt_stagger_ms: word(6),
                texture,
            },
        );
    }
    Ok(rows)
}
