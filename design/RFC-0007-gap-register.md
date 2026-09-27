# RFC-0007 — What the language is missing for the scope the charter claims

**Status:** draft for review — a gap register and an ordering, not an implementation
**Author:** the machine author (Claude)
**Scope:** every piece of language and toolchain infrastructure that CHARTER.md v0.2's stated scope (UI through `mzizi-ui`, server and edge through Cloudflare Workers and Containers, ML through Candle, native mobile and embedded later) requires and that neither the RFCs nor the compiler provide today — named _before_ a benchmark run or a deployment discovers each one the expensive way. Also: the one-way doors among them that must be decided before any is built.

> Written against CHARTER.md v0.2 (branch `claude/charter-v0.2`, unmerged when this was written). Tier 0 and Tier 1 below follow from v0.1 as well; Tier 2 onward is contingent on v0.2's scope.

<!-- Two separate notes; this separator keeps them distinct. -->

> **Every claim about the code below was checked against `main` at `cd36430`** (the commit this branch starts from), by the command or file named beside it. Where the draft of this RFC and the code disagreed, the code won and the text was corrected (RFC-0003 §7.1's rule). Claims about the 2026-09-27 pilot are quoted from its record and were _not_ re-derived here: that record is not on `main` at `cd36430`.

---

## 0. Why this RFC exists now

The first scored Phase 0 pilot (2026-09-27) ran three tasks — `badge`, `button` and `nyuchi-changelog-renderer` (`benchmarks/tasks/`) — through both arms. Its record is `benchmarks/results/2026-09-27-pilot/`, on branch `claude/phase0-pilot-results` until that branch merges and on `main` after. It surfaced one missing language feature by accident. The Mzizi-arm author, asked to port `nyuchi-changelog-renderer`, wrote:

> Mzizi has no list/array prop type, so the original `entries[]` … reduce here to the fields of one entry; render this component once per entry to reproduce the original feed.

That is the cheapest a gap will ever be found: at design time, by reading. Every gap the benchmark finds instead costs a run, and it also makes that run's numbers suspect, because the Mzizi arm was scored on a smaller problem than the Rust arm. So this RFC does the reading up front. It lists everything the charter's scope needs, checks each item against the code as it stands, and orders the list by what blocks what.

## 1. Method

A **gap** is something the charter's scope requires that the grammar (RFC-0001, RFC-0006) or the compiler (`compiler/src/`) does not provide. Each gap below carries:

- **Evidence** — a code path, a command and its output, or a pilot result. A gap asserted without evidence does not belong here (RFC-0001 §0's rule, applied to absences).
- **What it blocks** — the phase or metric that cannot proceed without it.
- **Tier** — Tier 0 blocks Phase 0 on its own corpus. Tier 1 blocks interactive UI. Tier 2 is Phase 1 (full stack on Cloudflare). Tier 3 is ML (v0.2's Phase 2). Tier 4 covers mobile and embedded (v0.2's Phases 3 and 5). Tiers 0–1 are in scope before the Phase 0 gate. Tier 2 onward is **designed** now and **built** after the gate, by CHARTER.md §4's rule ("don't build Phase 1 until Phase 0 has a real number attached to it"). Designing early is not building early: the point of listing Tier 2 now is to catch Tier 0 decisions that would foreclose it.

## 2. What exists, measured

| Surface                  | State at time of writing                                                                                                                                                                                                                                                                          |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Top-level declarations   | `component` only (`compiler/src/ast.rs` `Component`: "exactly one component", one per file)                                                                                                                                                                                                       |
| Keywords                 | 23 (`compiler/src/lex.rs` `KEYWORDS`): `component end use enum prop view fn contract when else not is match case for each in emit nothing none event true false`                                                                                                                                  |
| Types                    | Prop types are stored as the written string (`PropDecl.ty: String`, "Declared type, as written"). The only parameterised form is `event(T)`, special-cased in the parser for that one keyword. There are no list, record, option, result, or map types.                                           |
| Type and name checking   | **None.** `prop x: flarp` and `class = "p-2 {nosuch.class}"` both pass `mz check --agent` with 0 errors, exit 0 (§3, G0.1). The Mzizi authoring guide says so: "The compiler does not check type names or the names inside `{...}`, so spell them exactly." (`benchmarks/prompts/mzizi-guide.md`) |
| Capabilities             | `use <name>` records any identifier (`parse.rs`, `component.uses.push(cap)`); nothing checks the name. `use ml` passes with 0 errors, although RFC-0001 §1.7 says it "errors as 'not yet available'".                                                                                             |
| `fn`                     | The name is recorded (`Component.fns: Vec<String>`); the body is skipped (`parse.rs`, `skip_block_body`: "Consume a `fn` body without modelling it"). No statements, no expressions.                                                                                                              |
| `when` / `match` / `for` | Accepted as view tag words (`parse.rs`, `matches!(k, "when" \| "match" \| "for")`). Any element line may carry a keyword-led tail (`when state is offline`), which the parser stores as attribute pairs. Nothing is evaluated (RFC-0006 §10.3).                                                   |
| Contracts                | Evaluated by `mz contract` against enums, their variant columns, view attributes (taken from the outermost element carrying them), and prop defaults, in that fixed order (RFC-0006 §3).                                                                                                          |
| IR                       | Content-addressed, in memory (`ir.rs`, `hash.rs`, RFC-0003)                                                                                                                                                                                                                                       |
| Subcommands              | `check`, `contract`, `outline`, `hash`, `ir` (`main.rs`; anything else prints usage and exits 2). RFC-0001 §3 promises that `mz` rewrites every file it touches to canonical form, and §4.3 promises `mz fix`. RFC-0003 §5 designs `refs`, `path`, `patch` and `diff`. None of these exist.       |
| Lowering                 | **None.** Nothing produces Rust, WASM, or any artifact.                                                                                                                                                                                                                                           |

## 3. The gaps

### Tier 0 — blocks Phase 0 on its own corpus

**G0.1 — The checker cannot fail on types or names.** _Evidence:_ this component, run against `cd36430` (`cargo run --bin mz -- check --agent flarp_probe.mz`):

```text
## Probe: an undeclared type name and an unresolved interpolation.
component flarp_probe

  prop x: flarp
  prop label: text

  view
    mark
      slot = "flarp-probe"
      class = "p-2 {nosuch.class}"
      text = label
    end
  end

  contract
    slot is "flarp-probe"
  end

end component flarp_probe
```

```text
{"summary":true,"errors":0,"warnings":0,"exact_fixable":0,"ms":0}
exit 0
```

`mz contract` on the same file: `mz: 0 errors (0 exact-fixable), 1 contract clauses, 0 failed, 0ms`, exit 0. Replacing the prop with `use ml` gives the same 0-error summary.

_Blocks:_ the Phase 0 metric this language is supposed to win. "Iterations to a clean compile" measures how fast an author reaches a program the checker accepts. When the checker accepts every type name, every capability name and every `{...}` reference, a Mzizi candidate is "clean" on iteration 1 almost by construction. The misspelling a Rust arm pays a compile cycle for passes through, and it lands in the defect column only if a contract or the harness happens to catch it. The pilot's record reports 3/3 first-iteration-clean for the Mzizi arm; that result cannot be read as evidence for the thesis until this is fixed. It is the first item on the list because it corrects a measurement, not only a missing capability. _Needs:_ type-name resolution (built-ins plus types declared in the file); resolution of every prop reference, `{...}` interpolation, and dotted access (`variant.class`); a closed capability list, so `use ml` says what RFC-0001 §1.7 says it does; and nearest-name `exact`/`guess` fixes in the RFC-0001 §4 protocol.

**G0.2 — No collection type.** _Evidence:_ the pilot quote in §0; `benchmarks/tasks/nyuchi-changelog-renderer/spec.tsx` takes `entries: ChangelogEntry[]` and its reference `pub entries: Vec<ChangelogEntry>`. _Blocks:_ every feed-, list-, table-, menu-, tab- and breadcrumb-shaped component, which is a large share of N7–N11 by reading (not yet counted).

**G0.3 — No record type.** _Evidence:_ `ast.rs` has no declaration for named, typed fields — its only declarations are `EnumDecl` and `PropDecl`. _Blocks:_ G0.2 is nearly useless without it. A `list(text)` cannot express `entries[]`, where each entry has `version`, `title`, `description` and `date`, a list of affected node ids, and three component lists (`spec.tsx` `ChangelogEntry`).

**G0.4 — No optional values, so authors rebuild the parallel-truth defect.** _Evidence:_ the pilot's Mzizi changelog candidate, per its record, encodes each optional field as two props, `has_added: bool` and `added_label: text`. That is two declarations for one fact, free to disagree: RFC-0006's FM-11 (parallel truth), and RFC-0001 §1.3's parallel-`Record` drift in new clothes. **The language is currently pushing its authors into the defect class it was designed to remove.** `none` is already a keyword and a value (`parse.rs` accepts it as one; `event(none)` uses it) with no optional type for it to inhabit. _Needs:_ `option(T)` for scalars and records. A list is never optional: the empty list is its absence. Offering both `option(list(T))` and the empty list would be two ways to say "nothing" (FM-1).

**G0.5 — `for each` iterates over nothing.** _Evidence:_ `for` is a view tag word with no binding semantics (§2); `each` and `in` are keywords the parser stores as attribute pairs and does nothing with. _Needs:_ `for each <name> in <list-prop>` that binds `<name>` for its block, with element-key semantics (the reference's `key: "{entry.version}"`) written as one form.

**G0.6 — No composition and no child content.** _Evidence:_ two of the three pilot references (`benchmarks/tasks/badge/reference.rs`, `benchmarks/tasks/button/reference.rs`) take `children: Element`; the pilot's Mzizi candidates substituted `prop label: text`, as `primitives/badge.mz` and `primitives/button.mz` do. The third, `nyuchi-changelog-renderer`, takes `entries` instead and needs G0.2 rather than this. No `.mz` component can use another. _Blocks:_ any component built from primitives (a card holding a button), and honest interface parity in the benchmark. The harness does not score this divergence today, so it is invisible.

**G0.7 — No caller class or attribute passthrough.** _Evidence:_ all three pilot references have `class: String` and `#[props(extends = GlobalAttributes)]` (`button` also `extends = button`). The language has neither. _Needs:_ a decision. Either add it (for example an implicit `class`/attribute passthrough on the view root, which suits machine authors because it is one rule, never re-decided), or declare it a deliberate, documented divergence that the harness excludes. Not deciding means every task's interface differs silently.

**G0.8 — Lookups keyed by anything but a variant are inexpressible.** _Evidence:_ the changelog spec's `NODE_AXIS: Record<number, …>` and `NODE_LABELS: Record<number, string>` map an integer node id to an axis and to a label (`spec.tsx`). Mzizi's only table is an enum's columns, keyed by variant. _Needs:_ a keyed table (int- or text-keyed rows with a **required** fallback row). This keeps "the enum _is_ the table" (RFC-0001 §1.3) and makes the `?? "Unknown"` fallback structural: a missing fallback is a compile error, not a runtime `undefined`. The reference already documents what the missing fallback cost: `NODE_AXIS[11]` is undefined, so N11 and N12 are painted in the horizontal colour while labelled "Unknown (unknown axis)" (`reference.rs` module doc).

**G0.9 — Contracts cannot speak about collections, records, or the view.** _Evidence:_ RFC-0006 §10.3 leaves the evaluation of conditions, `match`/`case` and `for each` open; §3's subject order reaches only enums, columns, view attributes and prop defaults. Once G0.2–G0.5 land, `every entry …` and "renders one article per entry" are the assertions the corpus will need.

**G0.10 — The harness matches variants by name alone, and scores no view structure.** _Evidence:_ the harness maps a Rust enum to a Mzizi one by snake-casing its name, and scores each enum's variant set, `#[default]`, class-token Jaccard (reported, not counted), and — where the class carries an `h-N`/`size-N` token — pixel height (`benchmarks/harness/src/lib.rs`, RFC-0006 §10.1). Variants are paired by name. The changelog spec names its colours by axis (`horizontal`, `vertical`, `depth`, `outlier`), while the Rust reference renamed them by mineral (`NodeAccent::Cobalt` — "Was the `horizontal` axis" — `Tanzanite`, …) with identical class strings. Per the pilot's record, `class_token_jaccard` is `null` for `nyuchi-changelog-renderer` and both arms were charged the same two defects: both candidates followed their spec, so the harness was measuring the reference's drift from its own spec. _Needs:_ rename-aware matching, reported rather than hidden, plus view-structure scoring. (Being fixed in the benchmark pipeline; listed so the register is complete.)

### Tier 1 — interactive UI

- **G1.1 Local state.** Open since RFC-0001 §7.1, then RFC-0002 §6.1, then RFC-0003 §8.1: three RFCs have deferred it. Signal semantics must survive content addressing.
- **G1.2 Function bodies.** Statements: assignment to state, `emit`, `when`, `match`, calls, and returning a value.
- **G1.3 An expression language.** Comparisons, boolean and integer arithmetic, and text operations beyond interpolation. It must be a closed, small set, decided once. RFC-0002 §1 rules out symbol-dense syntax "permanently", but `==` and `+` versus `is` and `plus` still has to be decided.
- **G1.4 Exhaustive `match` checking** over enum variants (RFC-0001 §1.2 promises it).
- **G1.5 Derived values**, so authors never duplicate a computed fact as a second prop (FM-11 again).

### Tier 2 — full stack on Cloudflare (Phase 1: design now, build after the gate)

- **G2.1 Lowering.** Nothing lowers, and it is the largest single gap. The first milestone should be lowering Tier 0 components to Rust against `mzizi-ui`'s contract and compiling that with `rustc`. The same milestone gives Phase 0 its strongest defect oracle: the corpus's own `tests/contract.rs` pattern (RFC-0001 §5) can then run against Mzizi output directly, replacing the regex-level harness diff (RFC-0006 §10.1) with the tests the corpus already trusts.
- **G2.2 Declarations beyond `component`.** An HTTP route or handler, Worker entry points (`fetch`, `scheduled`, queue consumer), and a container service. This generalises RFC-0001 §7.4 from "one component, one file" to "one top-level declaration, one file, name-identical".
- **G2.3 Modules and packages.** Cross-file references (an app is more than one file by definition), dependencies on `mzizi-ui` components, and a manifest. **`use` is already taken** by capabilities (RFC-0001 §1.7), so imports need a different common word, chosen once (see D7).
- **G2.4 Boundary data.** Records serialise to JSON by construction and are validated at the boundary. **The same record type serves both the component and the route that feeds it.** That shared, once-checked type is what makes CHARTER.md v0.2 §2's "a component and the handler that serves it ship from the same Mzizi source" a property rather than a slogan, and it is the differentiator to protect.
- **G2.5 An error model.** `result(T, E)` and one propagation form. No exceptions, and no panics reachable from surface code.
- **G2.6 Async.** `workers-rs` is async throughout. Following RFC-0001 §1.8's precedent for ownership, `async`/`.await` should not exist at the surface: a function that uses an I/O capability is effectful, and the compiler inserts the awaits. This is a design decision with real consequences (ordering, concurrency of independent calls), not a formatting choice.
- **G2.7 Cloudflare bindings as capabilities.** `kv`, `d1`, `r2`, `queue`, Durable Objects, Workers AI, service bindings, the container binding, secrets and vars. **The Wrangler configuration is generated from these declarations, never hand-written beside them.** A hand-maintained `wrangler.toml` next to the code is FM-11 at deployment scale.
- **G2.8 Containers.** A native build target, image generation, and Worker-to-Container invocation through the binding. `mz build` needs a target concept (`worker`, `container`, `web`, `native`).
- **G2.9 Rust interop.** A typed, capability-tagged foreign boundary to crates (`workers-rs`, `serde`, Candle). CHARTER.md v0.2 §1's larger bet is "Rust becoming a genuine end-to-end solution for agentic software". Without interop, Mzizi can reach only what its own standard library re-implements. With it, the Rust ecosystem _is_ the standard library.
- **G2.10 Standard library scope.** Text formatting, time, JSON, HTTP client, hashing and randomness, and logging. Thin wrappers over crates via G2.9, decided as a list, not grown by accretion.
- **G2.11 Contracts for handlers.** `get "/api/x" status 200`-shaped assertions, and a local run under `wrangler dev`/Miniflare, so the defect metric covers server code as well as UI.
- **G2.12 Runtime-owned observability.** Structured logs and traces, owned by the runtime (RFC-0002 §2.2 names "structured telemetry" among the things a framework should own), not reinvented by each app.

### Tier 3 — ML

- **G3.1 A model declaration.** Typed input and output with no tensor types at the surface. Backends: Candle (native and container) and Workers AI (edge). RFC-0001 §1.7 says `use ml` "parses today and errors as 'not yet available'"; the code accepts it silently (§2, G0.1). This gap is what makes it available; G0.1 is what makes it error honestly until then.

### Tier 4 — native mobile and embedded (constraints to honour now, nothing to build)

- **G4.1 FFI-describable types.** CHARTER.md v0.2 §2 names a longer-term native-codegen track and "UniFFI, cxx" as the proven pattern for it, while stating it is "not designed yet". If that track takes a UniFFI-shaped boundary for Swift, ArkTS and Kotlin, the boundary can describe records, enums with payloads, lists, options and results. It cannot describe user generics, traits, or closures crossing the boundary. **Tier 0's type design should not introduce anything outside that set.** This constraint is cheap to honour now and a rewrite to honour later.
- **G4.2 No hidden `std` assumptions in the runtime core.** Named only, until Phase 5 (v0.2's embedded phase) is scoped.

### Cross-cutting tooling

Canonical rewriting (RFC-0001 §3: `mz` rewrites every file it touches; no command does today), `mz fix` (RFC-0001 §4.3), `mz refs`/`path`/`patch`/`diff` (RFC-0003 §5) and IR store persistence (RFC-0003 §8.3), a language server for human readers, documentation generated from `##` lines, and a version on the agent protocol itself.

## 4. One-way doors — decide before building Tier 0

**D1. The type system's shape.** Closed built-ins: `bool`, `int`, `text`, `list(T)`, `option(T)`, `result(T, E)`, `event(T)`. User-declared records and enums. No user generics and no traits at the surface in v0. Rationale: the smallest decision space that covers the corpus (RFC-0002 §1: "Constrain the space"), serialisable by construction (G2.4), and describable across a UniFFI-shaped mobile boundary (G4.1). _Open inside D1:_ enums today carry **static columns** (data per variant, fixed at compile time). Enums with **payloads** (data per value, at run time) are a different thing. Whether one construct does both, or two constructs split them, has to be decided explicitly, not arrive by accident.

**D2. Type-constructor syntax: `list(entry)`, not `[entry]` or `list<entry>`.** The parenthesised form already exists in the language as `event(T)` (`primitives/button.mz`: `prop on_tap: event(none)`), though the parser special-cases it for `event` alone and would need generalising. It is LL(1) and symbol-free. It avoids importing TypeScript and Rust generic syntax, which would invite FM-1 idiom sampling from both priors at once.

**D3. One top-level declaration per file, name-identical.** This generalises RFC-0001 §7.4 so that records, routes and workers follow the same rule as components, and every cross-file reference stays an exact anchor.

**D4. Effects are capabilities, and async is never written.** See G2.6.

**D5. Errors are values.** One `result` type and one propagation form. No exceptions.

**D6. The IR must not be UI-shaped.** RFC-0002 §4 required that the IR encode no Dioxus assumptions. The charter's v0.2 scope widens that rule: the IR must encode no UI-only assumptions, because routes, workers and models will share it.

**D7. Separate words for "needs a capability" and "depends on a module".** `use` means the first today, and it should not start meaning both.

## 5. What this means for the benchmark

1. **No Phase 0 number is evidence for the thesis until G0.1 is fixed.** Until then the Mzizi arm's compile loop is measured against a checker that cannot fail on names or types.
2. **The corpus the benchmark can honestly cover is bounded by Tier 0.** List- and record-shaped components are a large share of N7–N11. The pilot handled one only by shrinking it.
3. **The comparison arms are the strongest Rust has, not a primitive layer.** For UI that means Dioxus, plus Leptos (`benchmarks/arms/leptos/`, on `main` since `37d785b`) as the full-stack, fine-grained-reactive comparison. Once Tier 2 tasks exist, server tasks compare against raw `workers-rs` and `axum`.
4. **Two different benchmarks, not one.** Phase 0 measures _authoring_: tokens, iterations to a clean compile, and defect rate. Runtime performance is a separate claim, measurable only once G2.1 produces artifacts. For UI that means js-framework-benchmark-style measurement. For handlers it means TechEmpower-style measurement plus Worker cold start and bundle size. It must not be pooled with, or quoted as, the Phase 0 result.
5. **A small open-weight model arm is still required** (RFC-0002 §5.4). The pilot was frontier-only.

## 6. Proposed order

1. G0.1 (the checker can fail), together with G0.2–G0.5 (list, record, option, `for each`), because resolution needs the types to resolve against. **Work in flight:** the Tier 0 types are being implemented on branch `claude/lang-tier0-types`, with their design as `design/RFC-0008-types-collections-records.md`. Neither is on `main` as this is written.
2. G0.10 (harness), in parallel, in `benchmarks/`.
3. G0.6/G0.7 decisions, then G0.8, then G0.9.
4. Re-run the pilot with the Leptos arm and a small-model arm. **This is the Phase 0 number.**
5. Gate. Then G2.1 lowering, which serves Phase 1 and hardens the Phase 0 oracle at once. Then G2.2–G2.7 as one full-stack slice, deployed to a real Worker, then G2.8.
6. Tier 1 interleaves wherever the corpus needs it. Tier 3 follows Phase 1. Tier 4 is constraints only.

## 7. What this RFC does not claim

- That the list is complete. It is complete against the charter's scope as written, read once. A gap found later is a finding, and belongs here as an amendment.
- That any Tier 2 item is approved for building. CHARTER.md §4's gate is unchanged.
- That the pilot measured anything beyond itself: three tasks, one frontier model, one seed.
- That the pilot's figures quoted here (3/3 first-iteration-clean, the `null` Jaccard, the two shared defects, the `has_added`/`added_label` candidate) were re-verified. They are quoted from the pilot's record, which is not on `main` at `cd36430`. Everything else was checked against the code.
