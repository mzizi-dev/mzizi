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

**Phase 0 has not run. Nothing in this project has been measured against the charter's kill
criterion.** [`CHARTER.md`](../CHARTER.md) §4 makes Phase 0 the gate on everything after it:

> If this doesn't show a measurable advantage, nothing downstream matters — don't build
> Phase 1 until Phase 0 has a real number attached to it.

There is no such number. What exists is a prototype front end — lexer, recovering parser,
agent NDJSON protocol, content-addressed IR, nine primitives — and the measurements in
`compiler/tests/ir_measured.rs`, which measure the IR's own properties, not the charter's
claim. The [README](../README.md) states this; it is repeated here because a roadmap is
exactly the document that tends to imply progress it cannot show.

Two things are load-bearing and absent:

| Gap | Evidence | Consequence |
| --- | --- | --- |
| **Contract bodies parse but are not evaluated** | `mz` accepts `check`, `outline`, `hash`, `ir` and no `contract` subcommand (`compiler/src/main.rs`) | The charter's defect metric — "compiles cleanly but is behaviourally wrong" ([`CHARTER.md`](../CHARTER.md) §6) — has no toolchain that can measure it. [`MIGRATION.md`](../MIGRATION.md) §4.1 calls this the highest-value work available, and it is first in the queue for that reason. |
| **The benchmark harness does not exist** | [`benchmarks/`](../benchmarks/README.md) holds the resolved decisions and no runner | Its most important metric depends on the row above. [`MIGRATION.md`](../MIGRATION.md) §4.2. |

## Phases

Defined in [`CHARTER.md`](../CHARTER.md) §4. Reproduced here only as far as their *gates*,
because the phase descriptions belong to the charter and a second copy of them would drift.

| Phase | State | Gate on the next one |
| --- | --- | --- |
| **0** — prove the core claim, no rendering | in progress; **unmeasured** | A real number from the benchmark. Until then Phase 1 is not started, by the charter's own rule. |
| **1** — Dioxus renderer interop, WASM/native artifact | not started | Blocked on Phase 0's number |
| **2** — edge-first deployment | not started | Blocked on Phase 1 |
| **3** — Candle integration | not started | Blocked on Phase 1 |
| **4** — distribution adapters, Astro first | not started | Blocked on Phase 1's artifact standing alone |

## Where each plan lives

Every one of these is next to the code it plans. That is the point.

| Area | The plan | Where |
| --- | --- | --- |
| The language and compiler | Work queue in dependency order: contract evaluation → benchmark harness → `mz patch`/`refs`/`diff`, IR store persistence, local state → Phase 1 | [`MIGRATION.md`](../MIGRATION.md) §4, in this repo beside `compiler/` |
| Language design | RFC-0001 syntax, RFC-0002 runtime and prior art, RFC-0003 IR, RFC-0004 test topology | [`design/`](.) |
| Phase 0 benchmark | Corpus, defect definition and visibility split resolved; harness mechanics unresolved | [`benchmarks/README.md`](../benchmarks/README.md), [`CHARTER.md`](../CHARTER.md) §6, [RFC-0004](./RFC-0004-test-topology.md) |
| The benchmark corpus — the Rust reference implementations Phase 0 scores against | Epic plus four dependency-ordered waves | [`mzizi-dev/mzizi-registry#222`](https://github.com/mzizi-dev/mzizi-registry/issues/222) → [#223](https://github.com/mzizi-dev/mzizi-registry/issues/223), [#224](https://github.com/mzizi-dev/mzizi-registry/issues/224), [#225](https://github.com/mzizi-dev/mzizi-registry/issues/225), [#226](https://github.com/mzizi-dev/mzizi-registry/issues/226) |
| The registry's own correctness backlog | Open issues, labelled | [`mzizi-dev/mzizi-registry` issues](https://github.com/mzizi-dev/mzizi-registry/issues) |
| What a registry is for once the language ships | RFC-0005, drafted as an issue; it belongs in [`design/`](.) and is not here yet | [`mzizi-dev/agent-tools#76`](https://github.com/mzizi-dev/agent-tools/issues/76) |
| Org topology, renames, and what each rename breaks | The migration record | [`MIGRATION.md`](../MIGRATION.md) §6, §7 |
| Decisions still owed by the owner | Four, one now three | [`MIGRATION.md`](../MIGRATION.md) §9 |

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

1. **The npm scope for anything the framework publishes** — `@bundu`, `@nyuchi`, or `@mzizi`. One-way door once published.
2. **The held-out set's home** — a private repository, or one withheld directory. [`MIGRATION.md`](../MIGRATION.md) §5.
3. ~~**`mzizi-dev/roadmap`**~~ — resolved; this file.
4. **Whether the React design system is renamed** to say what it is.

## Drift already present in the migration record

[`MIGRATION.md`](../MIGRATION.md) §7.1 is a plan written on 2026-08-23 and the org has moved
since. Read as of 2026-09-11, `gh repo list mzizi-dev`:

- The component registry is **`mzizi-dev/mzizi-registry`**, not `nyuchi/mzizi-registry` as §7.1 and §8 have it. Every "nyuchi/" path in those sections resolves to the wrong org.
- The tooling monorepo is **`mzizi-dev/agent-tools`** — `nyuchi/mzizi-tools`, renamed and moved into this org, still private (`.github/workflows/ci.yml` in this repo records the rename). §7.1 planned a split instead: a **public** `mzizi-agents` holding the MCP server, skills bundle and plugins, with the revenue tooling staying behind in `nyuchi/mzizi-tools`. The split has not happened, so the argument §7.1 made for it — "the three things an outside adopter needs in order to use Mzizi travel together and stay public" — is currently unmet, and the repo name also breaks §1's `mzizi-`-prefix rule.
- **`mzizi-dev/mzizi-benchmark` does not exist.** §1 lists it as the second repository to create.

None of that is fixed by this file, and this file is not the place to fix it — the migration
record is. It is recorded here because an index that pointed at those sections without
saying they are partly stale would be doing the thing this file exists to prevent.

## How this stays current

Four rules, in the order they matter:

1. **This file owns no work items.** It cannot go stale about a plan it does not contain. Every time something here starts to look like a task list, that is the signal to delete it and link instead.
2. **Snapshot numbers carry their source and their date.** The two in this file — 358/3, and the org listing — say where they came from and when they were read, so a reader can re-derive them. Registry [#226](https://github.com/mzizi-dev/mzizi-registry/issues/226) makes the same point about the same numbers and asks for them to be generated from `/api/v1/stats`; when that lands, this file should link to the endpoint and delete the figures.
3. **Three edits are required to keep it honest**, and each has an obvious trigger: a `contract` subcommand landing in `mz` retires the first gap row; a runner landing in `benchmarks/` retires the second; a number from a benchmark run replaces the whole "Status, stated plainly" section.
4. **Nothing here may imply measurement that has not happened.** That is the failure mode a roadmap has, and the charter's kill criterion is worthless if this document quietly asserts the thing the criterion is supposed to be able to refute.
