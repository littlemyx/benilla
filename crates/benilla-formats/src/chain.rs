//! The patch chain: a priority-ordered set of MPQ archives, read through `benilla-mpq`. A read
//! resolves a name to the highest-priority archive holding it, so a patch wins; base archives
//! carry no `(listfile)`, so resolution is by name hash. The layout is chosen from the shape of the
//! `Data` directory: the vanilla set of base archives, or the 2.4.3 `common.MPQ` plus a locale
//! folder.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use benilla_mpq::Archive;

use crate::VANILLA_BASE_ORDER;

/// One entry from a chain listing: an internal path and its uncompressed size.
pub struct ChainEntry {
    pub name: String,
    pub size: u64,
}

/// A priority-ordered patch chain of MPQ archives (`Send + Sync`; reads are `&self` and lock-free).
pub struct Chain {
    /// Ascending priority: later archives win.
    archives: Vec<Archive>,
    /// The locale folder a 2.4.3 layout mounted; `None` for the vanilla layout and a single file.
    locale: Option<String>,
}

/// `patch-?.MPQ` as the reference's `FindFirstFileW` glob matches it (template `0x82edbc`, wrapper
/// `0x42ad10`): `?` is exactly one character, any case, so `patch-10.MPQ` never mounts.
fn is_patch_glob_match(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(mid) = lower
        .strip_prefix("patch-")
        .and_then(|rest| rest.strip_suffix(".mpq"))
    else {
        return false;
    };
    mid.chars().count() == 1
}

/// The reference's mount order over a `Data` listing, ascending priority (`0x403740`): the ten
/// [`VANILLA_BASE_ORDER`] archives, `patch.MPQ`, every `patch-?.MPQ` by case-folded name (the
/// reference sorts descending with `strnicmp` and walks backwards), then `speech2.MPQ`. Names
/// match case-insensitively and come back as found on disk.
fn mount_order(dir_names: &[String]) -> Vec<String> {
    let find = |want: &str| {
        dir_names
            .iter()
            .find(|n| n.eq_ignore_ascii_case(want))
            .cloned()
    };
    let mut order: Vec<String> = VANILLA_BASE_ORDER.iter().filter_map(|b| find(b)).collect();
    order.extend(find("patch.MPQ"));
    let mut patches: Vec<String> = dir_names
        .iter()
        .filter(|n| is_patch_glob_match(n))
        .cloned()
        .collect();
    patches.sort_by_key(|n| n.to_ascii_lowercase());
    order.extend(patches);
    order.extend(find("speech2.MPQ"));
    order
}

/// `<prefix>?.MPQ` with `?` exactly one character, any case: the glob shape of `patch-?.MPQ` and
/// `patch-%s-?.MPQ`.
fn is_glob_one(name: &str, prefix: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower
        .strip_prefix(&prefix.to_ascii_lowercase())
        .and_then(|rest| rest.strip_suffix(".mpq"))
        .is_some_and(|mid| mid.chars().count() == 1)
}

/// The locale a 2.4.3 `Data` directory is read as, from its locale folders: those holding
/// `locale-<folder>.MPQ`. Convention, not the reference's rule: the client takes the locale from
/// `SET locale` in `WTF/Config.wtf`, and until that is read this picks `enUS`, else `enGB`, else
/// the first folder by name. `folders` is `(folder name, the file names in it)`.
fn pick_locale(folders: &[(String, Vec<String>)]) -> Option<&str> {
    let has_locale_archive = |(dir, files): &&(String, Vec<String>)| {
        let want = format!("locale-{dir}.MPQ");
        files.iter().any(|f| f.eq_ignore_ascii_case(&want))
    };
    let mut candidates: Vec<&str> = folders
        .iter()
        .filter(has_locale_archive)
        .map(|(d, _)| d.as_str())
        .collect();
    candidates.sort_unstable();
    ["enUS", "enGB"]
        .into_iter()
        .find_map(|want| {
            candidates
                .iter()
                .copied()
                .find(|c| c.eq_ignore_ascii_case(want))
        })
        .or_else(|| candidates.first().copied())
}

/// The 2.4.3 mount order over a `Data` listing and its locale folder `locale`, ascending priority,
/// as `(folder, file)` pairs (`None` is `Data` itself): `common`, `expansion`, then
/// `locale-<L>`, `speech-<L>`, `expansion-locale-<L>`, `expansion-speech-<L>`, `patch`,
/// `patch-?` ascending, `patch-<L>`, `patch-<L>-?` ascending. `base-<L>` and `backup-<L>` are not
/// mounted. Evidenced by the 2.4.3 exe's strings: the archive name templates (`common.MPQ`,
/// `expansion.MPQ`, `%s\locale-%s.MPQ`, `%s\speech-%s.MPQ`, `%s\expansion-locale-%s.MPQ`,
/// `%s\expansion-speech-%s.MPQ`, `patch.MPQ`, `patch-?.MPQ`, `patch-%s.MPQ`, `patch-%s-?.MPQ`);
/// and by the install's files: `patch-<L>-2` over `patch-<L>` over `locale-<L>` (the stock
/// `FrameXML.toc` states 20400 in `patch-enGB`, 20000 in `locale-enGB`; `Spell.dbc` is in all of
/// `locale-enGB`, `patch-enGB` and `patch-enGB-2`). Convention, no string or file orders them: the
/// order among the templates (the exe names no priorities), `patch`/`patch-?` below
/// `patch-<L>`/`patch-<L>-?`, and leaving `base-<L>`/`backup-<L>` out. On the enGB install the
/// `(listfile)`s of {`patch`, `patch-2`} and {`patch-enGB`, `patch-enGB-2`} share no name, so the
/// `patch`-versus-`patch-<L>` convention decides no read there.
fn tbc_mount_order(
    data_files: &[String],
    locale: &str,
    locale_files: &[String],
) -> Vec<(Option<String>, String)> {
    let find =
        |names: &[String], want: &str| names.iter().find(|n| n.eq_ignore_ascii_case(want)).cloned();
    let glob_sorted = |names: &[String], prefix: &str| {
        let mut v: Vec<String> = names
            .iter()
            .filter(|n| is_glob_one(n, prefix))
            .cloned()
            .collect();
        v.sort_by_key(|n| n.to_ascii_lowercase());
        v
    };
    let mut order = Vec::new();
    let top = |name: String| (None, name);
    let loc = |name: String| (Some(locale.to_string()), name);
    order.extend(find(data_files, "common.MPQ").map(top));
    order.extend(find(data_files, "expansion.MPQ").map(top));
    for stem in ["locale", "speech", "expansion-locale", "expansion-speech"] {
        order.extend(find(locale_files, &format!("{stem}-{locale}.MPQ")).map(loc));
    }
    order.extend(find(data_files, "patch.MPQ").map(top));
    order.extend(glob_sorted(data_files, "patch-").into_iter().map(top));
    order.extend(find(locale_files, &format!("patch-{locale}.MPQ")).map(loc));
    order.extend(
        glob_sorted(locale_files, &format!("patch-{locale}-"))
            .into_iter()
            .map(loc),
    );
    order
}

/// The file names in `dir`, sorted, symlinks followed.
fn file_names(dir: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .with_context(|| format!("listing {}", dir.display()))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            // `path().is_file()` follows symlinks (`read_dir`'s file_type doesn't).
            entry.path().is_file().then(|| entry.file_name())
        })
        .filter_map(|name| name.into_string().ok())
        .collect();
    // read_dir order is arbitrary; sort so case-variant ties resolve deterministically.
    names.sort();
    Ok(names)
}

/// The archive paths of a `Data` directory in ascending priority, and the locale folder when the
/// layout is 2.4.3's. The layout is chosen before any archive is read: any vanilla base archive
/// means [`mount_order`] (unchanged); else `common.MPQ` means [`tbc_mount_order`].
fn data_layout(path: &Path) -> Result<(Vec<PathBuf>, Option<String>)> {
    let names = file_names(path)?;
    let vanilla = mount_order(&names);
    let is_vanilla = VANILLA_BASE_ORDER
        .iter()
        .any(|b| names.iter().any(|n| n.eq_ignore_ascii_case(b)));
    let has_common = names.iter().any(|n| n.eq_ignore_ascii_case("common.MPQ"));
    // A directory with only patch archives keeps the vanilla order it always had.
    if is_vanilla || (!has_common && !vanilla.is_empty()) {
        return Ok((vanilla.iter().map(|n| path.join(n)).collect(), None));
    }
    if !has_common {
        bail!(
            "no known MPQs found in {}: expected a 1.12.1 Data directory ({}, ...) or a 2.4.3 one \
             (common.MPQ and a locale folder)",
            path.display(),
            VANILLA_BASE_ORDER[0]
        );
    }
    let mut folders = Vec::new();
    for entry in std::fs::read_dir(path)
        .with_context(|| format!("listing {}", path.display()))?
        .flatten()
    {
        let dir = entry.path();
        if let (true, Ok(name)) = (dir.is_dir(), entry.file_name().into_string()) {
            folders.push((name, file_names(&dir)?));
        }
    }
    folders.sort();
    let locale = pick_locale(&folders)
        .with_context(|| {
            format!(
                "no locale folder with a locale-<name>.MPQ in {}",
                path.display()
            )
        })?
        .to_string();
    let locale_files = &folders
        .iter()
        .find(|(d, _)| *d == locale)
        .expect("picked")
        .1;
    let paths = tbc_mount_order(&names, &locale, locale_files)
        .into_iter()
        .map(|(dir, file)| match dir {
            Some(d) => path.join(d).join(file),
            None => path.join(file),
        })
        .collect();
    Ok((paths, Some(locale)))
}

impl Chain {
    /// The locale folder a 2.4.3 `Data` directory was mounted with, or `None` for the vanilla
    /// layout and a single `.MPQ` file.
    pub fn locale(&self) -> Option<&str> {
        self.locale.as_deref()
    }

    /// Open a `Data` directory's archives in [`mount_order`] (vanilla) or [`tbc_mount_order`]
    /// (2.4.3), or a single `.MPQ` file.
    ///
    /// Deviation: an archive that fails to open is an error, where the reference logs
    /// `"Failed to open archive"` and goes on, because a skipped corrupt archive surfaces only as
    /// missing files far downstream.
    pub fn open(path: &Path) -> Result<Self> {
        let mut archives = Vec::new();
        let mut locale = None;
        if path.is_dir() {
            let (paths, picked) = data_layout(path)?;
            locale = picked;
            for mpq in paths {
                archives.push(
                    Archive::open(&mpq).with_context(|| format!("opening {}", mpq.display()))?,
                );
            }
        } else {
            archives.push(
                Archive::open(path).with_context(|| format!("opening MPQ {}", path.display()))?,
            );
        }
        Ok(Self { archives, locale })
    }

    /// The highest-priority archive with an entry for `name`, a delete marker included, as a
    /// tombstone shadows every lower copy: check [`Archive::is_delete_marker`] for a readable file.
    fn resolve(&self, name: &str) -> Option<&Archive> {
        self.archives.iter().rev().find(|a| a.contains(name))
    }

    /// Whether `name` (`/` or `\`, any case) is a readable file, not a delete marker.
    pub fn contains(&self, name: &str) -> bool {
        self.resolve(name)
            .is_some_and(|a| !a.is_delete_marker(name))
    }

    /// The path of the archive `name` resolves to, for debugging and extraction.
    pub fn find_file_archive(&self, name: &str) -> Option<&Path> {
        self.resolve(name).map(|a| a.path())
    }

    /// Read a file by internal path (`/` or `\`) from its winning archive.
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        let archive = self.archive_for(name)?;
        archive
            .read_file(name)
            .with_context(|| format!("reading {name} from {}", archive.path().display()))
    }

    /// The archive `name` reads from, as a cheap handle: a caller holding the chain behind a lock
    /// looks the file up under it and reads (`Archive::open_file`) after releasing it. No I/O.
    pub fn archive_for(&self, name: &str) -> Result<Archive> {
        let archive = self
            .resolve(name)
            .ok_or_else(|| anyhow!("file not in patch chain: {name}"))?;
        // A tombstone deletes the path from the composite: not found, never a stale lower copy.
        if archive.is_delete_marker(name) {
            bail!(
                "file deleted from patch chain: {name} (tombstoned by {})",
                archive.path().display()
            );
        }
        Ok(archive.clone())
    }

    /// `&mut` alias of [`Chain::read`] for call sites that thread a `&mut Chain`.
    pub fn read_file(&mut self, name: &str) -> Result<Vec<u8>> {
        self.read(name)
    }

    /// The chain's named files with sizes, for development and extraction; a file in no listfile
    /// (most of `texture.MPQ`) is readable by name but not listed. Unions every archive's
    /// `(listfile)`, as each names only its own files; sizes come from the winning archive.
    pub fn list(&self) -> Result<Vec<ChainEntry>> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for archive in &self.archives {
            let Ok(listfile) = archive.read_file("(listfile)") else {
                continue;
            };
            for raw in String::from_utf8_lossy(&listfile).split([';', '\r', '\n']) {
                let name = raw.trim();
                // Dedupe the way MPQ hashing compares names: any case, `/` and `\` alike.
                if name.is_empty() || !seen.insert(name.replace('/', "\\").to_ascii_lowercase()) {
                    continue;
                }
                if let Some(a) = self.resolve(name) {
                    // A tombstoned path is not a file in the composite.
                    if a.is_delete_marker(name) {
                        continue;
                    }
                    out.push(ChainEntry {
                        name: name.to_string(),
                        size: a.file_size(name).unwrap_or(0) as u64,
                    });
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn patch_glob_matches_exactly_one_character_case_insensitively() {
        assert!(is_patch_glob_match("patch-2.MPQ"));
        assert!(is_patch_glob_match("patch-3.MPQ"));
        assert!(is_patch_glob_match("PATCH-A.mpq"));
        assert!(!is_patch_glob_match("patch-.MPQ"));
        assert!(!is_patch_glob_match("patch-10.MPQ"));
        assert!(!is_patch_glob_match("patch-33.MPQ"));
        assert!(!is_patch_glob_match("patch.MPQ"));
        assert!(!is_patch_glob_match("patch-2.MPQ.bak"));
        assert!(!is_patch_glob_match("mypatch-2.MPQ"));
    }

    #[test]
    fn mount_order_is_the_carved_law() {
        // base.MPQ is telemetry-only in the reference and never mounts.
        let dir = owned(&[
            "patch-2.MPQ",
            "backup.MPQ",
            "model.MPQ",
            "base.MPQ",
            "dbc.MPQ",
            "patch.MPQ",
            "eula.html",
            "patch-3.MPQ",
            "speech2.MPQ",
            "texture.MPQ",
        ]);
        assert_eq!(
            mount_order(&dir),
            owned(&[
                "dbc.MPQ",
                "texture.MPQ",
                "model.MPQ",
                "patch.MPQ",
                "patch-2.MPQ",
                "patch-3.MPQ",
                "speech2.MPQ",
            ])
        );
    }

    #[test]
    fn patch_sort_is_ascending_and_case_folded() {
        let dir = owned(&["patch-B.MPQ", "patch-3.MPQ", "patch-a.MPQ", "patch-2.MPQ"]);
        assert_eq!(
            mount_order(&dir),
            owned(&["patch-2.MPQ", "patch-3.MPQ", "patch-a.MPQ", "patch-B.MPQ"])
        );
    }

    #[test]
    fn base_archives_are_found_case_insensitively() {
        let dir = owned(&["DBC.mpq", "Model.MPQ", "PATCH.mpq"]);
        assert_eq!(
            mount_order(&dir),
            owned(&["DBC.mpq", "Model.MPQ", "PATCH.mpq"])
        );
    }

    /// A `Data` folder of empty files named `files` (`folder/name` for a locale folder), removed on
    /// drop; the layout choice reads names only, so no archive is opened.
    struct FakeData(PathBuf);

    impl FakeData {
        fn new(tag: &str, files: &[&str]) -> Self {
            let root = std::env::temp_dir()
                .join(format!("benilla-chain-layout-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            for f in files {
                let path = root.join(f);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, b"").unwrap();
            }
            Self(root)
        }

        /// The layout's archives as `<folder/>name` relative to the folder, plus the locale.
        fn layout(&self) -> Result<(Vec<String>, Option<String>)> {
            let (paths, locale) = data_layout(&self.0)?;
            let rel = paths
                .iter()
                .map(|p| {
                    p.strip_prefix(&self.0)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            Ok((rel, locale))
        }
    }

    impl Drop for FakeData {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const TBC_FILES: &[&str] = &[
        "common.MPQ",
        "expansion.MPQ",
        "patch.MPQ",
        "patch-2.MPQ",
        "enGB/locale-enGB.MPQ",
        "enGB/speech-enGB.MPQ",
        "enGB/expansion-locale-enGB.MPQ",
        "enGB/expansion-speech-enGB.MPQ",
        "enGB/patch-enGB.MPQ",
        "enGB/patch-enGB-2.MPQ",
        "enGB/base-enGB.MPQ",
        "enGB/backup-enGB.MPQ",
    ];

    #[test]
    fn a_tbc_data_dir_mounts_in_the_tbc_order_and_leaves_base_and_backup_out() {
        let d = FakeData::new("tbc", TBC_FILES);
        let (order, locale) = d.layout().unwrap();
        assert_eq!(locale.as_deref(), Some("enGB"));
        assert_eq!(
            order,
            [
                "common.MPQ",
                "expansion.MPQ",
                "enGB/locale-enGB.MPQ",
                "enGB/speech-enGB.MPQ",
                "enGB/expansion-locale-enGB.MPQ",
                "enGB/expansion-speech-enGB.MPQ",
                "patch.MPQ",
                "patch-2.MPQ",
                "enGB/patch-enGB.MPQ",
                "enGB/patch-enGB-2.MPQ",
            ]
        );
    }

    #[test]
    fn the_locale_is_enus_else_engb_else_the_first_by_name() {
        let with = |tag: &str, folders: &[&str]| {
            let mut files = vec!["common.MPQ".to_string()];
            files.extend(folders.iter().map(|l| format!("{l}/locale-{l}.MPQ")));
            let refs: Vec<&str> = files.iter().map(String::as_str).collect();
            FakeData::new(tag, &refs).layout().unwrap().1
        };
        assert_eq!(
            with("l1", &["ruRU", "enGB", "enUS"]).as_deref(),
            Some("enUS")
        );
        assert_eq!(with("l2", &["ruRU", "enGB"]).as_deref(), Some("enGB"));
        assert_eq!(with("l3", &["ruRU", "deDE"]).as_deref(), Some("deDE"));
        // A folder without its own `locale-<folder>.MPQ` is no locale.
        let d = FakeData::new(
            "l4",
            &["common.MPQ", "enUS/patch-enUS.MPQ", "frFR/locale-frFR.MPQ"],
        );
        assert_eq!(d.layout().unwrap().1.as_deref(), Some("frFR"));
    }

    #[test]
    fn a_tbc_data_dir_without_a_locale_folder_is_an_error() {
        let d = FakeData::new("noloc", &["common.MPQ", "expansion.MPQ"]);
        let err = d.layout().unwrap_err().to_string();
        assert!(err.contains("locale"), "{err}");
    }

    #[test]
    fn a_vanilla_data_dir_keeps_the_vanilla_order_and_no_locale() {
        let d = FakeData::new("van", &["dbc.MPQ", "model.MPQ", "patch.MPQ", "patch-2.MPQ"]);
        let (order, locale) = d.layout().unwrap();
        assert_eq!(order, ["dbc.MPQ", "model.MPQ", "patch.MPQ", "patch-2.MPQ"]);
        assert_eq!(locale, None);
    }

    #[test]
    fn a_data_dir_of_neither_shape_is_an_error_naming_both() {
        let d = FakeData::new("none", &["readme.txt"]);
        let err = d.layout().unwrap_err().to_string();
        assert!(err.contains("1.12.1") && err.contains("2.4.3"), "{err}");
    }

    #[test]
    fn locale_patch_glob_takes_one_character_after_the_locale() {
        assert!(is_glob_one("patch-enGB-2.MPQ", "patch-enGB-"));
        assert!(is_glob_one("PATCH-ENGB-A.mpq", "patch-enGB-"));
        assert!(!is_glob_one("patch-enGB.MPQ", "patch-enGB-"));
        assert!(!is_glob_one("patch-enGB-10.MPQ", "patch-enGB-"));
        assert!(!is_glob_one("patch-ruRU-2.MPQ", "patch-enGB-"));
    }
}
