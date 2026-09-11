# Mzizi

**A Rust framework for the agentic web** — a language whose syntax, type system and
compiler feedback loop are designed for **machine authorship**, and specifically for the
machines that need the help most: small open-weight models with limited parameters, context
and long-range attention. It lowers to Rust and Dioxus; the runtime is the product.

This is Phase 0 of the Bundu Foundation's Mzizi research charter. See
[`CHARTER.md`](./CHARTER.md) for the full thesis, the phasing, the explicit non-goals, and
the kill criterion.

---

## Status: prototype front end, nothing measured

Read this before anything else in the repository.

**What exists and is tested:** the lexer, the recovering parser, the agent diagnostic
protocol (`mz check --agent`), the content-addressed IR, `mz outline`, and nine primitives
written in Mzizi itself. 75 tests, all of it gated in CI.

**What does not exist:** contract evaluation — `contract` blocks parse and are then
ignored. There is no lowering to Rust, no code generation, no runtime, no rendering, no
release, and no published binary.

**What has not been measured:** the Phase 0 benchmark has not run, so **nothing here has yet
been measured against the charter's kill criteria**. The charter is unambiguous about what
that means: if the benchmark does not show a measurable advantage over raw Dioxus and
Leptos on at least two of three metrics, the thesis is wrong and Phase 1 does not start.
Every claim in the RFCs is a design claim awaiting that number.

The RFCs are a design record written ahead of an implementation, not a specification of
working software. Where an RFC and the code disagree, the code is the fact — see
[RFC-0003](./design/RFC-0003-ir.md) §7.1, which records a claim the implementation
disproved and the narrower claim that replaced it.

## Layout

```text
mzizi/
├── CHARTER.md          # the charter: thesis, phasing, non-goals, kill criterion
├── MIGRATION.md        # the record of the move out of agent-tools, and the work queue
├── design/             # the RFCs, and ROADMAP.md — read these first
├── compiler/           # the `mz` binary: lex → parse → lower → IR
├── primitives/         # nine primitives written in Mzizi itself
├── examples/           # one real corpus component, ported by hand
└── benchmarks/         # Phase 0 benchmark harness — public; the task set is not
```

## Building and running it

A stable Rust toolchain is the whole dependency list — the compiler crate has no
dependencies, deliberately (see `compiler/Cargo.toml`).

```bash
cd compiler
cargo test                            # 75 tests

cargo run --bin mz -- check    ../primitives/button.mz
cargo run --bin mz -- check --agent ../examples/connectivity_bar.mz   # NDJSON for an agent
cargo run --bin mz -- outline  ../primitives/alert.mz                 # the interface, as valid Mzizi
cargo run --bin mz -- ir       ../primitives/card.mz                  # nodes, hashes, structural paths
```

`mz check --agent` is the surface an agent should use: whole-program NDJSON, deterministic
order, at most one diagnostic per real error, every message written for a reader with no
prior context, and machine-applicable fixes tagged `exact` or `guess`. See RFC-0001 §4.

CI gates this repository on `cargo fmt -- --check`, `cargo clippy --all-targets -D warnings`,
`cargo test`, and a `mz check` over the corpus example and every primitive, plus a
full-history secret scan — see [`.github/workflows/ci.yml`](./.github/workflows/ci.yml).
The two `mz check` steps are not redundant with `cargo test`: `cargo test` proves the test
harness parses the corpus, `mz check` proves the binary this project ships does, and only
the second is the claim the charter makes. [`CONTRIBUTING.md`](./CONTRIBUTING.md) explains
the rest.

## The RFCs

| RFC                                                                        | What it settles                                                                                                                                                                                               |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [0001 — syntax](./design/RFC-0001-syntax.md)                               | The nine failure modes an agent hits writing Rust UI code, and the syntax that answers each. `end <kind> <name>`, one construct per intent, enum data columns, contracts, capabilities, no ownership surface. |
| [0002 — runtime and prior art](./design/RFC-0002-runtime-and-prior-art.md) | The design-target correction: small models, not frontier ones. Why the runtime is the product. Thirteen languages mined for ideas, with the licence discipline that keeps the tree clean.                     |
| [0003 — IR](./design/RFC-0003-ir.md)                                       | The eight barriers an agent hits _reading_ a codebase, and the content-addressed IR that answers them. One decision buying incremental compiles, semantic patching, caching and free renames.                 |
| [0004 — test topology](./design/RFC-0004-test-topology.md)                 | What testing is public (nearly all of it) and what is private (a held-out benchmark set), why, and the dependency rule — private consumes public, never the reverse — that keeps forks working.               |

RFC-0001 was amended by RFC-0002 and carries a note saying so; read the two together. Every
RFC ends with open questions addressed to the next one, and resolved questions are struck
through in place rather than deleted, so the document records what was believed as well as
what is believed now.

[`design/ROADMAP.md`](./design/ROADMAP.md) sits alongside them as the index of where every
plan in the Mzizi ecosystem lives — the language's work queue, the benchmark, the registry
epic that supplies the benchmark's ground truth — and of what has and has not been
measured. It holds no work items of its own, deliberately: plans live next to the code they
plan.

To propose a language change, write an RFC. [`CONTRIBUTING.md`](./CONTRIBUTING.md)
describes the conventions the four existing ones follow.

## Where this sits in the ecosystem

Mzizi-the-language is one repository in [`mzizi-dev`](https://github.com/mzizi-dev), the
Foundation-governed Mzizi org. The naming rule is that **the language repo is plain
`mzizi`; everything else in the org is `mzizi-`-prefixed** — the language is the project.

| Repository                                                                | What it is                                                                                                                                            | Relationship to this repo                                                                                                                                |
| ------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`mzizi-registry`](https://github.com/mzizi-dev/mzizi-registry)           | The canonical component registry — 571+ components, brand system, developer portal. Formerly `nyuchi/mzizi`.                                          | **The benchmark corpus.** Charter §6 makes Mzizi's own components the Phase 0 task set, and the hand-written `.rs` ports there are the ground truth.      |
| [`mzizi-docs`](https://github.com/mzizi-dev/mzizi-docs)                   | Documentation content for mzizi.dev — language reference, RFC index, runtime guides, agent-facing surfaces.                                            | Documents this repo. The RFCs stay **here**, next to the code they govern, so tests like `compiler/tests/ir_measured.rs` can verify their numeric claims. |
| [`mzizi-site`](https://github.com/mzizi-dev/mzizi-site)                   | mzizi.dev — the live site tying the language, the registry and the docs together.                                                                     | Publishes; is not depended on.                                                                                                                           |
| [`mzizi-api-gateway`](https://github.com/mzizi-dev/mzizi-api-gateway)     | api.mzizi.dev — the registry API, as a pure-Rust Cloudflare Worker.                                                                                   | Serves the registry, not the language.                                                                                                                   |
| ~~`mzizi-roadmap`~~                                                       | **Archived 2026-09-11.** Folded into [`design/ROADMAP.md`](./design/ROADMAP.md).                                                                       | A roadmap living apart from the code it plans is how plans go stale; it now lives here.                                                                  |
| `mzizi-benchmark`                                                         | **Planned, not yet created.** The public Phase 0 harness: runner, metric definitions, scoring code, fixture format. Today `benchmarks/` holds only the design. | Will consume `mz` from this repo.                                                                                                                        |

**The rule that makes the ecosystem honest, and it is RFC-0004 §3 applied to code instead
of tests:**

> Everything else consumes Mzizi. Mzizi consumes nothing else.

This repository's CI must stay green with no access to any other repository, no secrets and
no tokens. A fork that has none of those must be able to run the full suite and get a green
result — otherwise the project is open source in name only. The same rule governs the
held-out benchmark set (RFC-0004): private consumes public, public never consumes private,
and a missing private result is `neutral`, never `failure`.

## Contributing

- [`CONTRIBUTING.md`](./CONTRIBUTING.md) — how to build and test, the merge-only convention
  and why commit messages here are the research record, what a good commit message looks
  like, and the RFC process.
- [`SECURITY.md`](./SECURITY.md) — what counts as a vulnerability in a compiler that reads
  untrusted source, and how to report one privately.
- [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md) — Contributor Covenant 2.1.

Pull requests are **merged, never squashed or rebased**. MIGRATION.md §1.1: "Squash
discards the per-commit reasoning this project depends on."

## History and ownership

This repository was created by a `git subtree split` out of `mzizi-dev/agent-tools`,
carrying the full fifteen-commit design history of the `mzizi-lang/` directory it grew up
in. [`MIGRATION.md`](./MIGRATION.md) is the record of that move — the repository plan, the
history-preserving procedure, the remaining work queue in dependency order, and the
do-not-rename list. It is a document written at a point in time and it names some
repositories by slugs that have since changed; the settings tables and the work queue are
still current.

Mzizi is **100% Bundu Foundation IP** (CHARTER.md), licensed **Apache-2.0** — see
[`LICENSE`](./LICENSE).
