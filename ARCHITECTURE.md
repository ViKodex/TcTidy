# TcTidy: Architecture

This is the design of record for TcTidy, an open-source Rust formatter for
TwinCAT 3 Structured Text (IEC 61131-3 + Beckhoff ExST).

## The one rule

**Absolute non-destruction of code.** A formatter that changes the meaning of a
program, drops a comment, or corrupts a file is unacceptable, regardless of
speed. Every design decision below serves this. Concretely:

- **Idempotence**: `format(format(x)) == format(x)`.
- **Semantic preservation**: `parse(x)` and `parse(format(x))` produce the same
  tree, modulo trivia.
- **Lossless round-trip**: the syntax tree re-renders to the exact input bytes.
- **Byte-identical XML**: everything outside the ST in a TwinCAT file is
  preserved to the byte.
- **Atomic writes**, and **EOL/encoding (CRLF, UTF-8 ± BOM) preservation**.

The tool is **opinionated by default**: with no configuration it applies one fixed
house style. `StyleConfig` is the central representation of the style options; every
field is read by the rules, and the settings among them are user-configurable through
a `.tctidy.toml` (see "Configuration" below). The defaults are listed under "Default
style".

## Crate graph

Strict, acyclic layering. `tctidy-pretty` is language-agnostic; neither parsing
crate depends on the formatter.

```
tctidy-pretty            (generic Doc IR + printer, no ST knowledge)
      ▲
tctidy-syntax ─► tctidy-format ─► tctidy ─► tctidy-cli
                     ▲               ▲
                tctidy-pretty   tctidy-twincat   (injected via callback)
```

| Crate | Responsibility |
|---|---|
| **tctidy-syntax** | Bytes → lossless CST (`rowan`) → typed AST. `logos` lexer; hand-written direct-build, resilient recursive-descent parser. No formatting knowledge. |
| **tctidy-pretty** | Generic pretty-printing IR (`Doc`) and the line-breaking engine. Testable in complete isolation. |
| **tctidy-format** | Lowers the typed AST to `Doc` under the centralized `StyleConfig`; owns comment attachment. |
| **tctidy-twincat** | `.TcPOU`/`.TcGVL`/`.TcDUT`/`.TcIO` XML layer: locate ST CDATA byte-ranges and **splice** formatted ST back; everything else byte-identical. |
| **tctidy** | Library facade / public API; raw-ST vs TwinCAT-file; wires the XML callback to the formatter. |
| **tctidy-cli** | Binary `tctidy`: argv → facade → IO (atomic write, `--check`, `--diff`, stdin, batch). |

## Key design decisions

- **Lossless CST via `rowan`, trivia-as-tokens.** Whitespace, newlines, all three
  comment syntaxes (`//`, `(* *)`, `/* */`), and pragmas (`{ ... }`) are token
  kinds living in the tree. Losslessness is then structural: `Σ token.len ==
  src.len()`, asserted in tests. `SyntaxKind` is one flat `#[repr(u16)]` enum;
  category predicates are range checks. Keywords are recognized
  case-insensitively while the original source text is preserved automatically,
  so keyword-case normalization is purely a formatter concern. Alternative
  spellings (`TOD`/`TIME_OF_DAY`, `DT`/`DATE_AND_TIME`) map to distinct kinds so
  they are never converted.
- **`logos` lexer; hand-written callbacks** for what regex cannot express:
  depth-counting nested comments, verbatim brace-matched multi-line pragmas
  (quote-aware), and `$`-escaped strings. Trivia is never skipped; the error
  fallback yields an `ERROR` token, so the lexer is total.
- **Direct-build, error-resilient parser:**
  the recursive-descent grammar drives a `GreenNodeBuilder` directly, using
  `checkpoint`/`start_node_at` for left-associative constructs, and emits trivia
  verbatim around the significant tokens it precedes (so the tree is lossless by
  construction). Every loop advances by at least one token (`err_and_bump` on no
  progress) so parsing always terminates; recovery wraps unexpected tokens in
  `ERROR` nodes instead of panicking. Parentheses are preserved as `PAREN_EXPR`
  nodes and never added or removed, so precedence-table mistakes can never change
  meaning.
- **Group-based Doc IR.** `Text` carries its pre-measured width so the
  fitting check is O(width). `HardLine` implies `BreakParent`. The printer is an
  iterative command stack (no recursion → no stack overflow on deep nesting).
- **`StyleConfig` centralizes style defaults.** `FormatContext` exposes the
  spacing and casing helpers the rules share. The "no style leaked" tests prove
  that every field with more than one value affects the output.
- **Comments via `LineSuffix`; pragmas verbatim.** Trailing comments are deferred
  to end-of-line so they never jump ahead of code. Comment tokens remain in source
  order. Because fitting ignores line suffixes, the printer can align them across
  consecutive lines after layout (`align_line_suffixes`) without moving a line
  break. The default intentionally normalizes leading and trailing whitespace in
  `//` comments; this is a formatting rule, distinct from comment loss. Pragma
  interiors bypass style helpers.
- **Output guard.** After printing, the formatter re-lexes its output and checks
  that every comment and pragma of the input is still there, in order
  (whitespace aside). A rule that fails to print one makes formatting fail, so
  the caller keeps the input; a missed comment is a refusal, never a deletion.
- **TwinCAT XML: splice, never re-serialize.** No XML writer preserves bytes
  (attribute order, quoting, escaping, whitespace are all free choices). So TcTidy uses
  `quick-xml` only to *locate* the ST CDATA byte-ranges and splices formatted ST
  back into the original buffer. Byte-identity of everything else is then a
  theorem, not an aspiration. The XML layer depends on neither the parser nor the
  formatter: the formatting function is injected (dependency inversion).
- **Atomic writes**: temp file in the same directory → fsync → rename → fsync of
  the directory (Unix; Windows has no supported equivalent, so a write there is
  atomic but not guaranteed durable); on any error before the rename the original
  is left untouched, and a failed directory sync is reported as written but not
  durable, never as success. A symbolic link is
  followed to the file it points to, which is the one replaced, so the link
  stays a link; a link that cannot be followed is refused. The replaced file's
  metadata (Unix owner, extended attributes and mode; Windows attributes and
  creation time) is applied to the temp file before the rename, and metadata
  that cannot be kept refuses the write; `io_atomic` documents the limits.

## Configuration

`tctidy_format::options` lists every user-configurable setting as a `StyleOption`:
its key, group, label, description, accepted values, and typed accessors. It is the
single source of truth for anything that reads or writes settings from outside the
engine. The facade's `config` module parses `.tctidy.toml` against that table (strict:
unknown keys, wrong types, unknown choices and out-of-range numbers are all reported,
with line and column), discovers the nearest file for an input, and renders a style
back to TOML. The CLI resolves and caches one configuration per directory.

Every `StyleConfig` field falls in one of three classes:

- **Settings**: every field in the table. Each changes the output
  (`crates/tctidy-format/tests/style_leak.rs`), and every value keeps the guarantees:
  `crates/tctidy-format/tests/configurable_options.rs` drives each setting to its
  boundaries, alone and in random combinations, and checks idempotence and semantic
  and comment preservation; the corpus tests run at the default and at two profiles
  that move every setting to a boundary value.
- **Internal**: `preserve_line_breaks` is read by the rules but is not a setting:
  keeping the breaks of a width-wrapped layout reshapes it on the next run, which is
  not idempotent.
- **Fixed**: `comment_style` has a single variant, `Preserve`; comments are never
  rewrapped or converted.

Line endings, identifier case and comma placement are not style options: line endings
follow the input, identifiers keep their source case, and a wrapped list ends each line
with its comma.

The facade's `settings` module serves settings UIs. `catalog` lists every setting by
group with its kind, default and current value (serializable to JSON), and
`preview` formats a short ST sample per setting twice: with the setting at its default
and with the chosen value, everything else as in the given style. Each sample is picked
so the setting visibly changes it; a unit test holds every sample to that and to
idempotence for every boundary value. `EXAMPLE` is a function block, declaration and
implementation, written in the default style, for a UI to format with all the chosen
values at once; a unit test keeps it in that style.

`tctidy --settings` is that UI: a std-only HTTP/1.1 server in the CLI (`settings_server.rs`)
bound to `127.0.0.1` on a free port, serving a fixed page (`settings_page/`) under a strict
Content-Security-Policy and a small JSON API (state, preview, save, format). The URL's
fragment carries a one-time token that every API call must send back in a custom header,
and `Host` and `Origin` are checked, so neither another local user nor a web page can use
it. The file it edits is fixed at start; saving goes through `config::update_config`,
which keeps a hand-written file's comments and layout, and through the atomic write. The
XAE Shell extension opens the page from its menu and stops the server with the
solution.

## Default style

`StyleConfig::default()` stores the house style:

| Group | Defaults | How the rules apply them |
|---|---|---|
| Indentation and width | spaces, width 4; line width 160; `preserve_line_breaks = false` | line width and line-break policy drive binary expressions and call arguments; line endings always follow the input |
| Casing | keywords and type keywords uppercase | identifiers always keep their source case |
| Block tokens | `THEN`, `DO`, and `OF` on the header line; no token indent; no single-line `IF` | `own_line` always breaks before the token; `same_line` keeps it on the header line unless the source broke before it under a wrapped condition; the single-line `IF` switch puts an `IF` whose only branch is one non-block statement, without comments, on one line when it fits |
| Spacing | spaces around binary operators and `:=`, after commas, and around `:`; none just inside parentheses | inside-parentheses spacing applies to calls, parenthesized expressions and initializers kept on one line, never to brackets or broken lists |
| Expressions | wrap before operators; expand parenthesized branches in wrapped `OR` and `XOR` chains; at most 3 inline operands in an `OR`, `XOR` or `AND` chain; use hanging parenthesized expressions | the logical-operand limit counts each precedence level on its own |
| Collections | no limit on inline call arguments; at most 2 inline structure fields; use hanging call arguments; keep a source-broken argument list broken; no trailing separator | structure initializers with more than 2 named fields keep the first field beside `(` and align later fields below it; call arguments wrap as one width-sensitive group, and calls with at least two named arguments, or more arguments than a nonzero `collection_max_inline`, always break; so does an argument list the source wrote across lines, unless `arg_break = "fit"` |
| Alignment | declaration colons per block; align `AT`; do not align implementation assignments or trailing comments | the declaration `:=` column builds on the `:` column; named structure initializers align separately from scalar declarations; implementation-assignment alignment pads the targets of a run of plain `:=` assignments (a blank line, a comment line or another statement ends the run); trailing-comment alignment pads the comments ending consecutive lines to the rightmost one, after layout, so it never moves a line break |
| Comments | preserve; one space before a trailing comment; exactly one space after `//` | comment text is never rewrapped (`comment_style` is fixed) |
| Blank lines | maximum 2; 0 at block start/end; no minimum after declaration blocks, around block statements, after `CASE` arms or before a multiline comment | the block-start and block-end budgets are caps; the other budgets are minimums, raised from the source's count and then capped by the maximum; a `CASE` arm with several statements or a nested block takes the multiline minimum, and a multiline comment that opens its block gets none |

Long binary expressions wrap when they exceed column 160. Operators align under the
first operand, and parenthesized branches of a wrapped `OR` or `XOR` chain expand by
default. An `OR`, `XOR` or `AND` chain with more than 3 operands breaks one operand per
line whatever its width (`logical_chain_max_inline`). Long call argument lists wrap as
one hanging group. Other flat statement shapes are not wrapped merely because they
exceed the configured width. Existing multiline constructs follow the rules
implemented for that construct.

Declarations use `name : TYPE`, align `:` and `AT` within their formatting groups,
and align declaration/enum initializers where those rules apply. `THEN`, `DO`, and
`OF` stay on the header line by default (`same_line`); a source break before one of
them is kept when the condition before it wraps. Line comments keep their content but
normalize `//x` or `//   x` to `// x` and remove trailing spaces or tabs.

The layout choices (compact versus expanded logical branches, hanging versus vertical
parenthesized expressions and call arguments) are settings like the others.

## TwinCAT edge cases

- TcTidy lexes `/* */` comments as nesting, like `(* *)`.
- Formatted ST never contains `]]>`: the splice layer refuses such output instead of
  breaking the CDATA section.
- ST stored outside a CDATA section (entity-escaped plain text) is left unformatted.

## Conventions

- **`#![forbid(unsafe_code)]`** everywhere, except `tctidy-syntax` which uses
  `#![deny(unsafe_code)]` with a single narrow `allow` on the `logos`-generated
  lexer module (the derive expands to `unsafe`). No hand-written `unsafe` exists.
- **Keyword table.** `crates/tctidy-syntax/keywords.csv` is the checked-in source
  of truth. `build.rs` generates the keyword code, and the generated-source gate
  detects drift.
- Latest stable Rust (no fixed MSRV), edition 2021, MIT OR Apache-2.0. Release profile uses `lto`.

## Testing

Workspace unit, integration, snapshot, and property tests cover idempotence, semantic
round-trip, totality, and the corpus invariant harness (canonicalization, idempotence,
comment preservation) over public TwinCAT projects pinned in
`xtask/public-corpus.txt`, at the default style and at two profiles that move every
configurable setting to a boundary value. The automated gates run on Linux and
Windows.

The XAE Shell extension under `editors/vs/` keeps its logic in a Core library that is
tested against the real `tctidy` binary. Automated gates build the extension for
TcXaeShell 15.0 x86 and 17.0 x64 and check the installer's 15.0-only, 17.0-only, and
coexistence discovery shapes. Those checks establish package compatibility.

## Design decisions

A few choices are deliberate and worth knowing before changing them:

- **Opinionated by default.** With no `.tctidy.toml`, indentation becomes four
  spaces; long binary expressions and call argument lists wrap at the line width,
  while other flat statements are left intact and source-broken layout is handled
  by construct-specific rules. The redundant blank before a CDATA `]]>` is dropped,
  and enum members are always exploded one per line. Settings change these
  defaults only where a rule reads them.
- **Test strategy.** The guarantees are reference-independent **hard
  invariants**, checked over real code: canonicalization
  (`format(scramble(x)) == format(x)`), idempotence, semantic preservation, and
  comment preservation. The corpus is never compared with a reference output; the
  `goldencheck` xtask is an *informational* drift gauge only, never a gate.
- **Editor integration is a manual command.** Format-on-save is infeasible because
  the TwinCAT ST editor exposes no standard Visual Studio text buffer. The
  extension implements in-memory Automation Interface edits for open POUs and one
  disk batch for closed files. The in-memory orchestration and transaction behavior
  are covered off-shell.

## Building

```sh
cargo build --workspace     # compile everything
cargo test  --workspace     # run all tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```
