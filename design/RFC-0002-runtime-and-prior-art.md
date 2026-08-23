# RFC-0002 — The design target, the runtime as the product, and what to mine

**Status:** draft for review — nothing here is implemented
**Author:** the machine author (Claude), correcting RFC-0001's design target
**Scope:** who the language is optimized for, the runtime architecture that is
the actual product, and a systematic prior-art survey with licence notes.
Supersedes parts of RFC-0001 §1.1 and §6 as noted.

---

## 1. The design target was wrong, and it changes the optimization function

RFC-0001 was written by a frontier model reasoning about its own experience.
That produced a subtle but load-bearing error: it optimized for the author
rather than for the population.

**The design target is small models.** A 7B open-weight model running locally,
not a frontier model with a million-token context. Frontier capability is
improving on its own; the language cannot help there and does not need to.
The population that _needs_ a language built for machine authorship is the one
that is parameter-constrained, context-constrained, and attention-constrained —
and that population is not going away, because local, private, cheap inference
is a permanent requirement, not a transitional one.

This inverts three conclusions:

| RFC-0001 assumed                                                  | Actually true for the target                                  | Consequence                                                     |
| ----------------------------------------------------------------- | ------------------------------------------------------------- | --------------------------------------------------------------- |
| Token efficiency declines in value as context windows grow        | Small models have small effective context, permanently        | Token efficiency is a first-order goal, not a fading one        |
| Redundant closers are parser elegance, not real value             | Weak long-range attention cannot reliably track nesting depth | `end <kind> <name>` is an **error-correcting code**; reinstated |
| Novel syntax pays a familiarity penalty that outweighs constraint | Constraint helps weak models more than familiarity does       | A small, closed grammar beats a familiar, open one              |

The general principle: **a familiarity penalty is a one-time cost, amortized by
a spec, a formatter, and a tight error loop. A large decision space is a
permanent tax, paid on every generation, and it scales inversely with model
size.** Constrain the space.

Two further consequences follow specifically from the small-model target and
belong in the grammar:

- **Locally predictable grammar.** Keep the grammar trivially parseable
  (LL(1) where possible, one statement per line, no expression/statement
  duality). A grammar a parser can predict from one token of lookahead is a
  grammar a small model can predict from local context — the same property
  serves both.
- **Small, common-word keyword vocabulary.** Small models have weaker
  tokenizer coverage of rare tokens and weaker priors on unusual identifiers.
  Every keyword should be a common English word that tokenizes to a single
  token. This reinforces RFC-0001 §2 and rules out symbol-dense syntax
  permanently.

RFC-0001's §1.1 self-criticism (that the `end` echo was a diagnostics problem
in disguise) is withdrawn. Its §6 metric mapping stands, with the addition that
**the benchmark must be run against a small open-weight model as well as a
frontier one** — the frontier number alone cannot validate the thesis, because
the frontier model is the one least helped by the design.

## 2. The runtime is the product

The syntax is the interface. The runtime is the thing worth owning, and the
thing that makes adoption easy: **shared logic, written once, running
everywhere.** A component's logic, contracts, and capability requirements are
target-independent; only the host binding changes.

```
        .mz source
             │
       [ front end ]         ← the grammar (RFC-0001)
             │
     content-addressed IR    ← the core asset (§2.1)
             │
   ┌─────────┼─────────┬──────────┐
 WASM     native      edge     (future targets)
 (web)  (desktop)  (Workers)
```

### 2.1 Content-addressed IR — one choice, four wins

Every IR node is stored under the hash of its content (the idea is Unison's,
and it is the single most valuable thing to borrow from anywhere). One
architectural decision delivers four of this project's stated goals at once:

1. **Fast incremental compilation** — a change alters one hash; only its
   dependents recompile. This is the cheapest known route to the sub-second
   check loop the charter demands as a Phase 0 success metric.
2. **Semantic addressing** — code is addressed by structure, not by text
   offsets, so an agent can patch "the body of `retry`" without exact-string
   matching. This kills FM-4 (edit-anchor collisions) at the architecture
   level rather than papering over it.
3. **Perfect caching** — an unchanged hash never rebuilds, locally or in CI,
   and the cache is trivially shareable.
4. **Renames are free** — a name is metadata pointing at a hash, so renaming
   touches one binding instead of every call site. Small models are
   particularly bad at exhaustive multi-site refactors; this removes the class.

### 2.2 The host interface is capability-shaped

The runtime exposes capabilities (`net`, `storage`, `ml`, `motion`); a
component declares what it needs (RFC-0001 §1.7) and the runtime supplies a
per-target implementation. This is Roc's platform/application split, and it
means one component runs on web, desktop, and edge with no source change —
the adoption story in one sentence.

It also makes the runtime the natural home for the things a framework should
own rather than each app reinventing: supervision and restart of failed
subtrees, hot reload, and structured telemetry.

## 3. What to mine, and from where

The instruction is right: most of this is open-licensed, so the correct move is
systematic borrowing plus improvement, not invention from zero. Below is what I
would take from each, and why it fits the small-model target.

**Licence discipline, stated once:** _design ideas_ are not copyrightable and
can be freely adopted. _Code_ carries obligations — MIT and BSD require
retaining the copyright notice; Apache-2.0 additionally requires a NOTICE file
and grants patent rights; UPL-1.0 is permissive; **GPL/AGPL sources must not be
copied into this project at all** (viral licence, incompatible with the intended
`@bundu` distribution). Where the table says "code," a licence-compliance step
is required and must be recorded in a NOTICE file.

| Source               | Licence            | Take                                                                                                            | Why it fits the target                                                                                                           |
| -------------------- | ------------------ | --------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| **Elm**              | BSD-3              | Error-message design: expected vs. found vs. hint, one error at a time made _actionable_; no runtime exceptions | Still the best compiler-error UX shipped. A small model cannot infer from indirect hints — Elm's directness is the model to copy |
| **Roc**              | UPL-1.0            | Platform/application split; error quality; no hidden allocation                                                 | The platform concept _is_ the runtime-as-product architecture (§2.2)                                                             |
| **Gleam**            | Apache-2.0         | Tiny keyword set; simple ML types without higher-kinded generics; one canonical formatter; friendly errors      | Living proof a small, learnable language is productive — and a small type system is a small decision space                       |
| **Elixir / Ruby**    | Apache-2.0 / BSD-2 | `end`-delimited blocks; the pipe operator; `do` blocks                                                          | `end` has real training corpus _and_ suits weak attention — the rare case where familiarity and constraint agree                 |
| **Unison**           | MIT                | Content-addressed codebase; abilities                                                                           | §2.1 — the highest-value borrow in this table                                                                                    |
| **Koka / Effekt**    | MIT / BSD-2        | Row-polymorphic effect types                                                                                    | The rigorous version of RFC-0001's capability declarations                                                                       |
| **Zig**              | MIT                | "No hidden control flow" as a hard rule; comptime; explicit allocation                                          | No-hidden-control-flow is a machine-authorship principle: what you read is what runs (cf. FM-7)                                  |
| **Rust**             | MIT / Apache-2.0   | Exhaustive `match`; algebraic enums with payloads; trait dispatch; Cargo's single-tool UX                       | Enum-with-data is the structural fix for the corpus's parallel-map drift; Cargo is the UX bar for `mz`                           |
| **Swift**            | Apache-2.0         | Result builders; named arguments at call sites                                                                  | Precedent for a declarative view grammar with no macro layer                                                                     |
| **Erlang / BEAM**    | Apache-2.0         | Supervision trees; hot code reload; per-process isolation                                                       | If we own the runtime, this is the proven resilience model (§2.2)                                                                |
| **Lean 4 / Idris 2** | Apache-2.0 / BSD-3 | Refinement/contract types; incremental elaboration                                                              | Rigour for `contract` blocks, which carry the Phase 0 defect metric                                                              |
| **SQLite**           | Public domain      | Testing discipline (exhaustive, adversarial); single-artifact distribution                                      | The reliability bar a runtime must clear to be adoptable                                                                         |
| **Tree-sitter**      | MIT                | Error-tolerant incremental parsing                                                                              | Parse-with-recovery is what makes "all diagnostics at once" possible (FM-5)                                                      |

### 3.1 What is genuinely missing everywhere — the actual contribution

Nothing above ships these, which is what leaves room for a new language:

1. **Contracts in the language, checked by the toolchain, as the primary
   correctness surface.** Elm/Roc give type safety; Lean gives proofs at
   research cost. Nobody offers lightweight behavioural contracts beside the
   code, run by default, aimed at the compiles-clean-but-wrong defect class.
2. **A diagnostic protocol designed for a machine reader** — structured,
   whole-program, with machine-applicable fixes as data. Every compiler above
   emits prose for humans and, at best, a JSON wrapper around it.
3. **Content-addressed IR wired to an agent-facing patch API.** Unison has the
   store; nobody has exposed it as the editing interface for a non-human author.
4. **A grammar whose ambiguity budget is deliberately near zero**, with the
   small-model constraint as the stated design rationale rather than an
   afterthought.

Those four, together, are the defensible research contribution — and none of
them are syntax-flavoured preferences; they are all measurable against the
Phase 0 metrics.

## 4. Open question: own primitives vs. Dioxus interop

The charter stages Phase 1 as Dioxus rendering interop; the direction now
stated is that Mzizi owns its primitives and competes. These are reconcilable
as a sequence rather than a contradiction, and the recommendation is:

- **Phase 1 keeps Dioxus interop.** It is the cheapest way to get a real
  rendering artifact behind the IR and start measuring, and it does not
  constrain the IR's design.
- **Own primitives arrive when the runtime is proven**, replacing the Dioxus
  binding target-by-target behind the same IR. Because §2.1 puts a stable IR
  in the middle, swapping the back end is not a rewrite.

Deciding this now is not necessary; deciding it _later than the IR_ is. The IR
must not encode Dioxus assumptions — that is the one design constraint this
open question imposes today.

## 5. What changes in the plan

1. Reinstate `end <kind> <name>` (RFC-0001 §1.1) with the error-correcting-code
   rationale; withdraw the §1.1 self-criticism.
2. Add two grammar rules from §1: locally predictable (LL(1)-leaning), and a
   small common-word keyword vocabulary.
3. Promote token efficiency back to a first-order goal.
4. The Phase 0 benchmark **must** include a small open-weight model arm; the
   frontier arm alone cannot validate the thesis.
5. Build the content-addressed IR early — it is the prerequisite for the
   compile-speed metric, semantic patching, and cheap caching, and it is
   cheaper to design in than to retrofit.
6. Record borrowings in a NOTICE file as they land, per §3's licence discipline.

## 6. Open questions for RFC-0003

1. **Local state** — still unanswered from RFC-0001; needs signal semantics
   that survive the IR's content addressing.
2. **The IR's concrete shape** — node schema, hashing scheme, and the patch API
   an agent drives.
3. **Effect-system depth** — full row polymorphism (Koka) or the simpler
   declared-capability list already in RFC-0001.
4. **Contract language expressiveness** — how far past equality and structural
   assertions to go before it becomes a proof assistant.
