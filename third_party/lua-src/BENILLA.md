# benilla's `lua-src` fork: what differs from upstream, and how to check

Upstream: [`mlua-rs/lua-src-rs`](https://github.com/mlua-rs/lua-src-rs), MIT, version `550.0.0`,
the version mlua 0.11 resolves to. Wired in through `[patch.crates-io]` in the workspace root.

## Why a fork exists at all

benilla's Lua has to accept exactly the grammar the 1.12.1 client's Lua accepts, no more and no
less. Every hunk below is that one rule: three restore 5.0 constructs 5.1 deleted, three delete 5.1
constructs 5.0 never had. A grammar difference in either direction is invisible to every instrument
until an addon trips over it, and the addon corpus asks about the difference directly.

The 1.12 addon corpus is Lua 5.0 code. It uses the iterator-less generic-for:

```lua
for k, v in someTable do ... end        -- no pairs(), no iterator function
```

183 of 218 corpus addons are reached by it (118 carry it, 65 inherit it through a declared
dependency), across 1,163 sites. On stock Lua 5.1 it raises `attempt to call a table value`, the
first session-start error for 60 of the 218.

Lua 5.1 removed it at the opcode level, which is why no layer above the VM can reach it: a `__call`
metamethod would need a per-type default metatable for tables, which 5.1 does not have; rewriting
the chunk source means parsing Lua with a regex; and `lua-src` ships 5.1 through 5.5, every one of
which removed it.

## What is not being done

benilla is not adopting Lua 5.0. It stays on 5.1.5 and restores the behaviour of a single opcode
that Lua itself shipped and labelled `/* for compatibility only */`. Every other 5.1 fix, and the
whole of mlua's surface, stay exactly as upstream.

## The delta, and the command that proves it

Since the build profile (2.4.3 beside 1.12.1) every hunk is a switch, not a deletion. A `global_State`
carries a one-byte `dialect` (`lstate.h`: `LUA_BENILLA_DIALECT_50` = 0, `_51` = 1), set to 50 in
`lua_newstate`, so a state nobody touches behaves as before. `benilla_setdialect(L, d)` (`lapi.c`,
declared in `lua.h`) sets it; benilla-ui calls it through FFI on the VM's own state, after its own
Lua is loaded and before any user chunk is compiled. The macro `luai_dialect50(L)` is the test.
In the 5.1 dialect each hunk is upstream 5.1.5's text. No unpatched `lua-src` is in the cargo
registry, so that text is reconstructed from this tree (every hunk's removed lines are recorded
below and in the code) and from 5.1.5's grammar; the tests in
`crates/benilla-ui/src/script/tests/dialect.rs` exercise each construct in both dialects.

Five hunks switch what 5.1 changed or removed back to 5.0:

| file | hunk | 5.0 dialect | 5.1 dialect |
|---|---|---|---|
| `lvm.c` | 5.0's `OP_TFORPREP` table-to-`next` substitution, folded into the top of `OP_TFORLOOP` | `for k, v in someTable do` runs `next` over the table | stock: calling a table raises "attempt to call a table value" |
| `llex.c` | long-string nesting (`read_long_string`'s `[` and `]` arms), formerly `LUA_COMPAT_LSTR` = 2 in `luaconf.h` | `[[ [[ ]] ]]` nests, no error | stock `LUA_COMPAT_LSTR` = 1 (restored in `luaconf.h`): the nested `[[` raises "nesting of [[...]] is deprecated" |
| `luaconf.h`, `lobject.c`, `llex.c` | `LUA_QL(x)`: kept as 5.0's `` `x' `` in every literal; `luaO_pushvfstring` copies each format literal with backquotes turned into apostrophes, and `luaX_lexerror` does the same for its message, when the state is 5.1 (`luaO_qlfix`) | `` `x' `` | `'x'` |
| `lparser.c` | `constructor()`'s loop head `testnext(ls, ';')` | one extra `;` after a field separator is skipped | not skipped: parse error |
| `lparser.c` | `recfield`'s `cc->nh++` | inside its `TK_NAME` arm: `[expr] = value` credits neither size hint | after the `if`/`else`: both forms credit it |

Three delete what 5.1 added; the 5.0 dialect lands on 5.0's own `unexpected symbol` at `prefixexp`, the
5.1 dialect has 5.1.5's code (all in `lparser.c`, byte-read out of the 1.12.1 client's parser:
`simpleexp 0x6fd240`, `getunopr 0x6fe0a0`, `getbinopr 0x6fe0c0`):

| hunk | 5.0 dialect | 5.1 dialect |
|---|---|---|
| `simpleexp`'s `case TK_DOTS`: `...` as an expression | falls to `primaryexp`, "unexpected symbol" | `check_condition(is_vararg, "cannot use '...' outside a vararg function")`, clears `VARARG_NEEDSARG`, `OP_VARARG` |
| `getunopr` takes the `LexState`; `case '#'` | `OPR_NOUNOPR` | `OPR_LEN` |
| `getbinopr` takes the `LexState`; `case '%'` | `OPR_NOBINOPR` | `OPR_MOD` |

The `luaconf.h` compat macros that are not grammar are untouched upstream defaults for both
dialects (`LUA_COMPAT_VARARG`, `_MOD`, `_GFIND`, `_OPENLIB`); where the 2.4.3 library differs
(`string.gfind` only raises there, `mod` is `math.fmod`) benilla-ui's library layer for the dialect does it.
`LUA_COMPAT_VARARG` is stock in both: with `...` as a 5.1 expression a vararg function that names it
loses the `arg` table, as in 5.1; with the 5.0 dialect no function ever names it, so every vararg
function keeps its `arg`.

`src/lib.rs` additionally differs by having Lua 5.2/5.3/5.4/5.5 stripped from the `Version` enum (with
their source trees deleted): benilla builds 5.1 and only 5.1, and a fork that still offered the other
four would answer a request for one with a missing directory at build time instead of a compile error.

The 5.0 dialect's deletions are safe because nothing the reference runs uses those constructs: a
comment- and string-stripping scan of the 1.12 FrameXML (177 files), GlueXML, Blizzard's own addons and the
corpus finds zero sites of all three. No chunk of benilla's own Lua may use a construct the 1.12.1
client's parser rejects; benilla's own chunks are compiled before the switch to the 5.1 dialect.

To verify the Lua sources against upstream at any time:

```sh
# 550.0.0 is the pinned version; adjust the path if cargo's registry hash differs.
diff -r third_party/lua-src/lua-5.1.5 \
  ~/.cargo/registry/src/*/lua-src-550.0.0/lua-5.1.5
```

That must print exactly the hunks in the tables above (and the `benilla_setdialect` plumbing in
`lstate.h`, `lstate.c`, `lapi.c`, `lua.h`, `lobject.c`, `lobject.h`) and nothing else.

## The generic-for, in detail

The `lvm.c` comment carries the three details that are the client's own, each byte-read there: the
substitution test is a bare type-tag equality that never consults a metatable (a table carrying
`__call` still gets `next`); the callee is the global `next`, read raw and fetched fresh at every loop
entry (an addon assigning `next = myfn` changes every later generic-for in the session); and userdata
is not substituted. The substitution happens in `OP_TFORPREP`, an opcode 5.1 deleted, so the top of
`OP_TFORLOOP` is where 5.1 can host it.
