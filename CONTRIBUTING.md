# Contributing to Mzizi

## Read this part before you read the rest

Mzizi is **Phase 0 of a research charter**, and the honest description of what exists today
is in the README: a prototype front end. The lexer, the recovering parser, the agent
diagnostic protocol, the content-addressed IR and nine primitives exist and are tested.
`mz contract` evaluates a contract block: a component's against its own declarations, and a
`service`'s by running its examples in process. `mz build` lowers a `service` to a local Rust + axum package; no component lowers, and there is no Workers, WebAssembly or Containers target. There is no runtime
and no rendering. The Phase 0 benchmark has not run, so **nothing here has been measured against
the charter's kill criteria** — and [`CHARTER.md`](./CHARTER.md) §4 is explicit that if the
benchmark does not show a measurable advantage, Phase 1 does not start.

That matters for how you should read every other document in this repository. The RFCs are
a design record written ahead of an implementation, not a specification of working
software. Where an RFC and the code disagree, the code is the fact and the RFC is the claim
— see [RFC-0003](./design/RFC-0003-ir.md) §7.1, which records a claim the implementation
disproved and the narrower claim that replaced it. Contributions that keep that gap visible
are worth more here than contributions that paper over it.

Read, in this order, before proposing anything substantial:

1. [`CHARTER.md`](./CHARTER.md) — the thesis, the phasing, the non-goals, and the kill
   criterion. §5 is a list of things this project has decided not to build.
2. [`README.md`](./README.md) — what exists and what does not.
3. [RFC-0001](./design/RFC-0001-syntax.md) — the surface syntax, the canonical-form rule,
   and the `mz check --agent` protocol. Read it with [RFC-0002](./design/RFC-0002-runtime-and-prior-art.md),
   which amends its design target.
4. [RFC-0006](./design/RFC-0006-contracts.md) — what a `contract` block means, what
   `mz contract` proves, and the four divergences between the RFCs and the code that
   evaluating contracts exposed.
5. [RFC-0003](./design/RFC-0003-ir.md) and [RFC-0004](./design/RFC-0004-test-topology.md) —
   the IR, and what is tested in public versus held out.
6. [`design/ROADMAP.md`](./design/ROADMAP.md) — the index of where every plan in the
   ecosystem lives, and what has and has not been measured. It holds pointers and gates,
   not work items; if a row there disagrees with what it links to, the link is right.

## Getting set up

A stable Rust toolchain with `clippy` and `rustfmt` is the whole dependency list:

```bash
rustup toolchain install stable --component clippy,rustfmt
git clone https://github.com/mzizi-dev/mzizi.git
cd mzizi/compiler
cargo test
```

The compiler crate has **no dependencies**, deliberately — the reason is recorded in
`compiler/Cargo.toml` and it is not incidental: a compiler whose check loop has to stay
sub-second on a modest laptop should not start life with a dependency tree to build first,
and RFC-0002 §3's licence discipline is far easier to hold with an empty
`[dependencies]` table. `Cargo.lock` is gitignored for the same reason: there is nothing to
lock. **A pull request that adds a dependency has to argue for it in the commit message**,
against that standing decision.

## Building and testing: the six gates

CI runs exactly these, from `compiler/`, and you should run all of them before pushing.
[`.github/workflows/ci.yml`](./.github/workflows/ci.yml) is the authority; this is the same
list in copy-pasteable form.

```bash
cd compiler

cargo fmt -- --check                        # formatting, not negotiable
cargo clippy --all-targets -- -D warnings   # every lint is an error, including in tests
cargo test                                  # 561 tests in the compiler crate

# The ones that are the point — the shipped binary, not the test harness:
cargo run --quiet --bin mz -- check ../examples/connectivity_bar.mz
for f in ../primitives/*.mz; do
  echo "checking $f"
  cargo run --quiet --bin mz -- check "$f"
done
for f in ../examples/*.mz ../primitives/*.mz; do
  echo "evaluating $f"
  cargo run --quiet --bin mz -- contract "$f"
done
```

### Why the last two steps exist, and why they are not redundant

They look like they duplicate `cargo test` — `compiler/tests/primitives.rs` and
`compiler/tests/corpus.rs` already parse the same files. They do not duplicate it, and the
distinction is the reason the CI job was worth recreating when this repository was split
out of `agent-tools`:

> `cargo test` proves the **test harness** parses the repo's `.mz` files. `mz check` proves the
> **binary this project ships** does. Those are different claims, and only the second is
> the one the charter makes.

A test harness can drift from the shipped binary in several ordinary ways — a library entry
point the `main.rs` path does not take, a CLI argument that never reaches the checker, an
exit code that reports success on a file with diagnostics. Any of those leaves `cargo test`
green while `mz check button.mz` is broken for every actual user. The charter's rule is that
unverified bytes are worthless, and "verified" has to mean verified through the artifact
people run.

So: **if you touch `compiler/src/main.rs`, the CLI surface, or exit-code handling, the two
`mz check` steps are the ones that will catch you**, not the test suite. Run them.

### The secret scan

A second CI job runs `gitleaks` over the **full history** (`fetch-depth: 0`), not the tip.
This repository's fifteen founding commits arrived by `git subtree split` out of a private
repository and had never been scanned in a context where a leak would be world-readable.
Keep it that way: nothing that looks like a credential belongs in a commit here, including
in a test fixture.

### The supply chain

[`.github/workflows/supply-chain.yml`](./.github/workflows/supply-chain.yml) checks
dependencies, not code (#62). **`cargo deny`** runs `cargo deny check` with
[`deny.toml`](./deny.toml) over the workspace and over B1's Rust reference, which pins the
same `axum` and `tokio` as `mz build`'s generated package. Any RustSec advisory fails it, so
does a license `deny.toml` does not allow, and so does a source other than crates.io.
`Cargo.lock` is gitignored, so it checks the versions a fresh build would fetch, and it also
runs weekly: an advisory can land against a crate this repo already uses with no change here.
A pull request that adds a crate with a new license widens `deny.toml`'s `allow` list in the
same pull request.

The org's required workflows also run on every pull request, from outside this repo:
Semgrep over the changed files, dependency review, a lockfile audit and a release version
check. Their audit runs only when a lockfile changes, and `Cargo.lock` is gitignored here, so
`cargo deny` is this repo's Rust audit. Semgrep fails a workflow step whose action is not
pinned to a commit SHA, so pin every `uses:` line to a SHA with the ref in a comment, as the
workflows here do.

The same workflow's **`workflow audit`** job runs [zizmor](https://docs.zizmor.sh) over
`.github/workflows`. It fails on template injection, unpinned or impostor actions, a
checkout that leaves its token on disk (set `persist-credentials: false` unless the job
pushes), over-broad `permissions` and dangerous triggers. A finding that is safe in context
is suppressed on its line with `# zizmor: ignore[<audit>]` and a comment saying why, as
`main-release.yml`'s `workflow_run` trigger is. `.github/dependabot.yml` proposes action
updates to `staging` weekly, a week after each release (`cooldown`), and
`.github/CODEOWNERS` names the owner for `.github/`, `deny.toml` and `SECURITY.md`.

To run `cargo deny` locally, install [cargo-deny](https://github.com/EmbarkStudios/cargo-deny)
and run `cargo deny check` from the repository root.

### What CI does not check

The `lowering` job gates one lowered service: `mz build` of `examples/registry.mz`, which it
compiles, tests and serves. It also runs `mz run` on every example program and diffs its output
against `examples/<name>.expected`, and runs `benchmarks/perf/run.sh --check-only`, which
builds each perf-suite program and its two Rust references and checks that all three print the
committed `.expected`; it times nothing. No component lowers, and there is no runtime and no
rendering, so there is nothing more there to gate.

Contract evaluation **is** gated, but read what it proves narrowly. `mz contract` checks a
component against its own declarations: its variant tables, its view tree, its prop
defaults. For a component it executes nothing. For a `service` it runs the `example`
clauses in process, against the service's own handlers. Neither compares against a reference
implementation — which is what CHARTER.md §6's defect metric actually requires
([RFC-0006](./design/RFC-0006-contracts.md) §5, §10.1). A green CI run today means _the
repo's `.mz` files lex, parse, lower to IR, keep their own promises, and the shipped binary
agrees_ — it does not mean any component matches the ground truth the benchmark will score
against.

## Rebase merges only, never squash — and why that is a research decision, not a style preference

**Squash merging and merge commits are disabled, org-wide and on this repository; rebase
merging is the only method.** Merge the pull request:

```bash
gh pr merge <n> --rebase --delete-branch
```

This section previously said the opposite — merge commits only, rebase disabled — and the
API disagreed. Measured on 2026-09-12 (commit `c52a7d8`, which corrected the README but not
this file) and again on 2026-09-27, when `--merge` on PR #10 was refused with "Merge commits
are not allowed on this repository": `allow_rebase_merge` is true, merge commits and squash
are both false. Because rebase merging cannot replay merge commits, **keep branches linear**
— cherry-pick or rebase, never merge `main` into a PR branch.

The settings table [`MIGRATION.md`](./MIGRATION.md) §1.1 planned for every repository in the
org read:

| Setting              | Planned | Actual  | Why the part that survived matters                               |
| -------------------- | ------- | ------- | ---------------------------------------------------------------- |
| Allow merge commits  | yes     | **no**  | —                                                                |
| Allow squash merging | no      | **no**  | Squash discards the per-commit reasoning this project depends on |
| Allow rebase merging | no      | **yes** | Rebase keeps every commit and its message                        |

Read the squash row literally. **The commit messages in this repository are the research
record.** This is a project whose entire output so far is a set of design decisions and the
reasoning behind them; the RFCs carry the large decisions and the commit log carries every
smaller one — why a lint was allowed, why a filter was removed, why a claim in an RFC was
narrowed, which of two defensible options was taken and what it cost.

A squash replaces six such messages with one, keeps the diff, and throws the reasoning away.
The diff survives anyway — `git diff` can always reconstruct it — so squash trades the only
irreplaceable half of a commit for tidiness in `git log --oneline`. For a project whose
value is currently the reasoning rather than the code, that is a bad trade, and it is the
one this convention exists to prevent.

A rebase merge loses something smaller: the branch point, so a sequence of commits that only
makes sense as a unit of work lands in `main` without a merge commit saying what it was. The
per-commit reasoning — the part this convention exists to protect — survives intact. Say
what the unit of work was in the PR description, which GitHub keeps linked to every commit.

Practical consequences:

- **Do not squash your own branch before opening a PR.** If your work took four commits with
  four different reasons, keep four commits. A tidy branch is not a goal here.
- **Do fix commits that are genuinely noise** — `wip`, `fix typo`, `address review`. Those
  carry no reasoning and amending or fixup-rebasing them _before_ pushing loses nothing.
  The rule protects reasoning, not commit count.
- **Stacked pull requests are supported.** CI runs on `pull_request` targeting `main` **and**
  `claude/**`, precisely so a PR stacked on another branch gets checks instead of silently
  getting none — a PR with zero checks renders identically to a passing one.

## What a commit message looks like here

Read `git log` before writing one. The house style is unusually substantial and it is
deliberate: a subject line stating what changed, then **prose explaining why**, including
what was considered and rejected, what it costs, and anything discovered along the way that
the next reader would otherwise have to rediscover.

The shape, drawn from the existing history:

```
<type>: <what changed, imperative, lower case, no trailing period>

<Why this change exists. Often the most important paragraph: what was
wrong, or what decision is being recorded. Name the alternative if there
was one, and say why it lost.>

<What it costs, or what it does not do. Honesty about limits is the
house style — "this saves little today; it costs nothing and stops
being a decision later" is a real sentence from this log.>

<Anything found in passing that the next reader needs. Several commits
here end with a "Related:" paragraph recording a live bug noticed while
doing something else.>

Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>
```

Conventions the log actually follows:

- **Types in use:** `ci:`, `docs:`, `chore:`, `feat:`, `fix:`. Scope in parentheses when it
  helps (`docs(mzizi-lang): …`).
- **Reference pull requests and repositories by full slug** — `mzizi-dev/agent-tools#106`,
  not `#106` — because this history moved between repositories and a bare number now
  resolves to the wrong place. There is a commit in this log whose entire subject is fixing
  a comment for exactly that reason.
- **Record silent failures.** The best message in this history explains how a `license`
  field vanished: one PR added it, a concurrent PR deleted the directory it lived in, git
  merged both cleanly and nothing warned. "That is the failure mode of parallel work on one
  tree: not a conflict, a silent subtraction." Write that paragraph when you find one.
- **Say when a change is comment-only or has no behaviour change.** Reviewers should not
  have to derive it from the diff.
- **Every commit ends with the `Co-Authored-By:` trailer** when a machine author wrote it.
  Most of this repository was written by one, which is not incidental to a project about
  machine authorship.

If your message is one line and the change is not a typo, you have probably not finished
the commit.

## Site and docs freshness (hard rule)

_Owner rule, 2026-09-30._ mzizi.dev (`mzizi-dev/mzizi-site`) and docs.mzizi.dev
(`mzizi-dev/mzizi-docs`) must never lag this repository. Any change here to the language
changes what they must say: its syntax, diagnostics, commands, test counts, RFCs, charter,
or benchmark status and results. Two standing freshness agents keep both sites current.

Every pull request here that changes something user-visible has a **Site/docs impact**
heading in its body. It lists what changed, so the freshness agents pick it up. A PR that
changes nothing user-visible says `None.` under it.

This is a rule about what a PR says, not a dependency. Nothing in this repository reads,
builds or calls either site, and CI stays self-contained (see "Repo boundaries" in `AGENTS.md`).

## Proposing a language change: the RFC process

Anything that changes the surface syntax, the type system, the IR node model, the
`mz check --agent` protocol, or the public/private test boundary goes through an RFC in
[`design/`](./design/). The process is not written down anywhere else, so it is written down
here, described from what RFC-0001 through RFC-0006 actually do rather than invented.

**Numbers are not reused.** RFC-0005 is reserved by
[`mzizi-dev/agent-tools#76`](https://github.com/mzizi-dev/agent-tools/issues/76) and not yet
written, so RFC-0006 is the contracts RFC. Check the open issues as well as `design/` before
claiming a number.

**File and title.** `design/RFC-NNNN-kebab-case-topic.md`, four digits, next free number.
The H1 is `# RFC-NNNN — <sentence describing what it settles>`. RFC numbers are permanent;
a superseded RFC stays in the tree.

**The header block.** Three bold fields, then a horizontal rule:

```markdown
**Status:** draft; core implemented in this PR (hashing, store, outline)
**Author:** the machine author (Claude)
**Scope:** the IR node model, canonical serialization, the content-addressed store,
structural sharing, `mz outline`, and the query/patch surface an agent drives.
Implements RFC-0002 §2.1. Contract evaluation and lowering are later RFCs.
```

`Status` states implementation honestly and precisely. The three used so far are
`draft for review — nothing here is implemented`, `draft; core implemented in this PR (…)`,
and `draft; the public half of the mechanism is implemented in this PR`. There is no
`accepted` or `final` status, because nothing in this project has been validated yet;
inventing one would be the exact dishonesty this repository is trying to avoid.

`Scope` says what the RFC settles **and what it defers to a later RFC**. Every RFC here
names its own boundaries in its second sentence.

**Design against named failure modes, not against a wish list.** This is the method the
RFCs share and the strongest convention in the set. RFC-0001 §0 opens with a table of nine
failure modes (`FM-1` … `FM-9`) — concrete things that go wrong when a model authors code —
and then every section heading cites the ones it answers:
`### 1.1`end`with name echo, not braces — _FM-2_`. RFC-0003 does the same for _reading_
code with eight reading barriers (`RB-1` … `RB-8`). RFC-0001 states the rule outright: _if a
decision doesn't trace to a failure mode, it doesn't belong in the language._ A proposal
that cannot name the failure it removes will be asked to.

**Examples come from real components, never invented.** RFC-0001 §1 says so explicitly
and uses `mzizi-connectivity-bar`, a real component whose TypeScript reference and Rust
port both exist, so every line is checkable against known ground truth. Use a component
with a reference implementation, or one of the nine primitives.

**Numbers are measured, not asserted.** RFC-0003 §7 is a table of claims with the
measurements beside them, produced by `compiler/tests/ir_measured.rs` over the nine
primitives and the example components in `examples/`. That test exists so the RFC's numeric claims are
verifiable and stay true — it is also why RFCs live next to the code they govern rather
than in a separate `rfcs` repository (MIGRATION.md §1, "Not repositories": "Split them and
the RFCs become documentation nobody checks"). If your RFC claims a number, land a test that measures it,
and label a prediction as a prediction: RFC-0003 §7 does exactly that with its structural-
sharing figure.

**Corrections are recorded, not edited away.** Two mechanisms are in use:

- A later RFC that changes an earlier one adds a blockquote at the top of the earlier one —
  RFC-0001 carries an **"Amended by RFC-0002"** note explaining that RFC-0001 optimised for
  a frontier model and the real design target is small models, with an instruction to read
  the two together. The wrong text is not deleted.
- Resolved open questions are struck through **in place** and answered, not removed:
  `1. **~~Where the private repository lives.~~** Settled: **`mzizi-dev`** …`, followed by
  the reasoning, including the two earlier answers that were wrong. The charter does the
  same in §7.

If your implementation disproves your own RFC, the house response is RFC-0003 §7.1: a
subsection titled after the correction, stating what the draft claimed, what the test
caught, and the narrower claim that is actually true.

**Every RFC ends with open questions**, usually addressed to the next RFC by number. Closing
a design document by pretending it is complete is not the convention here.

**How it lands.** Open a pull request with the RFC, and where possible the implementation in
the same branch — RFC-0003 and RFC-0004 were both written alongside the code that
implements part of them, which is what lets `Status` say "implemented in this PR" and what
keeps the RFC from drifting into fiction. An RFC that is pure design is fine too; say so in
`Status`. Either way the branch is merged, not squashed, so the RFC's review history stays
readable.

## The language harness

A pull request that adds or changes a language feature adds or updates its harness entry in
the same pull request (owner, 2026-10-07: "The harness work should be part of the build").
The entry lives in `compiler/src/harness.rs`: one entry in `FEATURES` per feature, one `Code`
in `CODES` per diagnostic code with a `trigger` that emits it, and examples that check and, for
a program, run with their stated output. [RFC-0012](./design/RFC-0012-harness.md) §1.2 is the
rule, and `compiler/tests/harness.rs`, part of `cargo test` in CI, enforces it.
`mz harness entry <name>` prints what an agent will read.

## The changelog

Every pull request adds an entry to [`CHANGELOG.md`](./CHANGELOG.md) under
`## [Unreleased]` (owner rule, 2026-10-07). [`AGENTS.md`](./AGENTS.md), "Changelog", has the
rules, and the `changelog / entry required` CI job checks for the entry. Label a pull request
`no-changelog` only when it genuinely has nothing to record; Dependabot version bumps are let
through. Each release to `main` publishes the entries added since the previous release as its
release notes (`.github/scripts/release_notes.py`), so write the entry for someone reading
the release.

Install the pre-commit hook once per clone, before your first commit (agents too):

```bash
scripts/install-hooks.sh   # sets core.hooksPath to .githooks
```

It refuses a commit when neither the staged changes nor the branch's earlier commits (since
it left `origin/staging` or `origin/main`, whichever is nearer) touch `CHANGELOG.md`, and runs
`cargo fmt --all -- --check` when Rust under `compiler/` or `benchmarks/` is staged.
`MZ_NO_CHANGELOG=1` skips the changelog check, the local twin of the `no-changelog` label.
The hook is fast on purpose; the six gates below are still what you run before pushing.
`scripts/test-pre-commit.sh` tests it, and CI's `compiler` job runs that test. The hook
trusts your local `origin/staging` and `origin/main`, so fetch first: with stale refs, or on
a branch stacked on another unmerged one, other people's entries count as yours.

The hook is code from the checked-out branch: committing on someone else's branch, a fork's
pull request included, runs their `.githooks/pre-commit`. Read any change under `.githooks/`
before committing there, or commit with `--no-verify`.

## Changing the primitives, the examples, or the compiler

- **`primitives/` and `examples/` are verified source, not samples.** Every `.mz` file there
  is gated by `mz check` in CI, and `compiler/tests/ir_measured.rs` measures the IR across
  the whole set. Adding a primitive changes those measurements; adding one that does not
  exercise something new is not an improvement. `primitives/README.md` records what each of
  the nine is _for_, and a tenth should be able to fill in that column.
- **`compiler/` changes need a test in `compiler/tests/`**, which stays public permanently —
  RFC-0004 §2 commits to that, including the entire correctness suite. §1.2 goes further
  for security work: a crash seed corpus may be withheld, but "never to the fix or the
  regression test for it, both of which belong in public".
- **Do not commit lowered `.rs` output.** MIGRATION.md §7.2b makes this load-bearing:
  generated code in the source tree is reading barrier RB-5, one of the eight the IR exists
  to remove, and committing it recreates a defect class this ecosystem has already paid to
  remove once. `mz build` will write into a gitignored directory.
- **Formatting is the compiler's job, not yours.** RFC-0001 §3: there is exactly one
  rendering of any Mzizi program and `mz` owns it. Do not add a formatting option.

## Things that are out of scope right now

The charter's §5 is a list of non-goals and it is enforced, not aspirational: no competing
tensor runtime (Candle is the dependency), no native cross-platform renderer before Dioxus
interop has been tried and found insufficient, no post-quantum cryptography. Phase 1
rendering work does not start until Phase 0 has a benchmark number attached to it. A pull
request implementing a later phase will be held, not merged, however good it is — that is
what "research portfolios ship one thread at a time or nothing ships" means in practice.

[`design/ROADMAP.md`](./design/ROADMAP.md) is the current index of what is gated on what.
The most useful work available today is, roughly in order: **reference-implementation
comparison** — the half of the charter's defect metric that `mz contract` does not do
(RFC-0006 §10.1, MIGRATION.md §4.1a) — then the benchmark harness mechanics, then anything
that makes `mz check --agent`'s output denser or its recovery better against RFC-0001 §4's
stated target of at most one diagnostic per true author error.

## Reporting bugs

Open an issue, with the `.mz` source that reproduces it and the exact `mz` invocation. A
parser crash, a hang, or a panic on any input is a bug regardless of how malformed the input
is — see [`SECURITY.md`](./SECURITY.md) for which of those to report privately instead.

## Licensing

Apache-2.0. Contributions are accepted under the same licence, per Apache-2.0 §5 — there is
no separate CLA. The project is 100% Mzizi IP (CHARTER.md), and RFC-0002 §3's
discipline applies to anything copied in from elsewhere: a `NOTICE` file is required if
Apache-2.0 code is ever vendored, and **GPL/AGPL code must never be**, whatever licence
Mzizi itself carries. If you take an idea from another language — which RFC-0002 §3
encourages, and surveys thirteen of them for — take the idea and write the code, and say
where the idea came from in the commit message.
