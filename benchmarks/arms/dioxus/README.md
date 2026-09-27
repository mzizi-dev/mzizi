# Dioxus arm

The raw-Dioxus side of the Phase 0 benchmark ([`CHARTER.md`](../../../CHARTER.md) §4, §6;
[`MIGRATION.md`](../../../MIGRATION.md) §4.2). An authoring agent ports a registry `.tsx`
component to Dioxus Rust; `check.sh` is its compile check, the counterpart of `mz check`
on the Mzizi arm. The authoring agent's guide is
[`../../prompts/dioxus-guide.md`](../../prompts/dioxus-guide.md).

The arm has to be fair. A Dioxus arm that fails on version drift or a broken sandbox
would flatter Mzizi, which is the dangerous direction for a kill criterion. So the
sandbox reproduces exactly what the registry's own Rust references compile against.

## Pinned version: Dioxus `=0.7.10`

Source: `mzizi-dev/mzizi-registry` at commit `3afeb752253a86b5488867078c2238d2cf62b4f0`.

- `mzizi-rs/crates/mzizi-ui/Cargo.toml` (the crate that compiles the N2 references)
  declares `dioxus` at `version = "0.7"` with `default-features = false` and the
  features `macro`, `signals`, `hooks` and `html`.
- `mzizi-rs/Cargo.lock` resolves that to `dioxus 0.7.10`, and every `dioxus-*` crate
  to `0.7.10` as well.
- `mzizi-rs/Cargo.toml` sets `edition = "2024"`.

`sandbox/Cargo.toml` pins `dioxus = "=0.7.10"` with the same four features and no
renderer, at edition 2024. `sandbox/Cargo.lock` was seeded by copying the registry's
lockfile and letting Cargo prune it to this crate's dependency graph. Every one of the
92 dependency entries left has the same version as in the registry lock; the only new
entry is the sandbox package itself. `check.sh` runs with `--locked`, so the lockfile
is never rewritten.

## Support stubs: none

`button.rs`, `badge.rs` and `card.rs` import only `dioxus::prelude::*`. The registry
keeps each component self-contained on purpose: no shared `cn()`, no internal prelude,
no tokens import (`mzizi-ui/src/lib.rs`). Nothing is vendored or stubbed.

## Candidate → crate mapping

```
sandbox/
  Cargo.toml       dioxus =0.7.10, own [workspace]
  Cargo.lock       pruned copy of the registry lock
  src/lib.rs       `pub mod component;` and nothing else
  src/component.rs the candidate, written by check.sh, removed on exit (gitignored)
```

The candidate becomes a module, the same way `mzizi-ui/src/lib.rs` mounts each
registry file (`#[path = "generated/button.rs"] pub mod button;`). So a reference file
with its `//!` header compiles here byte for byte, as it does in the registry.

## `check.sh <candidate.rs>`

| Exit | Meaning                                                                                                                          |
| ---- | -------------------------------------------------------------------------------------------------------------------------------- |
| 0    | Compiles with no errors. Warnings are printed and allowed                                                                        |
| 1    | Compile errors, printed to stdout                                                                                                |
| 2    | Usage or setup error (bad arguments, missing tool, or Cargo failing with no diagnostic against the candidate); message on stderr |

**Diagnostic format.** The script runs `cargo check --message-format=json` and prints
each diagnostic's `rendered` field, keeping only diagnostics for the sandbox crate.
That field is rustc's normal human output with no colour, `help:` and `note:` lines
included. It leaves out Cargo's `Checking …` and `Finished … in 0.2s` lines, which
change from run to run. `--message-format=short` was rejected because it drops the
`help:` suggestions (`a type alias with a similar name exists: Element`). The Mzizi
arm's `mz check` gives exact fixes, so a Dioxus arm without rustc's suggestions would
be handicapped. Two rewrites make the output machine-independent: `src/component.rs`
is shown under the candidate's file name (line numbers are the candidate's, since the
file is copied verbatim), and Cargo registry paths become `<cargo-registry>/`.
Checking the same broken file twice gave byte-identical output.

**Isolation.** Each run takes an exclusive `flock` on `sandbox/target.lock`, so
concurrent calls queue instead of overwriting each other's candidate. It deletes
everything in `src/` except `lib.rs`, writes the candidate with a fresh mtime so
Cargo's fingerprint always sees new contents, and removes it on exit. `RUSTFLAGS`,
`CARGO_ENCODED_RUSTFLAGS` and `CARGO_BUILD_RUSTFLAGS` are cleared, and
`CARGO_TARGET_DIR` is forced to `sandbox/target`, so the caller's environment cannot
change the check. For real parallelism, give each worker its own copy of this
directory.

**Workspace.** The sandbox declares an empty `[workspace]`, so it is its own root, and
the repository `Cargo.toml` also lists `benchmarks/arms/dioxus` under `exclude`. CI's
`cargo … --workspace` never builds Dioxus.

## Measured

Measured on a 4-core container with rustc 1.94.1. "Cold" means an empty `target/`;
the crate sources were already downloaded into `~/.cargo/registry`, so fetch time is
not included.

| Run                                   | Result | Time            |
| ------------------------------------- | ------ | --------------- |
| `button.rs`, cold (two runs)          | exit 0 | 12.8 s / 17.5 s |
| `button.rs`, warm                     | exit 0 | 0.18–0.20 s     |
| `badge.rs`, warm                      | exit 0 | 0.27 s          |
| `card.rs`, warm                       | exit 0 | 0.34–0.39 s     |
| `badge.rs` with two injected errors   | exit 1 | —               |
| No arguments; two arguments; bad path | exit 2 | —               |

The three references print no diagnostics at all. The broken badge (`Elemnt` for
`Element`, `slugg()` for `slug()`) printed three `error[…]` blocks, each with its
`help:` suggestion. The `Elemnt` error shows up twice, because the `Props` derive
re-emits the field. That duplication is Dioxus's real behaviour, and an authoring agent
on this arm will see it too. A candidate with an unused variable exited 0 with the
warning printed, on both runs.

`sandbox/target/` takes 222 MB after a check and is gitignored.
