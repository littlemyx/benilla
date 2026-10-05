//! The stock interface of an install, loaded through the production path
//! ([`production_load_observed`], which [`super::layer_tests::production_load_with`] is too) over
//! that install's own chain, and reported per `FrameXML.toc` row: which rows loaded clean, and
//! each diagnostic of the others, classified. The ratchet tests below hold the totals of both
//! installs: a change that makes a class worse fails, and one that makes it better fails until
//! the number in the test is lowered, so the recorded gap only shrinks and is always current.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use benilla_build::ClientBuild;
use benilla_formats::Chain;
use benilla_ui::script::UiScript;

use super::addons::FileOutcome;
use super::reference_ui;
use crate::local_state::test_env::{EnvGuard, ENV_LOCK};

/// The production in-game load, the layer on unless `stock_ui`, over a hermetic AddOns root that
/// `addons` fills; `before` runs ahead of the load, and `observe` hears each core row finish. The
/// VM speaks the Lua of the chain's build ([`reference_ui::new_script`]). The caller holds
/// [`ENV_LOCK`]. Returns the VM and the load's failures.
pub(super) fn production_load_observed(
    tag: &str,
    stock_ui: bool,
    before: &str,
    addons: impl FnOnce(&Path),
    observe: &mut dyn FnMut(&str, FileOutcome),
) -> (UiScript, Vec<String>) {
    let tmp = std::env::temp_dir().join(format!("benilla-layer-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let home = tmp.join("benilla-config");
    let root = home.join("AddOns");
    std::fs::create_dir_all(&root).unwrap();
    addons(&root);
    let _c = EnvGuard::unset("WOW_CAPTURE");
    let _h = EnvGuard::set("BENILLA_HOME", home.to_str().unwrap());

    let mut s = reference_ui::new_script().unwrap();
    s.set_screen_size(1024.0, 768.0);
    s.set_unit(
        "player",
        Some(benilla_ui::script::UnitState {
            exists: true,
            name: Some("Probesix".into()),
            level: 60,
            class: Some("Warrior".into()),
            class_file: Some("WARRIOR".into()),
            ..Default::default()
        }),
    );
    // A reply that hid nothing, so `GetNumAddOns` counts the registry.
    s.note_addon_info_reply(&[]);
    s.register_cvars(crate::cvars::registered_pairs());
    s.run(before).unwrap();
    // The layer passed in, not set through `WOW_STOCK_UI`: every test in this process reads it.
    let mut failures = Vec::new();
    failures.extend(super::manifest::load_ingame_ui_observed(
        &mut s,
        None,
        &[],
        true,
        !stock_ui,
        observe,
    ));
    failures.extend(s.errors());
    let _ = std::fs::remove_dir_all(&tmp);
    (s, failures)
}

/// What a diagnostic is, by what the stock files need that the engine lacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Class {
    /// A chunk the VM's grammar rejects.
    LuaCompile,
    /// `attempt to call global 'X'`: a game API function (or a stock function that never defined).
    MissingGlobalFunction,
    /// `attempt to index/… global 'X'`: a global that is not a table, `self` in a handler.
    NilGlobal,
    /// `attempt to call method 'M'`: a widget method no widget provides.
    MissingMethod,
    /// `attempt to call field 'F'`: a library function the VM lacks.
    MissingField,
    /// Any other error a chunk or handler raised at load.
    LuaRuntime,
    /// `Unknown frame type: X`.
    UnknownFrameType,
    /// An XML element the loader never reads.
    UnknownElement,
    /// An XML attribute the loader never reads.
    UnknownAttribute,
    /// A `<Scripts>` handler `SetScript` refuses.
    UnsupportedHandler,
    /// An `inherits` name no template or font registers.
    UnresolvedTemplate,
    /// A CVar a stock file reads that the host table lacks.
    UnknownCVar,
    /// Anything else the load said.
    Other,
}

impl Class {
    pub(super) const ALL: [Class; 13] = [
        Class::LuaCompile,
        Class::MissingGlobalFunction,
        Class::NilGlobal,
        Class::MissingMethod,
        Class::MissingField,
        Class::LuaRuntime,
        Class::UnknownFrameType,
        Class::UnknownElement,
        Class::UnknownAttribute,
        Class::UnsupportedHandler,
        Class::UnresolvedTemplate,
        Class::UnknownCVar,
        Class::Other,
    ];

    fn label(self) -> &'static str {
        match self {
            Class::LuaCompile => "lua-compile",
            Class::MissingGlobalFunction => "missing-global-function",
            Class::NilGlobal => "nil-global",
            Class::MissingMethod => "missing-method",
            Class::MissingField => "missing-field",
            Class::LuaRuntime => "lua-runtime",
            Class::UnknownFrameType => "unknown-frame-type",
            Class::UnknownElement => "unknown-element",
            Class::UnknownAttribute => "unknown-attribute",
            Class::UnsupportedHandler => "unsupported-handler",
            Class::UnresolvedTemplate => "unresolved-template",
            Class::UnknownCVar => "unknown-cvar",
            Class::Other => "other",
        }
    }
}

/// One thing a row said, classified: `name` is the missing thing, `text` the whole message.
#[derive(Clone, Debug)]
pub(super) struct Diag {
    pub(super) class: Class,
    pub(super) name: String,
    pub(super) text: String,
}

/// One `FrameXML.toc` row: clean when it said nothing.
#[derive(Clone, Debug)]
pub(super) struct Row {
    pub(super) file: String,
    pub(super) diags: Vec<Diag>,
}

/// The stock interface's load over one install.
pub(super) struct Report {
    pub(super) build: Option<ClientBuild>,
    pub(super) rows: Vec<Row>,
    /// Failures no row owns: the load after the walk (the bindings file).
    pub(super) unattributed: Vec<String>,
}

impl Report {
    pub(super) fn clean(&self) -> usize {
        self.rows.iter().filter(|r| r.diags.is_empty()).count()
    }

    pub(super) fn occurrences(&self, class: Class) -> usize {
        self.diags(class).count()
    }

    pub(super) fn names(&self, class: Class) -> BTreeSet<&str> {
        self.diags(class).map(|d| d.name.as_str()).collect()
    }

    fn diags(&self, class: Class) -> impl Iterator<Item = &Diag> {
        self.rows
            .iter()
            .flat_map(|r| &r.diags)
            .filter(move |d| d.class == class)
    }

    /// The names of `class` with the number of rows each appears in, most first.
    pub(super) fn rows_per_name(&self, class: Class) -> Vec<(String, usize)> {
        let mut by: BTreeMap<&str, usize> = BTreeMap::new();
        for row in &self.rows {
            let names: BTreeSet<&str> = row
                .diags
                .iter()
                .filter(|d| d.class == class)
                .map(|d| d.name.as_str())
                .collect();
            for n in names {
                *by.entry(n).or_default() += 1;
            }
        }
        let mut out: Vec<(String, usize)> =
            by.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    /// The totals the ratchet holds, one line per class: `label occurrences distinct-names`.
    pub(super) fn table(&self) -> String {
        let mut out = format!("rows {} clean {}\n", self.rows.len(), self.clean());
        for class in Class::ALL {
            out.push_str(&format!(
                "{} {} {}\n",
                class.label(),
                self.occurrences(class),
                self.names(class).len()
            ));
        }
        out.push_str(&format!("unattributed {}\n", self.unattributed.len()));
        out
    }

    /// Every diagnostic, row by row, for a failure message and the scratch baseline.
    pub(super) fn detail(&self) -> String {
        let mut out = String::new();
        for row in self.rows.iter().filter(|r| !r.diags.is_empty()) {
            out.push_str(&format!("{}\n", row.file));
            for d in &row.diags {
                let text: String = d
                    .text
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect();
                out.push_str(&format!("  {} [{}] {}\n", d.class.label(), d.name, text));
            }
        }
        for u in &self.unattributed {
            out.push_str(&format!("unattributed: {u}\n"));
        }
        out
    }
}

/// The ratchet: `rows`, `clean`, and `(occurrences, distinct names)` per [`Class::ALL`] entry, in
/// that order, `unattributed` last.
pub(super) struct Baseline {
    pub(super) rows: usize,
    pub(super) clean: usize,
    pub(super) classes: [(usize, usize); 13],
    pub(super) unattributed: usize,
}

impl Baseline {
    fn table(&self) -> String {
        let mut out = format!("rows {} clean {}\n", self.rows, self.clean);
        for (class, (n, names)) in Class::ALL.iter().zip(self.classes) {
            out.push_str(&format!("{} {n} {names}\n", class.label()));
        }
        out.push_str(&format!("unattributed {}\n", self.unattributed));
        out
    }
}

/// Fails unless `report` is exactly `want`: worse is a regression, better is a number to lower.
pub(super) fn assert_baseline(what: &str, report: &Report, want: &Baseline) {
    let (got, want) = (report.table(), want.table());
    assert!(
        got == want,
        "{what}: the stock load's totals moved. A class that grew is a regression; one that \
         shrank is progress — lower the number in the test.\n--- recorded\n{want}--- measured\n{got}\
         --- rows\n{}",
        report.detail()
    );
}

/// The chain of the install at `data`, opened once per process: a leaked `Chain`, as the
/// reference UI's own shared chain is process-long.
fn chain_of(data: &Path) -> &'static Chain {
    static OPEN: Mutex<Vec<(PathBuf, &'static Chain)>> = Mutex::new(Vec::new());
    let mut open = OPEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, chain)) = open.iter().find(|(p, _)| p == data) {
        return chain;
    }
    let chain: &'static Chain = Box::leak(Box::new(
        Chain::open(data).unwrap_or_else(|e| panic!("opening {}: {e:#}", data.display())),
    ));
    open.push((data.to_path_buf(), chain));
    chain
}

/// Loads the stock interface of the install at `data` (a `Data` folder) through the production
/// path, the layer off, and reports each `FrameXML.toc` row. The caller holds [`ENV_LOCK`].
pub(super) fn load_stock(data: &Path, tag: &str) -> Report {
    let chain = chain_of(data);
    let _laid = reference_ui::fixture::use_chain(chain);
    let mut rows = Vec::new();
    let (_script, failures) = production_load_observed(tag, true, "", |_| {}, &mut |file, out| {
        rows.push(Row {
            diags: diagnose(file, &out),
            file: file.to_string(),
        });
    });
    // What no row owns: a failure the rows did not say (the bindings file, the VM's own errors).
    let mut owned: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows {
        for d in &row.diags {
            *owned.entry(d.text.as_str()).or_default() += 1;
        }
    }
    let unattributed = failures
        .into_iter()
        .filter(|f| match owned.get_mut(f.as_str()) {
            Some(n) if *n > 0 => {
                *n -= 1;
                false
            }
            _ => true,
        })
        .collect();
    let report = Report {
        build: chain.build(),
        rows,
        unattributed,
    };
    // `BENILLA_STOCK_LOAD_DIR` names a folder that receives the row-by-row detail of each load.
    if let Some(dir) = std::env::var_os("BENILLA_STOCK_LOAD_DIR") {
        let dir = PathBuf::from(dir);
        let _ = std::fs::write(dir.join(format!("{tag}.detail.txt")), report.detail());
        let _ = std::fs::write(dir.join(format!("{tag}.table.txt")), report.table());
        let blocked: String = report
            .rows_per_name(Class::MissingGlobalFunction)
            .iter()
            .map(|(n, rows)| format!("{n}\t{rows}\n"))
            .collect();
        let _ = std::fs::write(dir.join(format!("{tag}.missing.txt")), blocked);
    }
    report
}

/// Everything one row said, classified.
fn diagnose(file: &str, out: &FileOutcome) -> Vec<Diag> {
    let mut diags = Vec::new();
    for f in &out.failures {
        // `"<Addon>/<file>: <error>"`.
        let rest = f
            .strip_prefix("FrameXML/")
            .and_then(|r| r.strip_prefix(file))
            .and_then(|r| r.strip_prefix(": "))
            .unwrap_or(f);
        diags.push(classify(f, rest));
    }
    for e in &out.vm_errors {
        diags.push(classify(e, e));
    }
    for w in out.warnings.iter().chain(&out.host_warnings) {
        diags.push(classify(w, w));
    }
    for e in &out.unknown_elements {
        diags.push(Diag {
            class: Class::UnknownElement,
            name: e.clone(),
            text: e.clone(),
        });
    }
    for a in &out.unknown_attributes {
        diags.push(Diag {
            class: Class::UnknownAttribute,
            name: a.clone(),
            text: a.clone(),
        });
    }
    diags
}

/// The quoted name after `marker` in `text`: Lua 5.1 writes `'x'`, 5.0 `` `x' ``.
fn quoted_after(text: &str, marker: &str) -> Option<String> {
    let rest = text.split_once(marker)?.1;
    let rest = rest.strip_prefix(['\'', '`'])?;
    Some(rest[..rest.find('\'')?].to_string())
}

/// `text` with the chunk position (`name:LINE: `) cut off the front of a Lua message, and the
/// traceback off its back.
fn lua_message(text: &str) -> String {
    let first = text.lines().next().unwrap_or(text);
    let bytes = first.as_bytes();
    let mut cut = 0;
    for i in 0..bytes.len() {
        if bytes[i] == b':' {
            let digits = bytes[i + 1..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            if digits > 0 && bytes.get(i + 1 + digits..i + 3 + digits) == Some(b": ") {
                cut = i + 3 + digits;
            }
        }
    }
    first[cut..].chars().take(160).collect()
}

/// Classifies one message.
fn classify(whole: &str, text: &str) -> Diag {
    let diag = |class, name: String| Diag {
        class,
        name,
        text: whole.to_string(),
    };
    if let Some(name) = text.strip_prefix("Unknown frame type: ") {
        return diag(Class::UnknownFrameType, name.trim().to_string());
    }
    if let Some(name) = quoted_after(text, "unsupported script ") {
        return diag(Class::UnsupportedHandler, name);
    }
    if let Some(name) = quoted_after(text, "unknown CVar ") {
        return diag(Class::UnknownCVar, name);
    }
    if let Some(name) = quoted_after(text, "unknown template ") {
        return diag(Class::UnresolvedTemplate, name);
    }
    if text.contains("no template of that name is registered") {
        let name = text
            .split_once("inherits=\"")
            .and_then(|(_, r)| r.split_once('"'))
            .map_or("?", |(n, _)| n);
        return diag(Class::UnresolvedTemplate, name.to_string());
    }
    if text.contains("syntax error") {
        return diag(Class::LuaCompile, lua_message(text));
    }
    for (marker, class) in [
        ("attempt to call global ", Class::MissingGlobalFunction),
        ("attempt to call method ", Class::MissingMethod),
        ("attempt to call field ", Class::MissingField),
        ("attempt to index global ", Class::NilGlobal),
        ("attempt to perform arithmetic on global ", Class::NilGlobal),
        ("attempt to concatenate global ", Class::NilGlobal),
    ] {
        if let Some(name) = quoted_after(text, marker) {
            return diag(class, name);
        }
    }
    if text.contains("runtime error")
        || text.contains("attempt to")
        || text.contains("bad argument")
    {
        return diag(Class::LuaRuntime, lua_message(text));
    }
    diag(Class::Other, lua_message(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_reads_both_dialects_quoting_and_names_the_missing_thing() {
        let c = |t: &str| classify(t, t);
        let d = c("runtime error: [string \"x\"]:3: attempt to call global `Foo' (a nil value)");
        assert_eq!(
            (d.class, d.name.as_str()),
            (Class::MissingGlobalFunction, "Foo")
        );
        let d = c("<T>: OnLoad: runtime error: @a.lua:9: attempt to call method 'SetAttribute' (a nil value)");
        assert_eq!(
            (d.class, d.name.as_str()),
            (Class::MissingMethod, "SetAttribute")
        );
        let d = c("attempt to index global 'self' (a nil value)");
        assert_eq!((d.class, d.name.as_str()), (Class::NilGlobal, "self"));
        let d = c("x: compiling <OnLoad>: syntax error: [string \"A:OnLoad\"]:2: unexpected symbol near '#'");
        assert_eq!(
            (d.class, d.name.as_str()),
            (Class::LuaCompile, "unexpected symbol near '#'")
        );
        let d = c("Unknown frame type: Cooldown");
        assert_eq!(
            (d.class, d.name.as_str()),
            (Class::UnknownFrameType, "Cooldown")
        );
        let d = c("F: SetScript: unsupported script 'PreClick'");
        assert_eq!(
            (d.class, d.name.as_str()),
            (Class::UnsupportedHandler, "PreClick")
        );
        let d = c("unknown template 'XTemplate' referenced by inherits; skipping");
        assert_eq!(
            (d.class, d.name.as_str()),
            (Class::UnresolvedTemplate, "XTemplate")
        );
        let d = c("something else entirely");
        assert_eq!(d.class, Class::Other);
    }

    /// The control: the install's own 1.12.1 interface, through the path production takes.
    #[test]
    fn the_stock_1_12_1_interface_loads_as_recorded() {
        let data = benilla_formats::wow_data_or_skip!();
        let _l = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let report = load_stock(&data, "stock-112");
        assert_eq!(report.build, Some(benilla_build::VANILLA_1_12_1));
        assert_baseline("1.12.1", &report, &BASELINE_1_12_1);
    }

    /// The install's own 2.4.3 interface, the gap the structural work closes.
    #[test]
    fn the_stock_2_4_3_interface_loads_as_recorded() {
        let data = benilla_formats::wow_data_tbc_or_skip!();
        let _l = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let report = load_stock(&data, "stock-243");
        assert_eq!(report.build, Some(benilla_build::TBC_2_4_3));
        assert_baseline("2.4.3", &report, &BASELINE_2_4_3);
    }

    /// The control. Not clean: 1.12.1's own files carry an element and attributes the loader has
    /// never read (`<PushedTextOffset>`, `<FontString>` with `<FontHeight>` and no `font`,
    /// `bytes`, `maxLines`, `nonspacewrap`, `<Font monochrome>`), and `OnInputLanguageChanged`,
    /// a handler `SetScript` has no kind for.
    const BASELINE_1_12_1: Baseline = Baseline {
        rows: 90,
        clean: 79,
        classes: [
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (4, 2),
            (7, 5),
            (1, 1),
            (0, 0),
            (0, 0),
            (0, 0),
        ],
        unattributed: 0,
    };

    const BASELINE_2_4_3: Baseline = Baseline {
        rows: 113,
        clean: 67,
        classes: [
            (0, 0),
            (85, 22),
            (0, 0),
            (84, 3),
            (0, 0),
            (1, 1),
            (0, 0),
            (8, 3),
            (16, 8),
            (17, 7),
            (44, 1),
            (72, 72),
            (3, 3),
        ],
        unattributed: 1,
    };
}
