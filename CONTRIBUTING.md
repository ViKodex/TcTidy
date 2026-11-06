# Contributing to TcTidy

Thanks for helping! TcTidy's promise is that it never destroys code: the output is
idempotent, and comments and semantics are preserved. Every change keeps that promise,
and a test shows it.

## Before you start

- **Small fixes** (a bug, a typo, a doc tweak): open a pull request directly.
- **New features, new settings or formatting style changes**: open an issue first, to
  agree on the approach before you write code.

## Build and test

You need the latest stable Rust. [ARCHITECTURE.md](ARCHITECTURE.md) explains how the
workspace is organized. Before you push, run:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI also runs `cargo xtask release check` (versions and CHANGELOG format) and
`cargo run -p xtask -- keywords --check` (the generated keyword table).

Formatter output is covered by [insta](https://insta.rs) snapshot tests; review changed
snapshots with `cargo insta review` (`cargo install cargo-insta`).

The corpus tests check idempotence, comment preservation and canonicalization on real
TwinCAT code. They skip without a corpus; fetch the public one (open-source TwinCAT
projects pinned in `xtask/public-corpus.txt`) once, then run them:

```sh
cargo xtask corpus fetch
cargo test -p tctidy --test corpus_invariants --test corpus_comments --test canonicalization
```

To run them on your own code instead, point `TCTIDY_CORPUS` at any folder of TwinCAT
files, for example your project: `TCTIDY_CORPUS=/path/to/project cargo test -p tctidy`.
Nothing is written to that folder.

The XAE Shell extension has its own prerequisites: the .NET 8 SDK for the Core tests
(any OS), and Windows with the Visual Studio 2022 Build Tools and Inno Setup 6 for the
extension and the installer. See [editors/vs/README.md](editors/vs/README.md). Changing
a Rust dependency also needs [cargo-about](https://github.com/EmbarkStudios/cargo-about)
0.9.1 to regenerate the license notices (`cargo xtask licenses generate`).

[lefthook](https://lefthook.dev) can run the checks for you on commit and push
(`lefthook install`); it is optional. Its hooks also reject the em dash character
(U+2014) in staged changes and commit messages: the project keeps punctuation plain.

## Pull requests

- **Title**: it becomes the commit on `main`, so it follows
  [Conventional Commits](https://www.conventionalcommits.org): `type(scope): subject`,
  for example `fix(format): keep comments before ELSE`. The usual types are `feat`,
  `fix`, `perf`, `docs`, `test`, `refactor`, `build`, `ci` and `chore`. The scope is
  optional; when present it is one of `syntax`, `pretty`, `format`, `twincat`, `cli`,
  `lib`, `vsix`, `installer`, `ci`, `release`, `deps` or `docs`. Add `!` for a
  breaking change (`feat(cli)!: ...`). A bot comments if the title needs fixing.
- **Changelog**: for a `feat`, `fix`, `perf` or breaking change, add one line under
  `## [Unreleased]` in [CHANGELOG.md](CHANGELOG.md), in the right section (`Added`,
  `Changed` or `Fixed`), written for users. A check reminds you if it is missing.
- **Tests**: add a test that fails without your change (docs and tooling changes
  excepted).
- Tick **Allow edits from maintainers**, and push as many commits as you need: they are
  squashed into one on merge.

CI must be green before a merge. For your first contribution, a maintainer approves the
CI run.

## License

Unless you explicitly state otherwise, any contribution you intentionally submit for
inclusion in TcTidy, as defined in the Apache-2.0 license, is licensed under both the
MIT license and the Apache-2.0 license, without any additional terms or conditions.

## Security issues

Please don't open a public issue; see [SECURITY.md](SECURITY.md).
