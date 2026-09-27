# RFC-0008 — Types, collections, records, and a checker that can say no

**Status:** draft; implemented on `claude/lang-tier0-types` (`compiler/src/resolve.rs`,
plus grammar in `lex.rs` / `parse.rs`)
**Author:** the machine author (Claude)
**Scope:** RFC-0007's Tier 0 gaps G0.1–G0.5: name and type resolution for everything the
AST models (G0.1), `list(T)` (G0.2), `record` (G0.3), `option(T)` (G0.4), and `for each`
(G0.5). Collection and record contract predicates (G0.9), enum payloads, modules and
cross-file checking (G2.3), local state, and lowering are **not** settled here.

> **Implements part of RFC-0001 §1.2 that the code never did.** §1.2's table has listed
> `when <cond> … [else …] end` and `for each x in xs … end` as the only conditional and
> iteration forms since the first draft. The parser accepted `for` as an unchecked element
> word and rejected `else` outright, and `benchmarks/prompts/mzizi-guide.md` told authors
> "No `else`". This RFC adds no new conditional or iteration form; it makes the two
> RFC-0001 already promised real, and gives them the checking they need.

---

## 0. Method: the failure modes the pilot measured

RFC-0001 §0's rule stands: a decision that does not trace to a named failure mode does not
belong in the language. The failure modes below use their own `TY-` prefix, the way
RFC-0003 used `RB-`, because RFC-0007 is in flight on another branch and may allocate
`FM-14` onwards; a separate prefix cannot collide with it.

Every one was observed, in the 2026-09-27 Phase 0 pilot or in this repository.

| ID       | Failure mode                                                                                                                                                                                                                                                                                                                                                                                     |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **TY-1** | **The name nothing checks.** `prop x: flarp` passed `mz check --agent` with zero errors; so did `text = lable` and `class = "{x.nope}"` (§9.1). `PropDecl.ty` was a string, and names in attribute values, `{...}` and dotted cells were never resolved. The Mzizi arm's own system prompt said so: "The compiler does not check type names or the names inside `{...}`, so spell them exactly." |
| **TY-2** | **A cheap misspelling moves to a dear place.** A typo a Rust author pays for with one compile cycle is free in Mzizi, and resurfaces later as a behavioural defect or not at all. That is not a Mzizi advantage; it inflates the iterations-to-clean-compile metric in Mzizi's favour and moves the cost into the defect metric, where it is harder to see.                                      |
| **TY-3** | **The collection written by hand.** With no list type, the pilot's Mzizi author wrote "Mzizi has no list/array prop type, so the original `entries[]` … reduce here to the fields of one entry". The component the task asked for could not be written.                                                                                                                                          |
| **TY-4** | **Optionality as parallel truth.** Each optional field became two props, `has_added: bool` plus `added_label: text`. That is FM-11 (RFC-0006 §0), one fact written twice and free to drift, reintroduced by a gap in the language rather than by an author.                                                                                                                                      |
| **TY-5** | **Two ways to say nothing.** TypeScript's `null` / `undefined` / `[]`, and Rust's `Option<Vec<T>>` beside an empty `Vec<T>`, are two spellings of "there are none". FM-1 at the type level: a choice point where generation samples, and where two components end up disagreeing about what absence looks like.                                                                                  |
| **TY-6** | **The silent empty render.** In JSX, `{entry.added}` on an absent value renders nothing and passes every check. A missing value becomes an invisible blank instead of a diagnostic, so absence handling is unverifiable.                                                                                                                                                                         |

## 1. The closed type set — _D1, TY-1, TY-5_

```text
type := bool | int | text
      | <enum name> | <record name>          declared in this file
      | list(<type>) | option(<type>)
      | event(<type>) | event(none)
```

Closed: no user generics, no traits, no maps, no floats. Every type is describable across a
UniFFI-shaped FFI boundary and serialisable, because later mobile and server targets depend
on it:

| Mzizi        | UniFFI shape                       | JSON                        |
| ------------ | ---------------------------------- | --------------------------- |
| `bool` `int` | `boolean`, `i64`                   | `true`, number              |
| `text`       | `string`                           | string                      |
| enum         | enum; columns are static accessors | the variant name, a string  |
| record       | `dictionary`                       | object                      |
| `list(T)`    | `sequence<T>`                      | array                       |
| `option(T)`  | `T?`                               | the value, or absent        |
| `event(T)`   | callback interface (props only)    | not data — never serialised |

Three rules keep that property true:

- **Events are not data.** `event(...)` is legal only as a prop's type. A record field, a
  list element or an event payload of event type is `MZ0702`.
- **Records may not contain themselves**, directly or through another record, list or
  option (`MZ0705`). Recursive trees are a real use case (nested menus) and are listed as
  open (§10), because the FFI representation of a recursive dictionary is not settled.
- **No type is spelt with symbols** (D2, FM-1). Type constructors use the parenthesised form
  `event(T)` already had: `list(entry)`, `option(text)`. `list<entry>`, `entry[]` and
  `[entry]` are the TypeScript and Rust priors a model samples from; each is `MZ0105`, whose
  `exact` fix is the Mzizi spelling, and the lexer hands the parser the repaired tokens so
  one mistake is one diagnostic (FM-5).

Type names stay out of the keyword list, as `lex.rs` already argues for `bool` / `int` /
`text`: `list`, `option`, `record` and `field` are resolved by position. `field` in
particular is also an element word (`primitives/input.mz`'s view is a `field`), so
reserving it would have broken a primitive.

### 1.1 One way to say nothing — _TY-5_

- **A list is never optional.** The empty list is its absence. `option(list(T))` is
  `MZ0703` with an `exact` fix to `list(T)`. The spec's `componentsAdded?: string[]` is
  `list(text)`, which is also what the Rust reference already does (`Vec<String>`, empty
  when absent).
- **An option is never nested.** `option(option(T))` is `MZ0703`, fixed to `option(T)`.
- **An event is never optional.** An event the caller does not bind already does nothing
  when emitted, so `option(event(T))` is `MZ0703`, fixed to `event(T)`. This is the
  TypeScript `onClick?: () => void` prior.
- **`none` is the one spelling of absence** in a test, for both kinds (§4).

### 1.2 Defaults: one form each

| Type            | When the caller omits it                            | `= …` written                                                     |
| --------------- | --------------------------------------------------- | ----------------------------------------------------------------- |
| `bool` `int`    | required — no default means the caller must pass it | `= true` / `= 3`, type-checked (`MZ0706`)                         |
| `text`          | required                                            | `= "…"`, type-checked                                             |
| enum            | required                                            | `= <variant>`, checked against the enum with a nearest-name fix   |
| `list(T)`       | **empty**, implicitly                               | `MZ0706`, `exact` fix deletes it                                  |
| `option(T)`     | **none**, implicitly                                | `MZ0706`; `= none` is deleted, `= v` rewrites the type to `T = v` |
| record, `event` | required / unbound                                  | `MZ0706` — there are no record literals                           |

The rule behind the table: a list and an option each already _have_ a value that means
"not given", so writing it would be a second spelling of the default (FM-1), and writing
anything else contradicts the type. `option(text) = "x"` is not optional at all; the fix
says so by rewriting it to `text = "x"`.

## 2. Records — _G0.3, TY-3_

```mz
record changelog_entry
  field version: text
  field nodes_affected: list(ecosystem_node)
  field components_added: list(text)
end
```

- Declared inside the component file, like enums, and closed with a bare `end`. Cross-file
  sharing waits for modules (RFC-0007 G2.3).
- Canonical order puts `record` with `enum` (RFC-0001 §3: doc → use → enum/record → prop →
  view → fn → contract), and `mz outline` emits them there.
- **One field per line, `field <name>: <type>`.** Keyword-per-line, LL(1), and it mirrors
  `prop <name>: <type>` exactly, so a reader who has seen one has seen both. The obvious
  alternative, a bare `<name>: <type>` line, is what TypeScript and Rust priors produce; it
  is `MZ0308` with an `exact` fix inserting the word `field`, so the prior costs no round trip.
- Fields take no default (`MZ0310`): a record value comes from the caller whole.
- Field names are unique within a record; record and enum names are unique in the file and
  may not shadow a built-in (`MZ0704`).

Records participate in the IR (RFC-0003) as a `record` node whose children are `field`
nodes **in declaration order** — unlike a contract's clauses, field order is meaning,
because it is the layout a serialiser and an FFI binding emit.

## 3. `for each` — _G0.5, TY-3_

```mz
for each entry in entries
  key = entry.version
  article
    text = entry.title
    for each c in entry.components_added
      key = c
      chip
        text = "+{c}"
      end
    end
  end
end
```

- `for each <name> in <path>`, where `<path>` resolves to a `list(T)`. `<name>` has type `T`
  inside the block and nowhere else. Iterating anything but a list is `MZ0711`.
- A binding may not reuse a prop's, fn's or enclosing binding's name (`MZ0713`): shadowing
  would make `entry` mean two things in one file, and RB-1-style patching by name would
  become ambiguous.
- **The key is written exactly once, one way:** `key = <path>` as an attribute of the
  `for each` block itself, where the path starts at the binding and resolves to a scalar.
  Not on the body's element (a body may render several), and not as a string: the
  reference's `key: "{entry.version}"` is a Dioxus artefact of keys being strings, and
  `key = "{entry.version}"` is `MZ0713` with an `exact` fix to `key = entry.version`. A
  missing key, a second key, or a key that does not mention the binding (and is therefore
  the same for every item) are `MZ0713` too. Lowering moves the key onto the body's root
  element; that is Phase 1's business.
- A `for each` block holds its key and elements; any other attribute is `MZ0713`.
- Nested iteration over a field of the outer binding needs nothing special: `entry` is in
  scope, so `entry.components_added` resolves to `list(text)`.

## 4. Option narrowing: `when … is none … else … end` — _G0.4, TY-4, TY-6_

The constraint was one form, locally checkable, no new symbols, reusing `when` / `is` /
`none` / `else`. The form is the one RFC-0001 §1.2 already names:

```mz
when image is none
  row
    text = initials
  end
else
  picture
    source = image
  end
end
```

- `when <path> is none` is legal on an `option(T)` or a `list(T)`. On an option it tests
  absence; on a list it tests emptiness — the same question, because the empty list _is_ a
  list's absence (§1.1).
- **Inside the `else` branch of `when p is none`, an `option(T)` at `p` has type `T`.**
  Narrowing is keyed on the path as written, so `when entry.added is none … else …`
  narrows `entry.added` and nothing else. It is a property of one `when` line and the
  lines inside its `else` — locally checkable by construction.
- **Using an option anywhere it has not been narrowed is `MZ0710`:** in an attribute, in
  `{...}`, as a `for each` source, through a dotted access, as an `emit` payload. That is
  the TY-6 fix: absence can no longer render as a blank.
- `else` belongs to `when` only, once per `when` (`MZ0404`), and serves every `when` — it is
  not option-specific. `confirm_bar`'s `when destructive` / `when not destructive` pair
  remains legal.

Rejected alternatives, and why:

- **`when image is some`** — what `primitives/avatar.mz` actually wrote (§9.2). `some` would
  be a new keyword, and it pulls toward Rust's `Some(x)` pattern-binding prior, which Mzizi
  has no grammar for. It is not accepted; on an option it is `MZ0712` with an `exact` fix
  rewriting the line to `when image is none` / `nothing` / `else`, so the prior still costs
  no round trip.
- **`when image`** as a presence test — overloads bool truthiness. `when flag` on an
  `option(bool)` would silently mean "present", not "true". `when p` and `when not p` are
  for `bool` only; on an option or list they are `MZ0712`, naming the form to use.
- **A positive-first form** (`when image is not none`, `when present image`) — `is not` is
  a second negation beside `when not x`, and `present` is a new word. The cost of the
  chosen form is real and is recorded, not hidden: rendering only when present takes two
  extra lines (`nothing`, `else`). The changelog port pays it four times (§9.3).

The other `when` shapes the checker now enforces:

| Condition              | Legal when `p` is | Otherwise                                                                     |
| ---------------------- | ----------------- | ----------------------------------------------------------------------------- |
| `when p`, `when not p` | `bool`            | `MZ0712`                                                                      |
| `when p is <variant>`  | an enum           | unknown variant is `MZ0708` with a nearest-name fix                           |
| `when p is none`       | option, list      | on an enum, text, … it is `MZ0712`: those are never none                      |
| `when p is true/false` | `bool`            | `MZ0712`, `exact` fix to `when p` / `when not p` — one form per intent (FM-1) |

A `when` block holds elements only; an attribute inside one is `MZ0712`.

## 5. What the checker proves — _G0.1, TY-1, TY-2_

`mz check` now runs a resolution pass (`compiler/src/resolve.rs`) after parsing. Every
name below either resolves or is a diagnostic:

- **Every type name**, against the built-ins and the file's enums and records. Common
  priors carry an `exact` fix: `string` / `str` → `text`, `boolean` → `bool`, `number` /
  `integer` / `i32` / `i64` / `u16` / `usize` … → `int`. Otherwise the nearest declared
  type within two edits, `exact` when it is the unique nearest.
- **Every prop default**, against the prop's type (§1.2).
- **Every enum column value.** A column's type is that of its first variant's value (`"…"`
  text, an integer int, `true` / `false` bool, a bare word a variant of the one local enum
  that has it); a variant whose value disagrees is `MZ0716`. An enum-typed column is what
  lets `{n.accent.class}` resolve without enum payloads — columns stay static data.
- **Every reference in the view:** bare attribute values (`text = label`), every `{...}`
  inside a string attribute, every dotted path (an enum's column, a record's field, a
  loop binding's field, `<enum>.<variant>` and its columns), every `when` condition and
  every `for each` source and key.
- **Every `emit`.** `fn` bodies are still unmodelled statement lists, except `emit`, which
  the parser now reads: `emit <event>` for `event(none)`, `emit <event>(<value>)` otherwise.
  The target must be an event prop and the payload must fit its type — `emit
on_state_change(sync)` is RFC-0001 §4.2's example diagnostic, now real: `MZ0708`, `sync`
  is not a variant of `connection_state`, `exact` fix `syncing`.
- **Event wiring:** `tap = …` and `change = …` must name an event prop or a `fn`.

Type rules at value positions: an attribute value is a scalar (`bool`, `int`, `text`, an
enum), an event prop or a `fn`. A list, a record or an un-narrowed option is not a value
(`MZ0711` / `MZ0710`) — they are consumed by `for each`, by field access, and by
`when … is none`. `{...}` accepts scalars only. A `key` accepts scalars only.

Resolution order for a bare word is fixed, so adding a declaration can only ever create a
diagnostic, never silently change what an existing line means (the same argument as
RFC-0006 §3): the innermost loop binding, then a prop, then a `fn`, then a variant of exactly
one local enum. A dotted path whose head is a local enum name is `<enum>.<variant>`.

### 5.1 What it does not prove

- **Anything across a component boundary.** `button` in `confirm_bar`'s view is a bare
  element word, and the checker cannot see `button.mz`. So `variant = button_variant.ghost`
  — a dotted path whose head names nothing in this file — is accepted as an **external
  reference** with warning `MZ0502`, which says plainly that it cannot be checked until
  modules land (RFC-0007 G2.3) and that there is nothing to fix. A head within two edits of
  a local name is treated as a typo instead (`MZ0707`, `guess`). A _bare_ word must always
  resolve locally; that is the rule that catches `text = lable`.
- **Attribute names.** Element words and attribute names are open (RFC-0001 §1.5); `on_click
= on_tap` still compiles. `tap` and `change` are checked because their _values_ are.
- **Statements.** `fn` bodies beyond `emit` are still skipped (RFC-0001 §7.1).
- **Passing collections into composed components.** Rejected for now by the value rule
  above, since nothing could type-check the receiving prop. Open (§10).

### 5.2 Nearest-name fixes

A misspelling is matched against the candidates of the right kind only — variants of the
enum in question, fields of the record in question, names in scope — by edit distance. A
fix is `exact` when exactly one candidate is nearest and it is within two edits and half the
word's length; `guess` when several tie; absent when nothing is close. `exact` keeps its
RFC-0001 §4.3 meaning: applying it blind produces the only name that could have compiled.

## 6. Diagnostic codes

The hundreds digit keeps its existing meaning (01 lexical, 02 structure, 03 declarations,
04 view grammar, 05 warnings, 06 contracts). **`MZ07xx` is new: name and type resolution**,
the pass this RFC adds.

| Code     | Sev.    | Means                                                                                                                  |
| -------- | ------- | ---------------------------------------------------------------------------------------------------------------------- |
| `MZ0105` | error   | a type constructor written with symbols (`list<T>`, `T[]`, `[T]`) — `exact` fix                                        |
| `MZ0307` | error   | `record` without a name                                                                                                |
| `MZ0308` | error   | a record body line that is not `field <name>: <type>` — `exact` fix when it is `<name>: <type>` (inserts `field`)      |
| `MZ0309` | error   | a malformed type: `list(` with no `)`, `list()` with nothing inside                                                    |
| `MZ0310` | error   | tokens left over after a `prop` or `field` declaration (was silently ignored), incl. a field default                   |
| `MZ0404` | error   | `else` outside a `when`, or a second `else`                                                                            |
| `MZ0405` | error   | an `emit` with no event name                                                                                           |
| `MZ0502` | warning | an external reference (`<other>_enum.variant`) that cannot be checked until modules — nothing to fix                   |
| `MZ0701` | error   | an unknown type name — alias or nearest-name fix                                                                       |
| `MZ0702` | error   | a type built wrongly: bare `list` / `option` / `event`, `text(…)`, `none` outside `event(none)`, an event that is data |
| `MZ0703` | error   | a second way to say nothing: `option(list(T))`, `option(option(T))`, `option(event(T))` — `exact` fix                  |
| `MZ0704` | error   | a duplicate prop, field, enum or record, or one named like a built-in type                                             |
| `MZ0705` | error   | a record that contains itself                                                                                          |
| `MZ0706` | error   | a default that does not fit its type, or a default where the type has an implicit one (§1.2)                           |
| `MZ0707` | error   | a bare name that resolves to nothing in scope — nearest-name fix                                                       |
| `MZ0708` | error   | no such variant, column or field — nearest-name fix among that enum's or record's members                              |
| `MZ0709` | error   | a dotted access on something without members (`text`, `list`, `bool`, an event, a fn)                                  |
| `MZ0710` | error   | an option used without being narrowed by `when … is none … else`                                                       |
| `MZ0711` | error   | a value of the wrong kind for its position: a list as text, `for each` over a non-list, `tap` on a non-event           |
| `MZ0712` | error   | a `when` condition that does not fit its operand's type, incl. `is some` / `is true` (both `exact`-fixed)              |
| `MZ0713` | error   | a malformed `for each`: its shape, a shadowing binding, a missing / second / constant / string key                     |
| `MZ0714` | error   | a malformed `{...}`: empty, not a name or dotted path, or unclosed                                                     |
| `MZ0715` | error   | an `emit` whose target is not an event prop, or whose payload does not fit the event                                   |
| `MZ0716` | error   | an enum column whose values disagree in type, or a bare-word column value naming no variant                            |

All follow RFC-0001 §4: `say` at most 200 characters and quoting the source, deterministic
order, and one diagnostic per real error — an unknown type is reported once, at its
declaration, and every later use of that prop is silently the error type rather than a
cascade of `MZ0709`s.

## 7. The IR and the outline

- `record` → a `record` node, `field` children in declaration order, each with its type.
- A prop's `type` field is the canonical spelling of its type (`list(changelog_entry)`);
  existing props hash exactly as before, because their canonical spelling is unchanged.
- An `else` branch → a trailing `else` child of its `when` node. A `when` without one
  hashes exactly as before.
- `emit` statements → `emit` children of their `fn` node. `fn` bodies were not hashed at
  all until now, so a component whose `retry` emitted a different variant had the same
  hash — RB-6's re-verification story was false for every component with a `fn`.
- `mz outline` emits records with their fields, and emits enums and records before props,
  in RFC-0001 §3's canonical order. It stays valid `.mz` and is reparsed in the tests.

## 8. Contracts

Unchanged: every existing clause evaluates as before. A contract cannot yet say anything
about a list, a record or an option (G0.9) — `every entries …` or `entry.version is …` are
unevaluable. A file with resolution errors does not compile, so, per RFC-0006 §7, its
contract is skipped rather than failed.

## 9. Measured

_Filled in by the implementing commits on this branch; nothing below is claimed until then._

## 10. Open questions

1. **Collection and record contract predicates (G0.9).** `every entry …`, `entries
at_least 1`, "the `for each` body renders a `chip`". The subject language of RFC-0006
   has no quantifier over a runtime list, and a static check cannot know a list's contents.
2. **Passing lists, records and options into composed components.** Needs modules to type
   the receiving prop.
3. **Recursive records** through `list(T)`, for trees. Needs a decision on the FFI shape.
4. **Maps.** UniFFI has `record<K, V>`; Mzizi has none. The changelog's `NODE_LABELS` map
   became an enum table instead (§9.3), which is the RFC-0001 §1.3 answer, but a runtime
   lookup table is a real need the reference argues for.
5. **Attribute-level choice.** Narrowing is view-level: choosing between two attribute
   values on an option means duplicating the element in both branches.
6. **The loop index and list length** (`aria-posinset`, `aria-setsize` in the changelog
   reference) have no expression; there is no arithmetic.
7. **Enum payloads** (RFC-0007 D1) — deliberately not added; columns remain static data.
8. **A tail on an ordinary element line** (`class "flex"`, the guide's missing-`=` trap) is
   still read as a child element and cascades. The checker could report it once, with an
   `exact` fix inserting `=`; not done here, to keep this change to G0.1–G0.5.
