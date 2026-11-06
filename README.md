<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/tctidy-logo-dark.svg">
    <img alt="TcTidy" src="assets/logo/tctidy-logo.svg" width="460">
  </picture>
</h1>

[![CI](https://github.com/ViKodex/TcTidy/actions/workflows/ci.yml/badge.svg)](https://github.com/ViKodex/TcTidy/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ViKodex/TcTidy)](https://github.com/ViKodex/TcTidy/releases)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT_OR_Apache--2.0-blue.svg)](#license)

A fast, safe, open-source code formatter for **Structured Text** (IEC 61131-3) in the
**TwinCAT 3** ecosystem.

TcTidy reformats Structured Text inside TwinCAT source files (`.TcPOU`, `.TcGVL`,
`.TcDUT`, `.TcIO`) as well as raw ST via standard input/output, so it fits cleanly into
editors and CI pipelines alike.

## Why TcTidy

In mainstream software development, formatters, linters and CI checks are free, open
source and taken for granted. PLC programming does not work that way: the industry is
closed, and its tools are mostly proprietary and paid, when they exist at all.

When I looked for a formatter for TwinCAT, the options I found were unmaintained,
incomplete or paid. TcTidy aims to fill that gap with a free, open-source formatter: a
small brick toward open tooling for automation engineers.

## Guiding principle

**Absolute non-destruction of code.** A formatter that changes the meaning of a program,
drops a comment, or corrupts a file is unacceptable, regardless of how fast it is. Every
design decision flows from that promise, enforced by:

- **Idempotence**: `format(format(x)) == format(x)`.
- **Semantic preservation**: `parse(x)` and `parse(format(x))` yield the same syntax
  tree (modulo trivia).
- **Atomic writes**: temp file + rename, never a partially written file.
- **Faithful round-trip**: comment tokens and order, pragmas, attributes, line endings and
  encoding (UTF-8 with or without BOM) are preserved, and everything outside the
  Structured Text of a TwinCAT file is kept byte for byte. The default style
  intentionally normalizes whitespace immediately after `//` and trims trailing
  whitespace in line comments; it does not discard their content.

## Example

Before:

```
if bStart and not bFault then
// start the pump
nState:=1;
    fbPump(Enable:=TRUE,Speed:=fSetpoint);
elsif bStop then nState:=0;
end_if
```

After `tctidy`, with the default style:

```
IF bStart AND NOT bFault THEN
    // start the pump
    nState := 1;
    fbPump(Enable := TRUE,
           Speed  := fSetpoint);
ELSIF bStop THEN
    nState := 0;
END_IF
```

## What it includes

- The `tctidy` command-line tool, which formats TwinCAT files and standard input in
  place, or checks or diffs them (see [Usage](#usage)).
- A TwinCAT XAE Shell extension that adds a **Format with TcTidy** command to the IDE
  (see [Editor integration](#editor-integration)).

[ARCHITECTURE.md](ARCHITECTURE.md) describes the design and the crates of the Cargo
workspace.

## Installation

Download the archive for your platform from the
[latest release](https://github.com/ViKodex/TcTidy/releases/latest), extract it, and put
`tctidy` (`tctidy.exe` on Windows) on your `PATH`. You can also build it from source
(below).

| Platform              | Asset                                              |
|-----------------------|----------------------------------------------------|
| Windows (x64)         | `tctidy-v<version>-x86_64-pc-windows-msvc.zip`      |
| Linux (x64)           | `tctidy-v<version>-x86_64-unknown-linux-gnu.tar.gz` |

The Windows binary needs no separately installed Visual C++ runtime. Every asset ships
with a matching `.sha256` checksum.

Building from source (latest stable Rust):

```sh
cargo build --release -p tctidy-cli
```

### Editor integration

A TwinCAT XAE Shell extension brings the formatter into the IDE as a **Format with
TcTidy** command. It installs into TcXaeShell 15.0 (x86) and 17.0 (x64). Install it
with the `TcTidySetup-<version>.exe` attached to the
[latest release](https://github.com/ViKodex/TcTidy/releases/latest) (administrator
rights required), or build it from source; see
[editors/vs/README.md](editors/vs/README.md) for what it does and how it works.

The installer is not code-signed, so Windows SmartScreen warns about an unknown
publisher when you run it.

The extension tells you when a new release is out; to update, run the new installer.
See [Update check](editors/vs/README.md#update-check) for what it sends and how to turn
it off.

## Usage

```sh
tctidy path/to/file.TcPOU      # reformat in place (atomic)
tctidy path/to/source.st       # a literally named raw-ST file is also supported
tctidy <paths...>              # files, directories, or globs
tctidy --check  <paths...>     # CI: non-zero exit if anything is unformatted
tctidy --diff   <paths...>     # show changes without applying them
tctidy --files-from list.txt   # read the target paths from a file (one per line)
tctidy --files-from -          # …or from stdin (avoids the OS arg-length limit)
tctidy -                       # read ST from stdin, write formatted ST to stdout
tctidy - --decl                # treat stdin as a declaration section
tctidy - --stdin-filepath F    # …formatted with the .tctidy.toml that applies to file F
tctidy --settings [DIR]        # serve a settings page for DIR's .tctidy.toml (see below)
```

Literal file arguments are accepted regardless of extension and are classified by their
content. Directory and glob discovery deliberately selects only TwinCAT object files:
`.TcPOU`, `.TcGVL`, `.TcDUT`, and `.TcIO`.

| Exit code | Meaning |
|-----------|---------|
| `0` | Success. |
| `1` | A file would be reformatted (`--check`). |
| `2` | Usage error (such as an invalid glob) or invalid configuration. |
| `3` | Parse, format or write error (the file is left untouched); a file formatted but not synced to disk, which a power loss could undo (the message says which); a path that does not exist; or a directory that cannot be read. |

A problem with one target does not stop the others: the run exits with the most severe
code, `2` over `3` over `1`. A valid glob that matches nothing is not an error.

### Pre-commit and CI

`tctidy --check <paths...>` exits with `1` when a file is not formatted, which makes it
a CI gate. The repository also provides pre-commit hooks, `tctidy` (format in place) and
`tctidy-check` (check only); `tctidy` must already be on your `PATH`:

```yaml
- repo: https://github.com/ViKodex/TcTidy
  rev: vX.Y.Z  # a release tag; `pre-commit autoupdate` moves it to the latest
  hooks:
    - id: tctidy
```

## Configuration

The default style needs no configuration. To customize it, add a `.tctidy.toml`
next to your solution (or in any parent directory of the files to format) and
commit it, so the IDE, the CLI, and CI all format the same way:

```toml
indent_style = "tabs"
keyword_case = "lower"
line_width = 120
blank_lines_max = 1
```

Every file uses the nearest `.tctidy.toml` in its own directory or a parent; stdin uses
the one that applies to `--stdin-filepath`, else the one found from the current directory.
`--config FILE` uses one file for all inputs.
Settings left out keep their default. Unknown settings and invalid values are errors,
reported with their line and column, and nothing is formatted with an invalid file.

The easiest way to write the file is the settings page: **Tools → TcTidy → Formatting
Settings…** in the XAE Shell, or `tctidy --settings` in the solution folder, which prints
a local URL to open. It lists every setting with its description and accepted values;
selecting one previews its effect on a short sample. Below it, an example function
block, or your own code pasted over it, is formatted with all the values on the page.
Saving writes only the settings that differ from the defaults. The page is served on
`127.0.0.1` only, behind a one-time token in the URL, and stops 10 minutes after it is
closed.

`tctidy --print-config` prints every setting with its effective value, grouped by
section: a complete starting point.

```sh
tctidy --print-config > .tctidy.toml
```

Every value keeps the formatter's guarantees: output stays idempotent and
meaning-preserving under each setting and under any combination of them.

## Contributing

Contributions are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) explains how to build,
test and open a pull request. [MAINTAINING.md](MAINTAINING.md) covers versions and
releases.

## License

Licensed under either of the [MIT license](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE), at your option.

TwinCAT is a trademark of Beckhoff Automation GmbH. TcTidy is not affiliated with or
endorsed by Beckhoff.

## AI disclosure

TcTidy is built with extensive help from AI coding assistants, and it would most
likely not exist without them: AI assistance is what made it possible to build a free,
open-source formatter for TwinCAT, and I hope it helps open up more of this field.

Contributions written with AI are welcome, and you don't need to mention it (no
"Assisted-by" or "Co-authored-by" trailer, no "generated with AI" note or anything
similar): AI is so widely used that flagging it makes no sense, and it changes nothing
about how a contribution is handled. With or without AI, you are responsible for what
you submit, and it gets the same review. Read and understand the change, test it, and
make sure it works and keeps TcTidy's guarantees (idempotent output, comments and
meaning preserved) before you open a pull request.
