# Maintaining TcTidy

Notes for maintainers: merging, versions, releases and dependencies.
Contributors only need [CONTRIBUTING.md](CONTRIBUTING.md).

## Merging pull requests

- Review, then **Squash and merge**. The PR title becomes the commit on `main`.
- Reword a clumsy CHANGELOG line directly in the PR; contributors allow edits.
- A `fix` that users cannot see (CI, release tooling) needs no entry: add the
  `no-changelog` label and the Changelog check passes.

## Versions

TcTidy has one product version for the CLI, the VSIX and the installer:
`[workspace.package].version` in the root `Cargo.toml`. `cargo xtask release prepare`
writes it everywhere else; never edit the derived versions by hand.

Before 1.0:

- **minor** (`0.X.0`): new features, formatting style changes and breaking changes;
- **patch** (`0.x.Y`): fixes.

A patch release never changes the output for code that is already formatted, except to
fix a bug that loses or alters code. Style changes ship in minor releases and are listed
under `Changed` in the CHANGELOG.

A published version, tag or asset set is never changed. A correction is a new patch
release.

## Releasing

Release when `Unreleased` holds something worth shipping; there is no schedule.

1. If the extension changed, you can first try it in TcXaeShell with a locally built
   installer: `cargo xtask licenses generate`, then `build-vsix.ps1`, which also builds the
   CLI it bundles, then `ISCC` with both payloads (see
   [the installer README](editors/vs/installer/README.md#build)). The CI Windows job shows
   the steps, and also runs `test-installer.ps1`, whose security fixture asks for elevation.
2. Prepare the release on a branch:

   ```sh
   git switch -c chore/release-X.Y.Z
   cargo xtask release prepare X.Y.Z
   ```

   This updates every version, turns `Unreleased` into a dated section and advances the
   comparison links. Review the diff, commit it, and open the PR
   `chore(release): vX.Y.Z`. Merge it once CI is green.
3. Tag the merged commit on `main`:

   ```sh
   git switch main
   git pull
   git tag -a vX.Y.Z -m "TcTidy vX.Y.Z"
   git push origin vX.Y.Z
   ```

4. `release.yml` does the rest: it reruns the tests, builds the Linux and Windows
   archives and the installer, checks every asset and publishes the GitHub Release with
   the CHANGELOG section. It then starts `update-manifest.yml`, which tells installed
   extensions about the release (see below).

If `release.yml` fails, nothing is published. Fix the cause with a normal PR, then move
the tag to the new `main` commit (`git tag -d vX.Y.Z`, `git push origin :refs/tags/vX.Y.Z`,
and tag again).

To reproduce a release build locally, run the scripts that `release.yml` calls
(`scripts/release/build-linux.sh`, `build-windows.ps1` and `validate-candidate.sh`) with
the same arguments.

## Update manifest

Installed extensions read `https://vikodex.github.io/TcTidy/update.json` to learn about
new releases (see [Update check](editors/vs/README.md#update-check)). `update-manifest.yml`
builds it from the latest GitHub Release and `.github/update-notices.json`, and deploys it
to GitHub Pages: after each release, whenever the notices change on `main`, and on demand
(*Run workflow*). If that URL stops answering, extensions fall back on the
`releases/latest` redirect of the repository: they still see new versions, but no
notices.

Every released extension reads this URL and format, so keep both working: fields may be
added, never renamed or removed, and `schema` stays `1`.

To warn the users of some versions, for example about a bug that loses code, add a notice
to `.github/update-notices.json` in a PR:

```json
[
  {
    "id": "unique-id",
    "text": "One sentence for the info bar.",
    "url": "https://github.com/ViKodex/TcTidy/issues/N",
    "severity": "warning",
    "from": "X.Y.Z",
    "before": "X.Y.Z"
  }
]
```

`url`, `severity` (`warning` or `info`, the default), `from` (first affected version) and
`before` (first fixed version) are optional. Users dismiss a notice by its `id`, so give a
new notice a new `id`, and remove notices once they no longer matter.

## Dependencies

Dependabot opens one grouped PR a month for Cargo and one for GitHub Actions, and skips
versions published less than 7 days ago. Cargo minor and patch updates merge on their own
once CI is green (`dependabot-automerge.yml`, which does nothing while `main` is
unprotected). GitHub Actions updates and major updates wait for review.

A dependency whose license is not accepted in `about.toml` fails CI on purpose. Review
the license, then add it to `about.toml` or drop the dependency.

## Tooling

| Command | Purpose |
|---------|---------|
| `cargo xtask release check` | Check that every derived version matches `Cargo.toml` |
| `cargo xtask release prepare X.Y.Z` | Prepare a release (versions, CHANGELOG, links) |
| `cargo xtask licenses generate` | Generate the dependency inventory and third-party notices |
| `cargo xtask vsix check` | Check shell targets, archive paths and managed architecture |
| `cargo run -p xtask -- keywords --check` | Check the generated keyword table |

Use `--help` after a command to see its inputs. `.cargo/config.toml` makes `cargo xtask`
work in a clean clone.

Line coverage is measured with `cargo-llvm-cov`, with the public corpus fetched
(`cargo xtask corpus fetch`):

```sh
cargo llvm-cov --workspace --exclude xtask --ignore-filename-regex '[/\\]debug\.rs$'
```
