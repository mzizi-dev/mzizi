# Mzizi

**A general-purpose framework for the agentic world, in Rust** — a language whose syntax,
type system and compiler feedback loop are designed for **machine authorship**, and
specifically for the machines that need the help most: small open-weight models with
limited parameters, context and long-range attention.

[![CI](https://github.com/mzizi-dev/mzizi/actions/workflows/ci.yml/badge.svg)](https://github.com/mzizi-dev/mzizi/actions/workflows/ci.yml)
[![Lint](https://github.com/mzizi-dev/mzizi/actions/workflows/lint.yml/badge.svg)](https://github.com/mzizi-dev/mzizi/actions/workflows/lint.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)
![Rust](https://img.shields.io/badge/Rust-edition_2024-000000?style=flat-square&logo=rust&logoColor=white)
![Dependencies](https://img.shields.io/badge/dependencies-none-informational?style=flat-square)

**Crate:** `mzizi-lang-compiler` 0.0.0 (`publish = false`, no release) | **Binary:** `mz` |
**Tests:** 174 | **Phase:** 0, unmeasured

## The bet

Every framework that won, won by being unmistakably better at one thing first — not by
matching every existing framework's feature set on day one. React was Facebook's fix for one
rendering problem. Svelte bet on a single contrarian idea years before anyone else took it
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

An enum variant carrying **two facts as structured data** — a Tailwind class _and_ a pixel
height — where a hand-written `.tsx` equivalent hides the height inside a class string an
agent has to parse back out. That's the syntax bet in one line: put what an agent needs to
reason about in the type system, not in a string it has to re-derive.

This claim doesn't stop at UI components. An agent authoring a server handler, a Cloudflare
Worker, an ML pipeline, or eventually a native mobile app is doing the same thing at the
syntax/compiler layer. See [`CHARTER.md`](./CHARTER.md) for the full thesis, the five-phase
plan, the explicit non-goals, and — most importantly — the kill criterion this project holds
itself to.

**This repository is the language.** It is not the component registry — that's
[`mzizi-dev/mzizi-registry`](https://github.com/mzizi-dev/mzizi-registry), a different body
of work with a different owner that happens to share the name and the org.

---

## Status: prototype front end, nothing measured yet

**The honest version of "wow" for a research project is not a claim — it's a discipline.**
Here is exactly what exists, what doesn't, and what would have to be true for the bet above
to pay off.

**Built and tested (174 tests, gated in CI):** the lexer, the recovering parser, the name and type resolver, the agent
diagnostic protocol (`mz check --agent`), the content-addressed IR, `mz outline`, contract
evaluation (`mz contract`), and nine primitives written in Mzizi itself.

**What contract evaluation does and doesn't do:** `mz contract <file>` evaluates a
component's own `contract` block against its own declarations and exits 1 if an assertion
doesn't hold — all 45 assertions in the corpus evaluated, none merely counted. It checks
nothing rendered, and **nothing against a reference implementation** — that's the other half
of the Phase 0 defect metric, and it doesn't exist yet ([RFC-0006](./design/RFC-0006-contracts.md) §5, §10.1).

**What doesn't exist yet:** reference-implementation comparison, lowering to Rust, code
generation, a runtime, rendering, a release, a published binary.

**The number that decides everything:** the Phase 0 benchmark — an LLM agent authoring N
equivalent components in Mzizi's syntax vs. raw Dioxus/Leptos, scored on tokens consumed,
iterations to a clean compile, and defect rate — **has not run.** If it doesn't show a
measurable advantage on at least two of three metrics, the thesis is wrong and Phase 1 does
not start. That's not hedging — it's the actual, written kill criterion (CHARTER.md §4), and
every claim in the RFCs is a design claim waiting on that number.

Where an RFC and the code disagree, **the code is the fact** — see
[RFC-0003](./design/RFC-0003-ir.md) §7.1 for a claim the implementation disproved and the
narrower claim that replaced it.

## Try it

```bash
git clone https://github.com/mzizi-dev/mzizi.git && cd mzizi/compiler
cargo test                                                           # 174 tests, zero dependencies
cargo run --bin mz -- check ../primitives/button.mz                  # does this compile
cargo run --bin mz -- contract ../primitives/button.mz                # does it do what it says
```

A stable Rust toolchain is the entire dependency list — see
[`AGENTS.md`](./AGENTS.md) for the full command set, the CI gates, and what an agent working
in this repo needs to know before pushing.

## Layout

```text
mzizi/
├── CHARTER.md          # the thesis, phasing, non-goals, kill criterion — read this first
├── AGENTS.md           # machine-facing: build commands, CI gates, conventions
├── design/             # the RFCs, and ROADMAP.md — read these next
├── compiler/           # the `mz` binary: lex → parse → lower → IR
├── primitives/         # nine primitives written in Mzizi itself
├── examples/           # one real corpus component, ported by hand
└── benchmarks/         # Phase 0 benchmark harness — public; the task set is not
```

## The RFCs

| RFC                                                                        | What it settles                                                                                        |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| [0001 — syntax](./design/RFC-0001-syntax.md)                               | The nine failure modes an agent hits writing Rust UI code, and the syntax that answers each.           |
| [0002 — runtime and prior art](./design/RFC-0002-runtime-and-prior-art.md) | The design-target correction: small models, not frontier ones. Why the runtime is the product.         |
| [0003 — IR](./design/RFC-0003-ir.md)                                       | The eight barriers an agent hits _reading_ a codebase, and the content-addressed IR that answers them. |
| [0004 — test topology](./design/RFC-0004-test-topology.md)                 | What testing is public vs. held-out, and the dependency rule that keeps forks working.                 |
| [0006 — contracts](./design/RFC-0006-contracts.md)                         | The contract clause grammar, what `mz contract` proves, and what it can't.                             |

RFC-0005 is reserved (`mzizi-dev/agent-tools#76`, private — not linked, since a link to a
private repo 404s for anyone without access) but not yet written. Every RFC ends with open
questions addressed to the next one; resolved questions are struck through in place, not
deleted, so the document records what was believed as well as what is believed now.

[`design/ROADMAP.md`](./design/ROADMAP.md) is the index of where every plan in the Mzizi
ecosystem lives. It holds no work items of its own — plans live next to the code they plan.

## Where this sits in the ecosystem

Mzizi-the-language is one repository in [`mzizi-dev`](https://github.com/mzizi-dev), the
Foundation-governed Mzizi org. **The language repo is plain `mzizi`; everything else is
`mzizi-`-prefixed** — the language is the project.

| Repository                                                            | What it is                                                                                        | Relationship to this repo                                                               |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| [`mzizi-registry`](https://github.com/mzizi-dev/mzizi-registry)       | The canonical component registry — 577+ components, the brand system, the DNA-helix architecture. | **The benchmark corpus.** Charter §6 makes Mzizi's own components the Phase 0 task set. |
| [`mzizi-docs`](https://github.com/mzizi-dev/mzizi-docs)               | The documentation site — language reference, RFC index, registry guides.                          | Documents this repo. The RFCs stay **here**, next to the code they govern.              |
| [`mzizi-site`](https://github.com/mzizi-dev/mzizi-site)               | The front door — serves `mzizi.dev`.                                                              | Publishes; is not depended on.                                                          |
| [`mzizi-api-gateway`](https://github.com/mzizi-dev/mzizi-api-gateway) | A pure-Rust Cloudflare Worker for `api.mzizi.dev`.                                                | Serves the registry, not the language.                                                  |
| `mzizi-benchmark`                                                     | Planned, not yet created — the public Phase 0 harness.                                            | Will consume `mz` from this repo.                                                       |

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

Mzizi is **100% Bundu Foundation IP** (CHARTER.md), licensed under the
[Apache License 2.0](./LICENSE) — an open-architecture project of the **Bundu Foundation**,
operated and developed by **Nyuchi**.
