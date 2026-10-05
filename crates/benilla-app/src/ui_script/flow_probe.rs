//! The interface half of the flow census ([`crate::flow_census`]): the Lua probe installed ahead
//! of the stock files, and the sampling of a live VM's errors, registrations and dispatches.

use std::collections::BTreeMap;

use benilla_ui::script::UiScript;
use bevy::prelude::*;

use crate::flow_census::{due, record_vm, stage, VmSnap, NEXT_SAMPLE_MS};

/// The Lua installed ahead of the stock files: counts every `RegisterEvent` by event name through
/// each widget class's method table, and a probe frame on `RegisterAllEvents` counts every event
/// the engine dispatches. Written for Lua 5.0 and 5.1 alike.
const LUA_PROBE: &str = r#"
BenillaCensus = { reg = {}, fired = {} }
local C = BenillaCensus
local seen = {}
local kinds = { "Frame", "Button", "CheckButton", "EditBox", "ScrollFrame", "Slider", "StatusBar",
  "GameTooltip", "MessageFrame", "SimpleHTML", "ColorSelect", "Cooldown", "Model", "Minimap",
  "ScrollingMessageFrame", "MovieFrame", "PlayerModel", "DressUpModel", "TabardModel" }
local function wrap(methods)
  local orig = methods.RegisterEvent
  if orig then
    methods.RegisterEvent = function(self, ev)
      C.reg[ev] = (C.reg[ev] or 0) + 1
      return orig(self, ev)
    end
  end
end
for _, kind in ipairs(kinds) do
  local ok, f = pcall(CreateFrame, kind)
  if ok and f then
    local mt = getmetatable(f)
    local idx = mt and mt.__index
    if type(idx) == "table" and not seen[idx] then
      seen[idx] = true
      wrap(idx)
    end
  end
end
local probe = CreateFrame("Frame")
probe:RegisterAllEvents()
probe:SetScript("OnEvent", function()
  C.fired[event] = (C.fired[event] or 0) + 1
end)
function BenillaCensusDump()
  local out = {}
  for k, v in pairs(C.reg) do table.insert(out, "R\t" .. k .. "\t" .. v) end
  for k, v in pairs(C.fired) do table.insert(out, "F\t" .. k .. "\t" .. v) end
  return table.concat(out, "\n")
end
"#;

/// Installs the probe in a VM about to load the in-game interface.
pub(crate) fn install_lua_probe(script: &UiScript) {
    if !crate::run_mode::dev_affordances() {
        return;
    }
    match script.run(LUA_PROBE) {
        Ok(()) => println!(
            "census: lua probe installed (VM session {})",
            script.session()
        ),
        Err(e) => println!("census: lua probe FAILED to install: {e}"),
    }
}

fn parse_dump(text: &str) -> (BTreeMap<String, u64>, BTreeMap<String, u64>) {
    let (mut reg, mut fired) = (BTreeMap::new(), BTreeMap::new());
    for line in text.lines() {
        let mut p = line.split('\t');
        let (Some(kind), Some(name), Some(n)) = (p.next(), p.next(), p.next()) else {
            continue;
        };
        let n = n.parse::<u64>().unwrap_or(0);
        match kind {
            "R" => reg.insert(name.to_string(), n),
            "F" => fired.insert(name.to_string(), n),
            _ => None,
        };
    }
    (reg, fired)
}

/// Copies the live VM's reports into the census, replacing its earlier sample.
fn sample_vm(script: &UiScript) {
    let mut snap = VmSnap {
        diagnostics: script
            .diagnostics()
            .into_iter()
            .map(|d| (d.kind.tag(), d.message, d.count))
            .collect(),
        ..Default::default()
    };
    if let Ok(dump) =
        script.eval::<String>("return BenillaCensusDump and BenillaCensusDump() or ''")
    {
        (snap.registered, snap.fired) = parse_dump(&dump);
    }
    record_vm(script.session(), snap);
}

pub(crate) fn sample_ui(script: Option<NonSend<UiScript>>) {
    if !due(&NEXT_SAMPLE_MS, 1000) {
        return;
    }
    if let Some(script) = script {
        sample_vm(&script);
        // The in-game interface's own stage: the probe's dump exists only in a VM that loaded it.
        if script
            .eval::<bool>("return BenillaCensus ~= nil and PlayerFrame ~= nil")
            .unwrap_or(false)
        {
            stage("ingame_ui", &format!("VM session {}", script.session()));
        }
    }
}

pub(crate) fn summary_on_exit(
    mut exits: MessageReader<AppExit>,
    script: Option<NonSend<UiScript>>,
) {
    if exits.read().next().is_none() {
        return;
    }
    if let Some(script) = script {
        sample_vm(&script);
    }
    stage("exit", "");
    crate::flow_census::print_summary();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_counts_registrations_and_dispatches() {
        let script = UiScript::new().unwrap();
        script.run(LUA_PROBE).unwrap();
        script
            .run(r#"local f = CreateFrame("Frame"); f:RegisterEvent("BAG_UPDATE"); f:RegisterEvent("SPELLS_CHANGED")"#)
            .unwrap();
        let mut script = script;
        script.fire_event("BAG_UPDATE", vec![]);
        script.fire_event("BAG_UPDATE", vec![]);
        let dump: String = script.eval("return BenillaCensusDump()").unwrap();
        let (reg, fired) = parse_dump(&dump);
        assert_eq!(reg.get("BAG_UPDATE"), Some(&1), "{dump}");
        assert_eq!(reg.get("SPELLS_CHANGED"), Some(&1), "{dump}");
        assert_eq!(fired.get("BAG_UPDATE"), Some(&2), "{dump}");
        assert_eq!(fired.get("SPELLS_CHANGED"), None, "{dump}");
    }
}
