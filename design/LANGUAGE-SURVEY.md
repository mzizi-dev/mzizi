# A survey of the top 10 languages, feature by feature

**Status: design input. Nothing in this file is implemented, and nothing in it has been
measured.** It is a list of what Mzizi intends to take, change or refuse from ten widely used
languages, and why. Where it says Mzizi "does better", that is a design goal: it becomes a
claim only when [RFC-0009](./RFC-0009-comparison-benchmark.md)'s benchmark measures it, and the
kill-criterion run has not happened. Two pilots ran on 2026-09-27 against Dioxus; neither showed
an advantage for Mzizi, and on the ~7B open-weight model Mzizi did worse on all three metrics
([`benchmarks/results/`](../benchmarks/results/)).

Where this survey and [`LANGUAGE-TRACKER.md`](../LANGUAGE-TRACKER.md) disagree about what
exists, the tracker and the code are right. Where it and an RFC disagree about a design, the RFC
is right until the tier RFC named in the matrix changes it.

## 1. Purpose

The owner's goal, 2026-10-07:

> look at the top 10 languages globally make sure we have the best in class features and we do
> it better

The owner also asked whether Mzizi is "essentially building a better Rust". The short answer
this survey supports: Mzizi is designed as a **different surface over Rust**, the way TypeScript
is a different surface over JavaScript ([`CHARTER.md`](../CHARTER.md) v0.4 calls it "Rust's
TypeScript"). It keeps Rust's semantics underneath (`Result`, exhaustive enums, `Option`, `Drop`,
Cargo, native code) and takes the best ideas from nine other languages for the surface. It gives
up Rust's fine-grained control over memory and aliasing on purpose (§6, R3), so it cannot claim
to be a faster Rust, and it does not try to. Whether the simpler surface makes agents and small
models write correct programs more often than they do in Rust is the open question RFC-0009
exists to answer.

This file feeds three design documents:

- **RFC-0013, the core language** (Tier 1, C1–C10), owner decision 1 in issue #69. Not yet
  written.
- **A future Tier 2 RFC** (P1–P11). It may split, for example a separate concurrency RFC for P9.
- **A future Tier 3 RFC** (H1, T1–T10). Parts already live in RFC-0001 (canonical form) and
  RFC-0012 (the harness).

## 2. Method and sources

**Which ten.** Python, JavaScript, TypeScript, Java, C#, Go, C, C++, Rust and Swift. The list was
chosen before this file was written; it was checked afterwards against two published rankings:

| Ranking                                                                                             | Date           | Its top 10                                                                                                                      | In that top 10, not surveyed here                                                                        | Surveyed here, outside that top 10   |
| --------------------------------------------------------------------------------------------------- | -------------- | ------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | ------------------------------------ |
| [TIOBE Index](https://www.tiobe.com/tiobe-index/)                                                   | September 2026 | Python, C, C++, Java, C#, JavaScript, Visual Basic, SQL, R, Rust                                                                | **Visual Basic** (7), **SQL** (8), **R** (9)                                                             | Go (12), Swift (18), TypeScript (39) |
| [Stack Overflow Developer Survey](https://survey.stackoverflow.co/2025/technology), all respondents | 2025 edition   | JavaScript, HTML/CSS, SQL, Python, Bash/Shell, TypeScript, Java, C#, C++, PowerShell; next C (11), PHP (12), Go (13), Rust (14) | **SQL** (3), **PHP** (12, ahead of Go and Rust); HTML/CSS, Bash/Shell and PowerShell are markup or shell | Swift (20, behind Kotlin at 15)      |

So the ten are a union of the two rankings, not either ranking's top 10. Eight of the ten
(Python, JavaScript, Java, C#, C++, C, Go, Rust) are in both rankings' top 14. **Gaps, named and not
researched here:** SQL (top 10 in both), PHP, R, Visual Basic and Kotlin. Swift is the weakest
inclusion by rank: it is here as the closest precedent for value semantics with copy-on-write
(P10). SQL is the gap that matters most for Mzizi's backend goal, because query building is where
the injection findings below come from (F30); it should be surveyed before the P8 design.
TypeScript ranks low on TIOBE but sixth on Stack Overflow, and is in because the charter's own
analogy is TypeScript.

**How each language was surveyed.** For each language, one research pass collected: why
practitioners choose it; up to ten features judged best in class, each with the agent failure
modes reported for it (measured where a study exists, marked anecdotal where not); a decision
(adopt, improve, reject, already-has); a Mzizi design; a Rust lowering; and the tracker row it
belongs to. This file merges those passes. A feature that appeared in several languages (records,
pattern matching, optionals, async) is one row here, with every source language named.

**What "best in class" means here.** Chosen by practitioners for a reason they can state, or the
model other languages copied. It does not mean measured best. The failure-mode evidence is uneven:
a few classes are measured in published studies (§7), most are practitioner reports.

**Main studies cited** (each research pass also cited language documentation, specifications and proposals, such as PEPs, JEPs and Swift Evolution proposals, for its rows):

- Spracklen et al., package hallucination, USENIX Security 2025 ([arXiv 2406.10279](https://arxiv.org/abs/2406.10279)).
- Wang et al., deprecated API use by LLMs, ICSE 2025 ([arXiv 2406.09834](https://arxiv.org/abs/2406.09834)).
- Mündler et al., type-constrained generation for TypeScript, PLDI 2025 ([doi 10.1145/3729274](https://dl.acm.org/doi/10.1145/3729274)).
- Lee, Ul Hassan and Hindle, `any` in agent-written TypeScript ([arXiv 2602.17955](https://arxiv.org/abs/2602.17955)).
- The 2026 Coimbra study of 86,726 errors in LLM-written C, C++, Java and Rust ([arXiv 2608.00661](https://arxiv.org/html/2608.00661v1)).
- Pearce et al., Copilot and CWE scenarios ([arXiv 2108.09293](https://arxiv.org/pdf/2108.09293)); FormAI-v2 ([arXiv 2404.18353](https://arxiv.org/html/2404.18353v2)).
- RustEvo, API drift ([arXiv 2503.16922](https://arxiv.org/abs/2503.16922)); RustAssistant ([arXiv 2308.05177](https://arxiv.org/abs/2308.05177)).
- BaxBench ([PMLR v267](https://proceedings.mlr.press/v267/vero25a.html)); [Veracode GenAI code security report 2025](https://www.veracode.com/blog/genai-code-security-report/).
- Liu et al., the quality of LLM-written Java, TOSEM 2024 ([doi 10.1145/3643674](https://dl.acm.org/doi/10.1145/3643674)); JavaBench ([arXiv 2406.12902](https://arxiv.org/abs/2406.12902)).
- SwiftEval ([arXiv 2505.24324](https://arxiv.org/abs/2505.24324)); CONCUR ([arXiv 2603.03683](https://arxiv.org/html/2603.03683v1)); Uber goroutine leaks ([arXiv 2312.12002](https://arxiv.org/html/2312.12002v1)).

None of these studies measured Mzizi.

## 3. The feature matrix

Decisions: **adopt** (take as is), **improve** (take the guarantee, change the form), **reject**
(§6), **already-has** (on `main` today, per the tracker). "Row" is the
[`LANGUAGE-TRACKER.md`](../LANGUAGE-TRACKER.md) row. "RFC" is where the design must land:
RFC-0013 for Tier 1, the future Tier 2 or Tier 3 RFC otherwise. Syntax shown is a placeholder
until G1.2/G1.3 ([RFC-0007](./RFC-0007-gap-register.md)) settle it. Rejected features are in §6,
not here.

### 3.1 Tier 1: the core language

| #   | Feature                                          | From                                                                                            | Area     | Decision    | Mzizi design (one line)                                                                                                                                                                                   | Rust lowering (one line)                                                        | Row     | RFC               |
| --- | ------------------------------------------------ | ----------------------------------------------------------------------------------------------- | -------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ------- | ----------------- |
| F1  | Declarative collection transforms                | Python comprehensions, Java streams, C# LINQ, C++ ranges, Rust iterators                        | stdlib   | improve     | One transform form, eager, returning a new value; a closed set of named folds (`count`, `sum`, `any`, `all`, `first`, `fold`, `sort_by`, `group_by`); form is open (§4.1)                                 | iterator chain ending in a concrete `collect::<Vec<_>>()`, or a push loop       | C7      | RFC-0013          |
| F2  | Safe indexing and a deterministic map            | C++ (out-of-bounds is 59.9% of LLM C++ runtime errors), Rust                                    | types    | improve     | `xs[i]` gives `option(T)`; `map(K, V)` iterates in a defined order                                                                                                                                        | `.get(i).cloned()`; `BTreeMap`                                                  | C7      | RFC-0013          |
| F3  | Named arguments and defaults                     | Python, C#, Swift                                                                               | syntax   | adopt       | Every argument after the first is labelled with its parameter name; one name per parameter; defaults are literals; unknown label gets an `exact` fix                                                      | labels erased, reordered to declaration order, defaults filled at the call site | C3      | RFC-0013          |
| F4  | String interpolation                             | Python f-strings, JS template literals, C# `$""`                                                | syntax   | improve     | `{expr}` in every `text` literal, the only way to build text; no `+` on text, no format mini-language; unnarrowed option inside is MZ0710                                                                 | `format!` with generated `Display`                                              | C1      | RFC-0013          |
| F5  | Records with value equality                      | Python dataclasses, Java records, C# records                                                    | types    | improve     | `record` exists; equality, debug text, hashing and JSON come with every record, no derive list                                                                                                            | `struct` with `#[derive(Clone, Debug, PartialEq, Eq, Hash)]` and serialisation  | C8      | RFC-0013          |
| F6  | Construction by field name, every field required | C designated initialisers, Java, C# (Go's zero values rejected, R9)                             | syntax   | improve     | A record literal names every field in any order; a missing field is an error with an `exact` fix; no positional construction                                                                              | Rust struct literal in declaration order                                        | C8, C1  | RFC-0013          |
| F7  | Copy-and-update                                  | JS spread, C# `with`, Java JEP 468                                                              | syntax   | improve     | `entry with version "2.0"` makes a new value; copies are deep in meaning; a repeated field is a diagnostic                                                                                                | `Entry { version: …, ..entry.clone() }`, a move when `entry` is dead            | C8      | RFC-0013          |
| F8  | Methods on records                               | Rust `impl`, Java, Swift `mutating`                                                             | types    | improve     | `fn` inside the record block, read-only `self`; a method that mutates says `changes self`; no `inout`                                                                                                     | `impl Name { fn m(&self) }`, `&mut self` for `changes self`                     | C8      | RFC-0013          |
| F9  | Exhaustive pattern matching                      | Rust, Java sealed + switch, C# switch, TS discriminated unions, Python `match`, C++ `variant`   | syntax   | improve     | `match` / `case` / `end` in function bodies and as an expression; missing case is an error whose `exact` fix inserts the `case` lines; a `case` names a variant, never a fresh binding                    | Rust `match`, no `_` arm emitted                                                | C4      | RFC-0013          |
| F10 | Enum payloads (sum types)                        | Rust, Swift associated values, Java sealed, TS unions                                           | types    | improve     | Per-variant data, bound by name in `case`; settles D1's static-columns vs payloads question                                                                                                               | Rust enum with struct variants                                                  | C8, C4  | RFC-0013          |
| F11 | Closed literal sets with per-variant data        | TS literal unions, Swift raw values and `CaseIterable`                                          | types    | already-has | Enums with data columns are on `main`; add an automatic `all` list                                                                                                                                        | fieldless enum, `const fn` columns, `const ALL`                                 | C8      | RFC-0013          |
| F12 | No null: absence is a type                       | Rust `Option`, Swift optionals, TS `strictNullChecks`, C# nullable references, Java's NPE story | types    | already-has | `option(T)` exists (RFC-0008); no null, no second absence, no nested option (MZ0703)                                                                                                                      | `Option<T>`                                                                     | C7      | RFC-0013          |
| F13 | Flow-sensitive narrowing                         | TS narrowing, Swift `guard let`, C# null-state analysis                                         | types    | improve     | `when x is not none … end` narrows inside; an early-exit `when x is none … return … end` narrows for the rest of the body; using an unnarrowed option is an error with an `exact` fix inserting the guard | `if let Some(x) = x`, `let Some(x) = x else { return … }`                       | C4      | RFC-0013          |
| F14 | One default for an absent value                  | JS/TS `??`, Swift `??`, C# `??`                                                                 | syntax   | improve     | One word, valid only on `option(T)`; on a non-option it is an error whose `exact` fix deletes it; word is open (§4.1)                                                                                     | `unwrap_or` / `unwrap_or_else`                                                  | C4, C1  | RFC-0013          |
| F15 | Errors are values, one visible propagation word  | Rust `?`, Swift `try`, Go `error`, Java checked exceptions, C++ `expected`, C error codes       | errors   | improve     | `result(T, E)` with `E` a user enum; `try` before a call propagates; a mismatched error type needs an explicit mapping (`exact` fix)                                                                      | `Result<T, E>`, `?`, generated `map_err` or `From`                              | C9      | RFC-0013          |
| F16 | An ignored result is an error                    | Rust `#[must_use]`, C23 `[[nodiscard]]`, Go's errcheck                                          | errors   | improve     | Error, not warning; fixes offered: propagate, or a `match` stub listing every variant                                                                                                                     | cannot reach Rust; `#[must_use]` as a backstop                                  | C9      | RFC-0013          |
| F17 | Integers that never overflow silently            | Python bigint and `/` vs `//`, Rust RFC 560, C `stdint` (and its UB)                            | types    | improve     | One `int`, 64-bit, overflow checked in every build; wrapping only through named functions; dividing by a literal zero is a compile error; one truncating division plus `remainder`                        | `i64`, `overflow-checks = true` in every generated profile, `checked_div`       | C5      | RFC-0013          |
| F18 | Floats and decimals                              | Python `decimal`, Rust                                                                          | types    | improve     | `float` is named, never the default for a literal; a `decimal` for money is decided in C5                                                                                                                 | `f64`; `rust_decimal` through P6                                                | C5      | RFC-0013          |
| F19 | Local inference, explicit signatures             | TypeScript, Rust                                                                                | types    | adopt       | Bindings are inferred; `take` parameters and `give` return are always written; a redundant local annotation is removed by `mz fix`                                                                        | `let x = …;`, signatures from `take`/`give`                                     | C2, C3  | RFC-0013          |
| F20 | Functions as values                              | JS closures, Java method references, C# lambdas                                                 | syntax   | improve     | A named top-level `fn` can be passed as a value, and nothing captures; whether inline lambdas exist is open (§4.1)                                                                                        | `fn` item or `fn` pointer; no `move` closures                                   | C3      | RFC-0013          |
| F21 | Unused bindings and imports are errors           | Go                                                                                              | errors   | adopt       | Every one carries an `exact` fix, so `mz fix` clears them without a model turn; same rule for unused capabilities                                                                                         | never emitted; `#![deny(unused)]` as a backstop                                 | C2      | RFC-0013          |
| F22 | Compile-time evaluation of pure code             | C++ `constexpr`                                                                                 | metaprog | improve     | No keyword: a pure function with literal arguments may be evaluated by the checker, so literal `example`s and `require`s are checked by `mz check`                                                        | `const` literals, `const fn` where possible                                     | C3      | RFC-0013          |
| F23 | Unicode-correct text                             | Swift `String`                                                                                  | stdlib   | improve     | Length and slicing count characters, never bytes, and return `option` out of range; graphemes vs scalars decided in C6                                                                                    | `unicode-segmentation` (in the lowered crate only) or `chars()`, `s.get(a..b)`  | C6      | RFC-0013          |
| F24 | A fast edit-run loop                             | JavaScript (no build step, REPL)                                                                | tooling  | improve     | `mz run` lowers to Rust and runs it (issue #69 decision 2); `mz check` answers without rustc; examples run in process                                                                                     | `mz build` → Cargo                                                              | C10, T7 | RFC-0013          |
| F25 | Generics with named constraints                  | C++20 concepts, Rust trait bounds                                                               | types    | improve     | Deferred past M1. When they come: every type parameter names an interface; no specialisation; one call-site diagnostic                                                                                    | Rust generics with trait bounds                                                 | C8      | RFC-0013 (defers) |
| F26 | Interfaces satisfied explicitly                  | Go interfaces, Swift protocols, Rust traits                                                     | types    | improve     | Deferred (D1). When they come: nominal, declared in the record's own block, missing method reported at the record with a stub fix                                                                         | trait plus `impl Trait for Record`                                              | C8      | RFC-0013 (defers) |

### 3.2 Tier 2: real programs

| #   | Feature                                               | From                                                                              | Area        | Decision | Mzizi design (one line)                                                                                                                          | Rust lowering (one line)                                                      | Row       | RFC        |
| --- | ----------------------------------------------------- | --------------------------------------------------------------------------------- | ----------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------- | --------- | ---------- |
| F27 | Static modules, explicit exports, a prelude           | JS ES modules, C++20 modules, Go packages                                         | modules     | adopt    | `import <name>` (not `use`, D7); file name is the declaration name (D3); `public` on the header; the stdlib is a prelude; cycles are errors      | one Rust module per file, `pub`, `use crate::…`                               | P1        | Tier 2 RFC |
| F28 | A decided, versioned standard library                 | Python's batteries-included stdlib                                                | stdlib      | improve  | A closed list (RFC-0007 G2.10), versioned with `mz`, every function with a contract, served by the harness; unknown names get nearest-name fixes | `mzizi-std`, thin wrappers over pinned crates                                 | P2        | Tier 2 RFC |
| F29 | JSON as native data, typed at the boundary            | JavaScript object literals, Pydantic                                              | interop     | improve  | Records encode to JSON by construction (runs today for `service` responses); input decodes only into a declared record, or a declared rejection  | generated writer or serde; `from_slice::<Record>`                             | P2 (G2.4) | Tier 2 RFC |
| F30 | Context-aware interpolation                           | JS tagged templates; C# `FromSql`                                                 | syntax      | improve  | The destination picks the escaping: escaped in a view, a bound parameter in a query sink; plain `text` into a query sink is an error             | escaping writer; `sqlx::query(…).bind(…)`                                     | C6, P8    | Tier 2 RFC |
| F31 | No async colouring                                    | Go goroutines, Java virtual threads; JS and C# async/await as the counter-example | concurrency | improve  | Functions that use an I/O capability are effectful; the compiler inserts every await (D4, G2.6); no futures in source                            | `async fn`, `.await` inserted                                                 | P9        | Tier 2 RFC |
| F32 | Structured concurrency                                | Java JEP 505, Swift task groups, Go errgroup                                      | concurrency | adopt    | `together … end together`: children finish or are cancelled before the block exits; first error cancels siblings; no free `spawn` in v0          | `tokio::try_join!`, `JoinSet` with abort on drop                              | P9        | Tier 2 RFC |
| F33 | Shared state through one owner                        | Swift actors                                                                      | concurrency | improve  | Mutable shared state lives only in a `service`'s state, one handler at a time, `always` invariants checked after each                            | `Arc<tokio::sync::Mutex<State>>` never held across an await, or an actor task | P9, P8    | Tier 2 RFC |
| F34 | Deterministic cleanup with nothing to write           | C++ RAII, Rust `Drop` (Python `with`, C# `using`, Go and C `defer` rejected, R12) | memory      | improve  | Capability handles close when they leave scope; an explicit `close` only where the close can fail and returns a `result`                         | `Drop` types; a consuming `close` returning `Result`                          | P8, P10   | Tier 2 RFC |
| F35 | Value semantics with copy-on-write                    | Swift value types                                                                 | memory      | adopt    | Every type is a value type, no classes, no references (RFC-0001 §1.8)                                                                            | owned values, `&T` for read-only parameters, `Rc::make_mut` for collections   | P10       | Tier 2 RFC |
| F36 | Zero overhead as a lowering goal, cost made visible   | C++                                                                               | memory      | improve  | Not claimed; lowering moves on last use and clones otherwise, and a command reports where it cloned                                              | moves, borrows, clones chosen by last-use analysis                            | P3, P10   | Tier 2 RFC |
| F37 | Using the host ecosystem through generated signatures | TypeScript `.d.ts`                                                                | interop     | improve  | `use crate <name> <exact version>`; a Mzizi view of the crate's API from rustdoc JSON, ownership erased; unknown crate → `guess` fix only        | direct calls with borrows and clones inserted at the boundary                 | P6, T5    | Tier 2 RFC |
| F38 | Callable from every language                          | the C ABI                                                                         | interop     | improve  | An `export c` line on a function; signature limited to C-representable types; Mzizi owns returned buffers and generates a `_free`                | an `ffi` module, the only `unsafe` code; `#[repr(C)]`; cbindgen               | P6 (new)  | Tier 2 RFC |
| F39 | Runtime faults reported like compile errors           | C sanitizers, Java helpful NPEs                                                   | errors      | improve  | Every remaining runtime fault (overflow, index, `require`) has a stable code and the `.mz` span, in the NDJSON shape                             | panic hook plus a generated span map                                          | P5        | Tier 2 RFC |
| F40 | One binary, cross-compiled with one flag              | Go                                                                                | tooling     | improve  | `mz build --target <triple>`; compile speed is not claimed to match Go's                                                                         | `cargo build --release --target …`                                            | P4, P11   | Tier 2 RFC |

### 3.3 Tier 3: adoption

| #   | Feature                                        | From                                                                         | Area    | Decision    | Mzizi design (one line)                                                                                                                     | Rust lowering (one line)              | Row     | RFC                      |
| --- | ---------------------------------------------- | ---------------------------------------------------------------------------- | ------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- | ------- | ------------------------ |
| F41 | One canonical format, no options               | Go `gofmt`, Prettier                                                         | tooling | adopt       | `mz check` rejects non-canonical code and the `exact` fix is the canonical text; no config file                                             | generated Rust is rustfmt-shaped      | T2      | Tier 3 RFC (RFC-0001 §3) |
| F42 | Diagnostics with machine-applicable fixes      | rustc `--error-format=json`, Roslyn code fixes                               | tooling | already-has | `mz check --agent` NDJSON, fixes tagged `exact`/`guess`, `mz fix`; every new construct must ship with codes and fixes                       | n/a                                   | T1      | every tier RFC           |
| F43 | The compiler as the editor's engine            | TypeScript tsserver                                                          | tooling | improve     | A language server that is a thin adapter over the same diagnostic stream, so editor and agent see the same facts                            | n/a                                   | T3      | Tier 3 RFC               |
| F44 | Library-supplied analyzers                     | Roslyn analyzers                                                             | tooling | improve     | Harness plugins declare diagnostics and `exact` fixes for their own API, in the same schema; no suppression pragma                          | n/a                                   | H1      | RFC-0012                 |
| F45 | Executable examples and properties as tests    | Python doctest, pytest, Hypothesis, Go examples and fuzzing, C++26 contracts | testing | improve     | `example` and `ensure` clauses (RFC-0010) are the tests; a failure reports observed vs expected and the seed; `mz test` for plain tests     | `#[test]`, seeded `proptest!`         | T4      | Tier 3 RFC, RFC-0010     |
| F46 | Documentation from source                      | Javadoc, JEP 413 snippets                                                    | tooling | improve     | `mz doc` renders `##` lines with the contract's examples, which have run; a `##` line that restates the signature is a lint                 | `///` on generated items              | T6      | Tier 3 RFC               |
| F47 | One package manager, exact pins                | Cargo, npm, Go modules                                                       | tooling | adopt       | Cargo underneath; exact versions and a lockfile; `mz` never installs on the agent's behalf                                                  | generated `Cargo.toml` with `=x.y.z`  | T5      | Tier 3 RFC               |
| F48 | Editions and a compatibility promise           | Rust editions, Go 1 promise and `go` line, Java JLS ch. 13                   | tooling | adopt       | An `edition` in the manifest; meaning never changes inside one; `mz fix --edition` emits only `exact` fixes; the harness serves one edition | an edition maps to one fixed lowering | T9, T10 | Tier 3 RFC               |
| F49 | A playground with the real compiler            | JS online playgrounds                                                        | tooling | improve     | The zero-dependency compiler crate built to wasm32                                                                                          | n/a                                   | T7      | Tier 3 RFC               |
| F50 | The agent reads the pinned API, not its memory | none does this; answers RustEvo and the deprecated-API findings              | tooling | improve     | The harness serves the signatures and contracts of the exact stdlib and crate versions in the manifest                                      | n/a                                   | H1      | RFC-0012                 |

### 3.4 Counts

63 deduplicated rows: 50 above and 13 rejected in §6.

| Decision    | Tier 1 | Tier 2 | Tier 3 | Total |
| ----------- | ------ | ------ | ------ | ----- |
| adopt       | 3      | 3      | 3      | 9     |
| improve     | 21     | 11     | 6      | 38    |
| already-has | 2      | 0      | 1      | 3     |
| reject      | —      | —      | —      | 13    |

The three already-has rows are already-has in part: F11 still needs the `all` list, F12 still
needs its operations (F13, F14), and F42 must stay true as every new construct lands.

## 4. What each tier RFC must include

### 4.1 RFC-0013 (Tier 1)

It must design F1–F26 together, and decide these, which the research passes left open or
disagreed on:

1. **One collection-transform form (F1).** Three designs were proposed: `for each … keep`
   as an expression with named folds and no `map`/`filter` (from Python); methods taking a named
   function, `users.keep(is_active)` (from Java); methods taking a one-line inline function (from
   C#, Rust, C++). Shipping more than one is FM-1. The choice depends on F20.
2. **Inline functions or named functions only (F20).** No inline lambdas removes captures and
   stale closures but makes transforms longer. Decide with F1.
3. **The default word (F14).** `or` was proposed by four passes, `otherwise` by one. If G1.3
   spells boolean logic with words, `or` would mean two things; `otherwise` avoids that. Decide
   with G1.3.
4. **The propagation word (F15).** `try` (five passes) or `check` (one, after the Go team's
   lesson that `try()` hid control flow inside expressions). Either way it leads a line or a
   binding, never inside a nested expression.
5. **Guards in `case` (F9).** Java's pass wants `case x when <condition>`; the Python and C#
   passes want no guards in v0. Also decide whether `case else` exists over enums, and whether it
   warns.
6. **Enum payloads (F10).** D1's open question: one construct for static columns and payloads,
   or two.
7. **Numbers (F17, F18).** Overflow checked in every build, and what overflow does at run time
   (a typed failure or a contract failure); division by zero; whether `decimal` is in M1.
8. **Record syntax (F5–F8).** The literal, the `with` update, and `changes self`, in one
   `name value` pair style shared with named arguments (F3), so a line is classified from its
   first two tokens.
9. **Text (F4, F23).** Interpolation in every expression; graphemes or scalars.
10. **Generics and interfaces stay deferred (F25, F26), and the tracker must say so.** C8's
    "Done when" names a generic function and a satisfied interface; D1 says no user generics or
    traits in v0; issue #69 defers generics past M1. That is an owner decision to record, not
    one for RFC-0013 to make silently.

It must also give every construct its diagnostic codes and `exact` fixes (F42), and state the
rejections in §6 that touch Tier 1 (R1–R11) so they are not reopened by accident.

### 4.2 The Tier 2 RFC

F27–F40. Must decide: the import word (D7 rules out `use`); the `mzizi-std` list (G2.10);
`deny_unknown_fields` or not at the JSON boundary; what a query sink is (F30), which needs the
SQL gap in §2 surveyed first; the effect-inference rule and the `together` block (P9, likely its
own RFC); whether `within <duration>` deadlines are in v0; the span map for P5; and how crate
signatures are generated (rustdoc JSON is unstable; say which nightly format version).

### 4.3 The Tier 3 RFC

F41–F50. Must decide: the canonical form enforcement path (RFC-0001 §3 is the design); the
manifest format and how it maps to Cargo; the edition policy and removal rules (every removal
ships with an `mz fix` rewrite); `mz test`'s plain-test form; what `mz doc` renders; and the
language server's dependency cost, since the compiler crate has zero dependencies and AGENTS.md
requires an argument for adding one.

## 5. Where Mzizi intends to do better, and how it will be measured

Each line below is a goal. None has been measured. RFC-0009 §6 measures three things per
gating family (`ui-spec`, `backend`): tokens, iterations to a clean check, and defect rate,
against the best incumbent arm on each. A goal with no task that can show it is marked **new task
needed**.

| Goal (design, not result)                                                                                                     | Features             | How it would be measured                                                                                                                                                                      |
| ----------------------------------------------------------------------------------------------------------------------------- | -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fewer iterations to a clean check than the best incumbent, because every diagnostic has a fix and `mz fix` applies exact ones | F42, F21, F41, F9    | Iterations metric, `backend` (B1–B5) and `ui-spec`; Aider polyglot's `pass_rate_1` → `pass_rate_2` once Tier 1 lands (RFC-0009 §8.1)                                                          |
| Fewer defects in clean code on error paths, because results cannot be ignored and handlers are total                          | F15, F16, F13        | Defect rate on B5 (error paths); **new task needed:** a `backend` task whose spec has three distinct failure causes with three distinct status codes, so a swallowed error is a failing probe |
| No null or absent-field defects                                                                                               | F12, F13, F14, F29   | Defect rate on B2 (validation) and B3; RFC-0008's TY-6 class is the defect definition                                                                                                         |
| No silent integer or division faults                                                                                          | F17                  | **New task needed:** a `backend` task with an arithmetic probe at the edge (a sum near 2^63, a zero divisor from input); MultiPL-E/EvalPlus cases that hit the edge                           |
| No injection in generated backends                                                                                            | F30, F29             | BaxBench's security-defect rate (RFC-0009 §8.1: a metric "we do not measure yet"); needs P8 and the SQL survey first                                                                          |
| No hallucinated packages or stale APIs reaching a build                                                                       | F37, F47, F50, F28   | **New task needed:** a task that needs one crate, scored on whether the episode ever declares a non-existent dependency; RustEvo-style tasks on post-cutoff APIs, served through the harness  |
| Fewer tokens than the best incumbent for the same program                                                                     | F3, F5, F6, F19, F41 | Tokens metric, every family                                                                                                                                                                   |
| No leaked tasks or races in concurrent code                                                                                   | F31, F32, F33        | **New task needed:** a `backend` fan-out task (call two fixtures, fail one) whose probes check that the failure propagates and the response time shows the sibling was cancelled              |
| Functional correctness comparable with the field on public problems                                                           | Tier 1 as a whole    | MultiPL-E, then EvalPlus (RFC-0009 §8.2); never gating, contaminated for incumbents                                                                                                           |
| Lowered code as fast as hand-written Rust                                                                                     | F36                  | Not an authoring claim; RFC-0009 §8.3 runtime suites, reported separately. Expect a cost from clones; do not claim parity                                                                     |

The new tasks are proposals for RFC-0009's task list, written as RFC-0009 §2.3 requires: a
language-neutral spec, two references in two languages, probes at the HTTP boundary, and held out
if they are to gate.

## 6. Rejected features, and why

| #   | Feature                                                                                           | From                                                      | Why rejected                                                                                                                                                                                     | Row             |
| --- | ------------------------------------------------------------------------------------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------- |
| R1  | Optional chaining `?.`                                                                            | JS, TS, Swift, C#                                         | Silent propagation of absence is the TY-6 defect class; chaining hides which link was absent; a symbol hurts small-model tokenisation (RFC-0002 §1)                                              | C4              |
| R2  | Escape hatches: `any`, `as`, `!`, `@ts-ignore`, force-unwrap, `try!`, `try?`, suppression pragmas | TS, Swift, C#                                             | Agents introduce `any` 9x more often than humans (arXiv 2602.17955); an error must be fixed, not silenced; unchecked code enters only at the declared P6 boundary                                | T1, C8          |
| R3  | Ownership, borrowing, lifetimes, move semantics, smart pointers, raw pointers                     | Rust, C++, C                                              | The largest documented LLM failure class in Rust (ownership and lifetime errors are 16.7% of LLM Rust compile errors, Coimbra 2026); the compiler pays instead (P10). Cost: no zero-copy control | P10             |
| R4  | Structural typing                                                                                 | TypeScript, Go's implicit interfaces                      | Long non-local errors small models cannot localise; the host (Rust) is nominal, so lowering stays direct                                                                                         | C8              |
| R5  | Computation in types                                                                              | TS mapped and conditional types, C++ templates and SFINAE | A second language with poor errors; agents reach for it more than humans (arXiv 2602.17955)                                                                                                      | C8              |
| R6  | Extension methods, extensions, retroactive conformance                                            | C#, Swift                                                 | What a value can do would depend on imports in the current file; behaviour must be found in the type's own declaration                                                                           | C8              |
| R7  | Implementation inheritance, annotation-driven frameworks, runtime reflection                      | Java                                                      | Hidden behaviour (FM-7, FM-3); no model fully completed any JavaBench OOP project; services declare routes in grammar instead                                                                    | C8              |
| R8  | User macros, the preprocessor, result builders                                                    | C, C++, Rust, Swift                                       | Errors land in code the author never wrote (FM-7); text substitution defeats the content-addressed IR                                                                                            | none (non-goal) |
| R9  | Zero values                                                                                       | Go                                                        | A forgotten field becomes a silent runtime bug; absence must be `option(T)`                                                                                                                      | C8              |
| R10 | Significant indentation                                                                           | Python                                                    | Whitespace does not survive transit; a mangled indent changes meaning and still compiles. Blocks close with `end` (RFC-0001 §1.1)                                                                | T2              |
| R11 | Exceptions as the error channel                                                                   | Python, Java, C#, C++, JS                                 | Invisible in signatures and caught too broadly; D5 says errors are values                                                                                                                        | C9              |
| R12 | Cleanup keywords: `defer`, `with`, `using`                                                        | Go, C2y, Python, C#                                       | Under value semantics the compiler owns cleanup, so there is nothing to schedule and no second way to do it (F34)                                                                                | P8              |
| R13 | Freestanding, no-allocator targets                                                                | C                                                         | Not rejected forever: CHARTER.md §2 leaves embedded unscoped until a real deployment exists; value semantics needs a separate design for it                                                      | P11             |

## 7. Agent failure modes, and what targets each

Measured findings first. Each names the design choice meant to remove or reduce the class. None
of these targets has been shown to work in Mzizi.

| Failure class                                    | Evidence                                                                                                                                                 | Measured?                                    | Mzizi target                                                                                              |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Hallucinated packages                            | 19.7% of recommended packages did not exist; 15.8% for Python, 21.3% for JavaScript on average (Spracklen et al.)                                        | yes                                          | Unknown dependency is a diagnostic with a `guess` fix only, never `exact`; `mz` never installs (F37, F47) |
| Deprecated or post-cutoff APIs                   | ~25–70% deprecated-API use by library (Wang et al., Python); 56.1% vs 32.5% before and after cutoff (RustEvo, Rust)                                      | yes                                          | One stdlib versioned with `mz`; the harness serves pinned signatures (F28, F50); editions (F48)           |
| Type errors dominate compile failures            | 94% of compile errors in LLM-written TypeScript are type-check failures (Mündler et al.)                                                                 | yes                                          | Narrowing in one place with a mechanical `exact` fix (F13); nominal types with one-line diagnostics (R4)  |
| Escape hatches under compile errors              | Agents add `any` 9x more than humans; their PRs were accepted more often (arXiv 2602.17955)                                                              | yes                                          | No escape hatch exists (R2)                                                                               |
| Missing imports                                  | 56.6% of LLM C++ compile errors, 35.5% for C (Coimbra 2026)                                                                                              | yes                                          | The stdlib is a prelude; an unimported name gets an `exact` fix (F27)                                     |
| Out-of-bounds access                             | 59.9% of LLM C++ runtime errors, 49.9% for C (Coimbra 2026)                                                                                              | yes                                          | Indexing returns `option(T)` (F2)                                                                         |
| Ownership and lifetime errors                    | 16.7% of LLM Rust compile errors (Coimbra 2026); RustAssistant's benchmark built around them                                                             | yes                                          | No ownership at the surface (R3, F35)                                                                     |
| Memory-unsafe and vulnerable C                   | ~40% of completions vulnerable in CWE scenarios (Pearce et al.); at least 62% of programs (FormAI-v2)                                                    | yes                                          | Lowered code is safe Rust only, `#![forbid(unsafe_code)]` outside an `export c` shim (F38, F39)           |
| Insecure backends                                | Best model correct on 62% of BaxBench tasks, about half of correct programs exploitable; security flaws in 38–72% of samples by language (Veracode 2025) | yes                                          | Typed decode at the boundary (F29); context-aware interpolation (F30); measured by BaxBench (§5)          |
| Concurrency deadlocks and races                  | Found even in frontier-model output (CONCUR, Java); leaks pervasive in production Go (Uber)                                                              | yes (not for Mzizi's languages specifically) | Structured concurrency only, no shared mutable state outside a service (F31–F33)                          |
| Maintainability and redundant ceremony           | 53% of passing Java solutions had style or maintainability issues (Liu et al.)                                                                           | yes                                          | No derive lists or getters (F5); canonical form (F41)                                                     |
| Idiom sampling across forms and versions (FM-1)  | Mixed loops, comprehensions and map/filter; mixed `?.`, `??`, `!`; mixed language eras                                                                   | anecdotal                                    | One construct per intent in the grammar (F1, F14, F15); editions (F48)                                    |
| Non-exhaustive dispatch, catch-all arms          | `default:` swallowing new variants; Python's capture-pattern trap                                                                                        | anecdotal                                    | Exhaustiveness is an error with an `exact` fix; a `case` never binds a fresh name (F9)                    |
| Ignored errors, `unwrap` to compile              | Dropped Go errors, `.unwrap()` in Rust, empty `catch`                                                                                                    | anecdotal                                    | An ignored result is an error; no `unwrap` in source (F15, F16)                                           |
| Floating promises, sync-over-async               | typescript-eslint and Microsoft analyzers exist for these                                                                                                | anecdotal                                    | No futures in source; every await inserted (F31)                                                          |
| Silent overflow and wrong division               | Debug-only overflow checks in Rust; `/` vs `//` in Python                                                                                                | anecdotal                                    | Checked in every build; one division word (F17)                                                           |
| Mutable defaults, shallow copies, stale closures | Python `def f(xs=[])`, JS spread, React stale closures                                                                                                   | anecdotal                                    | Literal-only defaults (F3), value semantics (F35), no captures (F20)                                      |
| Forgotten cleanup                                | Unclosed files, `HttpClient` per request, `defer` before the error check                                                                                 | anecdotal                                    | The compiler owns cleanup (F34)                                                                           |
| Whitespace edits changing meaning                | Indentation mangled in transit (RFC-0001 FM list)                                                                                                        | anecdotal                                    | `end`-closed blocks (R10)                                                                                 |

## 8. What this file does not claim

- That any feature here is implemented. Of the rows it touches, only T1 is ✅ in the tracker;
  F11 and F12 are partly on `main`.
- That Mzizi is better than any of the ten languages at anything. The pilots so far showed no
  advantage, and the kill-criterion run has not happened.
- That the top-10 list is a ranking. It is a union of two rankings, with the gaps named in §2.
- That the failure-mode numbers apply to Mzizi. They were measured on other languages.
