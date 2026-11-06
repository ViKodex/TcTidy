# TcTidy installer (TwinCAT XAE Shell)

`TcTidy.iss` builds the elevated Inno Setup package used to deploy TcTidy into a
TwinCAT XAE Shell all-users extension directory.

## Target discovery

The installer extracts its bundled `tctidy.exe` to a temporary directory and runs
`tctidy --discover-xae-shells` before showing the wizard. The helper resolves the
registered COM local server instead of guessing a `Program Files` path:

- TcXaeShell 17.0 from the 64-bit registry view (`TcXaeShell.DTE.17.0`);
- TcXaeShell 15.0 from the 32-bit registry view (`TcXaeShell.DTE.15.0`).

Setup aborts before copying files when discovery fails or no supported shell is
installed. A single instance is selected automatically. When both generations are
installed, the wizard shows what each one holds and asks which one to target. It
preselects the first of these, 17.0 first when both match:

1. a shell with an older TcTidy, one of unknown version, or the same number under another
   label, so that a test build replaces its release;
2. a shell without TcTidy;
3. a shell that already has this version;
4. 17.0, even though that downgrades its newer TcTidy.

Unattended installs follow the same order with 2 and 3 swapped, so that running the same
setup again stays in the shell that has TcTidy instead of spreading to the other one. To
name the target, pass `/SHELL=15.0` or `/SHELL=17.0`; setup fails if that shell is not
found. The target always comes from discovery, so `/DIR=` has no effect.

Setup updates one shell per run. The Ready page names the target and what happens to
its TcTidy. The last page names it again, and any other shell that still has an older
TcTidy, so that you can run setup again for that one. An unattended install writes the
same text to its `/LOG`.

## Installation and uninstall

For the selected instance, Setup:

1. copies the extracted VSIX payload and `tctidy.exe` into
   `Common7\IDE\Extensions\TcTidy`;
2. touches `Common7\IDE\extension.configurationchanged` and
   `Common7\IDE\Extensions\extensions.configurationchanged`, so the shell rebuilds its
   menus and re-merges every `.pkgdef` (registration changes such as a new autoload rule)
   at its next start;
3. resolves the physical-console user through Windows Management
   Instrumentation and removes only that user's selected-generation
   `ComponentModelCache`, best-effort.

15.0 and 17.0 have distinct AppIds, so both generations can coexist and are upgraded or
uninstalled independently. Uninstall derives the merge
marker from the installed extension path instead of reading target metadata from that
directory. A generation-specific, administrator-owned registry value binds uninstall
cache cleanup to the installed directory; its value is removed without deleting foreign
values in the private key. An upgrade overwrites the previous uninstall log
(`UninstallLogMode=overwrite`), so the log owns only the declared payload and uninstall
preserves unrelated files in that directory.

The all-users directory and merge marker require `PrivilegesRequired=admin`.

The payload includes the project's license files, generated Cargo notices and machine-readable
Cargo/NuGet inventories. Release validation derives the Rust inventory from the
target-specific `tctidy-cli` runtime tree and the redistributed assemblies from the built
VSIX plus restored NuGet assets; build/test-only packages and Visual Studio host
assemblies are not copied into the installer.

## Build

Requires Inno Setup 6 `ISCC`. Assemble `payload-xae15/` and `payload-xae17/` by
extracting the corresponding release VSIX files. Both must contain `tctidy.exe`.
Then run:

```sh
ISCC /DAppVersion=X.Y.Z-prerelease /DNumericVersion=X.Y.Z.0 /DPayload15Dir=<abs-xae15-payload> /DPayload17Dir=<abs-xae17-payload> editors/vs/installer/TcTidy.iss
```

Output: `editors/vs/installer/dist/TcTidySetup-X.Y.Z-prerelease.exe`.

`AppVersion`, `NumericVersion`, `Payload15Dir` and `Payload17Dir` default to
`0.0.0-dev`, `0.0.0.0`, `payload-xae15` and `payload-xae17`. Release builds must
pass the full and assembly values emitted by
`cargo run -p xtask -- vsix-version --workspace`.

In CI, the Windows job (`scripts/release/test-installer.ps1`) compiles 15.0-only,
17.0-only, and coexistence fixtures. It also validates normalized marker and cache
paths and the preselection rule, then exercises an isolated upgrade from an installer
whose uninstall log deletes the whole extension directory,
the release-shaped payload and license inventory, payload ownership, uninstall cleanup,
and foreign-file preservation without requiring TwinCAT on the CI runner. Finally it
runs unattended installs over fixtures of both generations, with real uninstall records
(`scripts/release/validate-installer-target.ps1`), to check which shell receives TcTidy.

## Test builds

Once its checks pass, the Windows job of every pull request and of every push to `main`
publishes a real installer, for trying the change by hand in TcXaeShell. The link is on
the Summary page of the CI run, and GitHub asks you to sign in to download it. It is kept
for 14 days.

Its version names where it comes from: `TcTidySetup-X.Y.Z-pr123-abc1234.exe` for pull
request 123 at commit `abc1234`, and `TcTidySetup-X.Y.Z-main-abc1234.exe` for `main`. On a
pull request, the build is that commit merged into its target branch, like the rest of
the run.

It installs like a release and replaces one of the same number. To go back, run the
release's installer.

A test build is not signed and setup runs as administrator. Pull requests can come from
anyone, so install one only after reading its code.
