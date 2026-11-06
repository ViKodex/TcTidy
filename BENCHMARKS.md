# Benchmarking TcTidy

TcTidy's promise is non-destruction, not speed, but the formatter should stay fast,
and any optimization is *measured first*. This document is the entry point for the
performance suite.

> **Read this first.** The committed synthetic fixtures are a **tripwire, not
> optimization targets.** Hand-authored ST does not match the shape distribution of
> real TwinCAT code. Optimize against real files from a corpus (below).

## What is measured

### In-process pipeline stages (`criterion`)

`cargo bench -p tctidy` runs `crates/tctidy/benches/pipeline.rs`. Each stage is timed
as far in isolation as the public API allows, plus the two end-to-end paths:

| Group          | Times                                       | Stage(s)                       |
|----------------|---------------------------------------------|--------------------------------|
| `lex`          | `tokens::lex`                               | lexer                          |
| `parse`        | `parse_declaration` / `_implementation`     | lex + parse                    |
| `format`       | `format_declaration` / `_implementation`    | attach + lower + print         |
| `format_st`    | `format_st`                                 | end-to-end raw ST              |
| `xml_locate`   | `locate_st_spans`                           | CDATA discovery                |
| `xml_reformat` | `reformat_document` (`identity` vs `real`)  | splice (`real − identity`)     |
| `format_file`  | `format_twincat_file`                       | end-to-end TwinCAT file        |

Throughput rows report MB/s automatically (`Throughput::Bytes`).

**Derived figures are point estimates, not confidence intervals**: subtracting two
noisy measurements compounds their variance.

- `parse − lex` ≈ parsing alone (lex is included in `parse`).
- `format − parse` is **not** valid (`format` takes an already-parsed tree); the
  `format` group itself is the post-parse cost.
- `xml_reformat/real − xml_reformat/identity` ≈ the parse+format of every span;
  `identity` alone is the locate+splice overhead.

`attach`, `lower`, and `print` are measured together as the `format` group: the
trivia module is private and there is no `Doc`-returning seam to time them apart.

### Per-file process cost / cold start (`hyperfine`)

`criterion` times the formatter in-process. It cannot see the per-file process cost
(launching `tctidy` **once per file**) that pre-commit and the extension's open-document
path pay, and that the extension's `Disk batch` amortizes into a single spawn.

```sh
cargo install hyperfine          # one-time
cargo run -p xtask -- perf cold  # writes target/perf/cold.md
```

- `version` = `tctidy --version`: the process-launch floor for the binary being
  measured. Runtime linkage depends on the target and build: the Linux GNU release
  artifact uses the platform's dynamic C runtime, while the Windows release artifact
  is built with a static CRT.
- `tiny` / `medium` = `tctidy <file>`: marginal format cost on top of that floor.
- `tiny − version` shows how much **startup dominates** the editor's per-file model:
  the motivation for formatting many files in **one** invocation.

**On Windows** (the extension's real platform, where process spawn is materially
slower), run the equivalent by hand on a dev box:

```powershell
hyperfine --warmup 5 -N "tctidy.exe --version" "tctidy.exe path\to\sample.TcPOU"
```

### End-to-end inside the XAE Shell extension

The harnesses above time the formatter; they do not time the *extension's* real flow,
from the **Format with TcTidy** click to the last file written, including the recursive
**file detection** of a selected folder/PLC project. That can only be measured inside the
running extension, so the extension instruments itself, opt-in:

1. Check **Tools → TcTidy → Performance Report** ("test mode"). It is persisted, needs
   no relaunch, and takes effect on the next run.
   Uncheck it for normal use.
2. Run **Format with TcTidy** on a POU (single file) or right-click a folder / PLC project
   (recursive). Both modes are covered by the same instrumented command.
3. Read the breakdown in **View → Output → "TcTidy"** pane, and in `%TEMP%\tctidy-perf.log`:

   ```
   [YYYY-MM-DD HH:mm:ss] TcTidy perf, 47 file(s): 12 changed, 35 unchanged, 0 failed
     Resolve tctidy.exe     :       1.20 ms
     Detect files           :      18.50 ms
     Detect open docs       :       2.30 ms
     Format                 :     200.00 ms   47 files
       Open via API         :      40.00 ms   2 files   20.00 ms/file   1 changed, 1 unchanged
       Disk batch           :     160.00 ms   45 files   3.56 ms/file   11 changed, 34 unchanged
     Total (end-to-end)     :     225.00 ms
   ```

   `Detect files` is the folder/project recursion; `Format` splits into `Open via API`
   (editor documents formatted in memory) and `Disk batch` (the rest formatted in a single
   batched `tctidy` spawn, the spawn-once design `SpawnBenchmarks` validates); `Total` is the
   real click-to-done wall-clock. Off by default, so it never appears in normal use.

## Running

```sh
# Full suite (HTML report under target/criterion/report/index.html)
cargo bench -p tctidy

# One group, quick (directional, not a gate)
cargo bench -p tctidy --bench pipeline -- --quick lex

# Compile-only (catches a broken bench without running it)
cargo bench -p tctidy --no-run
```

### Against real files (the surface to optimize)

```sh
cargo xtask corpus fetch                         # the public corpus, once
cargo bench -p tctidy
TCTIDY_CORPUS=/path/to/project cargo bench -p tctidy   # or any folder of TwinCAT files
```

With a corpus (`TCTIDY_CORPUS`, else the fetched public corpus), the suite adds the
smallest, median, and largest `.Tc*` files under it as `corpus_*` cases (and the
largest file's real ST spans feed the ST-level groups). This is the representative
signal; synthetic numbers are not.

## Comparing two runs (A/B)

```sh
# baseline on main
git switch main && cargo bench -p tctidy -- --save-baseline main

# your change
git switch my-branch && cargo bench -p tctidy -- --baseline main   # prints %Δ + significance

# compact table
cargo install critcmp
cargo bench -p tctidy -- --save-baseline feat
critcmp main feat
```

## Caveat: hosted-runner noise

GitHub-hosted runner timings are noisy (Windows more so), so **CI never gates on perf**.
The manual `Perf` workflow compiles the benchmark and runs the committed synthetic
panel on a hosted runner. It does not gate or comment on pull requests. Treat it as a
tripwire for accidental algorithmic regressions (for example, O(n²)), not as a precise
measurement. Real numbers come from a quiet local machine against a corpus.
