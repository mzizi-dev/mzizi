# Mzizi language tracker

**The one list of what Mzizi still needs before we can say it is a working programming language,
stacked against the languages it aims to replace.** Owner decision, 2026-09-30: this file is
the ultimate tracker. If a capability is not ✅ here, Mzizi does not have it, whatever any
other page says.

Mzizi is a general-purpose programming language, built to make Rust better, the way
TypeScript makes JavaScript better ([CHARTER.md](./CHARTER.md) v0.4). You write `.mz`; the
compiler is designed to lower it to Rust. Phase 0 has one goal: build Mzizi as a programming
language, measured against the best existing language for each kind of task
([RFC-0009](./design/RFC-0009-comparison-benchmark.md)). This tracker is how that goal is
broken into work.

## How to use this file

- **Every PR that changes a row's status updates the row in the same PR**, with its evidence
  (a file, a test, an RFC or a PR number), and says so in its CHANGELOG entry. A PR that adds a
  capability without updating this file is incomplete.
- **Status is what the code on `main` does, never what an RFC designs.** Where an RFC and
  the code disagree, the code is the fact (AGENTS.md).
- The site ([mzizi.dev](https://mzizi.dev)), the docs ([docs.mzizi.dev](https://docs.mzizi.dev))
  and the agent skills describe Mzizi's capabilities from this file. The freshness agents
  check them against it.

| Mark | Meaning                                                                                  |
| ---- | ---------------------------------------------------------------------------------------- |
| ✅   | On `main`, tested in CI.                                                                 |
| 🟡   | Partly there: a narrow form exists, or it exists in an open PR only. The row says which. |
| 📝   | Designed in an RFC, not implemented.                                                     |
| ❌   | Neither designed nor implemented.                                                        |

## Where Mzizi stands today (2026-09-30, `main` with the backend slice, #29–#33)

What exists is the front end of a language whose first domain is UI components:

- **Syntax:** 23 keywords (`compiler/src/lex.rs`): `component`, `end`, `use`, `enum`, `prop`,
  `view`, `fn`, `contract`, `when`, `else`, `not`, `is`, `match`, `case`, `for`, `each`, `in`,
  `emit`, `nothing`, `none`, `event`, `true`, `false`. One component per file.
- **Types:** `bool`, `int`, `text`, enums with data columns, records, `list(T)`, `option(T)`,
  `event(T)` (`compiler/src/ast.rs`, [RFC-0008](./design/RFC-0008-types-collections-records.md)).
- **Toolchain:** `mz check` (with `--agent`, NDJSON diagnostics), `mz fix`, `mz contract`,
  `mz outline`, and `mz build` for a service only; a recovering parser (blocks nest at most
  64 deep, past which one `MZ0411`; RFC-0001 §4.7 item 8), a resolver, a
  content-addressed IR ([RFC-0003](./design/RFC-0003-ir.md)). About 12,916 lines in
  `compiler/src`, 437 tests (`cargo test --workspace`).
- **Written in Mzizi:** nine primitives (`primitives/`), two component examples and one
  service, `examples/registry.mz` (`examples/`).
- **The backend slice ([RFC-0011](./design/RFC-0011-handlers.md), #29–#33):** a `service` with
  HTTP routes and handlers (`when`, `header`, `respond`), checked by `mz check` (#30) and run
  in process by `mz contract` (#31). A `service` lowers to a local Rust + axum package
  (`mz build`, #32), which CI compiles, tests and serves; no component lowers yet. The
  `mzizi-be` arm, the probe crate `mzprobe` and backend task B1 exist (#33); no backend
  episode has run.

**Not on `main` yet: the core language's foundation slice ([RFC-0013] §18.1, Wave 0), in
open PR #80 to `staging`.** It adds a `program` file with `fn main`; `fn`s with typed
parameters and return types, calls and recursion; `let`, `var` and assignment; `when` /
`else` and `return` in a function body; `int`, `bool` and `text` values with `+ - * / %`,
comparison, `and` / `or` / `not` and interpolation; and `print`. `mz check` types it
(`MZ09xx` codes, with `exact` fixes for the idioms other languages bring), and `mz run`
lowers it to a dependency-free Rust package, builds it with Cargo and runs it, with integer
overflow and division by zero trapping (exit 101). `mz build` writes the same package. A
program's blocks and expressions nest at most 32 deep together, past which one `MZ0411`. Two
example programs, `examples/hello.mz` and `examples/fib.mz`, run in CI through `mz run`
with their output compared. The rows below that it touches are 🟡 and say so; they turn ✅
when it reaches `main`.

What does not exist yet is almost everything a general-purpose program needs: expressions,
variables, callable functions, loops, error handling, modules, a standard library, and
lowering for anything but a service. `fn` bodies are not modelled beyond `emit`
(`compiler/src/ast.rs`). That is why the public benchmark suites are blocked: they need
functions.

## Against the languages Mzizi aims to replace

| Area                                     | Python                 | Go                  | C++                  | TypeScript           | Rust                             | **Mzizi**                                                                                                                                      |
| ---------------------------------------- | ---------------------- | ------------------- | -------------------- | -------------------- | -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| Expressions, variables, functions, loops | ✓                      | ✓                   | ✓                    | ✓                    | ✓                                | 🟡 in a `program`, in open PR #80: int and bool expressions, `let` / `var`, functions and recursion, `when`; no loops ([RFC-0013] Wave 0)      |
| Numbers                                  | int, float, decimal    | int, float          | int, float           | number, bigint       | int, float                       | 🟡 `int` only                                                                                                                                  |
| Text operations                          | ✓                      | ✓                   | ✓                    | ✓                    | ✓                                | ❌ (`text` type only)                                                                                                                          |
| Collections                              | list, dict, set, tuple | slice, map          | vector, map, set     | array, map, set      | Vec, HashMap, tuple              | 🟡 `list`, `option`; no map, set or tuple                                                                                                      |
| User types                               | classes                | structs, interfaces | classes, templates   | interfaces, generics | structs, enums, traits, generics | 🟡 enums with data, records; no methods, traits or generics                                                                                    |
| Error handling                           | exceptions             | `error` values      | exceptions           | exceptions           | `Result`, `?`                    | ❌ (contracts check behaviour; they are not error handling)                                                                                    |
| Modules and imports                      | ✓                      | packages            | headers, modules     | ES modules           | crates, modules                  | ❌ (one component per file)                                                                                                                    |
| Standard library                         | large                  | large               | large                | JS runtime           | std                              | ❌                                                                                                                                             |
| Concurrency                              | asyncio, threads       | goroutines          | threads              | async/await          | async, threads                   | ❌                                                                                                                                             |
| Compiles to something that runs          | bytecode               | native              | native               | JavaScript           | native                           | 🟡 a `service` → a local Rust + axum package (#32); in open PR #80, a `program` → a dependency-free Rust package that `mz run` builds and runs |
| Uses the host ecosystem                  | C extensions           | cgo                 | C ABI                | every JS library     | crates.io                        | ❌ (no Rust crate interop)                                                                                                                     |
| Package manager                          | pip                    | go mod              | vcpkg, conan         | npm                  | Cargo                            | ❌                                                                                                                                             |
| Formatter, editor support                | black, LSP             | gofmt, gopls        | clang-format, clangd | prettier, tsserver   | rustfmt, rust-analyzer           | ❌ (canonical form designed in RFC-0001)                                                                                                       |
| Tests                                    | pytest                 | go test             | many                 | many                 | cargo test                       | 🟡 `mz contract` for contracts; no `mz test`                                                                                                   |
| Diagnostics built for agents             | ✗                      | ✗                   | ✗                    | ✗                    | partial (JSON)                   | ✅ `mz check --agent`, `mz fix`                                                                                                                |
| Contracts on everything                  | ✗                      | ✗                   | ✗                    | ✗                    | ✗                                | ✅ components; 🟡 services, run in process by `mz contract` (#31), `ensure` tested over generated requests, not proven ([RFC-0010])            |
| Spec and stability                       | ✓                      | ✓                   | ISO                  | ✓                    | ✓                                | ❌ (RFCs are design, not a reference)                                                                                                          |

Mzizi's two ✅ rows are where it is already different on purpose. Every other row is work.

## The work, tier by tier

"Done when" is the acceptance test for each row. A row turns ✅ only when that test is on
`main` and green.

### Tier 1: the core language (write any program)

| ID  | Capability                                                                                             | Status | Evidence                                                                                                                                                                                                                                                                                                                                                                                                                                             | Done when                                                                                                 |
| --- | ------------------------------------------------------------------------------------------------------ | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| C1  | Expressions and operators: arithmetic, comparison, boolean logic, text concatenation and interpolation | 🟡     | In open PR #80, in a `program` ([RFC-0013] Wave 0): `+ - * / %` and unary `-` on `int`, `is`, `is not`, `< <= > >=`, `and`, `or`, `not`, parentheses, §3.5's precedence, and `{expr}` interpolation in text; `examples/fib.mz` computes and prints, and `mz check` types it (`compiler/tests/program.rs`). Not yet: `float`, `in`, the `and`/`or` mixing rule (`MZ0913` covers chained comparison only), §3.8's text forms beyond int, bool and text | `examples/` computes and prints a value from an expression, and `mz check` types it                       |
| C2  | Bindings: named values, scope, shadowing rules                                                         | 🟡     | `let`, `var` and assignment in a `program` ([RFC-0013] §5, Wave 0): block scope, no shadowing (`MZ0921`), use before binding or after its block (`MZ0920`), assignment to a `let` (`MZ0922`), Python's first assignment (`MZ0923`, `exact` `let` / `var`); tested in `compiler/tests/program.rs`, used in `examples/fib.mz`. **Done in PR #80 (open, to `staging`); ✅ when it reaches `main`**                                                      | A value can be named and reused; the resolver reports use before binding                                  |
| C3  | Functions: parameters, return types, calls, recursion                                                  | 🟡     | `fn name(a: int): int` … `end fn name` in a `program` ([RFC-0013] §6, Wave 0): typed parameters and return types, calls, recursion, every path returns (`MZ0906`), wrong calls (`MZ0905`). `examples/fib.mz` recurses and prints `fib(9) is 34` and `fib(20) is 6765` through `mz run`, in `compiler/tests/program.rs` and CI's `lowering` job. **Done in PR #80 (open, to `staging`); ✅ when it reaches `main`**                                   | One function calls another with arguments and returns a typed value; recursion works                      |
| C4  | Control flow: `when`/`else` and `match` as expressions, loops, early return                            | 🟡     | In open PR #80, in a `program` function body: `when` / `else` and early `return` ([RFC-0013] Wave 0). `else when`, `match`, `for each`, `while`, `break` and `continue` in a function body are `MZ0919` (designed, not built). In views and handlers, `when`/`else` and `for each` as before; `match` is `MZ0410` in a view                                                                                                                          | All four work in a function body, and non-exhaustive `match` is a diagnostic                              |
| C5  | Numbers: floats or decimals, overflow and division semantics                                           | 🟡     | `int` only. In open PR #80, in a `program`, integer overflow and division or remainder by zero trap with exit 101 and a line naming the `.mz` position, division truncates toward zero and `%` takes the dividend's sign, all tested through `mz run` (`compiler/tests/program.rs`); a constant fault is `MZ0915` at check time. `float` designed in [RFC-0013] §4.2, not implemented                                                                | A float type exists, with overflow and divide-by-zero behaviour specified and tested                      |
| C6  | Text operations: length, slicing, search, split, format                                                | 📝     | `text` is a type only. Operations designed in [RFC-0013] §10 (draft, not implemented)                                                                                                                                                                                                                                                                                                                                                                | The operations exist in the standard library (P2) with tests                                              |
| C7  | Collections: maps, sets, tuples; operations (map, filter, fold, index)                                 | 🟡     | `list(T)`, `option(T)` types ([RFC-0008]). `map`, `set` and their operations designed in [RFC-0013] §9 (draft, not implemented); tuples decided against (§2)                                                                                                                                                                                                                                                                                         | `map(K, V)` exists and collections can be built and transformed in a function                             |
| C8  | User types: methods, interfaces or traits, generics                                                    | 🟡     | Enums with data, records. Methods designed in [RFC-0013] §11 (draft, not implemented); generics and interfaces deferred past M1 (owner, #69)                                                                                                                                                                                                                                                                                                         | A record has a method; a generic function works for two types; an interface is satisfied                  |
| C9  | Error handling: a result type and propagation                                                          | 📝     | Designed in [RFC-0013] §12 (draft, not implemented): `result(T, E)`, `try`, `MZ0950`. Contracts check behaviour, not recoverable failure                                                                                                                                                                                                                                                                                                             | A function returns an error that its caller handles or propagates, and an unhandled error is a diagnostic |
| C10 | A program entry point that runs                                                                        | 🟡     | A `program` file with `fn main` and `print` ([RFC-0013] §1, Wave 0). `mz run` checks it, lowers it to a dependency-free Rust package, builds it with `cargo build --offline` and runs it: `mz run examples/hello.mz` prints `hello, world`, tested in `compiler/tests/program.rs` and in CI's `lowering` job against `examples/hello.expected`. **Done in PR #80 (open, to `staging`); ✅ when it reaches `main`**                                   | `mz run hello.mz` prints output                                                                           |

### Tier 2: real programs (build, run and ship software)

| ID  | Capability                                                                             | Status | Evidence                                                                                                                                                                                                                                                                                                         | Done when                                                                                                                                  |
| --- | -------------------------------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| P1  | Modules and imports across files; public and private                                   | ❌     | One component per file; `use` declares capabilities only                                                                                                                                                                                                                                                         | Code in one file calls code in another; private names are not visible                                                                      |
| P2  | Standard library: text, math, collections, time, JSON, environment, files, HTTP client | ❌     | JSON response bodies from a `service` only (#31, #32); no library module exists                                                                                                                                                                                                                                  | Each module exists with tests, and is taught to agents through the harness (H1)                                                            |
| P3  | Lowering all code to Rust                                                              | 🟡     | A `service` lowers to a local Rust + axum package (`mz build`, #32), which CI compiles, tests and serves. In open PR #80, a `program` in [RFC-0013]'s Wave 0 subset lowers to a dependency-free Rust package (`compiler/src/run.rs`), which CI builds and runs. Components, and the rest of Tier 1, do not lower | Every construct in Tier 1 lowers to Rust that `rustc` compiles, tested in CI                                                               |
| P4  | `mz build` / `mz run` produce a runnable program                                       | 🟡     | `mz build` for a service (#32); in open PR #80, also for a program, and `mz run` builds a program to a native binary with Cargo and runs it, exiting with its status ([RFC-0013] §13, Wave 0 subset). `mz run --agent` is not built                                                                              | A general program builds to a native binary and runs                                                                                       |
| P5  | Errors mapped back to `.mz`                                                            | 📝     | Designed in [RFC-0013] §4.3, §13.1 (draft, not implemented): traps name the `.mz` line (`MZ0991`), and lowered code `rustc` rejects is `MZ0990`                                                                                                                                                                  | A `rustc` error in lowered code is reported at the `.mz` line that caused it, or cannot happen by construction                             |
| P6  | Rust interop: call crates, mix `.mz` and `.rs` in one Cargo project                    | ❌     | —                                                                                                                                                                                                                                                                                                                | A `.mz` program uses a crates.io crate, and a Rust crate calls Mzizi code. This is TypeScript's `.d.ts` step: the reason the analogy works |
| P7  | Capabilities enforced                                                                  | 🟡     | `use net` parses ([RFC-0001])                                                                                                                                                                                                                                                                                    | A capability not declared is a compile error; declared capabilities reach the target (Workers, Containers)                                 |
| P8  | State and I/O: mutable state, files, databases                                         | ❌     | —                                                                                                                                                                                                                                                                                                                | A program reads and writes a file and holds state safely                                                                                   |
| P9  | Concurrency: async or tasks                                                            | ❌     | Not designed                                                                                                                                                                                                                                                                                                     | An RFC chooses the model, and a program runs two tasks concurrently                                                                        |
| P10 | Memory model: value semantics, no borrows, lifetimes or ownership at the surface       | 🟡     | [RFC-0001] §1.8. Realised in service lowering (#32), and, in open PR #80, in program lowering for Wave 0's types: every value is owned, a `text` read is a `.clone()`, no reference or lifetime is emitted (`compiler/src/run.rs`). Not yet proven for collections or records                                    | Tier 1 lowers to Rust with no ownership concept in the source, proven by tests                                                             |
| P11 | Targets: native, WebAssembly, Cloudflare Workers, Containers                           | 🟡     | Native via Rust for a service only, as a local axum package (#32); no WebAssembly, Workers or Containers target                                                                                                                                                                                                  | Each target builds and runs the same program ([CHARTER.md] §2)                                                                             |

### Tier 3: adoption (people and agents can use it every day)

| ID  | Capability                                                            | Status | Evidence                                                                                                             | Done when                                                                                             |
| --- | --------------------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| H1  | **The harness, the core of the language: what the agent reads**       | 🟡     | The agent protocol (`mz check --agent`, `mz fix`) exists; the rest is [RFC-0012], a draft                            | `mz harness` serves the language's agent-facing definition, and plugins attach to it (RFC-0012 §4–§6) |
| H2  | The skills fold into the harness                                      | 📝     | [RFC-0012] §7                                                                                                        | The four skills copies are generated from the harness                                                 |
| T1  | Agent diagnostics                                                     | ✅     | `mz check --agent` (NDJSON), `mz fix`, diagnostic codes                                                              | Keep ✅ as the language grows: every new construct has codes and exact fixes                          |
| T2  | Formatter / canonical form                                            | 📝     | Canonical form, [RFC-0001]                                                                                           | `mz fmt` exists and `mz check` rejects non-canonical code                                             |
| T3  | Editor support: a language server                                     | ❌     | —                                                                                                                    | Diagnostics, go-to-definition and completion in an editor                                             |
| T4  | Tests: `mz test` beside `mz contract`                                 | 🟡     | `mz contract` evaluates contracts. `test` blocks and `mz test` designed in [RFC-0013] §15.2 (draft, not implemented) | Ordinary test functions run with `mz test`                                                            |
| T5  | Package management                                                    | ❌     | —                                                                                                                    | A manifest declares dependencies, resolved through Cargo                                              |
| T6  | Documentation generator                                               | 🟡     | `##` doc lines parse                                                                                                 | `mz doc` renders a module's docs                                                                      |
| T7  | Playground or REPL                                                    | ❌     | mzizi.dev re-implements the contract check in JavaScript; it is not the compiler                                     | The real compiler runs in the browser (WebAssembly) or a REPL                                         |
| T8  | Debugging                                                             | ❌     | —                                                                                                                    | Step through a `.mz` program via Rust debug info mapped to `.mz`                                      |
| T9  | Language reference                                                    | ❌     | The RFCs are designs                                                                                                 | A reference defines every construct Tier 1 and Tier 2 ship                                            |
| T10 | Releases and stability: a published `mz`, versions, an edition policy | ❌     | `mzizi-lang-compiler` 0.0.0, `publish = false`                                                                       | `mz` installs with one command, and a stability promise is written down                               |

## The measurement: what each step unlocks (RFC-0009)

Phase 0's goal cannot be tested beyond what the language can express.

| Task family                                                                | Needs                 | Status                                                                                                                                                                                                                               |
| -------------------------------------------------------------------------- | --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| UI components against Rust (Dioxus, Leptos) and TypeScript (React)         | What exists today     | Runnable now. Two pilots on 2026-09-27, against Dioxus only, showed no advantage                                                                                                                                                     |
| Backend handlers (B1 routing) against TypeScript, Python, Go, C++ and Rust | A `service` (#29–#33) | Not yet runnable. The `mzizi-be` arm, `mzprobe` and B1 exist (#33), and both B1 references hold all 59 facts; the runner does not score an episode with probes, and B2–B5 and the other-language arms do not exist. Nothing measured |
| Public suites (MultiPL-E, EvalPlus)                                        | C1–C10 and part of P2 | **Blocked: functions run in a `program` in open PR #80 (RFC-0013 Wave 0), but loops, collections and text operations do not exist yet**                                                                                              |
| Aider polyglot, BaxBench                                                   | Tier 1, P1, P2, P8    | Blocked                                                                                                                                                                                                                              |

The kill-criterion run has not happened. Nothing in this file is a claim that Mzizi is better
than any language; it lists what has to exist before that can be measured.

## Milestones

1. **M1, a language that computes:** all of Tier 1 ✅. This unblocks the public suites.
2. **M2, a working programming language:** Tier 1 plus P1–P6 ✅. You can write a general
   program in several files, use a Rust crate, and build and run it. **Only at M2 do we say Mzizi
   is a working programming language.**
3. **M3, a language people choose:** the rest of Tier 2 and Tier 3 ✅, and the kill criterion
   run and published.

[RFC-0001]: ./design/RFC-0001-syntax.md
[RFC-0008]: ./design/RFC-0008-types-collections-records.md
[RFC-0010]: ./design/RFC-0010-contracts-everywhere.md
[RFC-0012]: ./design/RFC-0012-harness.md
[RFC-0013]: ./design/RFC-0013-core-language.md
[CHARTER.md]: ./CHARTER.md
