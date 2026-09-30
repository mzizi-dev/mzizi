# Mzizi — where the plans live

**Status of this file:** the index, not the plan. Snapshot claims are dated and name their
source; everything else is a link.

## Why this file exists here and not in its own repository

`mzizi-dev/mzizi-roadmap` was a public repository containing a one-line README and nothing
else. [`MIGRATION.md`](../MIGRATION.md) §1 required that it be resolved one way or the
other, and said why:

> …do not leave a roadmap living apart from the code it plans. Three of the stale-doc
> defects fixed on 2026-08-23 existed for exactly that reason.

§9.3 left the choice open. **Resolved 2026-09-11: folded here; `mzizi-dev/mzizi-roadmap` is
archived** with a README pointing at this file.

The deciding argument is already written down two paragraphs above the constraint itself.
§1's "Not repositories" section rejects a separate `rfcs` repo on the grounds that "RFCs live
next to the code they govern… Split them and the RFCs become documentation nobody checks." A
roadmap repository is that same shape. It would hold no code, so nothing in it could ever be
checked against anything, and its only content would be restatements of plans that live
elsewhere — which is the drift mechanism §1 names.

**So this file holds pointers and gates, not work items.** If a row here disagrees with what
it links to, the link is right and the row is the defect.

## Status, stated plainly

**The kill-criterion run has not happened. Nothing in this project has been measured against
the charter's kill criterion.** Two pilots ran on 2026-09-27, and neither showed an advantage
for Mzizi (below). [`CHARTER.md`](../CHARTER.md) §4 makes Phase 0 the gate on everything after it:

> If this doesn't show a measurable advantage, nothing downstream matters — don't build
> Phase 1 until Phase 0 has a real number attached to it.

There is no such number. What exists is a prototype front end — lexer, recovering parser,
agent NDJSON protocol, content-addressed IR, nine primitives — and the measurements in
`compiler/tests/ir_measured.rs`, which measure the IR's own properties, not the charter's
claim. The [README](../README.md) states this; it is repeated here because a roadmap is
exactly the document that tends to imply progress it cannot show.

Two things are load-bearing and absent:

| Gap                                             | Evidence                                                                                                                                                                                                                                                                                                                                                                                                                                                       | Consequence                                                                                                                                                                                                                                                                                                                                       |
| ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ~~**Half the defect metric has no toolchain**~~ | **Design resolved:** [RFC-0006](./RFC-0006-contracts.md) §10.1 picks harness-side comparison over the other two candidates it named, and specifies the mechanism — a bounded, regex-level diff of a `.mz` size enum's declared pixel heights against the Tailwind classes in its `.rs` reference's match arms, via the standard 4px spacing scale. `mz contract` is unchanged: still self-consistency only, no cross-language semantics added to the compiler. | A prototype now runs the mechanism end to end (`benchmarks/harness/`, below), but against exactly one component and a checked-in fixture standing in for the real `mzizi-registry` reference. Scoring the corpus for real is still open.                                                                                                          |
| ~~**The benchmark harness does not exist**~~    | A prototype does now: `benchmarks/harness/` (`mzizi-benchmark-harness`, a workspace member alongside `compiler/`), proved against `primitives/button.mz` and a fixture copy of its real Rust reference — [`benchmarks/README.md`](../benchmarks/README.md).                                                                                                                                                                                                    | _Updated 2026-09-29._ The mechanics now exist: `mzbench` (`benchmarks/runner/`) runs and scores episodes and measures tokens and iterations, and `benchmarks/kill-criterion/run.sh` drives a whole run. What is still missing is the held-out task set and the pre-registered settings — [`benchmarks/READINESS.md`](../benchmarks/READINESS.md). |

~~**Contract bodies parse but are not evaluated.**~~ Resolved 2026-09-11:
`mz contract <file>` evaluates them, CI runs it over the corpus example and every
primitive, and 33 assertions across ten files are evaluated rather than counted. The row
above is what is left of that gap, and it is a smaller and more specific thing than the one
it replaces.

**2026-09-27: a pilot was scored, and this section stands.** The first `mzbench` run with
scores attached — 3 tasks, 2 arms (mzizi, dioxus), one seed, one frontier model, agent-driven
— is recorded in
[`benchmarks/results/2026-09-27-pilot/RUN.md`](../benchmarks/results/2026-09-27-pilot/RUN.md).
It does not trigger rule 3 below ("a number from a benchmark run replaces the whole 'Status,
stated plainly' section"), because it is not the number that rule means. It is a pilot, three
data points per arm. It is frontier-only, and RFC-0002 §5.4 says the frontier arm alone cannot
validate the thesis. And the Mzizi checker it ran against does not check type names or
`{...}` interpolation names, so the Mzizi arm's first-iteration-clean result was measured
against a checker that cannot fail on names or types. Tokens were not measured at all. What
the pilot did produce is a harness fix (renamed variants were being scored as defects) and a
reproducible baseline; what it did not produce is a measurement against the kill criterion.

**2026-09-27, later: a second pilot added the open-weight arm, and it went against Mzizi.**
[`benchmarks/results/2026-09-27-pilot-2/`](../benchmarks/results/2026-09-27-pilot-2/RUN.md)
reran the same two scored tasks at the same `ded425a` with three seeds, measured tokens, and
added Qwen2.5-Coder-7B on CPU — the arm the first pilot names as missing. On the frontier
model the arms were again indistinguishable (Mzizi ~8% cheaper in tokens). On the 7B model
Mzizi did worse on all three metrics: 2/6 clean compiles against Dioxus's 4/6, 2/2 clean
episodes defective against 0/4, and 15 of 16 repair attempts resubmitting an unchanged file.
This section still stands, for the same reasons the first pilot gives — two tasks, a handful
of seeds, and a known harness bug that inflates Mzizi's token counts — but the one small-model
data point there is does not favour the thesis. Its write-up traces each loss to a mechanism;
one of them, `else` rejected in a view, is already fixed on `main` by RFC-0008.

**2026-09-29: pilot 2's pre-kill-criterion list, audited and mostly fixed. This section still
stands.** [`benchmarks/READINESS.md`](../benchmarks/READINESS.md) checks every item on that
list against `main` at `a9c928d`, then records what changed. The path strings are now
normalised out of every arm's feedback. The React idioms (`...props`, `asChild`) have named
diagnostics. FM-11 is gone from the variant table: a size's height is read from its class,
and a disagreeing `height` is a compile error. Of the seven compiler/RFC divergences, two were already fixed on `a9c928d`, four were fixed on 2026-09-29
(`mz fix` among them), and one was the RFC's error and is corrected there. A `slot_set`
fact and a `card` task were added. None of this is a measurement. The held-out task set the
run needs does not exist yet, and neither pilot's numbers change: they measured the code they
ran against.

## Phases

Defined in [`CHARTER.md`](../CHARTER.md) §4. Reproduced here only as far as their _gates_,
because the phase descriptions belong to the charter and a second copy of them would drift.

| Phase                                                                                            | State                       | Gate on the next one                                                                                         |
| ------------------------------------------------------------------------------------------------ | --------------------------- | ------------------------------------------------------------------------------------------------------------ |
| **0** — Mzizi against the best existing language for each kind of task (charter v0.4)            | in progress; **unmeasured** | A real number from the benchmark. Until then Phase 1 is not started, by the charter's own rule.              |
| **1** — full-stack: `mzizi-ui` + Cloudflare Workers and Containers                               | not started                 | Blocked on Phase 0's number                                                                                  |
| **2** — Candle integration                                                                       | not started                 | Blocked on Phase 1                                                                                           |
| **3** — native mobile: `mzizi-ui` interop first (Dioxus today), native codegen scoped separately | not started                 | Blocked on Phase 1                                                                                           |
| **4** — distribution adapters                                                                    | not started                 | Blocked on Phase 1's artifacts standing alone (the self-contained UI artifact CHARTER.md §4 Phase 1 defines) |
| **5** — hardware / embedded                                                                      | not started                 | Blocked on Phase 1 having shipped a real full-stack deployment                                               |

## Where each plan lives

Every one of these is next to the code it plans. That is the point.

| Area                                                                             | The plan                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | Where                                                                                                                                                                                                                                                                                                                                                     |
| -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The language and compiler                                                        | Work queue in dependency order: ~~contract evaluation~~ (done) → ~~reference-implementation comparison~~ (design resolved, prototype landed) → benchmark harness mechanics → `mz patch`/`refs`/`diff`, IR store persistence, local state → Phase 1                                                                                                                                                                                                                                                                                                                                                               | [`MIGRATION.md`](../MIGRATION.md) §4, in this repo beside `compiler/`                                                                                                                                                                                                                                                                                     |
| Language design                                                                  | RFC-0001 syntax, RFC-0002 runtime and prior art, RFC-0003 IR, RFC-0004 test topology, RFC-0006 contracts, [RFC-0007](./RFC-0007-gap-register.md) gap register (what CHARTER v0.2's scope needs that the language lacks, and in what order), [RFC-0009](./RFC-0009-comparison-benchmark.md) comparison benchmark (arms, task families, the best-incumbent kill criterion, publication), [RFC-0010](./RFC-0010-contracts-everywhere.md) contracts everywhere (functions, handlers, services, the standard library), [RFC-0012](./RFC-0012-harness.md) the harness, the core of the language (draft; mostly design) | [`design/`](.)                                                                                                                                                                                                                                                                                                                                            |
| Phase 0 benchmark                                                                | Corpus, defect definition and visibility split resolved; harness, runner and kill-criterion driver landed; two pilots recorded; the kill criterion fixed in RFC-0009 §6; the held-out task set, who writes it, and the new arms still open                                                                                                                                                                                                                                                                                                                                                                       | [`benchmarks/READINESS.md`](../benchmarks/READINESS.md), [`benchmarks/kill-criterion/README.md`](../benchmarks/kill-criterion/README.md), [`benchmarks/README.md`](../benchmarks/README.md), [`CHARTER.md`](../CHARTER.md) §6, [RFC-0004](./RFC-0004-test-topology.md), [RFC-0009](./RFC-0009-comparison-benchmark.md)                                    |
| The benchmark corpus — the Rust reference implementations Phase 0 scores against | Epic plus four dependency-ordered waves                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | [`mzizi-dev/mzizi-registry#222`](https://github.com/mzizi-dev/mzizi-registry/issues/222) → [#223](https://github.com/mzizi-dev/mzizi-registry/issues/223), [#224](https://github.com/mzizi-dev/mzizi-registry/issues/224), [#225](https://github.com/mzizi-dev/mzizi-registry/issues/225), [#226](https://github.com/mzizi-dev/mzizi-registry/issues/226) |
| The registry's own correctness backlog                                           | Open issues, labelled                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | [`mzizi-dev/mzizi-registry` issues](https://github.com/mzizi-dev/mzizi-registry/issues)                                                                                                                                                                                                                                                                   |
| What a registry is for once the language ships                                   | RFC-0005, drafted as an issue; it belongs in [`design/`](.) and is not here yet                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | [`mzizi-dev/agent-tools#76`](https://github.com/mzizi-dev/agent-tools/issues/76)                                                                                                                                                                                                                                                                          |
| Org topology, renames, and what each rename breaks                               | The migration record                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | [`MIGRATION.md`](../MIGRATION.md) §6, §7                                                                                                                                                                                                                                                                                                                  |
| Decisions still owed by the owner                                                | Four, one now three                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | [`MIGRATION.md`](../MIGRATION.md) §9                                                                                                                                                                                                                                                                                                                      |

## The cross-repo dependency that was written nowhere

[`MIGRATION.md`](../MIGRATION.md) §8 scopes the registry into the next session as
"**required**: the benchmark corpus and ground truth, including the 37 hand-written `.rs`
reference implementations". [`CHARTER.md`](../CHARTER.md) §6 fixes the corpus as Mzizi's own
components, authored in Mzizi "against the existing `.tsx`/`.rs` implementations as ground
truth".

What neither says is how much of that ground truth exists. Per
[`mzizi-dev/mzizi-registry#222`](https://github.com/mzizi-dev/mzizi-registry/issues/222),
measured at the time that issue was written: **358** N2 primitives have a `.tsx` on disk and
**3** have a `.rs` sibling — `button`, `badge`, `card`.

That is a Phase 0 constraint, not a registry detail. A task whose ground truth is a `.tsx`
alone can be scored on tokens and on iterations to a clean compile, but the defect metric
compares against a reference implementation, and for 355 of 358 primitives there is no Rust
reference to compare against. So the size of the first honest benchmark run is bounded by
[#223](https://github.com/mzizi-dev/mzizi-registry/issues/223)'s six primitives, not by the
571-component registry the charter gestures at.

Direction of the dependency, which matters: **the language repo does not consume the
registry's code.** It consumes the corpus as benchmark input, by path, exactly as
[RFC-0004](./RFC-0004-test-topology.md) §3 requires of the task set.

## Open decisions

From [`MIGRATION.md`](../MIGRATION.md) §9, which remains the record. Restated here only as a
list of what is open, because an index whose whole job is "where do I look" should say when
the answer is "nobody has decided yet".

1. **The npm scope for anything Mzizi publishes** — `@bundu`, `@nyuchi`, or `@mzizi`. One-way door once published. _As of 2026-09-29, npm packages keep the `@nyuchi/` scope; CHARTER.md's `@bundu` line was the plan, not the practice._
2. ~~**The held-out set's home**~~ — a private repository in `mzizi-dev`, per [RFC-0004](./RFC-0004-test-topology.md) §6.1, not yet created (its trigger is the first held-out task). Who writes the tasks is still open: [`benchmarks/READINESS.md`](../benchmarks/READINESS.md).
3. ~~**`mzizi-dev/roadmap`**~~ — resolved; this file.
4. **Whether the React design system is renamed** to say what it is.

## Drift already present in the migration record

[`MIGRATION.md`](../MIGRATION.md) §7.1 is a plan written on 2026-08-23 and the org has moved
since. Read as of 2026-09-11, `gh repo list mzizi-dev`:

- The component registry is **`mzizi-dev/mzizi-registry`**, not `nyuchi/mzizi-registry` as §7.1 and §8 have it. Every "nyuchi/" path in those sections resolves to the wrong org.
- The tooling monorepo is **`mzizi-dev/agent-tools`** — `nyuchi/mzizi-tools`, renamed and moved into this org, still private (`.github/workflows/ci.yml` in this repo records the rename). §7.1 planned a split instead: a **public** `mzizi-agents` holding the MCP server, skills bundle and plugins, with the revenue tooling staying behind in `nyuchi/mzizi-tools`. The split has not happened, so the argument §7.1 made for it — "the three things an outside adopter needs in order to use Mzizi travel together and stay public" — is currently unmet, and the repo name also breaks §1's `mzizi-`-prefix rule.
- **`mzizi-dev/mzizi-benchmark` does not exist.** §1 lists it as the second repository to create.

Read again on 2026-09-29, from the supervisor's current-facts list: `api.mzizi.dev` is
`mzizi-dev/mzizi-api-gateway`, a Hono Worker in TypeScript serving the registry's files
bundled at a pinned commit, which replaced the earlier Rust proxy. `mcp.mzizi.dev` is served
from `agent-tools` (`mzizi-mcp`). The registry holds no database (its PR #368). Only the
console uses Supabase. Mzizi's own Rust components are named **Mzizi Roots**.

None of that is fixed by this file, and this file is not the place to fix it — the migration
record is. It is recorded here because an index that pointed at those sections without
saying they are partly stale would be doing the thing this file exists to prevent.

## How this stays current

Four rules, in the order they matter:

1. **This file owns no work items.** It cannot go stale about a plan it does not contain. Every time something here starts to look like a task list, that is the signal to delete it and link instead.
2. **Snapshot numbers carry their source and their date.** The two in this file — 358/3, and the org listing — say where they came from and when they were read, so a reader can re-derive them. Registry [#226](https://github.com/mzizi-dev/mzizi-registry/issues/226) makes the same point about the same numbers and asks for them to be generated from `/api/v1/stats`; when that lands, this file should link to the endpoint and delete the figures.
3. **Three edits are required to keep it honest**, and each has an obvious trigger. The first fired on 2026-09-11: the `contract` subcommand landed, and the gap row it retired was replaced by the narrower one that survived it — comparison against a reference implementation. The second fired on 2026-09-27: a harness prototype landed in `benchmarks/harness/` and RFC-0006 §10.1 resolved the design question the first row's narrower version was tracking, so both gap-table rows above are now struck through rather than deleted — struck through because a prototype proved against one component and a checked-in fixture is not the same claim as "scored against the real corpus", which is still open. The third stands: a number from a benchmark run replaces the whole "Status, stated plainly" section.
4. **Nothing here may imply measurement that has not happened.** That is the failure mode a roadmap has, and the charter's kill criterion is worthless if this document quietly asserts the thing the criterion is supposed to be able to refute.
