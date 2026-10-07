# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

AGENTS.md (imported above) is the source of truth for the CI command block, commit and merge
conventions, the changelog rule, repo boundaries and naming. This file adds only what is
missing there or specific to Claude Code. Where an RFC and the code disagree, the code wins.

## Branches and pull requests

- `staging` is the integration branch: branch from `origin/staging` and target it with pull
  requests. `main` is updated by release PRs from `staging`
  (`chore(release): staging to main`). Dependabot also targets `staging`.
- CI (`ci.yml`, `changelog.yml`, `supply-chain.yml`) runs on pull requests into `main`,
  `staging` and `claude/**`, so a PR stacked on another `claude/` branch still gets checks.
- Keep branches linear (rebase or cherry-pick, never merge a base branch in): PRs are
  rebase-merged one commit at a time.
- Every PR body that changes something user-visible needs a **Site/docs impact** heading
  (`None.` when nothing changed). A PR that changes behaviour, diagnostics, the language, an
  RFC, the charter or the benchmarks adds a `CHANGELOG.md` entry under `## [Unreleased]`, or
  is labelled `no-changelog`; `.github/scripts/changelog-entry.sh` is the check. A PR that
  changes what the language can do also updates its row in `LANGUAGE-TRACKER.md`.

## Release and versioning flow

The version lives in git tags only; nothing is committed back and every crate stays at
`version = "0.0.0"`.

- A push to `staging` runs `staging-version.yml` (the org's reusable staging release), which
  tags the next **patch**.
- A push to `main` runs `CI`; when it succeeds, `main-release.yml` (a `workflow_run`
  trigger) tags the next **minor** and creates a GitHub release. A **major** is only ever
  made by hand through that workflow's `workflow_dispatch`.
- A push to `main` also runs `mzizi-lang-benchmark-dispatch.yml`, which notifies the private
  held-out benchmark repo. It never fails the build and reports "not configured" when its
  variable and secret are unset.

## Running a subset of tests

The CI block in AGENTS.md is the full gate. For a faster loop (verified):

```bash
cd compiler
cargo test --test fix                               # one integration test file (compiler/tests/fix.rs)
cargo test --test fix the_guides_three              # one test, filtered by name
cargo test --lib                                    # unit tests inside compiler/src only
cd ..
cargo test -p mzizi-benchmark-probe                 # one workspace member, from the repo root
```

`cargo` run from inside `compiler/` targets only that member; run from the root it needs
`--workspace` or `-p`. Formatting and clippy are gated twice: per-crate in `compiler/` and
`--workspace` from the root, so run both before pushing.

## Architecture

**The compiler pipeline** (`compiler/src/`, a zero-dependency library plus the `mz` binary in
`main.rs`; the `//!` header of each module names the RFC it implements):

1. `lex.rs`: newlines are tokens and blocks close with `end <kind> <name>`, which is what
   lets `parse.rs` recover per line and report one diagnostic per real error.
2. `parse.rs` (+ `parse/service.rs`) builds a `Program`: either a `component` (`ast.rs`) or a
   `service` (`service.rs`), one top-level declaration per file.
3. Checking: components go through `resolve.rs` (names and types, RFC-0008); services go
   through `service::check` (RFC-0011). Resolution runs even after parse errors, and
   `lib.rs::front_end_program` de-duplicates overlapping lexer/resolver diagnostics.
4. `diagnostic.rs` defines the human and `--agent` NDJSON output and the `exact`/`guess`
   fixes that `apply_exact_fixes` (`mz fix`) applies.
5. Back ends from the same front end: `contract.rs` evaluates a component's `contract` block
   against its own declarations; `serve.rs` runs a service's handlers in process for
   `mz contract`; `ir.rs` + `hash.rs` (hand-rolled SHA-256) build the content-addressed IR
   for `mz ir` / `mz hash`; `outline.rs` prints the interface only; `lower.rs` writes an
   axum package for `mz build`.

Exit status is the interface: 0 clean (warnings allowed), 1 errors or a failed contract
assertion, 2 usage or I/O. `mz` takes exactly one file.

**Generated vs hand-written.**

- `mz build` output (e.g. `target/mz-build/registry/`) is generated: `lower.rs` emits a fixed
  runtime (`RUNTIME`, verbatim), a generated route table, and one `#[tokio::test]` per
  `example` clause, so lowering is tested against the same clauses `mz contract` runs. Never
  edit the output; change `lower.rs`.
- `lower.rs`'s `PINS` (exact `axum`, `tokio`, ...) must stay in step with
  `benchmarks/tasks/b1-routing/reference-rust/Cargo.toml`. CI's `lowering` job builds both and
  `mzprobe verify` checks B1's probes against the Mzizi reference (lowered by `mz build`) and
  the plain axum reference. `cargo deny` checks that reference as a separate manifest.
- `Cargo.lock` is gitignored, so nothing is locked beyond the exact pins in each `Cargo.toml`.

**The `.mz` corpus.** `primitives/*.mz` and `examples/*.mz` are hand-written Mzizi read by
three gates: the `compiler/tests/` harness (`primitives.rs`, `corpus.rs`, `services.rs`, ...),
`mz check`, and `mz contract`. Editing one of them affects all three.

**Benchmarks** (workspace members, separate from the compiler): `benchmarks/harness` does the
reference comparison that `mz contract` deliberately does not; `benchmarks/runner` (`mzbench`)
drives model runs per task and arm; `benchmarks/probe` (`mzprobe`) checks backend tasks over
HTTP. `benchmarks/arms/dioxus`, `arms/leptos` and `tasks/b1-routing/reference-rust` are their
own workspace roots, excluded in the root `Cargo.toml`, so `--workspace` never builds them.
Results under `benchmarks/results/` are recorded runs: report them as they fell.
