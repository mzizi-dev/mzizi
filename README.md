# Mzizi

**Mzizi is built to make Rust better, the way TypeScript makes JavaScript better.**

Mzizi is a general-purpose programming language whose syntax, type system and compiler
feedback loop are designed for **machine authorship**, and specifically for the machines that
need the help most: small open-weight models with limited parameters, context and long-range
attention. Rust is its platform, the way JavaScript is TypeScript's: Mzizi is designed to lower
to Rust, with no borrows, lifetimes or ownership at the surface ([RFC-0001](./design/RFC-0001-syntax.md)
§1.8). **That is the design and the goal, not the state:** the compiler emits no Rust yet, and
"makes Rust better" is what Phase 0 exists to test. The harness is the core of the language,
what an agent reads ([RFC-0012](./design/RFC-0012-harness.md), a draft), and Mzizi Roots is its
component model, the way React is JavaScript's.

[![CI](https://github.com/mzizi-dev/mzizi/actions/workflows/ci.yml/badge.svg)](https://github.com/mzizi-dev/mzizi/actions/workflows/ci.yml)
[![Lint](https://github.com/mzizi-dev/mzizi/actions/workflows/lint.yml/badge.svg)](https://github.com/mzizi-dev/mzizi/actions/workflows/lint.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)
![Rust](https://img.shields.io/badge/Rust-edition_2024-000000?style=flat-square&logo=rust&logoColor=white)
![Compiler dependencies](https://img.shields.io/badge/compiler_dependencies-none-informational?style=flat-square)

**Crate:** `mzizi-lang-compiler` 0.0.0 (`publish = false`, no release) | **Binary:** `mz` |
**Tests:** 335 (`cargo test --workspace`) | **Phase:** 0, two pilots, kill criterion not yet run

## The bet

Every language or framework that won, won by being unmistakably better at one thing first — not by
matching every incumbent's feature set on day one. TypeScript made JavaScript better and still
runs as JavaScript. React was Facebook's fix for one rendering problem. Svelte bet on a single contrarian idea years before anyone else took it
seriously. Mzizi's bet: **the actual bottleneck of 2026-and-beyond development is an agent
iterating against a compiler in a tight loop, thousands of times a session** — and no
existing language, Rust included, was designed with that reader in mind. Every syntax
decision, every error message, every unit of the intermediate representation is aimed at a
machine reading it, not a human.

Here's what that looks like in practice — a real primitive from this repo, not a toy example:

```mz
enum button_size
  default   class "h-14 gap-2 px-5"     height 56
  sm        class "h-12 gap-1.5 px-4"   height 48
  icon      class "size-14"             height 56
end
```

An enum is a table: every fact about a variant sits on its row, where a `.tsx` spreads them
across a `cva` call and a string it has to parse back out. The `height` column is also the
language's first measured lesson. Pilot 2 found two 7B-model ports with `icon class "size-14"
height 48`: one fact written twice, and the two copies disagreeing. The compiler now reads the
height a `h-N` / `size-N` class renders, a row may leave `height` out, and a row whose
`height` disagrees with its class is a compile error with the right number as its fix
([RFC-0006](./design/RFC-0006-contracts.md) §5). The syntax bet is to put what an agent needs
to reason about where the compiler can check it.

This claim doesn't stop at UI components. An agent authoring a server handler, a Cloudflare
Worker, an ML pipeline, or eventually a native mobile app is doing the same thing at the
syntax/compiler layer. The direction, as of 2026-09-29, is that everything built with Mzizi
carries a contract (components, language functions, backend handlers), and that the benchmark
extends to TypeScript/React, Python, Go, C++ and Rust backends. Those are RFCs in progress,
not shipped. See [`CHARTER.md`](./CHARTER.md) for the full thesis, the five-phase
plan, the explicit non-goals, and — most importantly — the kill criterion this project holds
itself to.

**This repository is the language, and its one goal** is building Mzizi as a programming
language that stands against the best existing language for each kind of task: TypeScript,
Python, C++, Go and Rust ([`CHARTER.md`](./CHARTER.md) §1, §4). It is not the component
registry — that's [`mzizi-dev/mzizi-registry`](https://github.com/mzizi-dev/mzizi-registry),
which Mzizi also owns, and which holds the components that support the language.

---

## Status: prototype front end, two pilots, the kill criterion not yet tested

**The honest version of "wow" for a research project is not a claim — it's a discipline.**
Here is exactly what exists, what doesn't, and what would have to be true for the bet above
to pay off.

**Built and tested (335 tests in 14 suites, gated in CI; 213 of them in the compiler crate):**
the lexer, the recovering parser, the name and type resolver, the agent diagnostic protocol
(`mz check --agent`), `mz fix` (every `exact` fix in one pass), the content-addressed IR,
`mz outline`, contract evaluation (`mz contract`), nine primitives written in Mzizi itself,
and the Phase 0 benchmark harness, runner and pilot tasks. `compiler/src` is 7,454 lines.

**What contract evaluation does and doesn't do:** `mz contract <file>` evaluates a
component's own `contract` block against its own declarations and exits 1 if an assertion
doesn't hold — all 45 assertions in the corpus evaluated (29 of them across the nine
primitives), none merely counted. It checks
nothing rendered and nothing against a reference implementation. That comparison is the other
half of the Phase 0 defect metric, and it lives outside the compiler. `benchmarks/harness` diffs
a `.mz` component's variants, defaults, touch heights and (per task) `data-slot` set against
the hand-written Rust reference
([RFC-0006](./design/RFC-0006-contracts.md) §10.1). It is a prototype. It is tested end to end
against one component, `primitives/button.mz`, and a byte-identical copy of the registry's
`button.rs`. The `mzbench` runner calls it to score the pilot tasks in `benchmarks/tasks/`.

**The two pilots, 2026-09-27. Neither showed an advantage for Mzizi.** The
[first](./benchmarks/results/2026-09-27-pilot/RUN.md) was frontier-only, with one seed and no
token counts. The [second](./benchmarks/results/2026-09-27-pilot-2/RUN.md) ran two scored
tasks with three seeds on two models. On the frontier model the Mzizi and Dioxus arms tied on
compile rate and defects, and Mzizi used about 8% fewer transcript tokens. On the ~7B
open-weight model, which RFC-0002 makes the design target, Mzizi did worse on all three
metrics: 2/6 clean compiles against 4/6, both clean episodes defective against none, and more
tokens. Neither pilot is the kill-criterion run. The fixes pilot 2 asked for before that run
are audited, and mostly made, in
[`benchmarks/READINESS.md`](./benchmarks/READINESS.md). The held-out task set it needs does
not exist yet.

**What doesn't exist yet:** the kill-criterion run and its held-out task set, lowering to Rust,
code generation, a runtime, rendering, a release, a published binary.

**The number that decides everything:** the Phase 0 benchmark — an LLM agent authoring
equivalent code in Mzizi and in the best existing language for each kind of task, scored on
tokens consumed, iterations to a clean check, and defect rate — **has not run** as the
kill-criterion measurement. The pilots are not it. The rule is
[RFC-0009](./design/RFC-0009-comparison-benchmark.md) §6, which amends CHARTER.md §4. If Mzizi
does not beat the best incumbent on two of the three metrics in a gating task family, on
held-out tasks, the thesis is wrong for that family and the work it gates does not start.
That's not hedging — it's the actual, written kill criterion, and
every claim in the RFCs is a design claim waiting on that number.

Where an RFC and the code disagree, **the code is the fact** — see
[RFC-0003](./design/RFC-0003-ir.md) §7.1 for a claim the implementation disproved and the
narrower claim that replaced it.

## Try it

```bash
git clone https://github.com/mzizi-dev/mzizi.git && cd mzizi/compiler
cargo test                                                           # 213 tests; the compiler crate has zero dependencies
cargo run --bin mz -- check ../primitives/button.mz                  # does this compile
cargo run --bin mz -- contract ../primitives/button.mz                # does it do what it says
cargo run --bin mz -- fix path/to/file.mz                            # apply every exact fix, then re-check
```

The compiler crate needs only a stable Rust toolchain. It has no dependencies of its own. The
workspace's benchmark runner (`benchmarks/runner`) depends on `ureq` and `serde_json`, each
pinned to an exact version. Cargo resolves the whole workspace, so the first build needs the
crates.io index even though it downloads no crates for the compiler. After that it works
offline. See [`AGENTS.md`](./AGENTS.md) for the full command set, the CI gates, and what an
agent working in this repo needs to know before pushing.

## Layout

```text
mzizi/
├── CHARTER.md          # the thesis, phasing, non-goals, kill criterion — read this first
├── AGENTS.md           # machine-facing: build commands, CI gates, conventions
├── design/             # the RFCs, and ROADMAP.md — read these next
├── compiler/           # the `mz` binary: lex → parse → lower → IR
├── primitives/         # nine primitives written in Mzizi itself
├── examples/           # two real corpus components, ported by hand
└── benchmarks/         # Phase 0 benchmark harness, runner, arms, pilot tasks and results; READINESS.md; the held-out set is private
```

## The RFCs

| RFC                                                                            | What it settles                                                                                                                    |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| [0001 — syntax](./design/RFC-0001-syntax.md)                                   | The nine failure modes an agent hits writing Rust UI code, and the syntax that answers each.                                       |
| [0002 — runtime and prior art](./design/RFC-0002-runtime-and-prior-art.md)     | The design-target correction: small models, not frontier ones. Why the runtime is the product.                                     |
| [0003 — IR](./design/RFC-0003-ir.md)                                           | The eight barriers an agent hits _reading_ a codebase, and the content-addressed IR that answers them.                             |
| [0004 — test topology](./design/RFC-0004-test-topology.md)                     | What testing is public vs. held-out, and the dependency rule that keeps forks working.                                             |
| [0006 — contracts](./design/RFC-0006-contracts.md)                             | The contract clause grammar, what `mz contract` proves, and what it can't.                                                         |
| [0007 — gap register](./design/RFC-0007-gap-register.md)                       | What the charter's scope needs that the language and compiler lack, checked against the code, and the order to build it in. Draft. |
| [0008 — types, lists, records](./design/RFC-0008-types-collections-records.md) | Types, lists, records, options and `for each`, with a resolver that can say no.                                                    |
| [0009 — comparison benchmark](./design/RFC-0009-comparison-benchmark.md)       | The arms, the task families, the best-incumbent kill criterion and the publication rules.                                          |
| [0010 — contracts everywhere](./design/RFC-0010-contracts-everywhere.md)       | Contracts on functions, handlers, services and the standard library. Draft; nothing implemented.                                   |
| [0012 — the harness](./design/RFC-0012-harness.md)                             | The harness, the core of the language: what an agent reads, the agent protocol and the plugin host. Draft; mostly design.          |

RFC-0005 is reserved (`mzizi-dev/agent-tools#76`, private — not linked, since a link to a
private repo 404s for anyone without access) but not yet written. RFC-0011 is claimed by the
backend work in open pull requests. Every RFC ends with open
questions addressed to the next one; resolved questions are struck through in place, not
deleted, so the document records what was believed as well as what is believed now.

[`design/ROADMAP.md`](./design/ROADMAP.md) is the index of where every plan in the Mzizi
ecosystem lives. It holds no work items of its own — plans live next to the code they plan.

## Where this sits in the ecosystem

Mzizi-the-language is one repository in [`mzizi-dev`](https://github.com/mzizi-dev), the
Mzizi org. **The language repo is plain `mzizi`; everything else is
`mzizi-`-prefixed** — the language is the project.

| Repository                                                            | What it is                                                                                                                                                                                                                                                                                                                               | Relationship to this repo                                                  |
| --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| [`mzizi-registry`](https://github.com/mzizi-dev/mzizi-registry)       | The canonical component registry — 577+ components, the brand system, the DNA-helix architecture. It holds no database: its files are the data layer. Mzizi's own components are being converted to Rust as **Mzizi Roots** (UI and server components for the agentic web). The React/TSX components keep working and are deprioritised. | Supports the language: its components are the UI task corpus (Charter §6). |
| [`mzizi-docs`](https://github.com/mzizi-dev/mzizi-docs)               | `docs.mzizi.dev` (Mintlify), the one home of the docs — language reference, RFC index, registry guides.                                                                                                                                                                                                                                  | Documents this repo. The RFCs stay **here**, next to the code they govern. |
| [`mzizi-site`](https://github.com/mzizi-dev/mzizi-site)               | The front door — `mzizi.dev`, a static Astro site that leads with the language.                                                                                                                                                                                                                                                          | Publishes; is not depended on.                                             |
| [`mzizi-api-gateway`](https://github.com/mzizi-dev/mzizi-api-gateway) | `api.mzizi.dev`: a Hono Cloudflare Worker in TypeScript. It serves the registry's repository files, generated at a pinned registry commit and bundled into the Worker, with no origin and no database. It replaced the earlier Rust proxy.                                                                                               | Serves the registry, not the language.                                     |
| [`agent-tools`](https://github.com/mzizi-dev/agent-tools)             | `mcp.mzizi.dev` (`mzizi-mcp`): registry data and skills from files bundled at build time, and the docs as `docs_*` tools federated from `docs.mzizi.dev`.                                                                                                                                                                                | Serves agents the registry and the docs, not the language.                 |
| Held-out benchmark repository (private, unnamed)                      | Planned, not yet created. It will hold the held-out task set and its expected outputs, which stay private ([CHARTER.md](./CHARTER.md) §6, [RFC-0004](./design/RFC-0004-test-topology.md)). The harness, the runner and the public pilot tasks are in this repo, under `benchmarks/`, and CI runs the harness and runner tests.           | Will run this repo's public harness. This repo never reads from it.        |

**The rule that keeps the ecosystem honest** — RFC-0004 §3 applied to code, not just tests:

> Everything else consumes Mzizi. Mzizi consumes nothing else.

This repository's CI stays green with no access to any other repository, no secrets, no
tokens. A fork with none of those can run the full suite and get a green result — otherwise
the project is open source in name only.

## Contributing

- [`CONTRIBUTING.md`](./CONTRIBUTING.md) — how to build and test, the RFC process, what a
  good commit message looks like here.
- [`AGENTS.md`](./AGENTS.md) — the merge convention, the honesty rule, repo boundaries.
- [`SECURITY.md`](./SECURITY.md) — what counts as a vulnerability in a compiler that reads
  untrusted source, and how to report one privately.
- [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md) — Contributor Covenant 2.1.

## History and ownership

This repository was created by a `git subtree split` out of `mzizi-dev/agent-tools`,
carrying the full design history of the `mzizi-lang/` directory it grew up in.
[`MIGRATION.md`](./MIGRATION.md) is the record of that move.

Mzizi is **100% Mzizi IP** (CHARTER.md), licensed under the
[Apache License 2.0](./LICENSE). Mzizi owns and operates the language, its toolchain, the
registry, the design system, the docs and the API. **Nyuchi** operates the console
(`app.mzizi.dev`, the only part of the ecosystem that uses Supabase) and the revenue products.
Copyright notices name the **Bundu Foundation** as the parent copyright holder; Mzizi is not
a separate legal entity.
