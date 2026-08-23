# mzizi-lang

Phase 0 of the Mzizi Rust-framework research charter — see [`CHARTER.md`](./CHARTER.md) for
the full thesis, phasing, and the ownership/interim-location note.

A language whose syntax, type system and compiler feedback loop are designed for **machine
authorship**, and specifically for the machines that need the help most: small open-weight
models with limited parameters, context and long-range attention. It lowers to Rust and
Dioxus; the runtime is the product.

**Status: prototype front end.** The lexer, the recovering parser, the agent diagnostic
protocol, the content-addressed IR and nine primitives exist and are tested. The Phase 0
benchmark has not run, so **nothing here has yet been measured against the charter's kill
criteria**. Contract bodies parse but are not evaluated.

```text
mzizi-lang/
├── CHARTER.md          # the charter, plus resolved/open decisions
├── design/             # the RFCs — read these first
├── compiler/           # the `mz` binary: lex → parse → lower → IR
├── primitives/         # nine primitives written in Mzizi itself
├── examples/           # one real corpus component, ported by hand
└── benchmarks/         # Phase 0 benchmark harness — public; the task set is not
```

> **Moving to `mzizi-dev`.** See [`MIGRATION.md`](./MIGRATION.md) for the repository plan,
> the history-preserving move, the work queue in dependency order, and the do-not-rename
> list. It lives in this directory so a `git subtree split` carries it to the root of the new
> repository.

## The RFCs

| RFC                                                                        | What it settles                                                                                                                                                                                               |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [0001 — syntax](./design/RFC-0001-syntax.md)                               | The nine failure modes an agent hits writing Rust UI code, and the syntax that answers each. `end <kind> <name>`, one construct per intent, enum data columns, contracts, capabilities, no ownership surface. |
| [0002 — runtime and prior art](./design/RFC-0002-runtime-and-prior-art.md) | The design-target correction: small models, not frontier ones. Why the runtime is the product. Thirteen languages mined for ideas, with the licence discipline that keeps the tree clean.                     |
| [0003 — IR](./design/RFC-0003-ir.md)                                       | The eight barriers an agent hits _reading_ a codebase, and the content-addressed IR that answers them. One decision buying incremental compiles, semantic patching, caching and free renames.                 |
| [0004 — test topology](./design/RFC-0004-test-topology.md)                 | What testing is public (nearly all of it) and what is private (a held-out benchmark set), why, and the dependency rule — private consumes public, never the reverse — that keeps forks working.               |

## Using it

```bash
cd compiler
cargo test                            # 75 tests
cargo run -- check    ../primitives/button.mz
cargo run -- check --agent ../examples/connectivity_bar.mz   # NDJSON for an agent
cargo run -- outline  ../primitives/alert.mz                 # the interface, as valid Mzizi
cargo run -- ir       ../primitives/card.mz                  # nodes, hashes, structural paths
```

`mz check --agent` is the surface an agent should use: whole-program NDJSON, deterministic
order, at most one diagnostic per real error, every message written for a reader with no
prior context, and machine-applicable fixes tagged `exact` or `guess`. See RFC-0001 §4.

CI gates this directory on `cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
`cargo test`, and a `mz check` over every primitive — see the `mzizi-lang` job in
[`../.github/workflows/ci.yml`](../.github/workflows/ci.yml).
