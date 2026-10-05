//! The Lua 5.1 dialect: the VM's library as the 2.4.3 client's Lua 5.1.1 exposes it.
//!
//! Unlike [`super::lua50`] this removes little: 2.4.3 opens base, `string` (with its metatable),
//! `table`, `math`, `coroutine` and `bit`, and publishes its bare aliases from an embedded compat
//! chunk. What stays stock here is stated by what is *not* done: `select`, `coroutine`, the string
//! metatable, `string.match`/`gmatch`/`reverse`, `table.maxn`, `math.fmod`/`modf`/`huge` and the
//! hyperbolics, `gcinfo`/`collectgarbage` and `assert` keep their 5.1 shapes.

use mlua::{Lua, MultiValue, Table, Value, Variadic};

/// The base and aliases that must exist before the shared stdlib layer binds its own aliases.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // The compat chunk's table aliases (`tinsert` and `tremove` are the stdlib layer's).
    let table: Table = g.get("table")?;
    for name in ["foreach", "foreachi", "getn", "sort"] {
        g.set(name, table.get::<Value>(name)?)?;
    }

    // 5.1.1's `string.gfind` exists only to say it was renamed.
    let string: Table = g.get("string")?;
    string.set(
        "gfind",
        lua.create_function(|_, _: MultiValue| -> mlua::Result<()> {
            Err(mlua::Error::RuntimeError(
                "'string.gfind' was renamed to 'string.gmatch'".into(),
            ))
        })?,
    )?;

    // Neither the client's `print` nor its `load`/`loadfile` is decidable from its strings, and
    // no stock file calls them: they stay out, as in the sandbox.
    g.set("print", Value::Nil)?;

    // The same eight-function `bit` library as 1.12 (`bit.mod` included).
    super::lua50::install_bit(lua)?;
    Ok(())
}

/// The compat chunk's aliases that differ from 1.12's, and the string utilities 2.4.3 adds. Runs
/// after the shared stdlib layer, whose `mod` it replaces.
pub(super) fn install_aliases(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    let string: Table = g.get("string")?;
    let math: Table = g.get("math")?;

    // `mod = math.fmod` there; 1.12's `mod` is 5.0's `math.mod`.
    g.set("mod", math.get::<Value>("fmod")?)?;
    g.set("gmatch", string.get::<Value>("gmatch")?)?;
    g.set("strmatch", string.get::<Value>("match")?)?;
    g.set("strrev", string.get::<Value>("reverse")?)?;

    let strsplit = lua.create_function(
        |lua, (delim, s, pieces): (mlua::String, mlua::String, Option<i64>)| {
            let delim = delim.as_bytes();
            let s = s.as_bytes();
            let limit = pieces.filter(|&p| p > 0).map(|p| p as usize);
            let mut out: Vec<mlua::String> = Vec::new();
            let mut start = 0;
            for (i, b) in s.iter().enumerate() {
                if delim.contains(b) && limit.is_none_or(|l| out.len() + 1 < l) {
                    out.push(lua.create_string(&s[start..i])?);
                    start = i + 1;
                }
            }
            out.push(lua.create_string(&s[start..])?);
            Ok(Variadic::from_iter(out))
        },
    )?;
    let strjoin = lua.create_function(|lua, args: Variadic<Value>| {
        let mut it = args.into_iter();
        let delim = text_arg(lua, it.next(), 1, "strjoin")?;
        let mut out = Vec::new();
        for (i, v) in it.enumerate() {
            if i > 0 {
                out.extend_from_slice(&delim);
            }
            out.extend_from_slice(&text_arg(lua, Some(v), i + 2, "strjoin")?);
        }
        lua.create_string(out)
    })?;
    let strconcat = lua.create_function(|lua, args: Variadic<Value>| {
        let mut out = Vec::new();
        for (i, v) in args.into_iter().enumerate() {
            out.extend_from_slice(&text_arg(lua, Some(v), i + 1, "strconcat")?);
        }
        lua.create_string(out)
    })?;
    let strtrim =
        lua.create_function(|lua, (s, chars): (mlua::String, Option<mlua::String>)| {
            let s = s.as_bytes();
            let default = b" \t\r\n";
            let chars = chars.as_ref().map(|c| c.as_bytes());
            let set: &[u8] = chars.as_deref().unwrap_or(default);
            let from = s.iter().position(|b| !set.contains(b)).unwrap_or(s.len());
            let to = s
                .iter()
                .rposition(|b| !set.contains(b))
                .map_or(from, |i| i + 1);
            lua.create_string(&s[from..to.max(from)])
        })?;
    let strreplace = lua.create_function(
        |lua, (s, find, with): (mlua::String, mlua::String, mlua::String)| {
            let (s, find, with) = (s.as_bytes(), find.as_bytes(), with.as_bytes());
            let mut out = Vec::with_capacity(s.len());
            let mut i = 0;
            while i < s.len() {
                if !find.is_empty() && s[i..].starts_with(&find) {
                    out.extend_from_slice(&with);
                    i += find.len();
                } else {
                    out.push(s[i]);
                    i += 1;
                }
            }
            lua.create_string(out)
        },
    )?;
    for (name, f) in [
        ("strsplit", &strsplit),
        ("strjoin", &strjoin),
        ("strconcat", &strconcat),
        ("strtrim", &strtrim),
        ("strreplace", &strreplace),
    ] {
        g.set(name, f.clone())?;
    }
    // The compat chunk's last lines: the string table carries the utilities as methods.
    string.set("split", strsplit)?;
    string.set("join", strjoin)?;
    string.set("trim", strtrim)?;
    string.set("replace", strreplace)?;

    // `os.difftime` hoisted, as `time` and `date` are.
    g.set(
        "difftime",
        lua.create_function(|_, (later, earlier): (f64, Option<f64>)| {
            Ok(later - earlier.unwrap_or(0.0))
        })?,
    )?;
    Ok(())
}

/// A string-or-number argument as bytes, raising `bad argument` for anything else.
fn text_arg(lua: &Lua, v: Option<Value>, n: usize, who: &str) -> mlua::Result<Vec<u8>> {
    let got = match &v {
        Some(Value::Nil) | None => "no value",
        Some(Value::Boolean(_)) => "boolean",
        Some(Value::Table(_)) => "table",
        Some(Value::Function(_)) => "function",
        _ => "userdata",
    };
    match v {
        Some(v @ (Value::String(_) | Value::Integer(_) | Value::Number(_))) => lua
            .coerce_string(v)?
            .map(|s| s.as_bytes().to_vec())
            .ok_or_else(|| mlua::Error::runtime("unreachable")),
        _ => Err(mlua::Error::RuntimeError(format!(
            "bad argument #{n} to '{who}' (string expected, got {got})"
        ))),
    }
}
