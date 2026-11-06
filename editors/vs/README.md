# TcTidy for the TwinCAT XAE Shell

A TwinCAT XAE Shell extension that formats TwinCAT Structured Text by calling the
`tctidy` binary. It installs into TcXaeShell 15.0 (x86) and 17.0 (x64), with the
`TcTidySetup-<version>.exe` attached to each release. TcTidy is an independent project,
not affiliated with or endorsed by Beckhoff Automation.

## Why a command, not format-on-save

The TwinCAT ST editor is a custom editor, not a standard Visual Studio text editor, so it
exposes no text buffer to extensions: format-on-save is not feasible. The integration is
therefore a **manual "Format" command**. A POU
that is **open** in the editor is formatted in memory through the TwinCAT Automation
Interface (one undo step, no "file changed outside the environment, reload?" prompt);
files that are **not** open are formatted on disk in a single batched `tctidy` run.

## Features

- **Format with TcTidy** on the TwinCAT PLC source-object, folder and project context
  menus, and on the ST-editor menu. From the **Solution Explorer**, the selection resolves
  to its `.TcPOU`/`.TcGVL`/`.TcDUT`/`.TcIO` files (recursing folders and PLC projects).
  Each open POU is formatted in memory with **its methods, actions and properties**, as a
  single undo step with no reload prompt; closed files are formatted on disk in one
  `tctidy --files-from -` batch off the UI thread. From **inside a code editor**, only the
  open element (the POU body, or one method, action or property accessor) is formatted,
  never its siblings. Either way the open buffer is saved first, so unsaved on-screen
  edits are what gets formatted, and an element's declaration and implementation are
  always done together.
- **Error reporting**: a parse or format error leaves that file untouched and is reported
  without aborting the batch. A project-wide run shows progress and writes a per-file
  changed/unchanged/failed summary, with each failure's ST section and line, to the
  **TcTidy** Output pane; a parse error inside a child element is named against that
  element (for example `FB_Example.Init`). If `tctidy` cannot start, times out, stops on a
  configuration error or crashes, the report still counts what was formatted, open POUs
  included, and lists every file it could not confirm with the reason. When a run leaves
  any file untouched, a short warning dialog says how many failed and brings the Output
  pane to the front; a fully clean run stays silent.
- A **TcTidy** submenu under **Tools**: *Documentation*, *Report a Bug…* (opens the GitHub
  bug form with the extension version filled in), *Formatting Settings…* (see below),
  *Performance Report* (see below), *Check for Updates Automatically* and *Check for
  Updates* (see below), and *About TcTidy* (extension version and the commit it was built
  from, engine version, and the resolved `tctidy.exe` path).
- **Ctrl+K, Ctrl+T** formats the element open in the editor (see below).

`tctidy.exe` is located next to the extension (the installer puts it there), with a
`PATH` fallback. Like every command-table change, new menus and shortcuts appear after the
next shell restart.

## Formatting settings

**Tools → TcTidy → Formatting Settings…** opens a settings page in your default browser
for the open solution's `.tctidy.toml` (the nearest one from the solution folder up, or a
new one next to the `.sln`). The settings
are listed on the left and the selected one is previewed on the right, on a short ST
sample: the TcTidy default against your value, with the changed lines and words
highlighted. Below it, *Your code* shows an example function block, declaration above
implementation, as typed and beside it formatted with the settings on the page, what
formatting changed highlighted and the two sides scrolling together; edit it or paste
your own code over it, nothing there is saved. The page
follows the light or dark mode of Windows, and a switch in its top bar picks light or dark
instead. *Save* writes only the settings that differ from the defaults and keeps the
comments and layout of a hand-written file; if the file changed on disk meanwhile, the
page offers to reload it or overwrite it.

The page is served by the bundled `tctidy.exe` itself (`tctidy --settings`), on
`127.0.0.1` only, behind a one-time token, so nothing else on the machine or on the web
can read or change the settings. The server stops when the solution closes, when the
shell exits, or 10 minutes after the page was closed; choosing the menu entry again
reopens the running page. Format commands use the `.tctidy.toml` that applies to each
file, open or not: the same file the CLI and CI use.

## Keyboard shortcut

**Ctrl+K, Ctrl+T** ("Tidy") formats the element open in the editor: the same action as
*Format with TcTidy* on the ST-editor right-click menu. The binding is global (the TwinCAT
ST editor is not a VS text editor, so an editor-scoped binding would never fire there);
pressed outside an ST editor the command just shows its usual "open a POU" message. Rebind
it under *Tools → Options → Environment → Keyboard* (command `TcTidy.FormatElement`). Like
every command-table change, the shortcut appears after the next shell restart.

## Update check

When TcXaeShell starts, at most once a day, the extension checks in the background whether
a newer TcTidy release exists. If so, an info bar in the main window offers *Download*
(the release page, where the installer is) or *Skip this version*. The same bar can carry
a notice from the maintainers for some versions, such as a serious bug fixed in a later
release; *Don't show again* hides it for good. Nothing is downloaded or installed
automatically: you run the new installer yourself. With both shell generations installed,
setup updates one per run and preselects the one with an older TcTidy. *Tools → TcTidy →
Check for Updates* checks right away and also says when you are up to date, or why the check
failed.

The check is an anonymous request for a small file,
`https://vikodex.github.io/TcTidy/update.json`, or, if that fails, for the latest release
page on GitHub. It sends no identifier, version or project data. It uses the system proxy
with your Windows credentials. The automatic check silently ignores an offline machine or a
blocked request, and tries again at the next start.

To turn it off, untick *Tools → TcTidy → Check for Updates Automatically*. To turn it off
for every user of a machine, including the manual check, set this registry value (for
example by Group Policy):

```text
HKEY_LOCAL_MACHINE\SOFTWARE\Policies\ViKodex\TcTidy
    DisableUpdateCheck (DWORD) = 1
```

## Measuring performance (test mode)

The command can time the **real end-to-end cost** of a *Format with TcTidy* run: file
detection (the recursive folder/project walk), save, the per-file format loop, and the
total. It is **off by default**, so normal use is never affected.

### Enable

Check **Tools → TcTidy → Performance Report**. The setting is persisted (it survives shell
restarts) and takes effect on the **next** Format run with no relaunch needed, unlike the
menu/keybinding changes themselves, which only merge on a shell restart. Uncheck the same
entry to turn it off; each flip is confirmed in the status bar.

### Read the timings

Run *Format with TcTidy* on a POU (single file) or a folder / PLC project (recursive). In
test mode the **"TcTidy" Output pane is brought to the front** with the report after each run
(a normal run only surfaces it on failure), and the report is also appended, time-stamped,
to `%TEMP%\tctidy-perf.log`:

```
[YYYY-MM-DD HH:mm:ss] TcTidy perf, 100 file(s): 20 changed, 80 unchanged, 0 failed
  Resolve tctidy.exe     :       0.50 ms
  Detect files           :       5.00 ms   ← the recursive folder/project walk
  Detect open docs       :       1.00 ms
  Format                 :     100.00 ms   100 files
    Open via API         :      60.00 ms   2 files   30.00 ms/file   1 changed, 1 unchanged
    Disk batch           :      40.00 ms   98 files   0.41 ms/file   19 changed, 79 unchanged
  Total (end-to-end)     :     110.00 ms   ← click to last file written
```

`Format` is split into two paths. Files already open in the editor are formatted in memory
through the Automation Interface (`Open via API`, no reload prompt); the rest go through a
single batched `tctidy` invocation (`Disk batch`). Each sub-phase reports its own per-file
average and changed/unchanged split; the un-indented `Format` line shows the combined count
only, because a blended per-file average across two very different paths would mislead.
Batching the disk files into one spawn is what keeps `Format` cheap: process startup,
which dominates a per-file run on Windows, is paid once instead of once per file.

## Layout

| Project | Target | Role |
|---|---|---|
| `TcTidy.VsExtension.Core` | netstandard2.0 AnyCPU | Pure formatter core (`TctidyRunner`): shells out to `tctidy`, maps exit codes, no VS dependency. |
| `TcTidy.VsExtension.Core.Tests` | net8.0 (xUnit) | Tests the Core against the real `tctidy` binary. Runs on Linux and in CI, no VS or TwinCAT needed. |
| `TcTidy.VsExtension.Core.Benchmarks` | net8.0 (BenchmarkDotNet) | Measures the cost of spawning `tctidy` per file versus one batch; run by the manual `perf.yml` workflow. |
| `TcTidy.VsExtension` | net472/net48 AnyCPU (VSIX) | The thin VS shell: the commands and menus. Built separately for XAE Shell 15.0 x86 and 17.0 x64. Builds on Windows only. |

The Core is fully verifiable off Windows. Automated Windows gates build the VSIX, inspect
its payload, and compile installer fixtures for 15.0-only, 17.0-only, and coexistence
discovery. Those checks establish package compatibility, not successful execution inside
an installed XAE shell.

## Known limitations

- Running the commands inside an installed XAE Shell 15 or 17 is validated by hand, not
  by CI.
- The VSIX and the installer are not code-signed, so Windows SmartScreen reports an
  unknown publisher when you run the installer.

## Developing the Core

Point `TCTIDY_BIN` at a built `tctidy` and run the tests (needs the .NET 8 SDK; from
`editors/vs`):

```sh
cargo build -p tctidy-cli                      # produces target/debug/tctidy
export TCTIDY_BIN="$PWD/../../target/debug/tctidy"
dotnet test TcTidy.VsExtension.Core.Tests
```

CI (`ci.yml`) runs the same core tests on Linux and Windows for every pull request. The
manual `perf.yml` workflow also measures the extension's spawn cost.

## Building the extension and the installer

Prerequisites, on Windows: Visual Studio 2022 or its Build Tools (MSBuild, with the .NET
Framework 4.7.2 and 4.8 targeting packs), the .NET 8 SDK, Rust stable, and Inno Setup 6
for the installer. The `Microsoft.VSSDK.BuildTools` NuGet package replaces the "Visual
Studio extension development" workload. The release notices also need
[cargo-about](https://github.com/EmbarkStudios/cargo-about) 0.9.1.

`Cargo.toml` (`workspace.package.version`) is the only release-version source. Checked-in
release metadata is kept in sync by `cargo xtask release prepare` and verified by
`cargo xtask release check`. From the repository root:

```sh
cargo build -p tctidy-cli --release
cp target/release/tctidy.exe editors/vs/TcTidy.VsExtension/   # bundled in the VSIX
dotnet test editors/vs/TcTidy.VsExtension.Core.Tests           # the core, no VS needed
msbuild editors/vs/TcTidy.VsExtension -restore -p:Configuration=Release -p:XaeShellVersion=17
msbuild editors/vs/TcTidy.VsExtension -restore -p:Configuration=Release -p:XaeShellVersion=15
```

Distribution builds and CI use `scripts/release/build-vsix.ps1` to build the bundled CLI,
then restore, build, validate and extract both XAE 15 and 17 payloads with explicit
product-version and Cargo evidence inputs. `scripts/release/test-installer.ps1` runs the shared discovery,
security, upgrade and uninstall fixtures; the `Windows` job in `.github/workflows/ci.yml`
shows the full sequence. Releases are described in
[MAINTAINING.md](../../MAINTAINING.md).

The VSIX ships the project's `LICENSE-MIT` and `LICENSE-APACHE`, a short `LICENSE` that
the VSIX installer displays, `THIRD-PARTY-NOTICES.md` (the crates linked into
`tctidy.exe`) and `THIRD-PARTY-NOTICES-EXTENSION.md` (the NuGet packages the extension
redistributes), so they are installed in the extension's folder next to `tctidy.exe`.
*About TcTidy* shows that folder.

Numeric-only consumers receive `X.Y.Z` or `X.Y.Z.0`, while the assembly informational
version, About text, installed-product registration and installer metadata keep the full
SemVer.

To contribute, see [CONTRIBUTING.md](../../CONTRIBUTING.md).
