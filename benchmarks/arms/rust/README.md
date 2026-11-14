# The `rust` arm

Rust with axum for the `backend` task family (RFC-0009 §1, §3). An episode writes one `.rs` file, the `src/main.rs` of a binary crate.
Nothing has run in this arm: no model episode, no score. The guide is
[`../../prompts/rust-guide.md`](../../prompts/rust-guide.md), written to the family's token budget
([`../../prompts/BUDGET.md`](../../prompts/BUDGET.md)).

| Part       | What it is                                                                                                                                                                                                                               |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `arm.toml` | the arm, in the format the runner README specifies ("Arms: `arm.toml`")                                                                                                                                                                  |
| check      | `check.sh <candidate>`: `cargo check --locked --offline --message-format=json`, printing each message's `rendered` field (`sandbox/run-check.sh`; needs `jq`, which the image installs). Exit 0 clean, 1 errors (printed), 2 setup error |
| serving    | `serve.sh <candidate>`: builds the candidate and execs its server on `$PORT`, for `mzprobe serve`                                                                                                                                        |
| sandbox    | `sandbox/`: a Linux container image, `mzizi-arm-rust`, built by `setup.sh` from `sandbox/Containerfile`                                                                                                                                  |
| fixtures   | `fixtures/b1-good.rs`, `b1-bad-type.rs`, `b1-bad-behaviour.rs`: see "Verifying the arm"                                                                                                                                                  |

`check.sh`, `serve.sh` and `setup.sh` are one-line calls to [`../lib/`](../lib): `arm-run.sh`
runs the sandbox, `build-image.sh` builds it, `probe.sh` probes a candidate with a task's
probes, and `self-test.sh` is the verification below.

## Serving

`sandbox/run-serve.sh` runs `cargo build --locked --offline` and execs the binary (a debug build: probing needs correctness, not speed). The candidate's `main` reads `PORT` and listens on `0.0.0.0`.

## Pinned versions

| Crate        | Pin                                                                                               | Why it is here                                                                                            |
| ------------ | ------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `axum`       | `=0.8.9`                                                                                          | the framework; the pin B1's Rust reference and `mz build`'s lowering use (`compiler/src/lower.rs` `PINS`) |
| `tokio`      | `=1.53.3`                                                                                         | the same                                                                                                  |
| `serde_json` | `=1.0.151`                                                                                        | the same                                                                                                  |
| `serde`      | `=1.0.229` (`derive`)                                                                             | RFC-0009 §1 says "serde"; the latest 1.x on 2026-10-29                                                    |
| `rust:1`     | digest `sha256:09027f873d45b8ecd1dc86f1fa13b7a62b875c961f017a0e331267b350430137` (`rustc 1.99.1`) | the toolchain                                                                                             |

`sandbox/Cargo.lock` is committed (the arm's own `sandbox/.gitignore` un-ignores it), 52 packages, and `--locked` fails rather than change it. `install.sh` fetches and compiles that tree once in the image, so a check recompiles only the candidate. The Cargo.lock is not in the `cargo deny` job: `supply-chain.yml` lists the manifests it checks, and this one is not among them.

## The sandbox

Run `setup.sh` once per machine, with network access: it builds the image, the only step
that fetches anything (RFC-0009 §4.5: caches warmed before the first episode). After that:

- **A check** runs `container run --network none`, with the candidate copied into a fresh
  directory mounted at `/work`. Two checks never share a slot, and nothing a candidate writes
  survives.
- **Serving** publishes `$PORT` on `127.0.0.1` only, which needs the container's default
  network, so serving is not `--network none`. Nothing in a candidate is told of the network
  and the image fetches nothing at run time, but this is a gap against RFC-0009 §4.5's "no
  network", stated here rather than hidden. The server is killed after `ARM_SERVE_TTL`
  seconds (300 by default): `mzprobe` stops a server with SIGKILL, which the container CLI
  cannot forward, so the TTL is what reaps it.
- **Without the `container` CLI**, or with `MZ_ARM_RUNTIME=local`, the same
  `sandbox/run-check.sh` and `run-serve.sh` run on this machine against the toolchain on
  `PATH` and what `sandbox/install.sh` installed into `sandbox/`. CI does not run any of it:
  no arm's toolchain is a CI requirement (RFC-0009 §9).

## Reading the check

What the author sees for `fixtures/b1-bad-type.rs`, from a real run:

```text
error[E0308]: mismatched types
 --> b1-bad-type.rs:6:22
  |
6 |     let count: u32 = "2";
  |                ---   ^^^ expected `u32`, found `&str`
  |                |
  |                expected due to this


For more information about this error, try `rustc --explain E0308`.
```

## Verifying the arm

`../lib/self-test.sh rust` checks the arm against B1 (`benchmarks/tasks/b1-routing`), with
three files written for it. None is shown to an author or a model.

| Fixture               | Expected                             | Result, 2026-10-29                     |
| --------------------- | ------------------------------------ | -------------------------------------- |
| `b1-good.rs`          | check exits 0, and every probe holds | check 0; `mzprobe` 59 of 59 facts held |
| `b1-bad-type.rs`      | check exits 1                        | check 1, output above                  |
| `b1-bad-behaviour.rs` | check exits 0, probes fail           | check 0; 31 of 59 facts failed         |

`b1-good` is an implementation of B1's `spec.md` in this arm's language, written by hand and
checked only by B1's own probes. That the probes pass it shows the arm builds, serves and is
probed correctly. It says nothing about how well a model would write B1 in this language.

## What this arm does not do

- **It is not scored by the runner yet.** `extractor = "none"`, as for `mzizi-be`: a clean
  episode is clean but unscored until the runner calls the probe crate.
- **No episode has run.** Phase 0 has not passed its kill criterion
  ([`../../READINESS.md`](../../READINESS.md)), and this arm is the comparison side of that
  criterion, not a result.
- This is the Rust arm RFC-0009 §1 calls `rust`; it shares its pins with the Mzizi lowering on purpose, so Mzizi and this arm run on the same axum.
