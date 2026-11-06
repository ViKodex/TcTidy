# Changelog

All notable changes to TcTidy are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Configuration: `array_initializer_max_inline` (0 to 64, default 4) puts the
  values of an array initializer `[ … ]` one per line once there are more of
  them than the limit, aligned under the first value like a structure
  initializer. `0` keeps every array initializer on one line, as before.
- Configuration: `place_simple_if_on_single_line = true` writes an `IF` whose
  only branch is one simple statement on a single line when it fits
  (`IF bReset THEN nCount := 0; END_IF;`). An `IF` with `ELSIF`, `ELSE`, a
  nested block or a comment keeps its block layout. Off by default.
- Configuration: `collection_max_inline` (0 to 16) puts the arguments of a call
  or function block instance one per line once there are more of them than
  the limit, even when they fit the line width. The default `0` keeps wrapping
  by line width only.
- Configuration: `arg_break = "fit"` joins an argument list you wrote across
  several lines onto one line when it fits. The default,
  `"always_when_multiline"`, keeps such a list broken as before.
- Configuration: `align_impl_assignments = true` lines up the `:=` of
  consecutive assignments in code. A blank line, a comment line or any other
  statement starts a new group; `REF=`, `S=`, `R=` and chained assignments are
  not aligned. Off by default.
- Configuration: `align_trailing_comments = true` lines up the comments that
  end consecutive lines, in declarations and in code, on the column of the
  rightmost one. A blank line or a line without such a comment starts a new
  group. Off by default.
- Configuration: `blank_lines_after_decl` (0 to 10) puts at least that many
  blank lines after an `END_VAR` or `END_TYPE` that another block follows,
  never more than `blank_lines_max`. The default `0` keeps the blank lines you
  wrote.
- Configuration: `blank_lines_around_block_statement` (0 to 10) puts at least
  that many blank lines between an `IF`, `CASE`, `FOR`, `WHILE`, `REPEAT` or
  `__TRY` statement and the statement before or after it, never more than
  `blank_lines_max`. The default `0` keeps the blank lines you wrote.
- Configuration: `blank_lines_after_case` and `blank_lines_after_multiline_case`
  (0 to 10) put at least that many blank lines after a `CASE` arm, before the
  next arm or `ELSE`: the first for an arm with at most one simple statement,
  the second for an arm with several statements or a nested block. Never more
  than `blank_lines_max`; the default `0` keeps the blank lines you wrote.
- Configuration: `blank_lines_before_multiline_comment` (0 to 10) puts at least
  that many blank lines above a `(* ... *)` comment spanning several lines that
  heads a statement, a `CASE` arm, a declaration or a declaration block, unless
  it opens its block. Never more than `blank_lines_max`; the default `0` keeps
  the blank lines you wrote.
- Extension: when TcXaeShell starts, at most once a day, an info bar says when a
  new TcTidy release is out, with a link to its page, and shows notices about
  serious bugs in the installed version. *Tools → TcTidy → Check for Updates*
  checks right away and says why if the check fails. Turn it off with *Check
  for Updates Automatically* in the same menu, or for a whole machine with the
  `DisableUpdateCheck` policy value (see the extension README).
- Extension: *About TcTidy* shows the commit the extension was built from, next
  to its version.

### Changed

- License: TcTidy is now available under the MIT license or the Apache-2.0
  license, at your option. Releases up to 0.4.1 stay under Apache-2.0.
- Settings page: the *Preview* and *Your code* tabs are one panel. On top, the
  selected setting's sample, its default beside its current value; below,
  *Your code* opens on an example function block, declaration above
  implementation, in the default style, shown as typed and beside it
  formatted with the values on the page, what formatting changed marked. You
  can edit it or paste your own code over it, and *Restore example* brings it
  back. Each side has line numbers, and scrolling one side scrolls the other
  to the matching lines, the declaration and the implementation each on
  their own. The panel takes two thirds of the width; the group list shows
  from 1700 px wide, and a narrow panel stacks the panes.

### Removed

- Library: `StyleConfig` no longer has the `eol`, `function_case` and
  `wrap_before_comma` fields, nor the `EolPolicy` and `IdentCase` types. No
  rule read them, so they never changed the output: line endings always follow
  the input, identifiers keep their case, and a wrapped list ends each line
  with its comma.

### Fixed

- Installer: with both XAE Shell generations installed, setup no longer always
  preselects 17.0, which could leave the shell you use on the old version. It
  preselects the shell with an older TcTidy and shows what each shell holds. The
  Ready page names the target, and the last page names it and any other shell
  that still has an older TcTidy. An unattended install upgrades the same way
  and otherwise stays in a shell that already has this version; `/SHELL=15.0` or
  `/SHELL=17.0` names the target. An unattended setup no longer waits forever on
  an error message when it finds no shell.
- Settings page: the code of a setting's preview no longer moves a few pixels
  to the right when the preview shows changed lines. The column of the `+` and
  `−` signs keeps its width when it is empty.
- Formatting keeps the line break that ends the Structured Text, LF or CRLF,
  instead of removing it: the final newline of a `.st` file or of stdin, and
  the one before `]]>` that ends a section of a TwinCAT file (an empty last
  line in the TwinCAT editor). An already formatted `.st` file now passes
  `--check`, and text that does not end with a line break still gets none.
- An assignment with a comment between its target and `:=` (or `REF=`, `S=`,
  `R=`) is formatted instead of refused, and the comment stays in place:
  `b (* note *) := 3;` stays on one line, and after a line comment the
  operator continues on the next line.
- VS extension: a POU with an Action or a Transition is formatted again,
  including the Action's or Transition's Structured Text body, instead of
  failing as a whole because the element has no declaration.
- A property `Get` or `Set` whose declaration starts with its own access
  modifier (`PUBLIC`, `PRIVATE`, `PROTECTED` or `INTERNAL`) is formatted
  instead of refused with a parse error, and the modifier is kept.

## [0.4.1] - 2026-10-17

### Fixed

- Settings page: the groups, the settings and the preview fill the window
  height when only a few settings are shown (**Modified only**, or a search),
  and the top bar fits a phone-sized window instead of pushing the **Save**
  button out of it.
- VS extension: a project-wide run whose `tctidy` cannot start, times out, stops
  on a configuration error or crashes still reports what it formatted and lists
  each file left unconfirmed with the reason, and a file's error is no longer
  shown against another file whose path starts the same way.
- CLI: an invalid glob pattern now exits 2, and a directory or glob whose
  tree cannot be read in full now names the unreadable path and exits 3; both
  used to exit 0. The other targets are still formatted, and a valid glob
  matching nothing still exits 0.
- CLI: formatting a file through a symbolic link now updates the file the link
  points to and keeps the link, instead of replacing the link with a regular
  file; a link that cannot be followed (dangling, or a loop) is refused and
  nothing is changed. The settings page saves a linked `.tctidy.toml` the same way.
- CLI: on Linux and other Unix systems, a formatted file now survives a power
  loss: its directory is synced after the file is replaced. If that sync fails,
  the file is reported as formatted but not yet safe on disk and the run exits 3;
  the settings page shows the same warning after a save. Windows offers no
  supported directory sync, so writes there stay atomic without that guarantee.
- CLI: formatting a file keeps its metadata. On Linux and macOS: its owner and
  group, permission bits (such as `0751`, setuid included) and extended
  attributes, POSIX ACLs included. On Windows: its hidden, system and
  encrypted attributes and its creation time. A file whose owner, group or
  attributes cannot be kept (one owned by another user, for example) is
  refused and left unchanged instead of being silently altered.

## [0.4.0] - 2026-10-17

### Added

- Configuration: formatting style can be customized with a `.tctidy.toml`
  file. Each file uses the nearest `.tctidy.toml` in its directory or a parent
  directory (stdin uses the current directory); `--config FILE` pins one file
  for every input. 30 settings cover indentation, line width, keyword casing,
  spacing, the placement of `THEN`/`DO`/`OF`, wrapping layouts, alignment,
  trailing commas, comments, and blank lines; absent settings keep the
  defaults. Unknown keys and invalid values are rejected with their line and
  column (exit code 2) and no file is formatted with that configuration.
  `--print-config` prints the effective configuration.
- Settings page: `tctidy --settings [DIR]` serves a page for the `.tctidy.toml`
  that applies to DIR: the settings in a list, a preview of the selected one
  (its default against the chosen value) on a short sample, and a tab to try
  the settings on pasted code. It follows the system's light or dark mode,
  and its top bar can pick light or dark instead.
  Saving writes only non-default settings, keeps the comments and layout of a
  hand-written file, and refuses to overwrite a file changed on disk unless
  asked. The page is served on `127.0.0.1` only, behind a one-time token, and
  stops 10 minutes after it is closed.
- Visual Studio: **Tools → TcTidy → Formatting Settings…** opens that page for
  the open solution.
- CLI: `--stdin-filepath PATH` formats stdin with the `.tctidy.toml` that
  applies to PATH.
- VS extension: *About TcTidy* shows the folder that holds the license and the
  third-party notices, and the extension ships a notice for the NuGet packages it
  redistributes (`THIRD-PARTY-NOTICES-EXTENSION.md`).
- Branding: a TcTidy logo in `assets/logo/`, as SVG for light and for dark
  backgrounds, with a simplified icon for 16 to 32 px. The README shows it, and
  the settings page uses the icon.

### Changed

- Formatter: an `OR`, `XOR` or `AND` chain with more than 3 operands now
  breaks one operand per line, even when it fits the line width, in
  conditions and assignments alike. Each precedence level counts on its own:
  in `a OR b AND c OR d` the OR chain has three operands. Set
  `logical_chain_max_inline` (0–32) to change the limit, or to `0` to wrap by
  line width only as before.
- Build (breaking): source builds now require the latest stable Rust; a minimum
  supported Rust version is no longer tracked. Prebuilt CLI and VSIX
  installation requirements are unchanged. Update `ignore` to 0.4.32 and
  `globset` to 0.4.20.
- Formatter: calls with at least two named arguments now always use a multiline
  hanging layout, even when the complete call fits within the configured line width.
  With the default hanging layout, the closing parenthesis stays beside the final
  argument.
- Formatter: long logical expressions now use aligned operator continuations and
  expanded parenthesized branches by default (`logical_expression_layout`). Wrapped
  parenthesized expressions use a hanging layout by default, with the first operand
  beside `(` and the closing delimiter aligned under it; set
  `parenthesized_expression_layout = "vertical"` for the previous layout.
- Formatter: the default line width is now 160 columns. Long call argument lists
  wrap as one canonical hanging group: the first argument stays beside `(`,
  following arguments and `)` align under it, and named argument operators align.
  Set `call_argument_layout = "vertical"` for a vertical layout.
- Formatter: structure initializers with more than two named fields now use
  the hanging layout even when the source is on one line or later fields carry
  comments. Set `struct_initializer_max_inline` to change the limit.

### Fixed

- VS extension: **Tools → TcTidy → Report a Bug…** opens the bug report form again,
  with the extension version filled in.
- VS extension: the About box shows a prerelease engine version (such as
  `0.4.0-rc.1`) instead of reporting the engine as not found.
- Parser: accept constructs found in real TwinCAT projects, which used to make
  TcTidy refuse the whole file: an interface extending several interfaces
  (`INTERFACE I_C EXTENDS I_A, I_B`), `INTERNAL`, `ABSTRACT` and other modifiers
  on a type (`TYPE INTERNAL E_Mode`), an elementary type passed as an argument
  (`SIZEOF(BOOL)`, `__NEW(UINT, 1)`), `Get` and `Set` as names (`METHOD Set`),
  a stray `;` in a variable or structure block, and a `;` after `END_TYPE`.
- Formatter: a comment before the `.` of a method chain split over several
  lines (`a.B() // note`, then `.C();`) is kept with its line break instead of
  making TcTidy refuse the file.
- Formatter: a comment after `END_TYPE` is kept instead of making TcTidy refuse
  the file.
- Visual Studio: formatting a POU that is open in the editor now uses the
  `.tctidy.toml` that applies to its file, like formatting it on disk; it used
  to look for one from Visual Studio's own folder.
- Formatter: a comment or pragma between a statement's last token and its `;`
  (`x := a (* why *);`, also after `RETURN`, `EXIT`, `JMP`, calls and `END_*`)
  was deleted; it is now kept. As a safety net, formatting is now refused
  (exit 3, input left unchanged) whenever any comment or pragma would be
  missing from the output, instead of silently dropping it. Inline block
  comments in some other mid-statement positions (for example `b (* c *) := 1;`)
  are currently refused this way rather than formatted.
- Parser: accept `TIME()` and `LTIME()` clock calls in Structured Text.
- Formatter: align named structure initializers separately from following scalar
  declarations, and place a single adjacent line comment after the preceding
  initializer field's comma. Blank-separated or stacked comments stay above the
  next field.
- Formatter: align multiline named structure initializers with the opening
  delimiter instead of forcing a fully vertical layout.
- Parser: accept typed literals such as `USINT#0` as `CASE` labels without
  consuming the label separator as part of the literal.
- VS extension: build shell-specific packages for XAE Shell 15 and 17, using the
  matching Visual Studio Community Toolkit and .NET Framework target. The TcTidy
  menu now lives under Tools so XAE Shell 17 does not filter it as an unexpected
  top-level menu.
- Formatter: accept and preserve the optional semicolon after `END_VAR` that the
  TwinCAT compiler tolerates in existing projects.
- VS extension: format declarations of open GVLs whose TwinCAT Automation Interface
  adapter does not expose an implementation `Language` property.

## [0.3.3] - 2026-07-27

### Fixed

- VS installer: discover XAE Shell 15.0 (x86) and 17.0 (x64) from their COM
  registrations, including custom paths and side-by-side installations. Install,
  upgrade, cache refresh, and uninstall now target the selected instance without
  trusting writable metadata or recursively deleting unrelated extension files.
- VS extension: include the project license and verified Cargo/NuGet notices in
  the VSIX and installer payload, excluding host SDK and build-only assemblies.
- Release: publish a SHA-256 sidecar for the extension installer and verify its
  final bytes against GitHub's release asset digest before publication.
- VS extension: declare package compatibility for both XAE Shell 15.0 x86 and
  17.0 x64 while keeping the managed payload architecture-neutral.
- VS extension: skip open implementation bodies whose Automation Interface language is
  not Structured Text, while continuing to format their language-independent declarations.
- Release: keep VSIX, extension and installer metadata synchronized with the
  Cargo product version.
- Formatter: preserve a multiline line suffix at end of file instead of
  dropping it when the suffix contains an embedded hard line.
- Formatter: preserve comments located in token gaps inside expressions and
  statements.
- Formatter: preserve comments located at declaration token boundaries.

## [0.3.2] - 2026-07-06

### Fixed

- CLI: a literal path that does not exist now reports an error and exits 3
  instead of being silently skipped with exit 0; the targets that do exist are
  still processed. Globs matching nothing keep exiting 0.
- CLI: overlapping path arguments (the same file named literally and matched
  by a directory or glob) are formatted exactly once.
- CLI: `--decl` / `--impl` now apply to raw (non-XML) ST files; they were
  silently ignored on the file path, so a raw declaration file could not be
  formatted. The default for files is unchanged.

## [0.3.1] - 2026-07-05

### Added

- VS extension: a top-level **TcTidy** menu in the XAE shell with
  *Documentation*, *Report a Bug…* (GitHub issue pre-targeted at the bug
  template, extension version prefilled) and *About TcTidy* (extension and
  engine versions, the engine's resolved path).
- VS extension: a checkable **Performance Report** toggle on the TcTidy menu
  turns the end-to-end timing report on/off. Persisted across restarts,
  effective on the next Format run, confirmed in the status bar.
- VS extension: **Ctrl+K, Ctrl+T** formats the element open in the editor (same
  action as *Format with TcTidy* on the ST-editor menu). Global binding, shown
  next to the menu entry, rebindable under *Tools > Options > Environment >
  Keyboard* as `TcTidy.FormatElement`.
- VS extension: the *Format with TcTidy* commands now carry the stock Visual
  Studio format-document icon on the TwinCAT right-click menus.

### Changed

- VS extension: *Format with TcTidy* now sorts first on the four TwinCAT
  right-click menus (POU, folder, PLC project, ST editor) instead of below
  other formatters' entries.

### Removed

- VS extension: the `TCTIDY_PERF` environment variable is no longer read; the
  Performance Report menu toggle is the only switch.

## [0.3.0] - 2026-07-01

### Changed

- Blank lines right after a block header (`IF`/`ELSIF`/`ELSE`, `CASE ... OF`,
  `FOR`/`WHILE`/`REPEAT`, a `CASE` label `:`) are now collapsed before the first
  statement or arm, configurable via `blank_lines_at_block_start` (default 0).
- Blank lines right before a block/container closer (`END_IF`/`END_CASE`/`END_FOR`/
  `END_WHILE`/`END_REPEAT`, `END_STRUCT`/`END_UNION`/`END_VAR`, the enum `)`) are now
  collapsed, configurable via `blank_lines_at_block_end` (default 0).
- Dropping the redundant trailing comma of a broken argument or initializer list
  (`FB_Foo(a := 1,)` → `FB_Foo(a := 1)`), a call's args, an FB instance's default
  args, an array/struct initializer, and of enum bodies is now a config-driven
  normalization (`trailing_comma_collection` / `trailing_comma_enum`, both default
  to dropping it) rather than hard-coded. House-style output is unchanged.

### Fixed

- Comments inside a declaration's name list, between two names, or between the
  last name and the `:`, are no longer dropped (a non-destruction violation). A
  block comment stays inline (`a, (* c *) b : INT;`); a line comment forces the
  names one per line, each keeping its comment.
- Trailing whitespace inside line comments (`// note   `, `//   `, and trailing
  comments like `x := 1; // c   `) is now trimmed, consistently and regardless of
  `space_after_line_comment`. Block comments and pragmas stay byte-for-byte verbatim.
- Project-wide formatting no longer skips files whose path contains a glob
  metacharacter (`[`, `]`, `*`, `?`), e.g. a `[NN]-Name` folder. An existing file
  or directory is now taken literally; only a non-existent path is expanded as a
  glob. Previously such files were silently left unformatted while the batch walked
  the base directory once per file (so a large project appeared to hang).

## [0.2.0] - 2026-06-22

This release adds a Visual Studio / TwinCAT XAE Shell extension and sharpens the CLI and its
diagnostics. While 0.x, formatter output may change between minor versions.

### Added

- **VS / TwinCAT XAE Shell extension, a *Format with TcTidy* command.** It appears on the TwinCAT
  PLC source-object, folder and project context menus (and the ST-editor menu), and formats through
  the tested `tctidy` core:
  - A POU selected in the Solution Explorer formats the **whole POU**, its declaration and
    implementation plus every method, action and property (and their get/set accessors). A POU that
    is **open** in the editor is formatted in memory through the TwinCAT Automation Interface as a
    single undo step, with no "file changed outside the environment, reload?" prompt; **closed** files
    are formatted on disk in one batched `tctidy --files-from -` run off the UI thread.
  - Launched from **inside** a method/action/property editor, only the open element is formatted,
    never its siblings. The open buffer is flushed with a targeted IDE save first, so unsaved
    on-screen edits are what gets formatted, and an element's declaration and implementation are always
    done together.
  - A parse/format error leaves that file untouched and is reported against the element (e.g.
    `FB_Example.Init`) without aborting the batch. A project-wide run shows progress and a
    per-file changed/unchanged/failed summary in the **TcTidy** Output pane; when any file is left
    untouched, a short warning dialog reports how many failed and brings that pane to the front, while
    a fully clean run stays silent.
  - Opt-in **end-to-end timing diagnostics**: set `TCTIDY_PERF` before launching the shell to log a
    per-run breakdown (file detection, save, and format, split into *Open via API* vs *Disk batch*,
    each with its file count, per-file average and changed/unchanged split, plus the total) to the
    **TcTidy** Output pane and `%TEMP%\tctidy-perf.log`. Off by default; normal use is unaffected.
  - A bundled **Inno Setup installer** (`TcTidySetup-<version>.exe`, attached to each release) deploys
    the extension and `tctidy.exe` into TcXaeShell 15.0.
- `tctidy --files-from <file>` reads the list of target paths (one per line; blank lines and `#`
  comments ignored) from a file, or from stdin with `--files-from -`, so thousands of files can be
  formatted in a single invocation without hitting the operating-system command-line length limit.
- `tctidy -` now auto-detects TwinCAT XML on stdin (a leading `<`) and formats the whole document,
  reformatting the ST inside while preserving every byte outside it, in addition to the existing
  raw-ST mode.
- Release archives and the extension installer bundle a `THIRD-PARTY-NOTICES.md` listing the licenses
  of every third-party crate linked into the binary, generated at release time by `cargo about`
  (config in `about.toml`).

### Changed

- Parse errors now name the failing ST section and the source lines involved (e.g. `parse error in
  implementation: lines 11, 25, 38`) instead of the opaque `parse error: N parse error(s)`. Line
  numbers are relative to the section, matching the XAE editor's separate declaration/implementation
  panes. On the `tctidy -` (stdin) path the redundant `<stdin>` locator is dropped, and the VS /
  TwinCAT XAE extension drops the redundant `tctidy:` prefix when surfacing a failure.

### Removed

- macOS release binaries (`x86_64-apple-darwin`, `aarch64-apple-darwin`) are no longer built or
  published. TwinCAT runs only on Windows; the Windows and Linux binaries remain.

### Fixed

- Line-comment spacing now collapses an *excess* run of spaces/tabs after `//` to a single space
  (`//   x` → `// x`), not only the missing-space case (`//x` → `// x`). The comment's content and
  internal spacing are otherwise preserved verbatim.

## [0.1.0] - 2026-06-17

### Added

- First release of the `tctidy` CLI: formats TwinCAT 3 Structured Text in
  `.TcPOU` / `.TcGVL` / `.TcDUT` / `.TcIO` files and via stdin (`-`).
- Modes: atomic write-in-place (default), `--check` (CI signal), `--diff`.
- stdin section selectors `--decl` / `--impl`.
- Exit codes: `0` ok, `1` would reformat (`--check`), `2` usage, `3` parse/format
  error (file left untouched).
- Guarantees: idempotent output, semantic and comment preservation, atomic
  writes, and faithful round-trip of line endings and UTF-8 BOM.
- Standalone binaries: Windows x86_64 (static CRT, no Visual C++ runtime), Linux
  x86_64-gnu, and macOS x86_64 + aarch64.

[Unreleased]: https://github.com/ViKodex/TcTidy/compare/v0.4.1...HEAD
