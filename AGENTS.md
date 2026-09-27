# AGENTS.md — mzizi

> Vendor-neutral instructions for any AI agent working in this repository. For deeper
> conventions see [`CONTRIBUTING.md`](./CONTRIBUTING.md) (the RFC process, the six CI gates,
> commit style) and [`MIGRATION.md`](./MIGRATION.md) (org history, settings, work queue).
> [`CHARTER.md`](./CHARTER.md) is the thesis and is load-bearing — read it first.

## What this repo is

**`mzizi-dev/mzizi` is the language** — a Rust compiler/syntax research prototype, Phase 0
of the Bundu Foundation's Mzizi charter. It is **not** the component registry
(`mzizi-dev/mzizi-registry`) and does not depend on it or any other repo in the org: this
repo's CI must stay green with no network access, no secrets, and no other repository
checked out. See "Repo boundaries" below before adding any dependency that would break that.

## The one rule that overrides the others

**Nothing in this repository has been measured against the charter's kill criterion.** The
Phase 0 benchmark has not run. Do not write or accept a commit message, PR description, or
comment that implies otherwise — "compiles to Rust", "the benchmark shows", "production
ready" are all false today and this project treats overclaiming as a defect class, not a
style nit. State what is tested (`cargo test`, gated in CI) separately from what is designed
(the RFCs). Where an RFC and the code disagree, **the code is the fact** — see
[RFC-0003](./design/RFC-0003-ir.md) §7.1 for a worked example of that rule in practice.

## Build, test, run

A stable Rust toolchain is the entire dependency list — no network fetch, no `Cargo.lock`
(gitignored deliberately; there is nothing to lock). Do not add a dependency to the compiler
crate without arguing for it in the commit message against that standing decision.

```bash
cd compiler
cargo test                                                              # 174 tests
cargo run --bin mz -- check          ../primitives/button.mz
cargo run --bin mz -- check --agent  ../examples/connectivity_bar.mz    # NDJSON for an agent
cargo run --bin mz -- contract       ../primitives/button.mz            # evaluate the contract block
cargo run --bin mz -- outline        ../primitives/alert.mz
cargo run --bin mz -- ir              ../primitives/card.mz
```

Run before every push, matching [`.github/workflows/ci.yml`](./.github/workflows/ci.yml)
exactly:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --bin mz -- check ../examples/*.mz     # every example
cargo run --bin mz -- check ../primitives/*.mz   # every primitive
cargo run --bin mz -- contract ../examples/connectivity_bar.mz
cargo run --bin mz -- contract ../primitives/*.mz
```

`mz check` and `mz contract` are not redundant with `cargo test`: `cargo test` proves the
test harness parses the corpus, `mz check`/`mz contract` prove the binary this project ships
does — that second claim is the one the charter makes. `mz check --agent` is the interface
an agent consuming this compiler should target: whole-program NDJSON, deterministic order,
one diagnostic per real error, fixes tagged `exact` or `guess` — see RFC-0001 §4.

## Commit and merge conventions

- **Write commit messages for a future reader, not a changelog.** State what you measured —
  which command you ran, which number came back — not just what you changed. This repo's
  commit history is the research record; a vague message throws that away.
- **Pull requests are rebase-merged**, one commit at a time, message intact:
  `gh pr merge <n> --rebase --auto`. Never `--admin`, never squash.
  **This contradicts what `MIGRATION.md` §1.1 and `CONTRIBUTING.md` say** ("merge commits
  only, squash and rebase both disabled") — that documented policy has not caught up with
  the live GitHub settings (`allow_rebase_merge: true`, `allow_merge_commit: false`,
  `allow_squash_merge: false`, read directly off the API on 2026-09-12, org-wide). The
  argument the docs make still holds — squashing would discard the per-commit reasoning
  this project depends on — it is only the _mechanism_ (rebase vs. merge commit) that
  changed. Do not "fix" this drift by editing MIGRATION.md/CONTRIBUTING.md without
  confirming with the owner first; it may be the settings that are wrong, not the docs.
- Proposing a language change means writing an RFC. Read the existing four before drafting a
  fifth; RFC-0005's number is reserved (`mzizi-dev/agent-tools#76`, private) but not written.

## Repo boundaries

> Everything else consumes Mzizi. Mzizi consumes nothing else. — RFC-0004 §3, applied to
> code as well as tests.

This repo's own CI must pass with zero access to any other repository. The held-out Phase 0
benchmark set follows the same rule in the other direction: private consumes public, public
never consumes private, and a missing private result reports `neutral`, never `failure`.
Do not wire a build step, test, or script here that reaches out to `mzizi-registry`,
`agent-tools`, or any other sibling repo.

## Naming and ownership

- The language repo is plain `mzizi`; every other repo in the org is `mzizi-`-prefixed.
- Mzizi (this repo) is 100% Bundu Foundation IP. The Mzizi console ("Fundi") is
  Nyuchi-owned. Keep that line — CHARTER.md draws it deliberately.
- Brand wordmarks are lowercase in prose: `mzizi`, `bundu`, `nyuchi`, `fundi`. Not
  "Mzizi™", not title case in running text.

## Further reading

- [`CHARTER.md`](./CHARTER.md) — thesis, phasing, non-goals, kill criterion.
- [`CONTRIBUTING.md`](./CONTRIBUTING.md) — full RFC process, the six CI gates in detail.
- [`MIGRATION.md`](./MIGRATION.md) — org history, the settings table, the work queue.
- [`design/ROADMAP.md`](./design/ROADMAP.md) — index of every plan in the ecosystem, and
  what has/hasn't been measured. Holds pointers, not work items.
