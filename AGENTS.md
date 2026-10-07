# AGENTS.md — mzizi

> Vendor-neutral instructions for any AI agent working in this repository. For deeper
> conventions see [`CONTRIBUTING.md`](./CONTRIBUTING.md) (the RFC process, the six CI gates,
> commit style) and [`MIGRATION.md`](./MIGRATION.md) (org history, settings, work queue).
> [`CHARTER.md`](./CHARTER.md) is the thesis and is load-bearing — read it first.

## What this repo is

**`mzizi-dev/mzizi` is the language** — Mzizi, a general-purpose programming language, and
its compiler (`mz`, written in Rust), a research prototype in Phase 0 of the Mzizi research
charter. Its one goal is building Mzizi as a language that stands against the best existing
language for each kind of task ([`CHARTER.md`](./CHARTER.md) §1, v0.4). "Compiles to Rust" is
the design, not the state: `mz build` lowers a `service` to a local Rust + axum package; no component lowers, and there is no Workers, WebAssembly or Containers target. The harness is the core of the language
([RFC-0012](./design/RFC-0012-harness.md), a draft); `benchmarks/harness/` is a different
thing, always called "the benchmark harness". It is **not** the component registry
(`mzizi-dev/mzizi-registry`) and does not depend on it or any other repo in the org: this
repo's CI must stay green with no secrets and no other repository checked out. The only
network access its builds need is to crates.io, for the index, the benchmark runner's two pinned
crates, and the pinned `axum` and `tokio` that CI's `lowering` job builds a generated service
against (see "Build, test, run"). Two CI tools also come from GitHub releases: gitleaks for
the `secret scan` job, and cargo-deny with the RustSec advisory database for `supply chain`. See "Repo boundaries" below before adding any dependency that would break that.

## The one rule that overrides the others

**Nothing in this repository has been measured against the charter's kill criterion.** Two
Phase 0 pilots ran on 2026-09-27, and neither showed an advantage for Mzizi: the frontier model
tied the arms, and the ~7B open-weight model did worse in Mzizi on all three metrics
([`benchmarks/results/`](./benchmarks/results/)). Neither is the kill-criterion run, and that
run has not happened; [`benchmarks/READINESS.md`](./benchmarks/READINESS.md) says what it still
waits on. Report results as they fell. "Designed for" is fine; "faster" or "better" is not.
Do not write or accept a commit message, PR description, or comment that implies otherwise — "compiles to Rust" (only a
`service` lowers, to a local axum package; no component does), "the benchmark shows", "production
ready" are all false today and this project treats overclaiming as a defect class, not a
style nit. State what is tested (`cargo test`, gated in CI) separately from what is designed
(the RFCs). Where an RFC and the code disagree, **the code is the fact** — see
[RFC-0003](./design/RFC-0003-ir.md) §7.1 for a worked example of that rule in practice.

## Build, test, run

The compiler crate (`mzizi-lang-compiler`, in `compiler/`) has zero dependencies, and so does
`benchmarks/harness`. The workspace as a whole does not: `benchmarks/runner` (`mzbench`)
depends on `ureq` and `serde_json`, each pinned to an exact version in its `Cargo.toml` (its
comments say why). `Cargo.lock` is gitignored, so those two crates' own dependencies are not
locked. Because the compiler is a workspace member, every `cargo` command resolves the whole
workspace. On a cold cache, `cargo test -p mzizi-lang-compiler --offline` fails with "no
matching package named `serde_json`", even though it never builds that crate. The first run
needs network access to the crates.io index (it downloads no crates for the compiler).
After that, `--offline` works. Do not add a dependency to the compiler crate without arguing
for it in the commit message against that standing decision.

```bash
cd compiler
cargo test                                                              # 297 tests (428 in the workspace)
cargo run --bin mz -- check          ../primitives/button.mz
cargo run --bin mz -- check --agent  ../examples/connectivity_bar.mz    # NDJSON for an agent
cargo run --bin mz -- fix            path/to/file.mz                    # apply every exact fix in place
cargo run --bin mz -- contract       ../primitives/button.mz            # evaluate the contract block
cargo run --bin mz -- outline        ../primitives/alert.mz
cargo run --bin mz -- ir              ../primitives/card.mz
cargo run --bin mz -- contract       ../examples/registry.mz            # run a service in process
cargo run --bin mz -- build          ../examples/registry.mz --out ../target/mz-build/registry
```

Run before every push. These are the commands in
[`.github/workflows/ci.yml`](./.github/workflows/ci.yml), job by job. The `mz` loops are copied
from it, because `mz` takes exactly one file and `mz check ../primitives/*.mz` exits 2 (usage).
The `secret scan` job (gitleaks), the `lowering` job (`mz build` of `examples/registry.mz`,
then `cargo test` and one request over a socket against the generated package, and
`mzprobe verify` of the backend task B1 against its Mzizi and axum references; it fetches
`axum` and `tokio` from crates.io), the `supply chain` workflow (`supply-chain.yml`:
`cargo deny check` against [`deny.toml`](./deny.toml), and zizmor over the workflows), the org's required workflows (Semgrep,
dependency review, a lockfile audit and a release version check, which run on every pull
request from outside this repo), and the lint gate (`lint.yml`: actionlint, JSON validity, prettier, markdownlint,
yamllint) are not listed here.

```bash
# job `compiler` — run from compiler/
cd compiler
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
for f in ../examples/*.mz; do
  echo "checking $f"
  cargo run --quiet --bin mz -- check "$f"
done
for f in ../primitives/*.mz; do
  echo "checking $f"
  cargo run --quiet --bin mz -- check "$f"
done
for f in ../examples/*.mz ../primitives/*.mz; do
  echo "evaluating $f"
  cargo run --quiet --bin mz -- contract "$f"
done
cd ..

# job `benchmarks` — run from the repo root
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p mzizi-benchmark-harness -p mzizi-benchmark-runner -p mzizi-benchmark-probe
```

Run the block as a script with `bash -e` (from the repo root), which is how CI runs every
step. In a shell without `-e`, a primitive that fails in the middle of a loop does not stop the
run, and the loop's exit status comes only from its last iteration. Do not paste `set -e` into
an interactive shell, because the first failure closes it.

`mz check` and `mz contract` are not redundant with `cargo test`: `cargo test` proves the
test harness parses the repo's `.mz` files, `mz check`/`mz contract` prove the binary this project ships
does — that second claim is the one the charter makes. `mz check --agent` is the interface
an agent consuming this compiler should target: whole-program NDJSON, deterministic order,
one diagnostic per real error, fixes tagged `exact` or `guess` — see RFC-0001 §4. `mz fix`
applies every `exact` fix in one pass. The `file` key is the path `mz` was given; the benchmark
runner normalises it to the bare file name before a model sees it.

The benchmark's kill-criterion driver (`benchmarks/kill-criterion/run.sh`) needs a model
endpoint, and CI does not run it. `benchmarks/kill-criterion/check-task.sh <task dir>` is
the offline check a task author runs.

## Commit and merge conventions

- **Write commit messages for a future reader, not a changelog.** State what you measured —
  which command you ran, which number came back — not just what you changed. This repo's
  commit history is the research record; a vague message throws that away.
- **Pull requests are rebase-merged**, one commit at a time, message intact:
  `gh pr merge <n> --rebase --auto`. Never `--admin`, never squash.
  [`CONTRIBUTING.md`](./CONTRIBUTING.md) already says this ("Rebase merges only, never
  squash"). Only [`MIGRATION.md`](./MIGRATION.md) §1.1 differs: its settings table is the
  original _plan_ ("merge commits only, squash and rebase both disabled"), and the live
  GitHub settings replaced it (`allow_rebase_merge: true`, `allow_merge_commit: false`,
  `allow_squash_merge: false`, read off the API on 2026-09-12, org-wide). A note under that
  table points to CONTRIBUTING.md. The argument behind the plan still holds: squashing would
  throw away the per-commit reasoning this project depends on. Only the mechanism changed,
  from merge commits to rebase. Rebase merging cannot replay merge commits, so keep branches
  linear.
- Proposing a language change means writing an RFC. Read the existing RFCs in
  [`design/`](./design/) before you draft one. RFC-0005's number is reserved (drafted as
  `mzizi-dev/agent-tools#76`, private) and is not in this repo. Take the next number that is
  not already used on `main` or claimed by an open PR. Other RFCs may be in flight on branches.
- A PR that changes what the language can do updates the matching row in `LANGUAGE-TRACKER.md` in the same PR.

## Site and docs freshness (hard rule)

_Owner rule, 2026-09-30._ mzizi.dev (`mzizi-dev/mzizi-site`) and docs.mzizi.dev
(`mzizi-dev/mzizi-docs`) must never lag this repository. Any change here to the language
changes what they must say: its syntax, diagnostics, commands, test counts, RFCs, charter,
or benchmark status and results. Two standing freshness agents keep both sites current.

Every pull request here that changes something user-visible has a **Site/docs impact**
heading in its body. It lists what changed, so the freshness agents pick it up. A PR that
changes nothing user-visible says `None.` under it.

This is a rule about what a PR says, not a dependency. Nothing in this repository reads,
builds or calls either site, and CI stays self-contained (see "Repo boundaries").

## Repo boundaries

> Everything else consumes Mzizi. Mzizi consumes nothing else. — RFC-0004 §3, applied to
> code as well as tests.

This repo's own CI must pass with zero access to any other repository. The held-out Phase 0
benchmark set follows the same rule in the other direction: private consumes public, public
never consumes private, and a missing private result reports `neutral`, never `failure`.
Do not wire a build step, test, or script here that reaches out to `mzizi-registry`,
`agent-tools`, or any other sibling repo.

## Changelog

_Owner rule, 2026-09-30: changelogs are super important._ [`CHANGELOG.md`](./CHANGELOG.md)
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), in dated sections
because the compiler has no releases. Every pull request that changes behaviour,
diagnostics, the language, the charter, an RFC or the benchmarks adds an entry under
`## [Unreleased]`:

- Put it under `### Added`, `### Changed`, `### Fixed`, `### Removed` or `### Security`.
- Say what changed, and name the diagnostic codes and commands. Keep what is tested apart
  from what is designed: an RFC is design, and says so.
- The one rule that overrides the others applies here too. An entry never implies a
  measured result that does not exist.
- Whoever merges moves the `[Unreleased]` entries into a section dated with the merge day.

The `changelog / entry required` CI job fails a pull request that changes other files
without touching `CHANGELOG.md`. It lets through a pull request labelled `no-changelog`, and
one that touches only `.github/**` or lint configuration (`.prettierrc`, `.prettierignore`,
`.markdownlint.jsonc`, `.yamllint.yaml`). The job is not a required check
in branch protection.

## Naming and ownership

- The language repo is plain `mzizi`. The org's other product repositories are
  `mzizi-`-prefixed, with `agent-tools` the one exception.
- Mzizi owns and operates the language, its toolchain, the registry, the design system, the
  docs and the API. Nyuchi operates the console (`app.mzizi.dev`) and the revenue products.
  Copyright notices name the Bundu Foundation as the parent copyright holder; Mzizi is not a
  separate legal entity. Keep that line — CHARTER.md draws it deliberately.
- Write the wordmark as `Mzizi`, capitalised, never "Mzizi™". The other wordmarks stay
  lowercase in prose: `nyuchi`, `mukoko`, `shamwari`, `bundu`. Code identifiers,
  such as this repo's name, stay as they are.
- **Mzizi Roots** is the name for Mzizi's own components in Rust: UI and server components
  for the agentic web. React/TSX components keep working but are deprioritised; where a
  Rust implementation exists, present it first.
- **The design system** is published as the "Design System" artifact,
  <https://claude.ai/artifact/G8CCtAbZ8w717uQ3R5itCc>: the brand book, voice, visual foundations, marks and component previews for
  `mzizi` and the `bundu` ecosystem. Its source of truth is `design-system/` in
  `mzizi-dev/mzizi-registry` (landing with registry PR #418). That folder's `PUBLISHING.md`
  says the artifact is built from the folder, file for file, and is never edited on the
  artifact page, so change the design system there, not in the artifact or in this repo.
  Nothing in this repository reads it (see "Repo boundaries").
- The rest of the ecosystem, for reference: `api.mzizi.dev` is `mzizi-api-gateway`, a Hono
  Worker in TypeScript that serves the registry's files bundled at a pinned commit (no
  database, no origin). `mcp.mzizi.dev` is `agent-tools` (`mzizi-mcp`). The registry holds
  no database; its files are the data layer. Only the console uses Supabase.
- Security reports: GitHub private reporting first, then `security@nyuchi.com` (see
  [`SECURITY.md`](./SECURITY.md)), the same address as the console's.

## Further reading

- [`CHARTER.md`](./CHARTER.md) — thesis, phasing, non-goals, kill criterion.
- [`CONTRIBUTING.md`](./CONTRIBUTING.md) — full RFC process, the six CI gates in detail.
- [`MIGRATION.md`](./MIGRATION.md) — org history, the settings table, the work queue.
- [`design/ROADMAP.md`](./design/ROADMAP.md) — index of every plan in the ecosystem, and
  what has/hasn't been measured. Holds pointers, not work items.

## Track big work in GitHub issues

Any substantial build, migration, investigation or multi-step task gets a GitHub issue in the repo that owns it — before or as work starts — so another session, agent or person can pick it up.

- The issue holds the goal, the owner's decisions (verbatim where given), the plan, acceptance criteria, owner-only steps and links.
- Every PR references its issue (`Refs #n`; `Fixes #n` only when the merge completes it).
- Post progress, decisions and a hand-off note (what's done, what's left, branch names) as issue comments — at each merge and before a session or agent finishes.
- Work spanning repos gets a tracking issue that links the per-repo issues.
- Never put secrets, credential status or exploitable detail in issues on public repos.

## Dev skills, progress reports and the merge gate

Load the Mzizi **dev skills** before starting work: `mzizi_get_skills category=dev` on the Mzizi MCP (`mcp.mzizi.dev`), or `@nyuchi/mzizi-skills` from npm. They are `digital-hygiene` and `progress-report`.

- **Digital hygiene.** Check free disk before starting, clone only under `$TMPDIR`, share build caches, and audit, then delete, your clones once the work merges (`digital-hygiene` skill).
- **Clone isolation.** Clone only into a directory unique to you; never touch another agent's.
- **Progress reports.** All dev work runs on a 10-minute progress-report loop (`progress-report` skill): measured bars, what changed, and a final "Needs you:" line. Report ticks never publish, release, merge or deploy without the owner's approval.
- **Merge gate.** Merge only when the work is complete, CI is green, it's verified at runtime, and `/code-review` has run with findings resolved.
