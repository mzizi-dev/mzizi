# RFC-0006 — Contract evaluation: the clause grammar, the subject language, and what `mz contract` proves

**Status:** draft; implemented in this PR (`compiler/src/contract.rs`, `mz contract`)
**Author:** the machine author (Claude)
**Scope:** what a `contract` block means, the clause grammar and the subject-resolution
order, what the evaluator checks and what it deliberately does not, whether contracts
participate in the content-addressed IR's hash, and why contract evaluation is its own
subcommand rather than part of `mz check`. Lowering contracts to Rust `#[test]`s
(RFC-0001 §5) is Phase 1 and is not settled here.

> **RFC-0005 is not this document.** `mzizi-dev/agent-tools#76` reserved that number for
> the catalogue-and-language RFC before this one was written, and it is still open. The
> number is left free rather than reused.

<!-- Two separate amendment notes; this separator keeps them distinct. -->

> **Answers RFC-0003 §8.4**, which left "contract evaluation semantics — the subject
> language for assertions like `every button_size height at_least 48`" as an open question
> addressed to a later RFC. This is that RFC.

---

## 0. Method: four more named failure modes

RFC-0001 §0 fixed the rule this project designs by: _if a decision doesn't trace to a named
failure mode, it doesn't belong in the language._ RFC-0001 named nine (FM-1…FM-9) in
_writing_ code; RFC-0003 named eight (RB-1…RB-8) in _reading_ it. Contracts need four more,
and the numbering **continues** RFC-0001's table rather than restarting, so an ID is unique
across the whole design record.

Each of these was observed in this repository or its corpus, not imagined.

| ID        | Failure mode                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **FM-10** | **The unverified promise.** An assertion that is parsed and discarded reads, to every later reader, as verification that has already happened. Before this RFC the compiler set `has_contract: bool` and threw the body away, while `primitives/README.md` said contracts were what held the corpus to its touch floor. Nothing was checking it.                                                                                                      |
| **FM-11** | **Parallel truth.** One rule written twice drifts. RFC-0001 §1.3 names the `Record<state, string>` version. The same shape reappeared _inside this repo_: the 48px floor lived both in `button.mz`'s `contract` block and in a hand-written Rust assertion in `compiler/tests/primitives.rs`, which labelled itself "this test is the check until the contract evaluator lands". Two copies of one rule, one of them in a language that is not Mzizi. |
| **FM-12** | **The silently-inapplicable assertion.** If an assertion the evaluator cannot apply defaults to passing, an unevaluable contract is indistinguishable from a satisfied one — which is strictly worse than no contract, because it reads as proof. `card.mz` shipped `card_radius uses "--radius-lg"`, naming a subject that appears nowhere in the file. It looked like a check for as long as nothing tried to run it.                               |
| **FM-13** | **Metric conflation.** CHARTER.md §6 measures two different things and is explicit that they are different: "a syntax/compile error is not itself a 'defect' for this metric… the defect rate measures what gets _past_ the compiler wrong". A single exit status covering both makes the Phase 0 defect metric unreadable — a run that failed to parse and a run that parsed and lied become the same observation.                                   |

## 1. What a contract is — _FM-10_

A `contract` block is a set of assertions about the component it sits in, checked against
that component's own declarations. It is the in-language form of the corpus's
`tests/contract.rs` pattern (RFC-0001 §1.6), and it exists because the charter defines a
defect as behaviour, not syntax.

Three consequences, all load-bearing:

- **A contract is a set, not a sequence.** Reordering the lines of a contract changes no
  meaning. §6 makes the IR agree with that.
- **A contract constrains; it does not describe.** `slot is "card"` is a claim the compiler
  must be able to refute, not documentation.
- **An assertion that cannot be evaluated is a failure.** Never a pass, never a warning.
  See §5.

## 2. The clause grammar

One assertion per line, `<subject> <predicate>`, with no punctuation between them.

```mz
contract
  every button_size height at_least 48
  button_size.default height is 56
  button_variant.default class contains "bg-primary"
  slot is "button"
end
```

Every form below is taken from `primitives/` or `examples/connectivity_bar.mz` — the RFC
rule is that examples come from the corpus and are never invented. The corpus's thirty-three
assertions across ten files use exactly these shapes and no others.

### Subjects

| Form                        | Means                                          | Corpus line                              |
| --------------------------- | ---------------------------------------------- | ---------------------------------------- |
| `every <enum> <column>`     | every variant's cell in that column            | `every button_size height at_least 48`   |
| `<enum>.<variant> <column>` | one cell                                       | `button_size.default height is 56`       |
| `<variant>.<column>`        | one cell, with the enum inferred (§3)          | `offline.color is "bg-terracotta"`       |
| `<name>`                    | a view attribute, else a prop default (§3)     | `slot is "button"`, `announce not_empty` |
| `<element> "<text>"`        | the view element carrying that text            | `button "Retry" min_height 48`           |
| `when <variant>`            | the branch of the view guarded on that variant | `when offline shows button "Retry"`      |
| _(none)_                    | the component itself                           | `uses button`                            |

### Predicates

| Form               | Holds when                                       | Corpus line                                        |
| ------------------ | ------------------------------------------------ | -------------------------------------------------- |
| `is <value>`       | the value is exactly that, as written            | `alert_variant.destructive announce is "alert"`    |
| `contains "<s>"`   | a string value contains that substring           | `avatar_size.default class contains "size-10"`     |
| `not_empty`        | a string value has non-whitespace in it          | `every avatar_size text_class not_empty`           |
| `at_least <n>`     | a numeric value is at least _n_                  | `every input_size height at_least 48`              |
| `in "<a>" "<b>"`   | a string value is one of a closed set            | `every alert_variant announce in "status" "alert"` |
| `uses "--token"`   | the value reads `var(--token…)`                  | `class uses "--radius-lg"` (see §8.3)              |
| `uses <name>`      | the view composes that component                 | `uses button`                                      |
| `shows <el> "<t>"` | the guarded branch renders that element          | `when offline shows button "Retry"`                |
| `min_height <n>`   | the element's declared height is at least _n_ px | `button "Retry" min_height 48`                     |

`is` and `in` are existing keywords. The other seven are ordinary identifiers, reserved only
in predicate position — RFC-0002 §1 asks for a small keyword vocabulary, and a word that is
only special after a subject costs nothing elsewhere.

## 3. Subject resolution is a fixed order, not a search — _FM-12_

A bare or two-part name is resolved by trying, **in this order**:

1. an enum of that name,
2. a variant of that name — accepted only when **exactly one** enum has a variant of that
   name carrying the named column,
3. a view attribute, taken from the **outermost** element that carries it,
4. a prop of that name, using its declared default.

Anything else is an error (`MZ0605`), never a pass.

Three things this order is chosen to prevent:

- **Adding a prop must not change what an existing clause means.** Props are last, so a new
  `prop slot: text` cannot quietly redirect `slot is "button"` away from the view.
- **A same-named variant in two enums is an error, not a coin toss.** Step 2 requires
  uniqueness. Resolving by declaration order would make a clause mean different things
  depending on where an enum was added.
- **`slot is "card"` is about the card, not about one of its rows.** `card.mz` declares
  three slots — `card`, `card-header`, `card-body`. A search that accepted any of them
  would make the assertion nearly unfalsifiable. Step 3 takes the outermost, breadth-first,
  because `slot` is the component's own identity attribute in the design system.

## 4. What "the value" is

Values are compared **as written**. A column holding `"bg-primary"` is a string; one holding
`56` is a number. `is` compares the written forms, so `height is "56"` and `height is 56` are
different assertions and the first fails against a numeric column. This is deliberate:
`announce is "alert"` and `announce is alert` differ (a string versus an identifier), and
silently unifying them would make the closed-set check on `alert` meaningless.

## 5. What `mz contract` checks, and what it does not — _FM-10_

Being precise about this is the point of the section. The README, `primitives/README.md` and
the ROADMAP have all now been corrected to match it.

**It checks**, statically, against the declarations in the one file:

- variant-table cells, individually and column-at-a-time;
- view attributes, and prop defaults;
- the presence and text of elements in the view tree, including inside a `when` branch;
- composition — that a named component appears in the view;
- design-token references inside class strings;
- declared heights, from a `height` attribute or a `min-h-[Npx]` / `h-[Npx]` class.

**It does not check:**

- **Anything rendered.** There is no renderer (CHARTER.md Phase 1). `when offline shows
button "Retry"` is a claim about the _shape of the view tree_, not about what a browser
  paints. A `when` whose condition the evaluator cannot interpret is matched by the variant
  name appearing in the condition, which is weaker than evaluating the condition.
- **Anything against a reference implementation.** `mz contract` is a _self_-consistency
  check: it asks whether a component does what it says. Scoring a Mzizi port against the
  hand-written `.rs` in `mzizi-dev/mzizi-registry` is a second, separate comparison, and it
  belongs to the benchmark harness that does not exist yet (§10.1). **The Phase 0 defect
  metric needs both halves; this RFC delivers one of them.**
- **Tailwind scale classes.** `h-12` is 48px in one Tailwind version and need not be in
  another. The evaluator reads only bracketed pixel values and reports `h-12` as
  _unevaluable_ rather than guessing, because a check that silently disagrees with what
  renders is FM-12 wearing a different hat.
- **Cross-file anything.** `uses button` checks that the view has a `button` element. It
  does not load `button.mz` or check that the props passed to it exist. That needs the
  manifest and the name→hash namespace (RFC-0003 §3), and it is §10.2.
- **Prop values.** A prop with no default has no value this component controls, so a clause
  about one is unevaluable and says so.

**Unevaluable is a failure.** `MZ0605` is an error, exits 1, and fails CI. A contract that
quietly does nothing is worse than no contract, because a reader takes it for verification.

## 6. Contracts participate in the IR hash — _RB-6_

RFC-0003 left this open. The decision is **yes**, and the argument is RB-6's own.

RB-6 is "no stable identity across time", and the IR's answer is that "a recorded fact can
be re-verified by asking whether that hash still exists". The fact an agent most wants to
record about a component is _that its behaviour was checked_. If the contract sits outside
the hash, that record is false: the assertions can be weakened or deleted while the hash
stands still, and the re-verification story quietly stops being true. A component that
promises less is not the same component.

The objection — that adding an assertion should not invalidate the implementation's
incremental compilation — turns out to cost nothing, because the store is a Merkle DAG with
structural sharing. Changing an assertion changes the clause node, the `contract` node and
the component root. **Every implementation subtree keeps its hash**, so what recompiles is
the contract, which is exactly what should. `compiler/src/ir.rs`'s
`a_contract_change_leaves_the_implementation_subtrees_untouched` is that claim as a test.

Two details follow from §1's "a contract is a set":

- Clause children are **sorted by hash and de-duplicated** before the `contract` node is
  built. Unlike a view's children — whose order is meaning, RFC-0003 §2.1 — reordering
  assertions changes nothing, so it must change no identity.
- A clause hashes over its **canonical spelling**, rebuilt from the parsed assertion rather
  than copied from the file, so source whitespace cannot alter identity.

### 6.1 A measured claim of RFC-0003's that this correction moves

RFC-0003 §7 reported **127 shared nodes vs 141 isolated — 14 saved across 10 files**. With
contract bodies in the IR the same measurement now reads **169 vs 174 — 5 saved**, and the
difference is not noise. Before this PR every component lowered its contract to the _same_
empty `contract` marker node, so nine of the fourteen "saved" nodes were ten copies of a
placeholder collapsing into one. Genuine sharing of authored content was five, and still is.

That is worth stating plainly because it cuts against the RFC that made the claim:
**RFC-0003's sharing figure was 64% an artifact of the thing this RFC replaced.** §7 has
been amended in place with the new number and this explanation, following the RFC-0003 §7.1
precedent — the implementation is the fact.

## 7. A separate subcommand, not folded into `mz check` — _FM-13_

RFC-0001 §1.6 wrote that "`mz check` runs them as part of the loop". That is **narrowed
here**: `mz contract <file>` is its own subcommand with its own exit status.

The reason is CHARTER.md §6's own sentence. The charter measures compile friction (§3's
compile-error-density goal) and behavioural defects as two separate metrics and says
explicitly that a compile error is not a defect. Folding contract failures into `mz check`'s
exit code would make the two indistinguishable to any harness that branches on status — and
branching on status without parsing output is a stated property of the protocol
(`compiler/src/main.rs`). One command, one question.

Consequences, all of which the implementation honours:

- **Exit status matches `mz check`'s existing contract**: 0 clean, 1 for any error
  (a failed assertion included), 2 for usage or I/O. Nothing new to learn.
- **A file that does not compile has its contract skipped**, not failed. Assertions are not
  run against a tree the parser had to guess at, and no `MZ0603` is ever emitted for a file
  with parse errors. `compiler/tests/contracts.rs` asserts this directly.
- **Malformed clauses are still `mz check` errors.** The clause grammar is grammar, so
  `MZ0601` and `MZ0602` come out of the parser and fail an ordinary check. Only the
  _evaluation_ is deferred to the second command.
- **The diagnostic protocol is unchanged, and extended.** `mz contract --agent` emits the
  same NDJSON, and RFC-0001 §4.4's summary line gains `contract_clauses` and
  `contract_failures` — which §4.4 always specified ("`mz: 3 errors (2 exact-fixable), 1
contract failure, 480ms`") and the implementation had not yet had anything to put there.

### 7.1 Diagnostic codes

| Code     | Severity | Means                                                      |
| -------- | -------- | ---------------------------------------------------------- |
| `MZ0209` | error    | a second `contract` block in one component                 |
| `MZ0601` | error    | a clause the grammar does not recognize                    |
| `MZ0602` | error    | a clause with no predicate — carries an `exact` fix (§8.2) |
| `MZ0603` | error    | **an assertion that does not hold** — the Phase 0 defect   |
| `MZ0605` | error    | an assertion that cannot be evaluated (§5)                 |

`MZ0603` is the code a benchmark harness counts. It is emitted only by `mz contract`, and
only on a file that compiled.

## 8. What the corpus forced

Four divergences between what was written down and what is true. Each is settled here, and
the file of record is corrected rather than the discovery being left in a commit message.

### 8.1 View attributes take `=`; RFC-0001 §1.5 was wrong

RFC-0001's worked example wrote `class "fixed inset-x-0…"` and `role "status"` — attribute
name then value, no `=`. Every `.mz` file in this repository writes `class = "…"`, and the
parser requires it.

The implementation is right and the RFC was stale. `name value` and a child element are
indistinguishable with one token of lookahead — `text state.label` could be an attribute or
an element opening a block — which is exactly the ambiguity RFC-0001 §1.2 and RFC-0002 §1
forbid. `compiler/src/parse.rs` has carried the full argument in a doc comment since the
parser was written; what had not happened is anyone amending §1.5. **RFC-0001 §1.5 and its
§1 example are amended in this PR**, following RFC-0003 §7.1: where an RFC and the code
disagree, the code is the fact.

This is not orthogonal to contracts. The same divergence runs through the contract
sub-grammar: §1.6 describes it as `subject: assertion` and the §1 example writes
`when offline: shows button "Retry"` and `button "Retry": min_height 48`, with colons. The
corpus files write neither colon, and this RFC's grammar has none. §1.6 is amended too.

### 8.2 `height 56` with no predicate is an error, not a second spelling

`button.mz` and `input.mz` wrote both `button_size.sm height 48` and
`button_size.default height is 56` — the same intent, two spellings. RFC-0001 §1.2 permits
exactly one form per intent, and the reason is FM-1: every choice point is a place to be
subtly inconsistent.

So the bare operand is `MZ0602`, and it carries an `exact` fix that inserts `is` — a purely
mechanical repair `mz fix` can apply with no model in the loop. Three lines across two
primitives were corrected in this PR.

### 8.3 `card_radius` named nothing — _FM-12 caught in the act_

`card.mz` asserted `card_radius uses "--radius-lg"`. There is no `card_radius` in that file:
no enum, no variant, no attribute, no prop. It had read as a check since the day it was
written, and the first run of an evaluator over the corpus reported it in the first second.

The intent is clear and worth keeping — the card's radius must come from the design token
rather than a hard-coded number — so the clause is rewritten as `class uses "--radius-lg"`,
which resolves to the root surface's class string and is genuinely checkable. The `uses`
predicate looks for `var(--radius-lg`, not for the bare token name, so a class that merely
mentions the token without reading it fails.

This is the single best argument for §5's "unevaluable is a failure". Under any other rule
this line would still be sitting there looking like verification.

### 8.4 `portal` is undocumented, and is not this RFC's business

All nine primitives carry a `portal = "https://mzizi.dev/components/<name>"` attribute on
their root element. No RFC mentions it, `primitives/README.md` does not mention it, and
`compiler/src/lex.rs`'s first-class-attribute list (RFC-0001 §1.5 names `slot`, `role`,
`class`) does not include it. The parser accepts it because view attributes are open.

It is orthogonal to contract evaluation — nothing in any contract refers to it — so it is
**reported, not fixed here**: it wants either an RFC-0001 §1.5 amendment making it
first-class with a checked value, or deletion. Tracked as
[`mzizi-dev/mzizi#5`](https://github.com/mzizi-dev/mzizi/issues/5).

## 9. Measured, in this PR

From `compiler/tests/contracts.rs` and `compiler/tests/ir_measured.rs`, over the nine
primitives plus `examples/connectivity_bar.mz`, so every figure is reproducible with
`cargo test -- --nocapture`.

| Claim                       | Measured                                                                                             |
| --------------------------- | ---------------------------------------------------------------------------------------------------- |
| Corpus coverage             | **33 assertions across 10 files, all 33 evaluated** — the test fails if any clause is merely counted |
| Corpus result               | **0 failures** — every primitive does what it says                                                   |
| Defect detection            | **6 mutations**, each compiling cleanly under `mz check` and failing `mz contract`                   |
| Parse + evaluate, whole set | **~1.3 ms** for 10 files; budget 250 ms                                                              |
| Structural sharing          | **169 shared nodes vs 174 isolated** — 5 saved across 10 files (§6.1 explains the move from 14)      |
| Identity                    | reordering a contract changes no hash; changing an assertion changes the root and nothing else       |

The six mutations are corpus defect classes, not invented ones: a button size below the 48px
touch floor, an ARIA role drifting from its severity, a design token dropped from a class
string, a composed component deleted, a control's text changed out from under a `shows`
clause, and a `min-h-[48px]` reduced to `44px`. Each is asserted to compile cleanly first —
a mutation that broke the parse would prove nothing, since CHARTER.md §6 excludes compile
errors from the metric by name.

**What this does not measure.** Nothing here is a Phase 0 benchmark number. Thirty-three
assertions written by the same author as the components they check is a self-consistency
result; the charter's metric requires scoring agent-authored Mzizi against independent
reference implementations. See §10.1.

## 10. Open questions for the next RFC

1. **The other half of the defect metric.** `mz contract` proves a component keeps its own
   promises. CHARTER.md §6 requires comparing against a reference implementation read from
   disk. Whether that is a `mz contract --against <ref>` mode, a harness-side comparison, or
   contract _generation_ from a `.rs` reference is undecided, and it is the last thing
   between here and a first benchmark run.
2. **Cross-file contracts.** `uses button` currently checks a tag in the view. With the
   manifest's name→hash namespace it could check that the composed component exists, that
   its contract holds, and that the props passed to it are ones it declares. That is the
   first real use of RFC-0003 §3 beyond renaming.
3. **Conditions, evaluated.** `when <variant> shows …` matches a guard by variant name. A
   real evaluation of `when state is offline` — and of `else`, `match`/`case`, and
   `for each` — needs the expression semantics RFC-0001 §7.1 deferred along with local
   state.
4. **Contract subjects in `mz outline`.** RFC-0001 §4.5 says the outline carries "contract
   subjects"; RFC-0003 §4's example shows `contract 5`. Today it emits an empty `contract`
   block, because the outline must itself be valid Mzizi and `contract 5` is not. Whether a
   dependency's assertions are interface is a real question and is not settled here.
5. **Lowering.** RFC-0001 §5 maps `contract` to Rust `#[test]`s. Phase 1.
