# Contributing to Mzizi

## Read this part before you read the rest

Mzizi is **Phase 0 of a research charter**, and the honest description of what exists today
is in the README: a prototype front end. The lexer, the recovering parser, the agent
diagnostic protocol, the content-addressed IR and nine primitives exist and are tested.
Contract bodies parse but are **not evaluated**. There is no lowering, no runtime, no
rendering. The Phase 0 benchmark has not run, so **nothing here has been measured against
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
cargo test                                  # 107 tests

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

> `cargo test` proves the **test harness** parses the corpus. `mz check` proves the
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

### What CI does not check

There is no lowering, no runtime and no rendering, so there is nothing there to gate.

Contract evaluation **is** gated, but read what it proves narrowly. `mz contract` checks a
component against its own declarations: its variant tables, its view tree, its prop
defaults. It does not execute anything, and it does not compare against a reference
implementation — which is what CHARTER.md §6's defect metric actually requires
([RFC-0006](./design/RFC-0006-contracts.md) §5, §10.1). A green CI run today means _the
corpus lexes, parses, lowers to IR, keeps its own promises, and the shipped binary agrees_ —
it does not mean any component matches the ground truth the benchmark will score against.

## Merge commits only — and why that is a research decision, not a style preference

**Squash merging and rebase merging are disabled, org-wide and on this repository.** Merge
the pull request:

```bash
gh pr merge <n> --merge --delete-branch
```

The reason is recorded in [`MIGRATION.md`](./MIGRATION.md) §1.1, in the settings table
applied to every repository in the org:

| Setting              | Value   | Why                                                              |
| -------------------- | ------- | ---------------------------------------------------------------- |
| Allow merge commits  | **yes** | The ecosystem convention is merge-only; history stays truthful   |
| Allow squash merging | **no**  | Squash discards the per-commit reasoning this project depends on |
| Allow rebase merging | **no**  | Same                                                             |

Read that middle row literally. **The commit messages in this repository are the research
record.** This is a project whose entire output so far is a set of design decisions and the
reasoning behind them; the RFCs carry the large decisions and the commit log carries every
smaller one — why a lint was allowed, why a filter was removed, why a claim in an RFC was
narrowed, which of two defensible options was taken and what it cost.

A squash replaces six such messages with one, keeps the diff, and throws the reasoning away.
The diff survives anyway — `git diff` can always reconstruct it — so squash trades the only
irreplaceable half of a commit for tidiness in `git log --oneline`. For a project whose
value is currently the reasoning rather than the code, that is a bad trade, and it is the
one this convention exists to prevent.

A rebase merge is the same loss in a different shape: it discards the branch point, so a
sequence of commits that only makes sense as a unit of work is flattened into `main` with
no record of what it was.

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

**Examples come from the benchmark corpus, never invented.** RFC-0001 §1 says so explicitly
and uses `nyuchi-connectivity-bar`, a real component whose TypeScript reference and Rust
port both exist, so every line is checkable against known ground truth. Use a component
from the corpus, or one of the nine primitives.

**Numbers are measured, not asserted.** RFC-0003 §7 is a table of claims with the
measurements beside them, produced by `compiler/tests/ir_measured.rs` over the nine
primitives and the corpus example. That test exists so the RFC's numeric claims are
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
no separate CLA. The project is 100% Bundu Foundation IP (CHARTER.md), and RFC-0002 §3's
discipline applies to anything copied in from elsewhere: a `NOTICE` file is required if
Apache-2.0 code is ever vendored, and **GPL/AGPL code must never be**, whatever licence
Mzizi itself carries. If you take an idea from another language — which RFC-0002 §3
encourages, and surveys thirteen of them for — take the idea and write the code, and say
where the idea came from in the commit message.
