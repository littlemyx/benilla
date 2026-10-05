//! The two script dialects of one VM binary: 1.12.1's Lua 5.0 (the default) and 2.4.3's Lua 5.1.
//! Every snippet here is written for the test; none is interface text from an install.

use crate::loader::load;
use crate::script::{ScriptDialect, ScriptValue, UiScript};

fn s51() -> UiScript {
    UiScript::with_dialect(ScriptDialect::Lua51).expect("construct a 5.1 UiScript")
}

fn s50() -> UiScript {
    UiScript::with_dialect(ScriptDialect::Lua50).expect("construct a 5.0 UiScript")
}

/// `loadstring`'s answer for a source text: the error message, or `None` when it compiles.
fn compile_error(s: &UiScript, src: &str) -> Option<String> {
    s.eval::<Option<String>>(&format!(
        "local f, e = loadstring({src:?}) if f then return nil end return e"
    ))
    .unwrap()
}

#[test]
fn a_new_state_and_the_default_constructor_speak_lua_50() {
    assert_eq!(ScriptDialect::default(), ScriptDialect::Lua50);
    assert_eq!(UiScript::new().unwrap().dialect(), ScriptDialect::Lua50);
    assert_eq!(s51().dialect(), ScriptDialect::Lua51);
    // 1.12.1's grammar: `...` as a value, `#` and `%` are not expressions.
    let s = UiScript::new().unwrap();
    for (src, want) in [
        ("return function(...) return ... end", "unexpected symbol"),
        ("return #\"x\"", "unexpected symbol"),
        ("return 5 % 2", "near `%'"),
    ] {
        assert!(compile_error(&s, src).unwrap().contains(want), "{src}");
    }
}

#[test]
fn lua_51_compiles_and_runs_the_51_grammar() {
    let s = s51();
    let (n, a, b, len, slen): (i64, i64, i64, i64, i64) = s
        .eval(
            r#"
            local function f(...) return select('#', ...), ... end
            local n, a, b = f(10, 20)
            return n, a, b, #{ 1, 2, 3 }, #"abcd"
        "#,
        )
        .unwrap();
    assert_eq!((n, a, b, len, slen), (2, 10, 20, 3, 4));
    let m: (f64, f64, f64, f64) = s.eval("return 7 % 3, -7 % 3, 7 % -3, 5.5 % 2").unwrap();
    assert_eq!(
        m,
        (1.0, 2.0, -2.0, 1.5),
        "5.1's modulo takes the divisor's sign"
    );
    // A vararg expression outside a vararg function is 5.1's own error.
    let e = compile_error(&s, "local function g() return ... end").unwrap();
    assert!(
        e.contains("cannot use '...' outside a vararg function"),
        "{e}"
    );
    // `...` clears the implicit `arg` table, as in 5.1; a vararg function that never names it keeps it.
    assert!(s
        .eval::<bool>("return (function(...) local x = ... return arg end)(1) == nil")
        .unwrap());
    assert_eq!(
        s.eval::<i64>("return (function(...) return arg.n end)(7, 8, 9)")
            .unwrap(),
        3
    );
}

#[test]
fn lua_51_does_not_nest_long_strings_and_does_not_skip_a_second_constructor_semicolon() {
    let s = s51();
    let e = compile_error(&s, "return [[a[[b]]c]]").unwrap();
    assert!(e.contains("nesting of [[...]] is deprecated"), "{e}");
    assert_eq!(
        s.eval::<String>("return [==[ [[x]] ]==]").unwrap(),
        " [[x]] "
    );
    let e = compile_error(&s, "return { a = 1;; }").unwrap();
    assert!(e.contains("unexpected symbol"), "{e}");
    assert_eq!(compile_error(&s, "return { a = 1; b = 2 }"), None);

    // The 5.0 dialect nests, and skips the one extra `;`.
    let s = s50();
    assert_eq!(s.eval::<String>("return [[a[[b]]c]]").unwrap(), "a[[b]]c");
    assert_eq!(compile_error(&s, "return { a = 1;; }"), None);
}

#[test]
fn a_table_is_not_a_generator_in_lua_51_but_is_in_lua_50() {
    let s = s51();
    let e: String = s
        .eval("local ok, e = pcall(function() for k, v in { a = 1 } do end end) return tostring(e)")
        .unwrap();
    assert!(e.contains("attempt to call a table value"), "{e}");
    assert_eq!(
        s.eval::<i64>("local n = 0 for k, v in pairs({ a = 1, b = 2 }) do n = n + v end return n")
            .unwrap(),
        3
    );
    assert_eq!(
        s50()
            .eval::<i64>("local n = 0 for k, v in { a = 1, b = 2 } do n = n + v end return n")
            .unwrap(),
        3
    );
}

#[test]
fn error_messages_quote_with_apostrophes_in_lua_51_and_backquotes_in_lua_50() {
    let probes = [
        (
            "local ok, e = pcall(function() local t = nil return t.x end) return tostring(e)",
            "attempt to index local 't' (a nil value)",
            "attempt to index local `t' (a nil value)",
        ),
        (
            "local ok, e = pcall(function() return string.rep(nil, 2) end) return tostring(e)",
            "bad argument #1 to 'rep'",
            "bad argument #1 to `rep'",
        ),
        (
            "local f, e = loadstring(\"return 1 +\") return tostring(e)",
            "near '<eof>'",
            "near `<eof>'",
        ),
        (
            "local f, e = loadstring(\"function f(1) end\") return tostring(e)",
            "<name> or '...' expected",
            "<name> or `...' expected",
        ),
    ];
    let (a, b) = (s51(), s50());
    for (src, want51, want50) in probes {
        let e51: String = a.eval(src).unwrap();
        let e50: String = b.eval(src).unwrap();
        assert!(e51.contains(want51), "5.1: {e51}");
        assert!(e50.contains(want50), "5.0: {e50}");
    }
}

#[test]
fn lua_51_keeps_the_library_the_2_4_3_client_opens() {
    let s = s51();
    // Present, with the type each answers.
    for (expr, ty) in [
        ("coroutine", "table"),
        ("select", "function"),
        ("string.gmatch", "function"),
        ("string.match", "function"),
        ("string.reverse", "function"),
        ("table.maxn", "function"),
        ("math.fmod", "function"),
        ("math.modf", "function"),
        ("math.huge", "number"),
        ("math.cosh", "function"),
        ("bit.mod", "function"),
        ("newproxy", "function"),
        ("unpack", "function"),
        ("getfenv", "function"),
        ("loadstring", "function"),
        ("gmatch", "function"),
        ("strmatch", "function"),
        ("strrev", "function"),
        ("strsplit", "function"),
        ("strjoin", "function"),
        ("strtrim", "function"),
        ("strconcat", "function"),
        ("strreplace", "function"),
        ("getn", "function"),
        ("foreach", "function"),
        ("foreachi", "function"),
        ("sort", "function"),
        ("tinsert", "function"),
        ("tremove", "function"),
        ("difftime", "function"),
        ("time", "function"),
        ("date", "function"),
        ("_VERSION", "string"),
    ] {
        assert_eq!(
            s.eval::<String>(&format!("return type({expr})")).unwrap(),
            ty,
            "{expr}"
        );
    }
    // Absent in 2.4.3: no filesystem or OS reach, and nothing from a later client.
    for name in [
        "wipe",
        "table.wipe",
        "tostringall",
        "forceinsecure",
        "scrub",
        "io",
        "os",
        "package",
        "require",
        "module",
        "dofile",
        "debug",
        "print",
    ] {
        assert_eq!(
            s.eval::<String>(&format!("return type({name})")).unwrap(),
            "nil",
            "{name}"
        );
    }
    // The string type has its metatable.
    assert_eq!(s.eval::<String>("return ('abc'):upper()").unwrap(), "ABC");
    assert_eq!(s.eval::<String>("return ('x'):rep(3)").unwrap(), "xxx");
    // Positional `format`, in the one function the bare alias shares.
    assert_eq!(
        s.eval::<String>("return format('%2$s %1$s', 'a', 'b')")
            .unwrap(),
        "b a"
    );
    assert_eq!(
        s.eval::<String>("return string.format('%2$s-%1$d', 5, 'x')")
            .unwrap(),
        "x-5"
    );
    // `gmatch` and `match`, and the coroutine library.
    assert_eq!(
        s.eval::<String>(
            "local out = '' for w in string.gmatch('a1b22c', '%d+') do out = out .. w .. ',' end return out"
        )
        .unwrap(),
        "1,22,"
    );
    assert_eq!(
        s.eval::<String>("return strmatch('key=val', '(%w+)=')")
            .unwrap(),
        "key"
    );
    assert_eq!(
        s.eval::<i64>(
            "local co = coroutine.wrap(function(a) local b = coroutine.yield(a + 1) return b * 2 end) \
             return co(1) + co(10)"
        )
        .unwrap(),
        22
    );
    // `mod` is `math.fmod` there: the dividend's sign.
    assert_eq!(s.eval::<f64>("return mod(-7, 3)").unwrap(), -1.0);
    // `gfind` only says it was renamed; `setn` is obsolete.
    let e: String = s
        .eval("local ok, e = pcall(string.gfind, 'a', 'a') return tostring(e)")
        .unwrap();
    assert!(
        e.contains("'string.gfind' was renamed to 'string.gmatch'"),
        "{e}"
    );
    let e: String = s
        .eval("local ok, e = pcall(table.setn, {}, 1) return tostring(e)")
        .unwrap();
    assert!(e.contains("'setn' is obsolete"), "{e}");
    assert_eq!(s.eval::<i64>("return getn({ 1, 2, 3 })").unwrap(), 3);
}

#[test]
fn lua_51_has_the_string_utilities_2_4_3_adds() {
    let s = s51();
    assert_eq!(
        s.eval::<Vec<String>>("return { strsplit('-', 'a-b--c') }")
            .unwrap(),
        vec!["a", "b", "", "c"]
    );
    assert_eq!(
        s.eval::<Vec<String>>("return { strsplit(',', 'a,b,c', 2) }")
            .unwrap(),
        vec!["a", "b,c"]
    );
    assert_eq!(
        s.eval::<Vec<String>>("return { strsplit(' -', 'a b-c') }")
            .unwrap(),
        vec!["a", "b", "c"],
        "every character of the delimiter splits"
    );
    assert_eq!(
        s.eval::<String>("return strjoin(', ', 'x', 2, 'z')")
            .unwrap(),
        "x, 2, z"
    );
    assert_eq!(
        s.eval::<String>("return strconcat('a', 1, 'b')").unwrap(),
        "a1b"
    );
    assert_eq!(
        s.eval::<String>("return strtrim('  hi \\t\\n')").unwrap(),
        "hi"
    );
    assert_eq!(
        s.eval::<String>("return strtrim('xxhixx', 'x')").unwrap(),
        "hi"
    );
    assert_eq!(
        s.eval::<String>("return strreplace('a.b.c', '.', '/')")
            .unwrap(),
        "a/b/c"
    );
    assert_eq!(s.eval::<String>("return ('  x '):trim()").unwrap(), "x");
    assert_eq!(s.eval::<f64>("return difftime(10, 4)").unwrap(), 6.0);
    let e: String = s
        .eval("local ok, e = pcall(strjoin, ',', {}) return tostring(e)")
        .unwrap();
    assert!(e.contains("bad argument #2 to 'strjoin'"), "{e}");
    // None of these exist in the 1.12.1 dialect.
    let b = s50();
    for name in [
        "strsplit",
        "strjoin",
        "strtrim",
        "strconcat",
        "strreplace",
        "strmatch",
        "select",
    ] {
        assert_eq!(
            b.eval::<String>(&format!("return type({name})")).unwrap(),
            "nil",
            "{name}"
        );
    }
}

/// Two VMs of different dialects in one process, interleaved, do not see each other's dialect.
#[test]
fn two_vms_of_different_dialects_do_not_affect_each_other() {
    let a = s51();
    let b = s50();
    let a2 = s51();
    for _ in 0..3 {
        assert_eq!(a.eval::<i64>("return #{ 1, 2 }").unwrap(), 2);
        assert!(compile_error(&b, "return #{ 1, 2 }").is_some());
        assert_eq!(a2.eval::<i64>("return 7 % 4").unwrap(), 3);
        assert_eq!(
            b.eval::<i64>("local n = 0 for k, v in { 1, 2 } do n = n + v end return n")
                .unwrap(),
            3
        );
        assert!(compile_error(&b, "return 7 % 4").is_some());
        assert_eq!(compile_error(&b, "return [[a[[b]]c]]"), None);
        assert!(compile_error(&a, "return [[a[[b]]c]]").is_some());
    }
    // A function compiled under one VM keeps running under it after the other is created.
    let f = s51();
    f.run("function Count(...) return select('#', ...) end")
        .unwrap();
    let _g = s50();
    assert_eq!(f.eval::<i64>("return Count(1, 2, 3)").unwrap(), 3);
}

/// A handler written the 2.4.3 way and one written the 1.12.1 way both run: `self`, `event` and the
/// arguments are parameters, and `this`, `event` and `arg1..` are still set.
#[test]
fn lua_51_passes_self_event_and_arguments_and_keeps_the_legacy_globals() {
    let mut s = s51();
    s.run(
        r#"
        local f = CreateFrame("Frame", "NewStyle")
        f:RegisterEvent("MY_EVENT")
        f:SetScript("OnEvent", function(self, event, a, b)
            seen = { self == NewStyle, event, a, b, this == NewStyle, _G.event, arg1, arg2 }
        end)
        local g = CreateFrame("Frame", "OldStyle")
        g:RegisterEvent("MY_EVENT")
        g:SetScript("OnEvent", function()
            old = { this == OldStyle, event, arg1, arg2 }
        end)
        local u = CreateFrame("Frame", "Ticker")
        u:SetScript("OnUpdate", function(self, elapsed)
            ticked = { self == Ticker, elapsed, arg1, this == Ticker }
        end)
    "#,
    )
    .unwrap();
    s.fire_event(
        "MY_EVENT",
        vec![ScriptValue::Str("x".into()), ScriptValue::Int(2)],
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
    assert_eq!(
        s.eval::<String>(
            "return table.concat({ tostring(seen[1]), seen[2], seen[3], seen[4], tostring(seen[5]), seen[6], seen[7], seen[8] }, '|')"
        )
        .unwrap(),
        "true|MY_EVENT|x|2|true|MY_EVENT|x|2"
    );
    assert_eq!(
        s.eval::<String>("return table.concat({ tostring(old[1]), old[2], old[3], old[4] }, '|')")
            .unwrap(),
        "true|MY_EVENT|x|2",
        "a handler that names no parameter still reads the globals"
    );
    s.tick(0.25);
    assert!(s.errors().is_empty(), "{:?}", s.errors());
    assert_eq!(
        s.eval::<String>(
            "return table.concat({ tostring(ticked[1]), ticked[2], ticked[3], tostring(ticked[4]) }, '|')"
        )
        .unwrap(),
        "true|0.25|0.25|true"
    );
    // The globals are restored after the call.
    assert_eq!(s.eval::<String>("return type(this)").unwrap(), "nil");
}

/// An inline XML handler body is compiled as the body of `function(self, <params>)` of its
/// handler type, whereas 1.12.1 compiles it as the chunk and a `self` there is a plain global.
#[test]
fn an_inline_xml_handler_is_compiled_with_the_parameters_of_its_type() {
    let xml = r#"<Ui>
        <Frame name="XmlFrame">
            <Scripts>
                <OnLoad>onload = { self == XmlFrame, this == XmlFrame, type(event) }</OnLoad>
                <OnEvent>onevent = { self == XmlFrame, event, select('#', ...), arg1 }</OnEvent>
                <OnUpdate>onupdate = { self == XmlFrame, elapsed, arg1 }</OnUpdate>
                <OnEnter>onenter = motion</OnEnter>
                <OnClick>onclick = { button, down }</OnClick>
                <OnSizeChanged>onsize = { w, h }</OnSizeChanged>
            </Scripts>
        </Frame>
    </Ui>"#;
    let doc = crate::framexml::parse(xml).expect("valid FrameXML");
    let no_files = |_: &str| None;

    let mut s = s51();
    let report = load(&s, &doc, &no_files);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(
        s.eval::<String>(
            "return table.concat({ tostring(onload[1]), tostring(onload[2]), onload[3] }, '|')"
        )
        .unwrap(),
        "true|true|nil",
        "OnLoad: `self` is the frame and `this` is still set"
    );
    s.fire_event("MY_EVENT", vec![ScriptValue::Int(1)]);
    s.run("XmlFrame:RegisterEvent('MY_EVENT')").unwrap();
    s.fire_event(
        "MY_EVENT",
        vec![ScriptValue::Str("a".into()), ScriptValue::Str("b".into())],
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
    assert_eq!(
        s.eval::<String>(
            "return table.concat({ tostring(onevent[1]), onevent[2], onevent[3], onevent[4] }, '|')"
        )
        .unwrap(),
        "true|MY_EVENT|2|a"
    );
    s.tick(0.5);
    assert_eq!(
        s.eval::<String>(
            "return table.concat({ tostring(onupdate[1]), onupdate[2], onupdate[3] }, '|')"
        )
        .unwrap(),
        "true|0.5|0.5"
    );
    // The other types, called directly with their parameters.
    s.run(
        r#"
        local f = XmlFrame
        f:GetScript("OnEnter")(f, 1)
        f:GetScript("OnClick")(f, "RightButton", true)
        f:GetScript("OnSizeChanged")(f, 30, 40)
    "#,
    )
    .unwrap();
    assert_eq!(
        s.eval::<String>(
            "return table.concat({ onenter, onclick[1], tostring(onclick[2]), onsize[1], onsize[2] }, '|')"
        )
        .unwrap(),
        "1|RightButton|true|30|40"
    );

    // 1.12.1: the body is the chunk; `self` is an ordinary (nil) global and no parameter exists.
    let mut s = s50();
    let plain = crate::framexml::parse(
        r#"<Ui><Frame name="OldFrame"><Scripts>
            <OnLoad>seen = { self == nil, this == OldFrame }</OnLoad>
            <OnEnter>motion_seen = motion</OnEnter>
        </Scripts></Frame></Ui>"#,
    )
    .unwrap();
    let report = load(&s, &plain, &no_files);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(s.eval::<bool>("return seen[1] and seen[2]").unwrap());
    s.run("OldFrame:GetScript('OnEnter')(OldFrame, 'ignored')")
        .unwrap();
    assert_eq!(s.eval::<String>("return type(motion_seen)").unwrap(), "nil");
    s.tick(0.1);
}

/// A body that names `...` in a handler type whose parameter list has none is 5.1's error, as in
/// the client (`return function(self,elapsed) ... end`); `OnEvent`'s list ends in `...`.
#[test]
fn a_handler_body_may_read_dots_only_where_its_parameter_list_has_them() {
    let s = s51();
    let doc = crate::framexml::parse(
        r#"<Ui><Frame name="DotsFrame"><Scripts>
            <OnUpdate>x = ...</OnUpdate>
        </Scripts></Frame></Ui>"#,
    )
    .unwrap();
    let report = load(&s, &doc, &|_: &str| None);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("cannot use '...' outside a vararg function")),
        "{:?}",
        report.errors
    );
}
