# benchmarks/perf: a performance suite against hand-written Rust

This suite measures how fast the Rust that `mz` lowers a `program` to runs, beside two
hand-written Rust versions of the same program. It is not the kill-criterion benchmark
([`../READINESS.md`](../READINESS.md)), which measures whether models write Mzizi well. It
measures the compiler's output, on one machine at a time.

## The honesty rule

The numbers are measurements on a stated machine. They are never a claim that Mzizi is faster
or slower than Rust in general, and nothing in this repository may quote them as one
([`AGENTS.md`](../../AGENTS.md), "The one rule that overrides the others"). A table from this
suite always names the machine, the `rustc` version and the commit, which `run.sh` writes into
its output. Results are not committed here. CI checks that the programs build and agree, and
times nothing, because shared CI runners are too noisy to gate on. Timing runs and their
history are meant to live in `mzizi-dev/mzizi-benchmarks` (private), which checks out this
repository; this repository never reads that one (RFC-0004).

**The owner's target is a goal, not a result** (issue #69, 2026-10-07): "Keep the overflow
checks and remove them where the compiler can prove they are unnecessary — safer than Rust with
speed recovery where possible." Mzizi's `int` arithmetic traps on overflow in every build
(RFC-0013 §4.3). The goal is checked semantics at the speed of Rust's unchecked release
default wherever the compiler can prove a check cannot fire. No such pass exists yet: today
every `+`, `-`, `*`, `/`, `%` and unary `-` on `int` is checked. This suite is how a pass
like that will be measured.

## What is compared

For each program in [`programs/`](programs/), three binaries:

| Variant          | What it is                                                                                                                                                                                                       |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mzizi`          | `mz build programs/<name>.mz --out …`, then `cargo build --release --offline`. Every `int` operation is a call to a checked helper that traps (exit 101), and the package sets `overflow-checks = true` as well. |
| `rust_unchecked` | [`rust/src/bin/<name>.rs`](rust/src/bin/), built with `cargo build --release`: Rust's release defaults, so overflow wraps, unchecked.                                                                            |
| `rust_checked`   | The same source, built with `--profile release-checked` ([`rust/Cargo.toml`](rust/Cargo.toml)): `release` plus `overflow-checks = true`, so overflow panics (exit 101), the same safety as Mzizi.                |

The Rust references are hand-written, std only, and follow the Mzizi program's algorithm,
recursion included, because the foundation slice has no loops (RFC-0013 §18.1); a loop would
measure a different program. Each runs its work on a thread with a 64 MiB stack, as the
lowered Mzizi program does. The two Rust variants differ only in the one profile setting.

| Program       | What it stresses                                                                                    |
| ------------- | --------------------------------------------------------------------------------------------------- |
| `fib`         | Naive recursive `fib(38)`: calls, a comparison and three `int` operations per call.                 |
| `ackermann`   | `ack(3, 0)` to `ack(3, 10)`: deep, irregular recursion (about 8,000 frames), one `+` or `-` a call. |
| `collatz`     | Collatz step counts for 1 to 300,000, by recursion: `*`, `+`, `/` and `%` on every step.            |
| `interpolate` | 200,000 labels built by `{…}` interpolation and compared with `is`: allocation and formatting.      |

Each program prints a few lines ending in a checksum, and its exact output is committed as
`programs/<name>.expected`. All three variants must print it byte for byte.

## How to run it

From the repository root, with `cargo` on the path:

```sh
benchmarks/perf/run.sh                     # build, check output, measure; 7 runs each
benchmarks/perf/run.sh --runs 15 fib       # one program, more runs
benchmarks/perf/run.sh --check-only        # build and compare output only (what CI runs)
benchmarks/perf/run.sh --out /some/dir     # write perf.json and perf.md there
```

It builds `mz` in release (or uses `MZ=<path>`, as CI does with its debug build, since
`--check-only` times nothing), then everything else with `--offline`: every package is std
only, so nothing is downloaded. Each build gets its own empty target directory, so a
`CARGO_TARGET_DIR` setting does not move it. Output goes to `target/perf/` by default
(`perf.json`, `perf.md`, and the build directories under `build/`). It needs Linux tools:
bash 4 or later, GNU `date` (for `%N`) and coreutils; it stops with exit 2 without them.

The exit status is 0 when every program's three outputs match, 1 when any differs or a
binary exits non-zero (such a program is reported and not timed), 2 on a usage error or a
missing tool, and 3 when a build fails.

A new program needs three files: `programs/<name>.mz`, `programs/<name>.expected` and
`rust/src/bin/<name>.rs`. Cargo finds the binary by its file name, so no manifest changes.

## What is measured

| Measure     | How                                                                                                                                                                                        |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Wall time   | One warm-up run of each variant, then N rounds (default 7), the three variants interleaved in an order that rotates each round. The median, and every run, in milliseconds (`date +%s%N`). |
| Max RSS     | One more run under `/usr/bin/time -f %M`, in KiB. `null` when GNU `time` is not installed.                                                                                                 |
| Binary size | The release binary's size in bytes, unstripped.                                                                                                                                            |
| Build time  | `cargo build --offline` in the variant's profile from an empty target directory: a cold build of one binary.                                                                               |
| `mz check`  | The median of N runs of `mz check programs/<name>.mz`, process start included.                                                                                                             |
| Machine     | CPU model, online cores, `uname -srm`, `rustc --version`, `cargo --version`, the commit and the date.                                                                                      |

Wall time includes process start and the 64 MiB thread spawn, about a millisecond. The
programs are sized to run for hundreds of milliseconds so that this stays small. `mz build`'s
own time is not counted in the Mzizi build time; `mz check` stands for the front end.

## Output: `perf.json`

One JSON document, schema `mzizi-perf/1`:

```json
{
  "schema": "mzizi-perf/1",
  "mode": "measure",
  "runs": 7,
  "date": "2026-10-07T22:28:09Z",
  "commit": "6e235eced4e5",
  "machine": {
    "cpu": "…",
    "cores": 4,
    "os": "Linux … x86_64",
    "rustc": "rustc 1.97.0 (…)",
    "cargo": "cargo 1.97.0 (…)",
    "max_rss_tool": "/usr/bin/time"
  },
  "programs": [
    {
      "name": "fib",
      "output_identical": true,
      "mz_check_ms": 4.16,
      "variants": {
        "mzizi": {
          "wall_ms_median": 261.07,
          "wall_ms_runs": [261.07, "…"],
          "max_rss_kb": 2432,
          "binary_bytes": 479632,
          "build_ms": 433.75
        },
        "rust_unchecked": { "…": "the same fields" },
        "rust_checked": { "…": "the same fields" }
      }
    }
  ]
}
```

`mode` is `measure` or `check-only`. In `check-only` mode `runs` and `mz_check_ms` are `null`,
and each variant has only `build_ms`; so does a program whose `output_identical` is `false`,
in either mode, because it is not timed. `max_rss_kb` and `machine.max_rss_tool` are `null`
without GNU `time`. Every time is in milliseconds, to two decimals.

`perf.md` is the same results as one Markdown table, headed by the machine line. It adds two
ratios: Mzizi's median wall time divided by each Rust variant's.
