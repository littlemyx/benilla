//! The texture reads that found nothing or would not decode. Both loaders drop such a texture to a
//! flat fallback (the reference's loaders fail silently too), so an instrument reads the count here.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// One miss class: `(what was loading, "missing" or "undecodable", the texture path)`.
type Miss = (&'static str, &'static str, String);

static MISSES: Mutex<BTreeMap<Miss, u32>> = Mutex::new(BTreeMap::new());

/// Notes a texture read that failed with `err`, whose outermost context says reading or decoding.
pub(crate) fn record(kind: &'static str, path: &str, err: &anyhow::Error) {
    let why = if err.to_string().starts_with("decoding") {
        "undecodable"
    } else {
        "missing"
    };
    *MISSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entry((kind, why, path.to_ascii_lowercase()))
        .or_default() += 1;
}

/// Every miss so far as `(kind, why, path, attempts)`, most attempts first.
pub fn texture_misses() -> Vec<(&'static str, &'static str, String, u32)> {
    let mut v: Vec<_> = MISSES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .map(|((k, w, p), n)| (*k, *w, p.clone(), *n))
        .collect();
    v.sort_by(|a, b| b.3.cmp(&a.3).then_with(|| a.2.cmp(&b.2)));
    v
}
