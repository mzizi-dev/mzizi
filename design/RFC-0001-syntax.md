# RFC-0001 — Mzizi-lang surface syntax, canonical form, and the agent protocol

**Status:** draft for review — nothing here is implemented
**Author:** the machine author (Claude), with the Phase 0 charter as the spec
**Scope:** the `.mz` surface syntax, the canonical-form rule, the compiler's
agent-facing output protocol, and what lowers to what. Grammar formalism,
the type system's internals, and the compiler architecture are later RFCs.

> **Amended by RFC-0002.** This RFC was written optimizing for a frontier
> model's experience. The design target is **small models** — see RFC-0002 §1,
> which restores token efficiency to a first-order goal, adds two grammar
> rules (locally predictable, small common-word keyword vocabulary), and
> supplies the real rationale for §1.1's closer. Read the two together.

---

## 0. Method: design against named failure modes

The charter says "designed for machine authorship" must cash out as measurable
goals. This RFC goes one level deeper: every syntax decision below is justified
against a **named failure mode I actually hit** when authoring code as an LLM
agent. If a decision doesn't trace to a failure mode, it doesn't belong in the
language.

| ID       | Failure mode (what actually goes wrong when I write code)                                                                                                                                                                                                                                                                                                                                 |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **FM-1** | **Idiom sampling.** When a language offers N equivalent ways to express one intent (`match` / `if let` / ternary / early return), generation samples from a distribution over them. Every choice point is a place to be subtly inconsistent with the codebase — or subtly wrong.                                                                                                          |
| **FM-2** | **Anonymous-delimiter mismatch.** A `}` closing something 200 lines up is the single most common purely syntactic failure. The delimiter carries no information about _what_ it closes, so neither I nor the parser can localize the mistake — one missing brace cascades into dozens of phantom errors.                                                                                  |
| **FM-3** | **Non-local fixes.** Errors whose repair site is far from the report site (Rust lifetimes/borrows are the extreme case). I fix what the error points at; if the true fix is elsewhere, I loop.                                                                                                                                                                                            |
| **FM-4** | **Edit-anchor collisions.** Agents mostly _edit_, via exact-string matching. Repeated boilerplate (identical derive lines, identical prop ceremonies) makes `old_string` non-unique and edits fail or land on the wrong copy.                                                                                                                                                             |
| **FM-5** | **Round-trip starvation.** Compilers that report one error at a time, or stop at the first parse error, turn one mistake into five compile cycles. Each cycle is a full agent turn.                                                                                                                                                                                                       |
| **FM-6** | **Token burn.** Ceremony that carries no decision — import rituals, derive lists, visibility keywords repeated on every line, `.to_string()` noise — consumes context window without informing the model. (The inverse trap is APL-terseness: sub-word symbol soup tokenizes badly and is underrepresented in training. The optimum is _plain words, no ceremony_, not _few characters_.) |
| **FM-7** | **Macro opacity.** When the visible source differs from what actually compiles (proc macros, rsx! blocks), errors point into generated code I can't see. The UI syntax must be _native grammar_, never a macro.                                                                                                                                                                           |
| **FM-8** | **Formatting entropy.** Any formatting freedom means diffs mix semantic and cosmetic change, wastes edit-match reliability, and burns tokens re-deciding line breaks.                                                                                                                                                                                                                     |
| **FM-9** | **Hidden capability.** To reason locally about a function I must know what it _can do_ (network? storage? inference?) without reading its transitive callees.                                                                                                                                                                                                                             |

## 1. The shape of a component

The worked example is a real corpus component — `nyuchi-connectivity-bar`
(N7), whose TypeScript reference and Rust port both exist, so every line
below is checkable against known ground truth. This is deliberate: RFC
examples must come from the benchmark corpus, never invented.

```mz
component connectivity_bar

  ## A status strip announcing the app's network state.
  ## Corpus reference: n7-shell/nyuchi-connectivity-bar.

  use motion

  enum connection_state
    online   label "Back online"      color "bg-malachite"
    syncing  label "Syncing changes"  color "bg-gold"
    cached   label "Viewing cached"   color "bg-sodalite"
    offline  label "You are offline"  color "bg-terracotta"
  end

  prop state: connection_state
  prop visible: bool = true
  prop on_state_change: event(connection_state)

  view
    when not visible
      nothing
    end
    strip slot "nyuchi-connectivity-bar"
      class "fixed inset-x-0 top-0 z-50 {state.color}"
      role "status"
      text state.label
      when state is offline
        button "Retry"
          class "min-h-[48px]"
          tap retry
        end
      end
    end
  end

  fn retry
    emit on_state_change(syncing)
  end

  contract
    online.label is "Back online"
    offline.color is "bg-terracotta"
    when offline: shows button "Retry"
    button "Retry": min_height 48
  end

end component connectivity_bar
```

What this example commits us to, decision by decision:

### 1.1 `end` with name echo, not braces — _FM-2_

Blocks close with `end`. Top-level declarations close with `end <kind> <name>`
(`end component connectivity_bar`), and the parser **cross-checks the echo**.
A mismatched or missing closer is a single, precisely-located diagnostic
("`end component connectivity_bar` expected before line 61, found `end view`"),
never a cascade. The redundancy is spent exactly where it buys error
localization; short inner blocks pay only a bare `end`.

Indentation is canonical (§3) but **not significant** — the parser reads
`end`, not whitespace, so whitespace mangling in transit can't change meaning.

The deeper reason, per RFC-0002 §1: the echo is an **error-correcting code for
weak long-range attention**. A parameter-constrained model cannot reliably
track nesting depth across 200 lines; the closer hands it the answer locally
instead of requiring it to reconstruct the stack. Parser locality is the
secondary benefit, not the primary one.

### 1.2 One construct per intent — _FM-1_

The grammar has exactly one form for each intent, and no alternatives:

| Intent                | The only form                                                      | Deliberately absent                                                                                                                        |
| --------------------- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Conditional           | `when <cond> … [else …] end`                                       | `if`, ternary, `unless`, expression-`if`                                                                                                   |
| Enumeration           | `enum … end` with per-variant data columns                         | separate const maps keyed by variant (the .tsx's `Record<state, string>` pattern — a real corpus defect class: the map and the enum drift) |
| Iteration             | `for each x in xs … end`                                           | `while`, `loop`, iterator-chain vs loop duality                                                                                            |
| Branching on variants | `match x / case a … / case b … / end`, exhaustive, no guards in v0 | `if`-chains over variants                                                                                                                  |
| Events out            | `emit <event>(args)`                                               | callback-calling conventions                                                                                                               |
| Absence               | `nothing` (in views), `none` (as a value)                          | `null`/`undefined` duality                                                                                                                 |

There is no statement/expression duality: logic is statements; only `view`
and `contract` have their own declarative sub-grammars. When there is only
one way to write it, generation cannot pick the wrong idiom and review
cannot bikeshed it.

### 1.3 Variant data lives on the variant — _FM-1, FM-4_

`enum connection_state` carries `label` and `color` as **columns on the
variants**. The TypeScript corpus keeps these in parallel `Record` maps —
and porting the corpus to Rust found exactly the drift you'd predict
(nodes 11/12 missing from a map, wrong fallback color). In Mzizi the enum
_is_ the table; a missing cell is a compile error, and `state.label` is a
total function by construction.

### 1.4 Props, events, and defaults are declarations, not types-of-functions — _FM-6_

`prop name: type [= default]`, one per line. Events are props of type
`event(payload)`; `emit` is the only way to fire one. No destructuring
ceremony, no interface + function-signature double-declaration (the .tsx
pattern declares every prop twice: once in the interface, once in the
destructure — pure FM-6 burn, and a real drift site).

### 1.5 The view grammar is native — _FM-7_

`view … end` is part of the language grammar, not a macro. Elements are
words (`strip`, `row`, `button`, `text`); attributes are `name value` lines;
children are nested blocks. Every diagnostic inside a view points at source
the author actually wrote. Interpolation is `{expr}` inside strings, and
that is the only string-building mechanism.

Attributes that the corpus proved load-bearing are first-class words with
checked values: `slot` (the `data-slot` contract), `role` (ARIA — and the
compiler knows that an explicit `role` _replaces_ implicit semantics, the
`role="listitem"`-on-a-button defect class), `class` (Tailwind, statically
scanned so dynamic-class construction is impossible — another corpus rule
made structural).

### 1.6 `contract` is part of the component — _FM-3, and the whole Phase 0 metric_

The charter defines a defect as _compiles clean but behaviorally wrong_.
So behavior assertions are **in the language**, beside the code they
constrain, in a declarative sub-grammar (`subject: assertion`). `contract`
blocks lower to the same contract-test pattern the corpus Rust ports use
(`tests/contract.rs`), and `mz check` runs them as part of the loop —
the defect metric is measured by the toolchain itself, not by an external
harness bolted on later. A component without a `contract` block compiles
with a warning; corpus ports require one.

The touch-floor rule (`min_height 48`) is expressible as a contract line
because the corpus found that exact violation five separate times. Rules
the doctrine calls non-negotiable should be one line to assert.

### 1.7 Capabilities are declared at the top — _FM-9_

`use net`, `use storage`, `use ml`, `use motion`. A component that declares
no `net` cannot call anything that needs it — checked transitively, reported
locally ("`retry` calls `fetch_status` which needs `net`; add `use net` to
`connectivity_bar` or remove the call"). An agent reading the first five
lines of any component knows its blast radius. `use ml` is the future
Candle seam (Phase 3); it parses today and errors as "not yet available".

### 1.8 No ownership at the surface — _FM-3_

Mzizi lowers to Rust (Phase 1: Dioxus interop), but **no borrow, lifetime,
or ownership concept exists in the surface language**. Values have value
semantics; props and state are cheap handles by construction; the compiler
owns the Rc/signal plumbing in lowered code. The entire class of
non-local-fix errors that makes Rust hard for agents is spent once, by the
compiler authors, instead of on every generation. If a lowering constraint
genuinely can't be hidden, the diagnostic must name a _surface-level_ fix.

## 2. Naming and lexical rules — _FM-1, FM-6_

- `snake_case` for everything the author names; `kebab-case` never appears
  in source (corpus component identity `nyuchi-connectivity-bar` maps to
  `connectivity_bar` by a fixed, invertible rule). Enforced by the grammar,
  not a linter — a `camelCase` identifier is a parse error with an exact fix.
- Keywords are common English words (`component`, `prop`, `view`, `when`,
  `emit`, `contract`) — single tokens in every mainstream tokenizer, and
  maximally supported by training priors.
- One string literal form (`"…"` with `{}` interpolation), one comment form
  (`##`, doc-significant; there are no throwaway comments — if it matters,
  it's a doc line; if it doesn't, it's deleted).
- Statements are newline-terminated. No semicolons, no line-continuation
  syntax; an expression too long for a line is a sign it needs a named `fn`.

## 3. Canonical form: the compiler owns formatting — _FM-8, FM-4_

There is exactly one rendering of any Mzizi program. `mz` **rewrites every
file it touches to canonical form** — two-space indent, one attribute per
line past two, fixed column alignment in enum tables, fixed declaration
order inside a component (`doc → use → enum/type → prop → view → fn → contract`).
Agents and humans never make a formatting decision, diffs are always
semantic, and any unique line of code is a unique edit anchor. There is no
configuration. (The formatter-as-compiler also means the benchmark's token
counts are measured against one canonical surface, not a formatting lottery.)

## 4. The agent protocol: `mz check --agent` — _FM-5, FM-3_

The compiler's machine-facing output is a contract, versioned like the
language itself.

1. **Whole-program, all at once, deterministic order.** Statement-per-line
   plus `end`-anchored blocks lets the parser resynchronize at every line;
   the recovery target is **at most one diagnostic per true author error**,
   never a cascade, and never "fix one to see the next".
2. **NDJSON, one diagnostic per line:**

   ```json
   {"code":"MZ0412","file":"connectivity_bar.mz","span":[38,10,38,17],
    "say":"`emit on_state_change(sync)` — `sync` is not a variant of `connection_state`; nearest is `syncing`",
    "fix":{"span":[38,10,38,17],"replace":"syncing"},"confidence":"exact"}
   ```

   `say` is written for a reader holding **zero file context**: it quotes
   the offending source inline, so the agent needn't re-read the file to
   understand the error. Target ≤ 200 chars — density is the budget.

3. **Fixes are data.** Every diagnostic carries a machine-applicable `fix`
   when one is unambiguous, tagged `exact` / `guess` / `none`. `mz fix`
   applies all `exact` fixes in one shot — deleting an entire iteration
   from the loop for the whole class of mechanical errors.
4. **The loop summary line.** Human or agent, the last line is always:
   `mz: 3 errors (2 exact-fixable), 1 contract failure, 480ms`.
5. **`mz outline <file>`** emits the interface — component name, props,
   events, capabilities, contract subjects — at roughly a tenth of the
   token cost of the source. This is the representation agents load into
   context for _dependencies_, reserving full source for the file being
   edited. Outline format is stable and is itself valid `.mz` (a component
   with everything but bodies).
6. **Compile speed is a protocol property.** The check loop budget is
   sub-second incremental for a single-component change; the benchmark
   records it per iteration. A slow compiler fails Phase 0 no matter how
   good its errors are.

## 5. What lowers to what

| Mzizi                     | Lowers to                                                                       |
| ------------------------- | ------------------------------------------------------------------------------- |
| `component` + `view`      | Dioxus component + rsx (Phase 1), via a stable IR                               |
| `enum` with data columns  | Rust enum + exhaustive `match` accessors                                        |
| `prop` / `event` / `emit` | Dioxus `Props` / `EventHandler`                                                 |
| `contract`                | Rust `#[test]` contract tests, same pattern as the corpus's `tests/contract.rs` |
| `use net/storage/ml`      | capability-gated shims per target (web/native/edge/Workers)                     |

The IR (not this RFC) is the second agent surface: token-efficient,
queryable, and what `mz outline` reads.

## 6. What the benchmark measures against this design

Phase 0's metrics map one-to-one onto the failure modes:

- **tokens consumed** → FM-6/FM-8 (ceremony + formatting entropy) and §2/§3
- **iterations to clean compile** → FM-2/FM-5 (§1.1, §4.1–4.4)
- **defect rate** (compiles-but-wrong) → FM-1/FM-3/FM-7 (§1.2–1.6, contracts in-language)

Baseline: the same corpus components authored in raw Dioxus and Leptos by
the same agent under the same harness. If Mzizi doesn't beat both on at
least two of three metrics, the thesis is wrong and Phase 1 does not start.

## 7. Open questions for the next RFC

1. **State.** This RFC covers props-in/events-out components (the corpus's
   dominant shape). Local state (`state count: int = 0` + assignment
   semantics that lower to signals) needs its own treatment.
2. **Grammar formalism + parser strategy** — hand-written recursive descent
   with per-line recovery is the working assumption, to keep error quality
   under our control.
3. **The IR** and `mz outline`'s stability guarantees.
4. **Naming the file of record**: whether component name must match file
   name (leaning yes — one component, one file, name-identical; it makes
   every cross-file reference an exact anchor).
