# RFC-0010 — Contracts everywhere: functions, handlers, services and the standard library

**Status:** draft for review — nothing here is implemented. The grammar in §3 extends RFC-0006's,
and depends on two things no RFC has designed yet: function parameters and bodies (RFC-0007
G1.2) and handler declarations (G2.2). Where a signature appears below it is a placeholder for
those, and is marked as one.
**Author:** the machine author (Claude)
**Scope:** the owner's direction of 2026-09-29, that everything built carries a contract. It
covers the clause forms for functions, handlers, services and records; which clauses are checked
statically, by generated tests or at runtime; how they lower to Rust; how failures reach an agent;
how a handler's contract relates to its HTTP behaviour and to the benchmark's facts (RFC-0009);
what "requires a contract" means for the toolchain; and the migration. It defers the expression
language (G1.3), which bounds what a clause can say, and any solver or proof, which §4.4 rules
out for v0.

> **Amends** RFC-0001 §1.6 (a missing contract is now a lint with two levels, §8) and RFC-0006
> (its grammar is the component case of §3, and its §10.5, lowering, is answered by §5). Both
> carry a note pointing here.

<!-- A second note; the separator keeps it distinct. -->

> **Amended by RFC-0011.** §3.2's and §3.3's declaration lines were placeholders for G2.2. A
> `route` is now an inner block of a `service`, one service per file, and a top-level `route`
> waits for modules (G2.3). A service's contract is evaluated by running it in process, so
> `MZ0607` never applies to one (RFC-0011 §7).

<!-- A third note; the separator keeps it distinct. -->

> **RFC-0013 proposes the signature syntax §3.1's placeholder waited for:**
> `fn positive_int(raw: option(text)): option(int)`, closed by `end fn positive_int`, in place of
> the `take` / `give` lines (RFC-0013 §6, §15.1). It is a draft for review; nothing in it is
> implemented.

---

## 0. Method: four more failure modes, one of them measured

The numbering continues RFC-0006's table (FM-10 … FM-13). RFC-0008 used a `TY-` prefix so as
not to collide with this range, and nothing on `main` has taken FM-14 since.

| ID        | Failure mode                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **FM-14** | **The contract on the copy.** A clause checks a value the author declared, not the value the program renders or returns. _Measured in pilot 2_ (`benchmarks/results/2026-09-27-pilot-2/RUN.md`): both clean 7B Mzizi buttons wrote `icon class "size-14" height 48`. The class renders 56px, and the `height` column says 48. `every button_size height at_least 48` read the 48, and RFC-0006 §5 deliberately cannot read `size-N`. Had the class been `size-10` (40px), the touch-floor contract would still have passed. It is FM-11, parallel truth, inside the contract system itself, and it is exactly the failure a contract system must not have. |
| **FM-15** | **The name as the only contract.** A function's only statement of its behaviour is its name, and the name is wrong. The API gateway's `positiveInt` (`mzizi-api-gateway` `src/routes/registry.ts`, `2468b2a`) accepts `"0"`, which `offset=0` needs. Through JavaScript's `Number()` it also reads `" 7 "` as 7, `"0x10"` as 16 and `"1e2"` as 100 (checked with Node). A port that trusts the name rejects 0 and breaks the first page. _Observed._                                                                                                                                                                                                       |
| **FM-16** | **Validation as a precondition.** Untrusted input is guarded by a precondition. If checks are compiled out of release builds, the input goes unvalidated in production. If they are kept, a malformed request becomes a panic instead of a `400`. _Anticipated_ from design-by-contract practice, not observed here.                                                                                                                                                                                                                                                                                                                                       |
| **FM-17** | **The tested claim read as proven.** A property that passed 256 generated cases is reported as "verified". That is FM-10, the unverified promise, one level up. _Anticipated._                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |

## 1. Rules every contract obeys

- **C-1. A clause's subject is an observed value — _FM-14_.** A subject must be what the program
  renders, returns or responds with, or a value the compiler derives from it. A declared copy
  that nothing renders cannot be a subject. For pilot 2's case this means the `height` column
  stops being authored: the compiler derives it from the class through the pinned spacing scale
  the harness already uses (`h-N`, `size-N` → `N × 4` px, and bracketed pixels as today). A
  written `height` that disagrees with its class is an error. Pilot 2's pre-kill-criterion list,
  item 3, is the first instance of this rule, and whatever mechanism lands for it must satisfy
  C-1. C-1 is also what makes contracts scoreable across languages (§7): an observed value is
  observable in every arm.
- **C-2. Unevaluable is a failure** (RFC-0006 §5, unchanged). A clause the toolchain cannot check
  is `MZ0605`, never a pass.
- **C-3. A contract is not validation — _FM-16_.** Untrusted input is parsed at the boundary by
  its record type (G2.4), and a rejection is a declared response. That rejection is behaviour, so
  it is stated with `example` and `ensure` on the handler. `require` exists only between trusted
  callers.
- **C-4. Checked, tested, proven: three words, never swapped — _FM-17_.** Reports say _checked_
  for the static tier, _tested (n cases, seed s)_ for generated tests, and never _proven_ or
  _verified_, because nothing in v0 proves.
- **C-5. One source.** Generated tests and benchmark probes are generated from the contract,
  never written again by hand beside it. A hand copy is FM-11.

## 2. The unit of contract is the top-level declaration

RFC-0007 D3 makes every file hold one top-level declaration: a component, `fn`, route, service
or record. The contract lives in that declaration, last in canonical order (RFC-0001 §3), and
participates in its root hash (RFC-0006 §6). An inner `fn` such as `connectivity_bar`'s `retry`
is covered by its owner's contract (`when offline shows button "Retry"`) and may carry its own.
So "everything has a contract" means every file does.

## 3. The clause grammar

It is the same block everywhere: `contract … end`, one clause per line, `subject predicate` with
no punctuation, and a set rather than a sequence (RFC-0006 §1). Four leading words are added.
Each is a common English word, and each is special only at the start of a clause line:

| Leading word | Means                                                  | Declared on          | How it is checked (§4)                  |
| ------------ | ------------------------------------------------------ | -------------------- | --------------------------------------- |
| _(none)_     | RFC-0006's assertions, unchanged                       | components           | statically                              |
| `require`    | a precondition, over the parameters                    | `fn`                 | statically where literal; debug runtime |
| `ensure`     | a postcondition, for every input that meets `require`  | `fn`, route          | generated property tests; debug runtime |
| `always`     | an invariant, over fields or state                     | record, service      | generated tests; debug runtime          |
| `example`    | one concrete input and the fact expected of its result | `fn`, route, service | a generated test each (after lowering)  |

`require`, `ensure` and `always` are the ideas behind Eiffel's `require`, `ensure` and
`invariant`, the origin of design by contract. RFC-0002 §3's rule is that design ideas are free to
adopt. Examples and properties need no separate keyword each: an `ensure` clause _is_ the
property, quantified over every valid input, and an `example` is one case of it.

New subjects: a parameter by name; `returns` (and `returns.<field>`) for a function's result;
`status`, `header "<name>"`, `body` and `body.<field>` for a response. New predicates, beside
RFC-0006's nine: `at_most <n>`, `is not <value>`, and `is error <variant>` for a `result`.
`in` accepts numbers as well as strings. A conditional clause reads
`ensure when <subject> <predicate> then <subject> <predicate>`, extending RFC-0006's `when`.

### 3.1 A function

The corpus has no Mzizi function with parameters yet, so this ports the gateway's `positiveInt`,
the FM-15 case. The first three lines are a placeholder for G1.2's signature syntax.

```mz
fn positive_int
  take raw: option(text)
  give option(int)
  ## the body is G1.2's to design
  contract
    example raw none returns is none
    example raw "" returns is none
    example raw "0" returns is 0
    example raw " 7 " returns is 7
    example raw "-1" returns is none
    example raw "1.5" returns is none
    ensure when returns is not none then returns at_least 0
  end
end fn positive_int
```

`example raw "0" returns is 0` is the line that makes the name's lie visible. The task spec has to
settle `"0x10"`, and the contract then records the decision as one more `example`: `is 16` to
match the gateway, or `is none` as a documented divergence.

An `ensure` with a value predicate on an `option` subject that is not narrowed is a static error,
with an `exact` fix that inserts the `when … is not none then` guard. Without that rule the clause
would pass vacuously on a function that always returns `none`, which is FM-12 by another route.

### 3.2 A handler

Every clause of a handler is phrased at the HTTP boundary: a method and path in, and a status,
headers and body out. The lines below are the gateway's real behaviour at `2468b2a`, taken from
its `test/api.test.ts`, `src/http.ts` and `src/redirects.ts`. The declaration line is a placeholder for G2.2.

```mz
route ui_item
  get "/v1/ui/{name}"
  ## the handler body is G2.2's to design
  contract
    example get "/v1/ui/button" status is 200
    example get "/v1/ui/button" header "content-type" is "application/json"
    example get "/v1/ui/does-not-exist" status is 404
    example get "/v1/ui/does-not-exist" header "cache-control" is "private, no-cache, no-store, max-age=0, must-revalidate"
    example get "/v1/ui/nyuchi-a11y" status is 308
    example get "/v1/ui/nyuchi-a11y" header "location" is "/v1/ui/mzizi-a11y"
    example delete "/v1/ui/button" status is 405
    ensure status in 200 204 308 404 405 500 503
    ensure header "x-mzizi-source" not_empty
    ensure when status is 500 then body.error is "Internal server error"
  end
end route ui_item
```

Request bodies are never written inline: `example post "…" body file "fixtures/version.json" …`
names a fixture file. JSON's braces collide with Mzizi's `{}` interpolation, and the fixture is
the same file the benchmark's probes send (C-5).

### 3.3 A service with state, and a record

A service's `always` clauses hold after every handler returns. An `example … end` block is a
sequence of requests run in order against fresh state. It is the one multi-line clause, and the
sequence is its meaning.

```mz
  contract
    always every versions count at_least 0
    example
      post "/v1/ui/button/versions" body file "fixtures/v1.2.0.json" status is 201
      post "/v1/ui/button/versions" body file "fixtures/v1.2.0.json" status is 409
    end
  end
```

A record's `always` clauses hold whenever a value is built. A record that crosses a boundary is
parsed into that shape or rejected (C-3), so `always` is never the validation step.

## 4. What is checked where

### 4.1 Statically, by `mz check` and `mz contract`, with no execution

- Everything RFC-0006 §5 checks today.
- Every subject resolves: parameters, `returns`, record fields, response facets. A miss is
  `MZ0605`.
- Every predicate fits its subject's type (`at_least` on `text` is an error), and every
  `example` binds every parameter exactly once with a literal of its type.
- **An `example` that violates its own `require` is an error.** It is cheap, literal and
  catches a contract that contradicts itself.
- **Clauses that contradict each other on literals are an error.** An `example` with status 418
  under `ensure status in 200 404` is one.
- A route's `example` paths must match its declared path pattern.

### 4.2 By generated tests, run by `mz contract` once the declaration lowers

- Each `example` becomes one test. For a handler, the test sends the request to the lowered
  server in process (axum's `tower::ServiceExt::oneshot`), with no socket.
- Each `ensure` and `always` becomes a property test over the parameters' types, filtered by
  `require`. Handler properties generate requests from the route's own path pattern, the method
  set and mutated query strings.
- Generation is **deterministic**: the seed is derived from the declaration's IR hash, so the
  same code gives the same cases and the same output, as the agent protocol requires
  (RFC-0001 §4.1). Counterexamples are shrunk before they are reported.
- This tier exists only once something lowers (G2.1). Until then, `mz contract` on a `fn` or
  route checks §4.1 and reports every behavioural clause as `MZ0607`, "not yet testable", which is
  an error, not a pass (C-2).

### 4.3 At runtime, in debug and test builds only

`require`, `ensure` and `always` lower to assertions in debug and test builds, and to nothing in
release. Two reasons. RFC-0007 G2.5 says no panic may be reachable from surface code, and a
release-mode contract failure would be one. And C-3 means no contract stands between a user and
bad input, so removing the checks from release removes no validation.

### 4.4 Not at all, in v0

There is no solver, no SMT and no proof. RFC-0002 §6.4 asked how far the contract language can go
"before it becomes a proof assistant", and this RFC's answer is that the static tier stays
literal and structural. Everything else is tested, and is reported as tested (C-4).

## 5. Lowering to Rust

The compiler emits plain Rust. It does not depend on a contracts macro crate: the lowered code
should not rely on a proc-macro whose semantics this project does not control. The `contracts`
crate (MIT/Apache-2.0) is the prior art for the shape.

| Clause              | Lowers to                                                                                         |
| ------------------- | ------------------------------------------------------------------------------------------------- |
| `require`           | `debug_assert!` at function entry, whose message carries `MZ0608` and the clause's canonical text |
| `ensure`            | `debug_assert!` on the value at each return, bound once so it is not evaluated twice              |
| `always`            | `debug_assert!` after construction, and after each service handler                                |
| `example`           | one `#[test]` in a `#[cfg(test)]` module                                                          |
| `ensure` / `always` | one `proptest!` property each, in the same module, seeded from the IR hash                        |
| a route's clauses   | tests that drive the lowered axum `Router` with `oneshot`                                         |

`proptest` (MIT/Apache-2.0) is a dev-dependency of the **lowered** crate, not of the compiler, so
the compiler's no-dependency rule (AGENTS.md) holds. The lowered test module is generated into
`mz build`'s gitignored output and never committed (CONTRIBUTING.md, RB-5).

## 6. What the agent sees

RFC-0006 §7 stands (FM-13): `mz check` reports compile errors, including malformed and
contradictory contracts, and `mz contract` reports contract failures. Both speak the same NDJSON
protocol, so an agent reads one format. Folding failures into `mz check`'s exit status would
make a compile error and a behavioural defect the same observation again. The owner's request,
that contract failures reach the agent as dense diagnostics through the agent protocol, is met
this way: the protocol is one, and the commands stay two.

The shape is today's, taken from a real `mz contract --agent` run on a mutated `button.mz`:

```json
{"code":"MZ0603","severity":"error","file":"b.mz","span":[47,5,47,41],"say":"`every button_size height at_least 48` does not hold — `sm` is 44, below 48"}
```

Generated-test failures add a shrunk counterexample, and keep `say` within RFC-0001 §4's 200
characters. These two lines are illustrative, since no function lowers yet:

```json
{"code":"MZ0611","severity":"error","file":"positive_int.mz","span":[11,5,11,55],"say":"`ensure when returns is not none then returns at_least 0` does not hold — raw \"-1\" returns -1 (shrunk; 256 cases, seed 9c41)","counterexample":{"raw":"-1"}}
{"code":"MZ0612","severity":"error","file":"ui_item.mz","span":[7,5,7,47],"say":"`example get \"/v1/ui/button\" status is 200` does not hold — status is 503"}
```

Proposed codes, continuing RFC-0006 §7.1. `MZ0604` stays unused, as it is today.

| Code     | Tool          | Means                                                                            |
| -------- | ------------- | -------------------------------------------------------------------------------- |
| `MZ0606` | `mz check`    | clauses contradict each other, or an `example` violates its own `require`        |
| `MZ0607` | `mz contract` | a behavioural clause is not yet testable, because the declaration does not lower |
| `MZ0608` | runtime       | a `require` failed in a debug build (the caller's defect)                        |
| `MZ0609` | runtime       | an `ensure` or `always` failed in a debug build (the callee's defect)            |
| `MZ0610` | `mz check`    | a value predicate on an un-narrowed `option`; carries an `exact` fix             |
| `MZ0611` | `mz contract` | an `ensure` or `always` failed a generated test                                  |
| `MZ0612` | `mz contract` | an `example` failed                                                              |
| `MZ0613` | `mz check`    | a top-level declaration has no evaluated clause (§8)                             |

The summary line gains `contract_tested` (generated cases run) beside `contract_clauses` and
`contract_failures`. Old consumers ignore the new key, as they did when contracts were added.

## 7. Contracts are the benchmark's facts

RFC-0009's facts and this RFC's clauses are one list, by C-1 and C-5:

- **A backend task's reference contract is its probe file.** Each `example` on a route is one
  probe, and each probe's `clause` field names the clause it came from.
  `mz contract --emit-probes` writes `probes.toml` from the reference solution, so the probes are
  never a second hand-written copy. `ensure` clauses become property probes the harness generates
  the same way in every arm.
- **A UI task's facts are clauses whose subject is observable in every arm's output**: variant
  sets, defaults, derived heights, `data-slot`. C-1 is what makes that set language-neutral, and
  FM-14 shows what happens without it.
- **A candidate's own contract is never a scored fact.** It is self-consistency (RFC-0006 §5).
  The facts come from the task's reference contract, which no agent sees.
- **Public suites fit the same shape.** MultiPL-E's and Exercism's hidden tests are `example`
  lines, and RFC-0009 §8's translators emit them as a contract block appended after generation,
  so one evaluator, `mz contract`, serves every suite.

## 8. What "requires a contract" means

- **One lint, `MZ0613`**: a top-level declaration with no clause that is evaluated. An empty
  block, or one whose clauses are all unevaluable, does not count (FM-12).
- **Two levels, set per package in the manifest (G2.3):** `contracts = "warn"`, the default, and
  `contracts = "required"`, which makes `MZ0613` an error. There is no per-file or per-line
  suppression: an escape hatch beside a rule is FM-1.
- **`required` from day one** for the nine primitives, the examples, the standard library
  (G2.10), every Mzizi Roots component authored in Mzizi, and every package that serves traffic.
  Everything the owner listed is covered: components, the language's own functions, handlers and
  services.
- **The language has a contract too.** For the compiler, every diagnostic code in an RFC table
  must have a test that triggers it and one that does not. A registry test fails if any code
  lacks either. This extends C-2 to the toolchain: a promised diagnostic nobody triggers reads as
  a check that exists.
- **The benchmark sandbox stays at `warn`.** The pre-registered task wording does not ask for a
  contract, and turning `MZ0613` on inside the loop would change the Mzizi arm between runs
  (RFC-0009 BM-5). Measuring contract authoring is a family of its own, if it is ever wanted.

## 9. Migration

1. **Nothing becomes an error before the next gating run.** The Mzizi arm must not change while
   its measurement is pending, so everything below is either additive syntax or waits.
2. **The primitives already comply.** All nine carry contracts: 29 clauses, 45 across the eleven
   `.mz` files with the two examples (`mz contract --agent` summary lines on `main` at
   `a9c928d`). Every one of them is evaluated. C-1 makes the `height` clauses depend on the
   derived column. They move when pre-kill item 3 lands, and not before.
3. **The lint arrives as a warning everywhere**, and packages move to `required` one at a time.
   The standard library and Roots-in-Mzizi start at `required`, because there is nothing to
   migrate.
4. **No generated contracts.** There is no `mz contract --scaffold` that writes `example` lines
   from current behaviour: that would record today's bugs as the specification and read as
   verification (FM-10). A missing contract stays a visible warning until someone writes one
   against the spec.
5. **Rust-authored Roots components keep their `tests/contract.rs`** in the registry. That file
   _is_ their contract. When a component is re-authored in Mzizi, its contract block replaces the
   Rust test, whose lowered successor is generated (C-5). While both exist, the Rust test is the
   reference, and the Mzizi port is scored against it: that is the benchmark.
6. **The backend port.** When Mzizi builds the backend (CHARTER.md v0.3), the gateway's
   `test/api.test.ts` and its parity script are the contract the Mzizi port must meet. Their
   assertions become `example` lines, and parity against live stays the acceptance test.

## 10. Costs, stated plainly

- **Tokens.** Every declaration grows. On the only small-model data there is, the 7B model already
  did worse in Mzizi than in Dioxus (pilot 2), and more required surface is more to get wrong.
  This is why §8 keeps the benchmark at `warn`.
- **Latency.** The static tier fits the sub-second check loop (RFC-0001 §4.6). Generated tests
  need lowering and `cargo test`, which takes seconds. That is one more reason they stay in
  `mz contract` and out of `mz check`.
- **False assurance.** A weak contract passes. Generated tests find only what a clause says.
  Mutation scoring, as RFC-0006 §9 did by hand for six mutations, is the honest measure of
  contract strength, and it is an open question (§11).
- **Expressiveness.** Until G1.3's expression language exists, a clause cannot relate two
  parameters arithmetically or state idempotence. Some real properties will not fit, and
  `example` lines carry them in the meantime.
- **A design dependency.** §3's function and handler forms wait on G1.2 and G2.2. If those RFCs
  choose differently, §3 follows them.

## 11. Open questions

1. **Mutation scoring** of contract strength: `mz contract --mutate`, reporting surviving mutants.
2. **Release-mode checks** for a `strict` build profile that logs and returns `500` instead of
   stripping. It is ruled out for v0 by G2.5, and worth revisiting once there is an error model.
3. **Relational properties** (idempotence, round-trips, `returns at_most limit`) need G1.3.
4. **Cross-file contracts** (RFC-0006 §10.2): whether a caller's `example` may rely on a callee's
   `ensure`.
5. **Whether `mz outline` carries contracts** (RFC-0006 §10.4). A handler's `example` lines are
   arguably interface.
