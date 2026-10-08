# RFC-0013 — The core language: expressions, bindings, functions, control flow, errors and a program entry point

**Status:** draft for review. Wave 0, the foundation slice of §18.1, and C4 and C9 of Wave 1 (§18.2) are implemented, and §18.6 records what landed and what it changed here; the rest is design. It measures nothing. Later pull requests implement the other waves of §18, and each one updates §18.6 with what landed.
**Amended** on 2026-10-07 from `design/LANGUAGE-SURVEY.md` (PR #75, not merged yet), a survey of
ten widely used languages: named arguments (§6.5), records built and copied by field name with
invariants (§11), enum payloads (§11.5), `otherwise` (§8), indexing that returns an option and a
closed set of named folds (§9), error mapping with `via` (§12.2) and unused bindings (§5.1). §21
lists every survey row against this text. The survey measured nothing in Mzizi, and neither does
this amendment.
**Author:** the machine author (Claude)
**Scope:** Tier 1 of [`LANGUAGE-TRACKER.md`](../LANGUAGE-TRACKER.md), rows C1–C10, as one design
(owner decision, 2026-10-07, issue #69: "Tiers by rfc, compile to rust, methods for now, go
ahead"). It covers a `program` file kind with an entry point and `print`; expressions and
operators; bindings; functions; control flow in function bodies; numbers; the text and
collection operations the language itself needs; methods on records; a result type and its
propagation; `mz run`; the lowering of each construct to Rust; the diagnostics; and the
canonical form. **Generics, interfaces and traits are deferred past M1** (owner decision), and
so is library breadth, which belongs to P2. Modules (P1), state beyond a function's own
bindings (P8), concurrency (P9) and Rust interop (P6) are not designed here.

> **Proposes to amend**, in full:
>
> - **RFC-0001 §1.2**, the one-form table, in function bodies: it gains `while` (listed there as
>   deliberately absent), `else when`, `match`, and `when` and `match` used as values (§7). It also
>   admits two forms the table names as deliberately absent. **The iterator-chain vs loop
>   duality**: `map`, `filter` and `fold` (§9.3) sit beside `for each` and `push`, a duality kept
>   on purpose and put to the owner (§20, Q19). **`if`-chains over variants**: an `else when` chain
>   that tests one enum value against its variants is a second spelling of `match`, so it is
>   `MZ0936`, with a `guess` fix to `match` (§7.1, CL-8).
> - **RFC-0001 §1.5**, which made `{expr}` interpolation the only string-building mechanism, and
>   **RFC-0008 §5**, which limits `{…}` to scalars named by a name or a path. In a program, `{…}`
>   holds any expression with a text form that contains no string literal and no braces (§3.6).
>   Views keep RFC-0008's rule.
> - **RFC-0008 §1**, the closed type set, which gains `float`, `map(K, V)`, `set(K)` and
>   `result(T, E)` (§2).
> - **RFC-0008 §2**, whose record body holds only `field` lines, anything else being `MZ0308`. A
>   record in a program may hold `fn` methods after its fields (§11.2), and such a `fn` block is not
>   `MZ0308`.
> - **RFC-0010 §3.1**, whose `take` / `give` lines were a placeholder for a function signature,
>   which §6 now designs.
> - **RFC-0011 §5**, which left `result(T, E)` and its propagation form to "the first `fn` that
>   can fail" (§12).
> - **RFC-0010 §4.3**, which lowers `always` to `debug_assert!`, checked in debug builds only. In a
>   program a record's `always` clauses are checked in every build, and a broken one traps (§11.4).
> - **RFC-0007 D1's open question**, static columns or payloads: one `enum` construct holds both
>   (§11.5).
> - **RFC-0001 §4.7's `MZ0106`** (a spread), which gains an `exact` fix to `with` in a program
>   (§11.1).
>
> Each of those RFCs carries a note pointing here and naming these sections. None of the
> amendments takes effect until the owner accepts this RFC. RFC-0010 §11's open question 3
> (relational properties) is not resolved here: §15.1's example avoids one.

---

## 0. Method: the failure modes a core language invites

RFC-0001 §0's rule applies: a decision must trace to a named failure mode. These use a `CL-`
prefix (core language), because `FM-`, `TY-`, `HD-`, `RB-` and `BM-` belong to other RFCs.

Only CL-1 has been observed in a Mzizi program: pilot 2 found the ~7B model writing React
idioms that have no Mzizi form (RFC-0001 §4.7). The others are predicted from the incumbents,
because no Mzizi program has computed anything yet.

| ID       | Failure mode                                                                                                                                                                                                                                                                               |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **CL-1** | **The prior's idiom.** A small model writes the form it saw most in training: `if`, `==`, `elif`, `x += 1`, `len(xs)`, `return Ok(x)`, `x?`, `console.log`, a `{ … }` block. Pilot 2 found the React version of this. A diagnostic that does not name the idiom costs a round trip.        |
| **CL-2** | **Arithmetic that is silently wrong.** Wrapping overflow (Rust release builds, C, Go), JavaScript's floats posing as integers, Python's floor division beside Rust's truncation. Each compiles clean and returns a wrong number, which is a charter defect: compiles, behaviourally wrong. |
| **CL-3** | **The ignored error.** Go's `_ = err`, an exception nothing catches, Rust's `.unwrap()`. The failure path exists, and nothing makes the author decide what it does.                                                                                                                        |
| **CL-4** | **The aliasing surprise.** In Python and TypeScript, `b = a; b.append(x)` changes `a`. A model that reasons about values gets a program that reasons about references.                                                                                                                     |
| **CL-5** | **Ownership at the surface.** Rust's borrow and lifetime errors are FM-3's extreme case: the fix is far from the report. RFC-0001 §1.8 already rules them out of the surface.                                                                                                              |
| **CL-6** | **Two equalities and truthiness.** `=` beside `==` (`if (x = 1)`), `==` beside `===`, and `if xs:` meaning "not empty" while `if n:` means "not zero". One question, several spellings, each with its own edge cases.                                                                      |
| **CL-7** | **The fallen-off end.** A function path with no `return` yields `None`, `undefined` or `()` in the incumbents. That is RFC-0011's HD-2 for functions: the author did not choose the result.                                                                                                |
| **CL-8** | **The missing case.** A `switch` with no `default`, a Python `match` with no `case _`, an `if` chain that forgets a variant. The program runs, and the forgotten value does nothing, or the wrong thing.                                                                                   |

## 1. A program — _C10_

A program is one file holding one `program`. It closes with `end program <name>`, as a
component and a service do (RFC-0001 §1.1, RFC-0011 §1).

```mz
## Prints the first ten Fibonacci numbers and the sum of the even ones.
program fibonacci

  fn main
    for each i in range(0, to = 10)
      print("fib({i}) is {fib(i)}")
    end
    print("even sum below 100 is {even_sum(100)}")
  end fn main

  fn fib(n: int): int
    when n < 2
      return n
    end
    return fib(n - 1) + fib(n - 2)
  end fn fib

  fn even_sum(limit: int): int
    var total = 0
    var i = 0
    while fib(i) < limit
      let f = fib(i)
      when f % 2 is 0
        total = total + f
      end
      i = i + 1
    end
    return total
  end fn even_sum

  contract
    example output contains "fib(9) is 34"
  end

end program fibonacci
```

Decisions, and why:

- **`program` is a file kind, like `component` and `service`.** It holds `use` lines, `enum` and
  `record` declarations, `fn`s and a `contract`. It holds no `view`, `prop`, `route` or `emit`;
  each of those is `MZ0901`, as a `view` in a service is `MZ0811`. One top-level declaration per
  file (RFC-0007 D3) holds.
- **The entry point is `fn main`**, exactly one, with no parameters. It returns nothing, or
  `result(none, E)` (§12.5). A program without one is `MZ0902`. Command-line arguments and the
  environment are P2's (`env` module), not this RFC's.
- **`print(v)` writes `v`'s text form (§3.8) and a newline to standard output.** It takes exactly
  one argument, of any type that has a text form. `print("{a} {b}")` is how two values are
  printed, because interpolation is the one way to build text (§3.6). `print` needs no `use`
  line: standard output is a program's interface, not a capability (RFC-0001 §1.7's capabilities
  stay `net`, `storage`, `ml`, `motion`). Writing to standard error, reading input and writing
  without a newline belong to P2.
- **No new keywords.** `program`, `let`, `var`, `return`, `while`, `break`, `continue`, `and`,
  `or`, `try`, `self`, `otherwise`, `with`, `via`, `changes`, `error`, `ok` and the type names `float`, `map`, `set` and `result` are
  contextual words, as `service`, `route` and `record` are (RFC-0011 §1). Each is special only
  in a position where nothing else can appear. They fall in three groups, by how far that
  position reaches:
  - **The type names `float`, `map`, `set` and `result` are special only in type position**,
    after a `:` or inside a type constructor, where a binding name cannot stand. They may name a
    binding, parameter, `fn`, record field or variant, but not a record or enum, whose name
    stands in type position itself (`MZ0921`): `let result = …` and `let map = …` are
    legal, because they are among the names a small model writes most, and banning them would be
    a CL-1 trap that buys no disambiguation. `xs.map(f)` is a method, read after a dot.
  - **`ok` and `error` may name an enum variant or a record field**, so `enum status` with
    variants `ok` and `error` is legal. A `match` on a result reads `case ok <name>`, and one on
    an enum reads `case ok`, and the scrutinee's type says which. `error(e)`, with parentheses,
    is always the failure value (§12.1), and `ok(v)` is always `MZ0952`, so a bare `ok` or
    `error` is the variant. In a function that returns a result, though, `return error` with
    `error` a variant would read as a failure and is a success, so there a bare `ok` or `error`
    variant is `MZ0953`, with the `exact` fix qualifying it (`return status.error`). They may not
    name a binding, parameter or `fn`, where the same misreading has no qualified form to fix it
    to (`MZ0921`).
  - **The rest** (`program`, `let`, `var`, `return`, `while`, `break`, `continue`, `and`, `or`,
    `try`, `self`, `otherwise`, `with`, `via`) name nothing in a program: no binding, parameter, `fn`, record, enum or variant
    (`MZ0921`), because each can begin a line or an expression. A record field may use one
    (RFC-0011's `problem` record has a field `error`), because a field is only ever read after a
    dot.
  - **`changes` is special only after a method's signature** (`fn bump changes self`, §11.2),
    where nothing else can stand, so it may name anything: `let changes = …` is legal, as
    `let result = …` is.

  `lex.rs`'s `KEYWORDS` list stays at 23. Whether some of them should become keywords, and what
  the remaining bans cost, is open (§20, Q12).

- **Canonical order inside a program:** doc lines, `use`, `enum` and `record`, `fn main`, the
  other `fn`s in source order, `test` blocks (§15.2), `contract`. RFC-0001 §3's order, extended
  as RFC-0011 extended it. `main` comes first so that a reader with a small context window sees
  the entry point before anything else.
- **A `fn` closes with `end fn <name>`.** RFC-0001 §1.1 spends the name echo where nesting is
  deep, and a function body is the deepest nesting in the language: loops inside branches inside
  loops. The echo is RFC-0002 §1's error-correcting code, used where it pays. A bare `end` that
  closes the `fn` itself, with every inner block already closed, is `MZ0208` (today's code for a
  top-level block closed without its name), with the `exact` fix `end fn <name>`; an `end fn`
  with the wrong name is `MZ0207`, as for any echo. Component `fn`s
  still close with a bare `end` today; whether they move to the echo is open (§20, Q4).

## 2. The types this RFC adds — _C5, C7, C9_

RFC-0008 §1's closed set, extended:

```text
type := bool | int | float | text
      | <enum name> | <record name>              declared in this file
      | list(<type>) | option(<type>)
      | map(<key>, <type>) | set(<key>)
      | result(<type>, <type>) | result(none, <type>)
      | event(<type>) | event(none)              props only, unchanged
key  := int | text | bool | <enum name>
```

Still closed: no user generics, no traits, no function types and no tuples. Every type stays
serialisable and describable across a UniFFI-shaped boundary, the property RFC-0008 §1 kept:

| Mzizi          | UniFFI shape      | JSON                                                         |
| -------------- | ----------------- | ------------------------------------------------------------ |
| `float`        | `f64`             | number; a non-finite value has no JSON form (§20, Q9)        |
| `map(K, V)`    | `record<K, V>`    | object when `K` is `text`; otherwise an array of pairs       |
| `set(K)`       | `sequence<K>`     | array, in key order                                          |
| `result(T, E)` | `[Throws=E]` on T | `{"ok": …}` or `{"error": …}`; never a response body as such |
| a record       | `record`          | an object, fields in declaration order (RFC-0011 §4.1)       |
| enum payload   | `enum` with data  | `{"circle": {"radius": 1.0}}`; a fieldless variant its name  |

- **No tuple.** A tuple is a record whose fields have no names, so its meaning is positional.
  RFC-0008 §2 made field order part of a record's meaning precisely so that the names carry it.
  Two values returned together are a record with two named fields. `(a, b)` is `MZ0963`, with
  no fix: only the author knows the field names.
- **A key is an ordered scalar.** Maps and sets iterate in key order (§9.2), so a key must have
  one: `int` and `text` in their natural order, `bool` with `false` first, an enum in
  declaration order. A `float` key has no total order without a rule about `nan`, so it is
  `MZ0964`, as is a list, record, option, map or set key.
- RFC-0008's rules on absence are unchanged and extended: a map or set is never optional, because
  the empty one is its absence (`MZ0703`, `exact` fix to `map(K, V)` or `set(K)`).
- A `result` is a function's return type, and a `let` may hold one until it is matched. It is
  never a parameter, field, element, key or option type (`MZ0950`, §12.3), and never the success or
  error type of another result: `result(result(int, e), e)` is `MZ0950`, with no fix. That keeps
  `return v` in a result function unambiguous (§12.1). The error type cannot be a result anyway,
  since a result has no text form (§3.8).

## 3. Expressions and operators — _C1_

### 3.1 Literals

| Kind      | Written                               | Notes                                                                      |
| --------- | ------------------------------------- | -------------------------------------------------------------------------- |
| `int`     | `0`, `42`                             | decimal digits only; `-` is the unary operator (§3.4)                      |
| `float`   | `1.5`, `0.25`, `3.0`                  | a digit on both sides of the point; `1.` and `.5` are `MZ0914` (`exact`)   |
| `bool`    | `true`, `false`                       | unchanged                                                                  |
| `text`    | `"…"`                                 | escapes `\n`, `\t`, `\"`, `\\`, `\{`, `\}`; `{expr}` interpolates (§3.6)   |
| absence   | `none`                                | unchanged (RFC-0008 §1.1)                                                  |
| list, set | `[1, 2, 3]`, `[]`                     | a set literal is a bracket literal where a `set` is expected (§9.1)        |
| map       | `["a": 1, "b": 2]`, `[]`              | Swift's spelling; `{"a": 1}` is not a form, because braces delimit nothing |
| record    | `point(x = 1.0, y = 2.0)`             | every field, by name (§11.1)                                               |
| variant   | `offline`, `connection_state.offline` | resolved as RFC-0008 §5 resolves a bare word                               |

Today's lexer has no escapes, and nothing in `primitives/` or `examples/` contains a backslash,
so adding them changes no existing file. Hexadecimal, octal, binary, exponent and `_`-grouped
literals are not forms in M1: `0x10` and `1e3` are `MZ0914` with no fix, and `1_000` is
`MZ0914` with the `exact` fix `1000`.

**How the lexer reads a point after digits.** A float literal is digits, `.`, and at least one
digit. Digits followed by `.` and a letter or `_` are an `int` literal and a method call:
`2.pow(10)` is `pow` called on the `int` `2`, and `1.0.to_int()` is `to_int` called on `1.0`.
Digits followed by `.` and anything else (a space, an operator, the end of the line) are
`MZ0914` with the `exact` fix appending `0`.

**`int`'s minimum has no literal.** `-` is always the unary operator (§3.4), so
`-9223372036854775808` is `-` applied to `9223372036854775808`, which does not fit and is
`MZ0103`. The minimum is written `-9223372036854775807 - 1`, as in C. Folding a `-` into the
literal was rejected, because it would make `-2.pow(2)` read `(-2).pow(2)`, which is `4`,
where every incumbent reads `-(2.pow(2))`, which is `-4`.

### 3.2 Arithmetic

`+`, `-`, `*`, `/` and `%` on two `int`s or two `float`s, with the same type on both sides.
There is no implicit conversion. `1 + 1.5` is `MZ0912`, whose fix on an `int` literal is the
`exact` `1.0`; on anything else it is the `guess` `x.to_float()`. Integer and float semantics
are §4's. There is no `**` operator; `x.pow(n)` is a method (§4.4).

Symbols are kept for arithmetic, against the word forms (`plus`, `times`) that G1.3 asked about.
Every language in every model's training writes `a + b`, the symbols are single tokens, and no
second spelling competes with them. RFC-0002 §1 rules out symbol-dense syntax, meaning sub-word
symbol soup; five arithmetic operators are not that.

### 3.3 Comparison and equality — _CL-6_

| Operator                 | Means                                                                    |
| ------------------------ | ------------------------------------------------------------------------ |
| `a is b`                 | equality, structural for every type with equality (lists, maps, records) |
| `a is not b`             | inequality                                                               |
| `a < b`, `<=`, `>`, `>=` | ordering, on `int`, `float`, `text` (by Unicode scalar value) and enums  |
| `a in xs`                | membership: an element of a list or set, or a key of a map               |

**Equality is `is`, not `==`.** `is` is already the equality word in views (`when state is
offline`), handler conditions (RFC-0011 §3) and contracts (`status is 200`). Adding `==` would be
a second spelling of one question, and `==` exists in other languages only because `=` was
taken; in Mzizi `=` never appears inside an expression, so the `if (x = 1)` defect class cannot
be written. `==` and `===` are `MZ0910` with the `exact` fix `is`; `!=` and `!==` get `is not`.

**A substring test is a method, `s.contains(t)`, not `in`** (§10). In an RFC-0011 handler
condition, `name in "badge"` is equality membership in a list of literals, and the same text in a
function body would have meant "is a substring of": the HD-4 hazard of one line with two meanings
in two sub-grammars. So `in` never takes `text` on its right: `t in s` with `s` a `text` is
`MZ0962`, with the `exact` fix `s.contains(t)`. The cost is that `x in xs` and `s.contains(t)`
are two spellings of "is inside", chosen by the type on the right (§20, Q14).

**One spelling each for inequality and emptiness.** `not a is b` is `MZ0910` with the `exact` fix
`a is not b`. A collection's emptiness is `c is none` (§3.4, RFC-0008 §4): on a list, map or set,
`c is []`, `c.length() is 0` and `c.is_empty()` are `MZ0962` with the `exact` fix `c is none`,
and their negations get `c is not none`. A `text` is not a collection and is never `none`, so its
emptiness is `s is ""`: on a `text`, `s.length() is 0` and `s.is_empty()` are `MZ0962` with the
`exact` fix `s is ""`, and their negations get `s is not ""`.

Both sides of a comparison have the same type (`MZ0912` otherwise). A function, an event and a
result have no equality. Comparisons do not chain: `a < b < c` is `MZ0913`, with the `guess` fix
`a < b and b < c` (a guess, because `b` is evaluated once in one and twice in the other).

`at_least` and `at_most` stay what they are: predicate words in contracts and handler conditions
(RFC-0010 §3, RFC-0011 §3), not operators. Inside a function body, `n at_least 3` is `MZ0910`
with the `exact` fix `n >= 3`. Handler conditions keep their own sub-grammar until handlers adopt
expressions (§20, Q14). Membership in a literal list is written `p in ["a", "b"]`; RFC-0011's
`p in "a" "b"` form is `MZ0910` in a function body, with the `exact` fix adding the brackets.

### 3.4 Boolean logic

`and`, `or` and `not`, on `bool` only. Python's spelling, and the words RFC-0001 already has
(`not`). `&&`, `||` and a prefix `!` are `MZ0910` with `exact` fixes. **There is no truthiness:**
`when n` on an `int`, `when name` on a `text` and `when xs` on a list are `MZ0712`, as RFC-0008
§4 already rules for views, and `MZ0712`'s fix names the question to ask (`n is not 0`, and
`xs is not none` for "has items", since `xs is none` means "is empty"). `and` and `or` short-circuit. A line that mixes `and` and `or` without parentheses is
`MZ0913`, whose `exact` fix adds the parentheses the precedence table implies, so the reading
never depends on remembering it.

### 3.5 Precedence

Highest first. Every binary level is left-associative except `otherwise`, which is
right-associative, and comparison, which does not associate at all.

| Level | Operators                                                                     |
| ----- | ----------------------------------------------------------------------------- |
| 1     | literals, names, `( … )`, `[ … ]`, calls `f(…)`, record construction          |
| 2     | postfix, left to right: `.field`, `.method(…)`, `[index]`, `with (…)` (§11.1) |
| 3     | prefix `-` (numbers), prefix `try` and its `via` (§12.2)                      |
| 4     | `*`, `/`, `%`                                                                 |
| 5     | `+`, `-`                                                                      |
| 6     | `otherwise` (§8.1), right-associative                                         |
| 7     | `is`, `is not`, `<`, `<=`, `>`, `>=`, `in`, which do not chain                |
| 8     | prefix `not`                                                                  |
| 9     | `and`                                                                         |
| 10    | `or`, never mixed with `and` without parentheses                              |

`otherwise` sits where Swift puts `??`: below arithmetic, so `xs[i] otherwise 0 + 1` reads
`xs[i] otherwise (0 + 1)`, and above comparison, so `n otherwise 0 < 5` reads
`(n otherwise 0) < 5`. It is the one right-associative level, so `a otherwise b otherwise 0`
reads `a otherwise (b otherwise 0)`, a chain of fallbacks.

`not x in xs` reads `not (x in xs)`, so Python's `x not in xs` is `MZ0910` with the `exact` fix
`not x in xs`. An expression fits on one line (RFC-0001 §2: no continuation); one that does not is
a sign it needs a `let` or a `fn`.

### 3.6 Text: interpolation is the one way to build text

`"{a} and {b}"` builds text. **There is no `+` on text.** RFC-0001 §1.5 made interpolation "the
only string-building mechanism", and concatenation is a case of it: `"{a}{b}"`. `a + b` on two
`text`s is `MZ0912`, with an `exact` fix to the interpolated form when both operands are names,
paths or literals (`"x" + name` becomes `"x{name}"`), and a `guess` otherwise. The cost is real
and recorded: the TypeScript and Python prior `+` costs a fix. It is the C1 row's "text
concatenation", met without a second form (§20, Q1).

In a program, `{…}` holds any expression with a text form (§3.8) **that contains no string
literal and no braces**: `"{fib(i)}"` and `"{p.x * 2.0}"` are legal; `"{f("x")}"` is `MZ0714`,
whose `say` suggests a `let`. The rule is a lexer's, not a type checker's: a string ends at the
next unescaped `"`, so a string inside an interpolation could not be read on one line by a small
model either. Inside a view, RFC-0008's narrower rule (a name or a dotted path, scalars only)
is unchanged. `str(x)`, `String(x)` and `x.to_string()` are `MZ0962` with the `exact` fix
`"{x}"`.

### 3.7 Calls, methods and indexing

- **A call** is `f(a, b = e)`: the first argument by position, **every later one labelled with
  its parameter's name**, in the order the signature declares, and a parameter with a literal
  default may be left out (§6.5). Variadic parameters are not a form (`MZ0903`). So a call site
  shows what each argument means, in one order.
- **An operation on a value is a method:** `xs.length()`, `s.split(",")`, `p.norm()`. The built-in
  operations (§4.4, §9, §10) and record methods (§11) share one call form, and Python's free
  functions `len(xs)` and `str(x)` are `MZ0962` with `exact` fixes. Parentheses are always
  written: `xs.length` is `MZ0962` with the `exact` fix `xs.length()`, because a bare `.name` is a
  field.
- **Indexing returns an option.** `xs[i]` on a list is `option(T)`, `s[i]` on text is
  `option(text)` (one character), and `m[k]` on a map is `option(V)`: `none` when the index is
  outside the value or the key is not in the map. The value is used as any option is, narrowed
  (§8) or given a default, `xs[i] otherwise 0`. There is no `.get`: `xs.get(i)` and `m.get(k)`
  are `MZ0962` with the `exact` fix `xs[i]`, so one intent has one form. A literal negative index
  (`xs[-1]`, Python's last element) is `MZ0915`, with the `guess` fix `xs[xs.length() - 1]`.
  _Amended from the survey (F2, §21):_ this draft first made `xs[i]` trap and `xs.get(i)`
  return an option. Out-of-bounds access is 59.9% of LLM-written C++'s runtime errors in the
  survey's one measured study (Coimbra 2026), and a reading that cannot fail silently is the
  survey's answer. The cost is an `otherwise` or a `when` where the author knows the index is in
  range (§20, Q22). Assigning through an index, `xs[i] = v`, still traps out of range (§4.3),
  because a statement has no value to make optional.

### 3.8 The text form of a value

`print` and interpolation use one text form per type. For every value except a `float` written
with an exponent, `inf` and `nan`, which have no literal (§3.1), the text form is how the value
would be written in Mzizi:

| Type           | Text form                                                                                                 |
| -------------- | --------------------------------------------------------------------------------------------------------- |
| `int`          | decimal, `-` when negative                                                                                |
| `float`        | the algorithm below: `1.0`, `0.1`, `1.0e21`, `1.5e-7`, `inf`, `-inf`, `nan`                               |
| `bool`         | `true`, `false`                                                                                           |
| `text`         | itself at the top level; quoted with escapes inside a collection or record                                |
| enum           | the variant's name; a payload variant as it is built, `circle(radius = 1.0)` (§11.5)                      |
| `option(T)`    | none at the top level: `print(x)` and `"{x}"` on an un-narrowed option are `MZ0710` (§8, RFC-0008's TY-6) |
| inside a value | an option element or field of a list, map or record prints as its value, or `none`                        |
| list, set      | `[1, 2, 3]`, a set in key order                                                                           |
| map            | `["a": 1, "b": 2]`, in key order                                                                          |
| record         | `point(x = 1.0, y = 2.0)`, fields in declaration order                                                    |
| `result(T, E)` | none: printing a result is `MZ0950` (§12.3), because it is an error nobody handled                        |

**The float algorithm.** The generated `MzText` implementation (§14.2) computes it; Rust's
`Display` (`1` for `1.0`) and `Debug` (`1e20`, `NaN`) are both wrong for it, and neither is used.

1. `nan` is `nan`; positive and negative infinity are `inf` and `-inf`. Zero is `0.0`, and
   negative zero is `-0.0`.
2. Otherwise take the shortest decimal digit string that reads back as the same `f64`, with its
   decimal exponent. Rust's `{:e}` formatting yields exactly that pair (`1.5e-7`, `1e21`), so
   `MzText` takes the digits and exponent from it and lays them out itself.
3. When `1e-6 <= |x| < 1e21`, write the digits in plain decimal, with at least one digit after
   the point: `1.0`, `0.1`, `123456.789`, `0.000001`. That is the range in which JavaScript's
   `Number.prototype.toString` writes plain notation.
4. Outside that range, write one digit, a point, the remaining digits or `0`, then `e` and the
   exponent, with `-` when it is negative and no `+`: `1.0e21`, `1.0e-7`, `1.5e-7`, `5.0e-324`.

Those exponent forms are text, not Mzizi: `1e3` is `MZ0914` as a literal (§3.1), and `parse_float`
rejects an exponent (§10). A value printed with an exponent therefore does not read back through
either. That is a known gap, and allowing exponent literals is P2's question.

## 4. Numbers — _C5, CL-2_

### 4.1 `int`

`int` is a signed 64-bit integer, as today (`Tok::Int(i64)`, RFC-0008 §1's `i64`).

- **Overflow traps.** `+`, `-`, `*`, unary `-`, `x.pow(n)` and `x.abs()` on `int` stop the
  program with a trap (§4.3) when the true result does not fit. They never wrap and never
  saturate. Wrapping is CL-2 exactly: a wrong number that compiles clean. A trap is a defect
  that announces itself.
- **Division truncates toward zero**, and `%` takes the sign of the dividend: `-7 / 2` is `-3`
  and `-7 % 2` is `-1`. That is Rust's, C's, Go's and Java's rule, and the lowered code's (§14). Python floors (`-7 // 2` is `-4`); a Python-trained model will
  be surprised, and the guide has to say so (§20, Q10).
- **Division or remainder by zero traps**, and so do `int` minimum `/ -1` and `int` minimum
  `% -1`. The first does not fit in `int`. The second is `0` in arithmetic, but Rust's
  `checked_rem` returns `None` for it (checked with `rustc`), and the lowering does not
  special-case it, so it traps as an integer overflow, one rule for both.
- **There is one division operator.** Python's floor division, `a // b` inside an expression,
  is `MZ0910` with the `guess` fix `a / b`: a guess, because `/` truncates where `//` floors,
  so the two differ on a negative operand. `//` after code is read as floor division only when
  the rest of the line reads as one expression of the left operand's type; otherwise it is a
  C or JavaScript comment after code (`let n = count // the total`), which is `MZ0911`'s.
- A constant fault the checker can see, such as `x / 0` with a literal `0` or a literal that
  overflows when folded, is `MZ0915` at check time, so it never reaches a run.

### 4.2 `float`

`float` is IEEE 754 binary64 (Rust's `f64`), and follows IEEE 754 exactly: **float division by
zero does not trap.** `1.0 / 0.0` is `inf`, `-1.0 / 0.0` is `-inf`, and `0.0 / 0.0` is `nan`.
Every incumbent does the same, and a float operation that traps would make `float` a different
type from the one every numeric library assumes. `nan is nan` is `false`, as IEEE says. Float
overflow gives `inf`. `%` on floats is the truncated remainder that Rust's `%` and C's `fmod` compute, with the sign
of the dividend (`5.0 % 3.0` is `2.0`), not IEEE 754's `remainder` operation. These are the
semantics C5's "Done when" asks to have specified; the tests that pin them are Wave 1's (§18.2).

### 4.3 Traps

A trap stops the program at once. It is not an exception: nothing can catch it, and it is not a
value. It is a defined halt with a fixed message and exit status:

```text
mz: trap MZ0991 at fibonacci.mz:22:12: integer overflow in `fib(n - 1) + fib(n - 2)`
```

The message names the `.mz` file, the line and column of the expression, the operation, and the
expression's canonical text, so it is P5's "errors mapped back to `.mz`" by construction for this
class. The exit status is **101**, and no Rust panic can share it, because the generated `main`
maps a panic to status 70 (§13.1). Under `mz run --agent` the trap is one NDJSON diagnostic with
code `MZ0991`, in the same protocol as `mz check` (RFC-0001 §4) but on standard error (§13.1).
The traps in M1:

| Trap                      | From                                                                       |
| ------------------------- | -------------------------------------------------------------------------- |
| integer overflow          | `+ - *`, unary `-`, `pow`, `abs` on `int`; `int` minimum `/ -1` and `% -1` |
| integer division by zero  | `/`, `%` on `int`                                                          |
| negative exponent         | `x.pow(n)` on `int` with `n` below zero                                    |
| index out of range        | `xs[i] = v` only; reading `xs[i]`, `s[i]` or a slice gives `none` (§3.7)   |
| float to int out of range | `f.to_int()` on `nan`, `inf`, or a value outside `int`                     |
| invariant broken          | a record's `always` clause false after it is built or changed (§11.4)      |
| no JSON form              | `v.to_json()` on a value holding `nan`, `inf` or `-inf` (§11.1; §20, Q9)   |

RFC-0007 G2.5 says no panic may be reachable from surface code. A trap is not a Rust panic: the
lowering never emits `panic!`, `unwrap` or `expect` (§14.3), and the trap is a function that
writes one line and exits. But it is a halt reachable from surface code, so this is a decision
the owner has to make, not one this RFC can make quietly. The alternative is that every `+`
returns a `result`, which no incumbent asks of its authors. What a trap does inside a service
handler (a `500`, presumably) is not decided here (§20, Q2).

**Recursion depth** is bounded by the stack. The lowered program runs `main` on a thread with a
64 MiB stack (§14.2). Deeper recursion than that aborts the process with Rust's stack-overflow
message and `SIGABRT`, not a trap, so `mz run` exits 134 (§13.1): catching it needs code the
lowering does not have. That is a limit, stated rather than hidden (§20, Q16).

### 4.4 Conversions and numeric methods

| Method                                              | On             | Returns       | Notes                                                         |
| --------------------------------------------------- | -------------- | ------------- | ------------------------------------------------------------- |
| `i.to_float()`                                      | `int`          | `float`       | nearest `float`; exact up to 2^53                             |
| `f.to_int()`                                        | `float`        | `int`         | truncates toward zero; traps out of range or `nan`            |
| `f.round()`, `f.floor()`, `f.ceil()`                | `float`        | `float`       | `round` takes half away from zero                             |
| `x.abs()`, `x.min(y)`, `x.max(y)`                   | `int`, `float` | the same type | `abs` traps on `int` minimum                                  |
| `x.pow(n)`                                          | `int`, `float` | the same type | `n` is an `int`, ≥ 0 on `int`; see below                      |
| `f.sqrt()`                                          | `float`        | `float`       | `nan` below zero, as IEEE                                     |
| `f.is_nan()`                                        | `float`        | `bool`        | the only way to ask, since `nan is nan` is `false`            |
| `a.wrapping_add(b)`, `wrapping_sub`, `wrapping_mul` | `int`          | `int`         | two's-complement wrap, never a trap; for hashes and checksums |

`pow` on `int` traps on overflow, and a negative `n` is a trap too (a literal one is `MZ0915`).
`pow` on `float` is `x.powf(n.to_float())`: the exponent converts as `to_float` does, exactly up
to 2^53 and to the nearest `float` beyond it, so every `int` exponent has a defined result,
including one outside Rust's `i32`. Rust's `powi` is not used: its precision is unspecified, and
it takes an `i32`.

Wrapping is asked for by name at every use, so `+` never wraps by accident (survey F17). That is
the whole numeric surface in M1. Trigonometry, logarithms, bit operations and a decimal type are
P2's `math` module.

## 5. Bindings — _C2_

### 5.1 `let` and `var`

```mz
let name = "Ada"
let limit: int = 100
var total = 0
total = total + 1
```

- **`let` binds a name that never changes; `var` binds one that may.** Assignment is
  `name = expr` on a `var`. Mutability is decided once, at the binding, and a reader can see from
  the binding line whether the name can change. Assigning to a `let`, a parameter or a loop
  binding is `MZ0922`, with the `exact` fix rewriting that `let` to `var` (no fix on a parameter
  or loop binding). A `var` that is never reassigned or mutated is the warning `MZ0924`, with the
  `exact` fix `let`, so one intent keeps one form.
- **The type annotation is optional**, and is needed only where the value alone does not fix the
  type: `[]`, an empty map, `none`. Signatures are always annotated (§6); local bindings are
  inferred from their value, forward only, one line at a time. `mz fmt` (T2) will drop an
  annotation the value already fixes; `mz check` does not flag one.
- **A binding always has a value.** `let x: int` with nothing after it is `MZ0926`. A `var` whose
  value is decided in branches is declared before them with an initial value, or bound with a
  `when` used as a value (§7.4).
- Python's first assignment, `x = 1` with no binding, is `MZ0923`. Its fix inserts the word
  `let` or, when the function assigns `x` again later, `var`, and it is `exact` only when two
  things hold: no name in scope is within RFC-0008 §5.2's nearest-name distance of `x`, and `x`
  is not read after the block holding the assignment ends. Otherwise one of two other repairs
  is the right one, and the line reports exactly one diagnostic:
  - **A near name is in scope** (`totl = totl + 1` with `total` bound): `MZ0923`, with
    RFC-0008 §5.2's nearest-name fix to `total`, `exact` or `guess` by that section's rule. The
    read of `totl` on the right is part of the same mistake and is not a second `MZ0707`.
    Inserting `let` would give `let totl = totl + 1`, which still fails.
  - **`x` is read after the block ends** (Python's assign-in-each-branch, then read): `MZ0920`
    at the first read after the block, with its `guess` fix declaring a `var` before the block
    (§5.2). The assignments inside the branches report nothing, since that `var` makes them
    legal. Inserting `let` in each branch would leave the read unbound, and `mz fix` would loop.
  - **Both hold**: the near-name case wins, since one misspelling explains every occurrence, and
    its fix is a `guess`. The later uses of `x`, inside the block or after it, are the same
    mistake and report nothing more (RFC-0001 §4's one diagnostic per true error); the next
    `mz check` reports whatever the fix left.

  `const`, `val` and `auto` are `MZ0925` with the fix `let`; Rust's `let mut` and
  `mut` get `var`; Go's `x := e` gets `let x = e`. `+=`, `-=`, `*=`, `/=`, `++` and `--` are
  `MZ0918`, with the `exact` fix `x = x + 1` and so on.

- **A binding nothing reads is an error**, `MZ0928`, as in Go (survey F21). A `let` or `var`
  whose name is never read after its binding line, and a program's `use` line that nothing
  needs, are each one `MZ0928`. The fix deletes the binding line and every later assignment to
  the name, so that no assignment is left to an unbound name. It is `exact` when every value it
  deletes is a literal, a name, a path or a bracket literal of those, since deleting them then
  changes nothing the program does; it is a `guess` when one of them calls a function or could
  trap, because deleting it removes the call or the trap. Parameters
  and loop bindings are exempt: a parameter's name is part of the signature and every caller's
  labels (§6.5), and a counted loop needs a name for its count, where Mzizi has no `_`. An error
  rather than a warning, because a binding nobody reads is most often a typo for one somebody
  does, and `mz fix` clears the `exact` cases without a model turn (§20, Q28).

A `var` is the only mutable state in M1, and it is local to one call of one function. There is
no global state, no static and no state shared between calls: G1.1 (component state) and P8
(state and I/O) are not decided here.

### 5.2 Scope

A binding is visible from the line after it to the `end` of the block that holds it. The blocks
are a `fn` body, each `when` / `else when` / `else` branch, each `case`, a `for each` body and a
`while` body. A name used before its binding, or after its block has ended, is `MZ0920`. It is a
different code from `MZ0707` (a name bound nowhere), because the repair is different: for the
Python pattern of binding in both branches and reading after them, `MZ0920`'s `guess` fix
declares a `var` before the `when`. This is C2's "Done when": the resolver reports use before
binding.

### 5.3 No shadowing

**A name is bound at most once in a function.** A `let`, `var`, parameter or loop binding that
reuses a name bound in an enclosing block, or the name of a `fn`, is `MZ0713`: RFC-0008 §3's
code, which already forbids a view's loop binding from reusing a prop's, `fn`'s or enclosing
binding's name. Shadowing keeps one code wherever it is written. A name a program reserves (a
record, enum or variant name, a built-in function's name (`print`, `range`), a contextual word as §1 limits it, or anything starting
with `mz_`) is a different mistake, a name that is not available at all, and is `MZ0921`.
Sibling blocks may bind the same name, because neither can see the other.

The reason is RFC-0008 §3's: with shadowing, `total` can mean two things in one function, and
patching a function by name (RFC-0003's RB-1) becomes ambiguous. Rust's idiomatic
`let x = x.trim()` is the cost. It gets `MZ0713` with a `guess` fix renaming the new binding
(`x_trimmed`), because the right name is the author's.

## 6. Functions — _C3, CL-7_

### 6.1 Declaration

```mz
fn area(width: float, height: float): float
  return width * height
end fn area

fn greet(name: text)
  print("hello, {name}")
end fn greet

fn answer: int
  return 42
end fn answer

fn main
  greet("Ada")
  print(answer())
end fn main
```

- **`fn <name>(<param>: <type>, …): <return type>`**, on one line. The colon before the return
  type reads as it does everywhere else in Mzizi, "has type": `prop x: int`, `field x: int`, and
  now "`area(…)` has type `float`". It is also TypeScript's spelling. Rust's and Python's `->` is
  `MZ0903` with the `exact` fix `:`; `def`, `function` and `func` get `fn`.
- **A function with no parameters has no parentheses** (`fn main`), as `fn retry` has none in
  every component today, and its return type follows the name directly: `fn answer: int`, which
  still reads "`answer` has type `int`". `fn main()` and `fn answer(): int` are `MZ0903` with the
  `exact` fix deleting `()`. A call always has parentheses (`answer()`), so a declaration and a
  call never look alike.
- **A function that returns nothing has no return type.** There is no `void`, `unit` or `none`
  return type to write: `: none` and `: void` are `MZ0903` with the `exact` fix deleting them.
- **Every parameter has a type.** `fn f(a, b)` is `MZ0903`, with no fix. Parameters are `let`
  bindings: they cannot be assigned (`MZ0922`), so a caller's value never changes under it.
- Names are snake_case (RFC-0001 §2), unique among a program's `fn`s, records and enums, and
  never a built-in function's (`print`, `range`) (`MZ0904`). There is no overloading.
- A `fn` may carry its own `contract` block, last in its body (RFC-0010 §2, §3.1; §15 here).

### 6.2 Return

- **`return <expr>` is the one way to produce a value**, and it may appear on any path, early or
  last. In a function that returns a result, §12.1 says when the value is wrapped as success
  and when it passes through. There is no implicit last-expression value (Rust's), because CL-7
  is the cost of making the end of a body mean something.
- **Every path of a function with a return type ends in `return`**, or in a `while true` with no
  `break`, which never exits. A path that reaches `end fn` is `MZ0906`, which
  names the line where the path ends. When the last line of the body is an expression of the
  return type (the Rust habit), the fix inserts the word `return` before it and is `exact`.
- A statement after `return`, `break` or `continue` on the same path can never run, and is
  `MZ0907` with the `exact` fix deleting it (RFC-0011's `MZ0805`, for functions).
- `return` with a value in a function with no return type, `return` without one in a function
  with a return type, and a value of the wrong type are `MZ0908`.
- **One exception: `result(none, E)`.** Its success carries no value, so a bare `return` and
  reaching `end fn` both return success, as in a function with no return type, and neither is
  `MZ0906` nor `MZ0908` (§12.1). Every other return type needs a value on every path.

### 6.3 Calls and recursion

A call evaluates its arguments left to right, then runs the body with each parameter bound to
its argument's value. **Arguments are values** (§14.1): the callee cannot change the caller's
bindings, whatever their type. That is CL-4's answer, and it is what lets the lowering keep
ownership out of the source (RFC-0001 §1.8). A call with the wrong number of arguments or an
argument of the wrong type is `MZ0905`, whose `say` quotes the signature.

Recursion, direct or mutual, needs nothing special: names resolve across the whole program, so a
`fn` may call one declared after it. Depth is bounded by the stack (§4.3).

### 6.4 A function as an argument, and no lambdas

The collection operations `map`, `filter` and `fold` (§9.3) take a function. **That function is
named**: `xs.map(double)`, where `double` is a `fn` of the program. There are no lambdas,
closures or function types in M1. `x => x * 2`, `|x| x * 2` and `lambda x: x * 2` are `MZ0909`,
with no fix, and the `say` names the form: declare `fn double(x: int): int`. A bare `fn` name
anywhere else (a `let`, a return, an argument to a user `fn`) is `MZ0909` too, except that a
bare zero-parameter `fn` name used as a value gets the `exact` fix appending `()`, because that
is the call the author meant.

This follows RFC-0001 §2: "an expression too long for a line is a sign it needs a named `fn`". A
named function is also a unit a contract can attach to (RFC-0010 §2) and an IR node an agent can
patch by name (RFC-0002 §2.1). The cost is real: a one-use helper takes three lines. Whether M2
adds a lambda form is open (§20, Q7).

### 6.5 Named arguments and defaults — _survey F3_

```mz
fn connect(host: text, port: int = 80, secure: bool = false): text
  return "{host}:{port} secure {secure}"
end fn connect

fn main
  print(connect("example.org"))
  print(connect("example.org", secure = true))
  print(connect("example.org", port = 8080, secure = true))
  print(area(3.0, height = 4.0))
end fn main
```

- **The first argument is given by position; every later argument is labelled** `name = value`,
  with its parameter's name. The function's name usually says what the first argument is
  (`greet(name)`, `parse_age(raw)`), and every later one is where positional calls go wrong:
  `area(3.0, 4.0)` cannot say which is the height. That is Swift's rule, and Python's and C#'s
  named arguments made the default. `=` is the label's spelling because it is the record
  literal's (§11.1), so a record value and a call read alike; `:` means "has type" everywhere in
  Mzizi, so Swift's and C#'s `height: 4.0` is not used.
- **One name per parameter.** There is no separate external label (Swift's
  `fn move(to target: point)`): the name a caller writes is the name the body reads. A second
  name is `MZ0903`, with no fix, because which name to keep is the author's.
- **Labelled arguments come in the order the signature declares**, skipping the defaulted ones
  the call leaves out. Arguments are evaluated left to right as written (§6.3), and canonical
  form writes them in declaration order (§17), so requiring that order is what keeps a reformat
  from changing the order in which calls run. The survey's lowering line ("reordered to
  declaration order") would have allowed any order; §21 records the change.
- **A default is a literal**: an `int`, `float`, `bool` or `text` literal with no `{…}`, `none`,
  `[]`, or a variant of an enum without payloads. Never an expression or a call, so there is
  nothing to evaluate when the call is made, and Python's mutable-default trap (`def f(xs=[])`)
  cannot be written; under value semantics a `[]` default is a fresh empty list at every call
  anyway (§14.1). The first parameter has no default, since it is never labelled. A non-literal
  default, or one on the first parameter, is `MZ0903`, with no fix.
- **It applies to every call**: a user `fn`, a method (whose receiver is not an argument, so
  `p.scaled(2.0)` and `p.moved(1.0, dy = 2.0)`), and the built-in functions and methods, whose
  parameter names this RFC fixes: `range(0, to = n)`, `xs.slice(a, to = b)`,
  `s.slice(a, to = b)`, `xs.fold(0, step = add)`, `s.replace(old, by = new)`. A one-argument call
  has no label. A record value labels every field, the first one too (§11.1), because a record
  has no name for its first field the way a function does.
- **Labels are checked with exact fixes where the program is determined** (`MZ0927`):
  - a positional argument after the first, `area(3.0, 4.0)`: `exact`, inserting the labels in
    declaration order, when the count is right, since position then has one reading;
  - Swift's and C#'s `height: 4.0`: `exact` `height = 4.0`;
  - a label on the first argument: `exact`, deleting it, when it names the first parameter;
  - an unknown label: RFC-0008 §5.2's nearest-name fix, `exact` or `guess` by that section's rule,
    and `exact` as well when exactly one parameter is left unfilled and the value has its type;
  - labelled arguments out of declaration order: `exact`, reordering them, when every argument is
    a literal, a name or a path, so the order cannot change what runs; a `guess` otherwise;
  - a label given twice: no fix, because which value is right is the author's.

  A missing argument without a default, and too many arguments, stay `MZ0905`, which quotes the
  signature.

- **Lowering erases the labels.** The arguments are already in declaration order, and each
  omitted default is written in at the call site as its literal (§14.2), so the Rust call is
  positional and `rustc` sees nothing new.

A contract `example` already names every parameter (`example a 2 b 3`, §15.1), the `name value`
style the survey suggested sharing with calls (its §4.1 item 8). Calls keep `=` inside
parentheses instead: a value that starts with a name (`k n + 1`) would make `name value` hard to
read on one line.

## 7. Control flow in function bodies — _C4, CL-8_

### 7.1 `when`, `else when`, `else`

```mz
when n < 0
  print("negative")
else when n is 0
  print("zero")
else
  print("positive")
end
```

RFC-0001 §1.2's one conditional, with one addition: **`else when <cond>` continues the same block**
and shares its one `end`. Without it, a three-way choice nests two `when`s and needs two `end`s,
and an `else if` chain is the most common conditional shape in every incumbent. Python's `elif`,
TypeScript's and Rust's `else if` are `MZ0933` with the `exact` fix `else when`. `if` is
`MZ0407`, as in a view, with the `exact` fix `when`. A trailing `:` (Python) or `{` (TypeScript,
Rust) on a block line, and a `}` closing one, are `MZ0937`: the `exact` fix deletes the opener,
and replaces a lone `}` with the closer that block needs. `} else {` becomes `else`.

The condition is a `bool` expression (§3.4). Narrowing an option works as in RFC-0008 §4, with
the positive form a function body needs (§8).

**A chain over one enum's variants is a `match`.** A `when` with at least one `else when`, whose
every condition is `<e> is <variant>` (or `<e> in [<variants>]`) on the same enum-typed path
`<e>`, is `MZ0936`: it is the `if`-chain over variants that RFC-0001 §1.2 lists as deliberately
absent, and it loses the exhaustiveness check, so a variant added later is silently skipped
(CL-8). The fix is a `guess` rewriting the chain to `match <e>`, one `case` per condition and the
chain's `else`, if it has one, as the `match`'s `else`. It is a guess because a chain without
`else` that misses variants becomes a `match` that is `MZ0930`, which the author has to finish.
A single `when e is v`, with or without a plain `else`, is a test of one variant, not a branch over
variants, and stays legal.

### 7.2 `match`

```mz
match shape.kind
  case circle
    return 3.14159 * shape.size * shape.size
  case square
    return shape.size * shape.size
  case triangle line
    return shape.size * shape.size / 2.0
end
```

- `match <expr>`, then `case <value> …` lines, each followed by its statements, then one `end`.
  A case may list several values separated by spaces (`case triangle line`), as RFC-0011's `in`
  lists its literals. `else` is the last case and takes everything left.
- **On an enum, `match` must cover every variant**, or end with `else`. A `match` that does not is
  `MZ0930`, whose `say` names the missing variants (`match shape.kind` misses `square`,
  `triangle`). It carries no fix: the bodies are the author's. This is C4's "non-exhaustive
  `match` is a diagnostic", and RFC-0007 G1.4.
- **On an `int` or a `text`, `match` must end with `else`** (`MZ0930` otherwise), since no list of
  literals covers every value. On a `bool` it covers `true` and `false`, or ends with `else`.
- **On a `result`, the cases are `case ok <name>` and `case error <name>`**, which bind the value
  and the error (§12.2). Both are required.
- A case that can never be reached is `MZ0931`: a value listed twice, a case after `else`, or an
  `else` on a `match` whose cases already cover every variant (the `exact` fix deletes the
  `else`).
- There are no guards and no destructuring of records in M1, as RFC-0001 §1.2 says ("no guards in
  v0"). A `match` on an option is `MZ0711`, naming `when x is none` (§8). `switch` is `MZ0933`
  with the `exact` fix `match`, `default:` gets `else`, and a trailing `:` on a `case` line is
  deleted. Python's `case _` and Rust's `_ =>` wildcard are `MZ0933` with the `exact` fix `else`,
  not a name `MZ0708` cannot find. RFC-0001 §4.7's `MZ0410` (`match` in a view) is unchanged:
  views still have no `match`.

### 7.3 Loops

```mz
for each item in items
  print(item)
end

while n > 1
  n = n / 2
end
```

- **`for each <name> in <expr>`** iterates a list, as in a view (RFC-0008 §3), and the key line
  is a view's business only: a function's `for each` has none. `range(a, to = b)` is the list of
  `int`s from `a` up to but not including `b`, so a counted loop is
  `for each i in range(0, to = n)` (§6.5's labels). A map
  or a set is iterated through `m.keys()`, `m.values()` or `s.to_list()`, all in key order;
  `for each k in m` is `MZ0711` with the `exact` fix `m.keys()`. **The loop iterates the value
  `<expr>` had when the loop began**, so assigning to the list inside the body changes the next
  use of the name, not this loop. That is value semantics (§14.1), and it removes the
  mutation-during-iteration defect class from the language.
- **`while <cond>`** is the one conditional loop. RFC-0001 §1.2 listed `while` as deliberately
  absent, to avoid a loop-versus-iterator duality; a program cannot compute a GCD, a Collatz
  sequence or a convergence without one, and recursion is not a substitute a small model handles
  well. There is no `loop`, no `do … while` and no C-style `for (i = 0; …)`. `loop` is `MZ0934`
  with the `exact` fix `while true`; Python's `for x in xs` gets the `exact` fix inserting
  `each`, and TypeScript's `for (const x of xs)` gets `for each x in xs`; the C-style loop has
  no fix, and the `say` names `range`. `range(n)` is `MZ0934` with the `exact` fix
  `range(0, to = n)`.
- **`break` leaves the innermost loop and `continue` starts its next pass.** Outside a loop each
  is `MZ0935`. Whether M1 keeps them is open (§20, Q3); they are in this design because the
  public suites' programs use them and a function written without them is longer.

### 7.4 `when` and `match` as values

```mz
let sign = when n < 0
  "negative"
else when n is 0
  "zero"
else
  "positive"
end

return match kind
  case circle
    "round"
  else
    "angular"
end
```

C4 asks for `when` and `match` "as expressions". This design gives them that, inside the one
construct, with three rules that keep every line locally predictable:

1. **Only in four places:** the value of a `let`, a `var`, an assignment, or a `return`. Never
   inside a larger expression (`1 + when …`), and never as a call argument, because a multi-line
   value inside a one-line expression is unreadable from one line of context. Elsewhere it is
   `MZ0932`.
2. **Each branch is exactly one line, an expression**, and that line is the branch's value. A
   branch that needs statements needs a `fn`.
3. **It must be total**: a `when` used as a value has an `else`, and a `match` used as a value is
   exhaustive (§7.2). Every branch has the same type. Each failure is `MZ0932`, with no fix.

RFC-0001 §1.2's "no statement/expression duality" is kept in the sense that matters: a line is
still classified by its first word, and a `when` line is a `when` line wherever it stands. What
changes is that its value may be bound. There is still no ternary, no expression-`if` on one line
and no `unless`.

## 8. Options in a function body

RFC-0008 §4's narrowing applies: inside the `else` of `when x is none`, `x` has type `T`. A
function body adds the positive form, **`when x is not none`**, which narrows `x` inside its own
branch. A view could write `nothing` in an empty branch; a function body has no such statement,
so the absence-first form alone would force an empty branch for the commonest case, "do this if
it is there". `is not` is no longer a second negation (RFC-0008 §4's objection), because §3.3
makes it the inequality operator everywhere. Whether views adopt the positive form is open
(§20, Q13).

**A guard narrows the rest of its block.** When the branch of `when x is none` always ends in
`return`, `break` or `continue`, `x` has type `T` from the line after its `end` to the end of the
enclosing block. That is the guard-clause shape every incumbent writes (`if x is None: return`),
and it is still a property of lines the reader can see: the `when`, its one branch, and its last
line.

**Assigning to a narrowed `var`.** Inside the region where a `var` `x` (or a path through one)
is narrowed, `x = v` with `v` of type `T` keeps it narrowed: the lowering writes both the
variable and the narrowed copy (`x = Some(v.clone()); mz_n1 = v;`, §14.2), so a later read in the
region sees `v`. Assigning `none`, or an `option(T)`, inside the region would end the narrowing
on a line the reader cannot see from the `when`, so it is `MZ0710`, with no fix; the author ends
the region first.

An option is never used un-narrowed (`MZ0710`). A value of type `T` is wrapped implicitly where
an `option(T)` is expected, so there is still no `some(…)` to write (RFC-0008 §5). `MZ0710`'s
`say` names the two repairs, a guard and `otherwise`, and it carries no fix: the survey asked
for an `exact` fix inserting the guard (F13), but what the guard's branch returns is the
author's, so no fix can be exact (§21).

### 8.1 `otherwise`: one word for a default — _survey F14_

```mz
let port = parse_port(raw) otherwise 80
let first = names[0] otherwise "nobody"
let label = nickname otherwise full_name otherwise "anonymous"
```

**`x otherwise d` is `x`'s value when it is present, and `d` when it is `none`.** `x` is an
`option(T)`. `d` is a `T`, and the whole is a `T`; or `d` is itself an `option(T)`, and the whole
is an `option(T)`, which is what lets a chain end in a value. **`d` is evaluated only when `x` is
`none`**, as `??` is in Swift, C# and JavaScript: a default that calls a function or could trap
runs only when it is used. Precedence is §3.5's level 6.

This replaces the draft's `x.or(d)` method. `or` is already the boolean operator (§3.4), so
`x.or(d)` beside `a or b` was one word with two meanings, HD-4's hazard; the survey's passes
proposed `or` four times and `otherwise` once, and its own analysis (§4.1 item 3) picked
`otherwise` for this reason. The idioms a model brings for it are `MZ0938`, each with the
`exact` fix `x otherwise d`: `x ?? d`, `x.or(d)`, `x.unwrap_or(d)`, `x.get_or(d)`, and `x or d`
or `x || d` with an option on the left. `otherwise` on a value that is not an option is
`MZ0938` too, with the `exact` fix deleting `otherwise d`: `d` could never have been evaluated,
so deleting it changes nothing.

**The escape hatches the survey rejected stay out** (R1, R2): optional chaining `x?.f`, a
force-unwrap `x!`, and `.unwrap()` or `.expect(…)` on an option are `MZ0939`, with no fix; the
`say` names `when x is not none` and `otherwise`. Chaining hides which link was absent, and a
force-unwrap is a halt the author chose not to handle, which §12.3 already refuses for results.

## 9. Collections — _C7_

### 9.1 Building them

```mz
let primes = [2, 3, 5, 7]
let empty: list(text) = []
let ages: map(text, int) = ["ada": 36, "alan": 41]
let seen: set(int) = [1, 2, 3]
var counts: map(text, int) = []
```

**A bracket literal takes its collection type from where it is used**: the annotation, the
parameter it is passed to, or the field or return type it fills. With nothing to say otherwise,
it is a list. `[]` with nothing to fix its element type, a literal whose elements have different
types, and a literal mixing `k: v` entries with plain elements are `MZ0961`. A set literal with a
repeated element keeps one; a map literal with a repeated key is `MZ0961`, because one of the two
values is a mistake.

Today's lexer rewrites `[entry]` to `list(entry)` wherever it appears (RFC-0008 §1, `MZ0105`).
In a program `[x]` is also a list literal, so that repair moves from the lexer into the type
parser, where only a type can stand. RFC-0008's `MZ0105` tests must keep passing unchanged; that
is the Wave 1 C7 work's obligation (§18.2).

### 9.2 Operations

The set M1 needs, all methods (§3.7):

| Operation                | On             | Returns           | Notes                                                                      |
| ------------------------ | -------------- | ----------------- | -------------------------------------------------------------------------- |
| `c.length()`             | list, map, set | `int`             | `len`, `size`, `count()`, `.length` are `MZ0962`, `exact`                  |
| `c is none`              | list, map, set | `bool`            | emptiness (RFC-0008 §4); `is []`, `length() is 0`, `.is_empty()`: `MZ0962` |
| `x in c`                 | list, set, map | `bool`            | element, or key; `.contains(x)`, `.includes(x)`, `.has(x)` are `MZ0962`    |
| `xs[i]`                  | list           | `option(T)`       | §3.7; `none` out of range; `.get(i)` is `MZ0962`, `exact` `xs[i]`          |
| `m[k]`                   | map            | `option(V)`       | `none` for a missing key; `.get(k)` is `MZ0962`, `exact` `m[k]`            |
| `xs.slice(a, to = b)`    | list           | `option(list(T))` | from `a` up to `b`; `none` when either end is out of range                 |
| `m.keys()`, `m.values()` | map            | `list`            | in key order                                                               |
| `s.to_list()`            | set            | `list(T)`         | in key order                                                               |
| `xs.map(f)`              | list           | `list(U)`         | `f(x: T): U`                                                               |
| `xs.filter(f)`           | list           | `list(T)`         | `f(x: T): bool`                                                            |
| the named folds          | list           | see §9.4          | `count`, `sum`, `any`, `all`, `first`, `fold`, `sort_by`, `group_by`       |
| `xs.join(sep)`           | `list(text)`   | `text`            |                                                                            |

Mutation, on a `var` only (`MZ0960` otherwise, with the `exact` fix `let` → `var` when the
binding is a `let`):

| Statement     | On   | Means                                              |
| ------------- | ---- | -------------------------------------------------- |
| `xs.push(v)`  | list | appends; `append` is `MZ0962` with the `exact` fix |
| `xs[i] = v`   | list | replaces; traps out of range (§4.3)                |
| `m[k] = v`    | map  | inserts or replaces                                |
| `m.remove(k)` | map  | removes the key if present                         |
| `s.insert(v)` | set  | adds                                               |
| `s.remove(v)` | set  | removes if present                                 |

**Maps and sets iterate in key order**, not insertion order and not hash order. Hash order would
make a program's output depend on a seed, which breaks RFC-0001 §4.1's determinism for any
program that prints a map. Insertion order (Python's `dict`, JavaScript's `Map`) would need an
ordered-map crate in the lowered package, which §13's dependency-free rule rules out. Key order
lowers to the standard library's `BTreeMap` and `BTreeSet`. The cost is that a Python-trained
model expects insertion order (§20, Q9).

Reversing, `last`, `zip`, list concatenation, `min` and `max` over a list, and the rest of a
collections library are P2's. §9.4's named folds, and `fold` itself, cover the commonest of them in
the meantime.

### 9.3 `map`, `filter`, `fold`

```mz
fn square(n: int): int
  return n * n
end fn square

fn is_even(n: int): bool
  return n % 2 is 0
end fn is_even

fn add(a: int, b: int): int
  return a + b
end fn add

fn sum_of_even_squares(xs: list(int)): int
  return xs.filter(is_even).map(square).fold(0, step = add)
end fn sum_of_even_squares
```

The function argument is a named `fn` (§6.4) whose signature must fit; a mismatch is `MZ0909`,
quoting both signatures. They are the only higher-order operations in M1, and the only place a
`fn` is a value. They are built in, and polymorphic in the element type the way `list(T)` itself
is: user code still has no type parameters. JavaScript's `reduce(f, init)` is `MZ0962` with the
`guess` fix `fold(init, step = f)` (a guess, because JavaScript's `reduce` without an initial
value has no Mzizi equivalent). In practice `xs.sum()` (§9.4) is this example's last stage; the
`fold` shows the general form.

**This is the duality RFC-0001 §1.2 excluded, kept on purpose.** `xs.map(square)` and a
`for each` that pushes `square(x)` onto a `var` build the same list, so two forms exist for one
intent. They are kept because the C7 row names "map, filter, fold" as the operations it asks
for; because code in every incumbent uses both shapes, so a model trained on it writes both (a
prediction, not a measurement); and because neither form can express the other's common case in one line: a loop
that prints, breaks or returns early is not a `map`, and a three-stage pipeline is three loops.
No diagnostic steers between them, since neither is a defect. Whether that is worth the second
form, or whether M1 keeps loops only and leaves `map` / `filter` / `fold` to P2, is the owner's
(§20, Q19).

### 9.4 The named folds — _survey F1_

| Fold                      | On                         | Returns           | Means                                                                      |
| ------------------------- | -------------------------- | ----------------- | -------------------------------------------------------------------------- |
| `xs.count(f)`             | list                       | `int`             | how many elements `f` is `true` for                                        |
| `xs.sum()`                | `list(int)`, `list(float)` | the element type  | `0` or `0.0` when empty; an `int` sum traps on overflow (§4.3)             |
| `xs.any(f)`, `xs.all(f)`  | list                       | `bool`            | stop at the first answer; `any` is `false` and `all` is `true` when empty  |
| `xs.first(f)`             | list                       | `option(T)`       | the first element `f` is `true` for, or `none`                             |
| `xs.fold(init, step = f)` | list                       | `A`               | `f(acc: A, x: T): A`, left to right (§9.3)                                 |
| `xs.sort_by(f)`           | list                       | `list(T)`         | ascending by `f(x: T): K`, `K` a key type (§2); stable, so ties keep order |
| `xs.group_by(f)`          | list                       | `map(K, list(T))` | elements by `f(x: T): K`; each list keeps the elements' order              |

**The set is closed.** Every incumbent offers a different, larger library of these, and a model
samples across all of them (FM-1). The survey's one-line design names these eight, and this RFC
takes them as they are: each answers a question that `map` and `filter` cannot, each has one name,
and each takes a named `fn` (§6.4) or nothing. A fold from another language that has a direct
equivalent is `MZ0962` with an `exact` fix: on a list, `find(f)` is `first(f)`, `some(f)` is
`any(f)`, `every(f)` is `all(f)`, `sorted(xs, key = f)` and `sortedBy(f)` are `sort_by(f)`, and
`count()` with no argument is `length()`. Any other name is `MZ0708`, with the nearest-name fix
over list methods. `sort_by` with a function whose result has no order (a `float`, a record) is
`MZ0964`, as a map key would be. Whether `min`, `max` and a key-less `sort` belong in the set is
the owner's (§20, Q26).

The survey left the form open (its §4.1 item 1): a `for each … keep` expression, methods taking
a named function, or methods taking an inline function. This RFC keeps the second, because it is
the only one consistent with no lambdas (§6.4, F20) that does not add a third loop form beside
`for each` and `while`.

## 10. Text operations — _C6_

`text` is UTF-8. **Lengths, indices and slices count Unicode scalar values**, Python's choice, so
`"héllo".length()` is `5` in every arm that agrees with Python, and an index never lands inside a
character. There is no `char` type: one character is a `text` of length 1. The minimum C6 needs,
all methods:

| Method                               | Returns         | Notes                                                       |
| ------------------------------------ | --------------- | ----------------------------------------------------------- |
| `s.length()`                         | `int`           | scalar values, not bytes                                    |
| `s[i]`, `s.slice(a, to = b)`         | `option(text)`  | `none` out of range (§3.7, survey F23)                      |
| `s.contains(t)`                      | `bool`          | substring; `t in s` is `MZ0962` (§3.3)                      |
| `s.find(t)`                          | `option(int)`   | the first scalar-value index, or `none`; `0` for `""`       |
| `s.starts_with(t)`, `s.ends_with(t)` | `bool`          |                                                             |
| `s.split(sep)`                       | `list(text)`    | `sep` is not empty; `s.split("")` is `MZ0915` when literal  |
| `s.chars()`                          | `list(text)`    | one element per scalar value                                |
| `s.trim()`                           | `text`          | Unicode whitespace at both ends; `strip` is `MZ0962`        |
| `s.to_upper()`, `s.to_lower()`       | `text`          | Unicode case mapping                                        |
| `s.replace(old, by = new)`           | `text`          | every occurrence                                            |
| `s.repeat(n)`                        | `text`          | `n` ≥ 0                                                     |
| `s.parse_int()`                      | `option(int)`   | RFC-0011 §4.2's rule: `-`? then ASCII digits, fitting `int` |
| `s.parse_float()`                    | `option(float)` | `-`? digits, `.` and digits optional; no exponent, no `inf` |

`parse_int` reuses RFC-0011 §4.2's rule on purpose, so a query parameter and a parsed string agree
(HD-4): `"0x10"`, `"1e2"`, `" 7 "` and `"1.5"` are all `none`. Formatting is interpolation (§3.6);
fixed decimal places, padding and number formatting are P2's. Regular expressions are P2's.

C6's "Done when" asks for these "in the standard library (P2) with tests". They are built-in
methods with a fixed lowering table (§14.2), and that table is the seed of P2's `text` module:
when modules exist (P1), the module is where they are documented, not a second implementation.
Whether built-in methods with tests meet C6's "Done when", or C6 waits for P2, is the owner's to
decide (§20, Q20). Until then the C6 row does not turn ✅ on built-in methods alone.

## 11. Records and methods — _C8_

### 11.1 Building and changing a record

A record is RFC-0008 §2's declaration. In a program it can be built:
`point(x = 1.0, y = 2.0)`. **Every field is given, by name, exactly once, in declaration order,
with a value of its type** (survey F6). There are no zero values (survey R9, Go's): a field the
author forgot is an error, never a silent `0` or `""`, and absence is a field of type
`option(T)` given `none`. The record's name is used like a call, and the `=` reads as it does in
a view attribute, "is set to", and as a call's labels do (§6.5).

Every problem is `MZ0808`, the code RFC-0011 §3 gave record literals, so one kind of mistake keeps
one code. Its fixes:

- **Positional construction**, `point(1.0, 2.0)` (Python's dataclass, a C struct initialiser):
  `exact`, inserting the field names in declaration order, when the count is right.
- **Fields out of declaration order**: `exact`, reordering them, when every value is a literal,
  a name or a path; a `guess` otherwise. Values are evaluated as written, and canonical form is
  declaration order (§17), so the rule is §6.5's, for the same reason.
- **An unknown field**: RFC-0008 §5.2's nearest-name fix.
- **A missing field**: no fix, except a `guess` of `name = none` for an `option` field. The
  survey asked for an `exact` fix (F6), but a value for a field the author left out is the
  author's choice, and inventing one is R9's zero value by another route (§21).
- **A repeated or ill-typed field**: no fix.

That makes two spellings of a record value: this one, and RFC-0011's `problem error "x"` in a
handler. Handlers are not changed here; whether they move to the parenthesised form, with an
`exact` fix, when they adopt §3's expressions is part of §20 Q14.

A record is a value. `var q = p` copies `p`, and `q.x = 3.0` changes `q` only (§14.1). Assigning
to a field needs a `var` (`MZ0960`).

**`p with (x = 3.0)` is a copy of `p` with the named fields replaced** (survey F7, C#'s `with`
and Java's JEP 468). It is an expression, so it can be returned, bound or passed; `p` is not
changed. The fields inside the parentheses follow the record literal's rules: names of `p`'s
record, at least one, each at most once, in declaration order, every other field copied from
`p`. It is a postfix form at §3.5's level 2, so `p with (x = 0.0).norm()` reads
`(p with (x = 0.0)).norm()`. Its problems are `MZ0974`: `with` on a value that is not a record
(no fix), an unknown field (the nearest-name fix), a field named twice (no fix), an empty
`with ()` (`exact`, deleting it) and fields out of order (`exact` when the values are literals,
names or paths). JavaScript's spread, `{...p, x: 3.0}`, keeps its code, `MZ0106`, and gains the
`exact` fix `p with (x = 3.0)`; C#'s `p with { X = 3.0 }` braces are `MZ0937` with the `exact`
fix to parentheses. A line that starts with `with` (Python's `with open(path) as f:`) is not this
form: cleanup belongs to the compiler (survey R12), so it is a line no function body can read,
`MZ0917`, whose `say` says so.

This draft first had no `with`, and made a `var` and field assignments the one way to change a
record. Both are now in the language, for different jobs: a field assignment changes a `var` in
place, as a statement, and `with` makes a new value inside an expression, which is what a method
returning a changed copy needs (`scaled` below). Whether field assignment should then go is the
owner's (§20, Q29).

**What every record has, with nothing to write** (survey F5). No derive list, no `@dataclass`,
no `equals` or `toString` to write:

- **Equality**: `a is b` compares every field, in declaration order (§3.3).
- **A text form**, the debug text: `point(x = 1.0, y = 2.0)` (§3.8), so `print(p)` and `"{p}"`
  work on every record.
- **A JSON form**: `p.to_json()` is `text`, an object with the fields in declaration order, as
  RFC-0011 §4.1 writes a response body (`none` is `null`, an enum is its variant's name). It
  works on every type with a JSON form (§2), not only records. Reading JSON into a record is
  P2's (survey F29), so a program can write JSON in M1 and not read it. A `float` that is `nan`
  or infinite has no JSON form, and encoding one traps (§4.3) until the owner decides §20 Q9.

The survey also lists hashing. A Mzizi map is ordered, never hashed (§9.2), so there is nothing
to derive; a record is not a map key or set element in M1 (`MZ0964`), because ordering records
would need a rule this RFC does not need yet.

### 11.2 Methods

```mz
record point
  field x: float
  field y: float

  fn norm: float
    return (self.x * self.x + self.y * self.y).sqrt()
  end fn norm

  fn scaled(k: float): point
    return point(x = self.x * k, y = self.y * k)
  end fn scaled
end
```

- **A method is a `fn` inside a `record` block**, after its fields. It is called on a value,
  `p.norm()`, and has the same signature, body, return and contract rules as any `fn` (§6). This
  amends RFC-0008 §2, whose record body holds only `field` lines: in a program, a `fn … end fn`
  block after the fields is not `MZ0308`. A `fn` before a field, or any other line, still is.
- **The receiver is `self`, always written, never declared.** `self.x` reads a field; `self` in
  the parameter list (Python's) is `MZ0970` with the `exact` fix deleting it, and TypeScript's
  `this.x` gets the `exact` fix `self.x`. `self` outside a method is `MZ0970`.
- **A method does not change its receiver, unless it says `changes self`.** In an ordinary
  method `self` is a `let`: assigning to `self` or a field of it is `MZ0971`, whose `say` names
  the two forms, returning a changed copy (`scaled` above) or `changes self`, and whose `guess`
  fix adds `changes self` (a guess, because it changes what callers may do). That keeps an
  ordinary method call a pure read of its receiver, which is what lets the lowering pass it as
  `&self` (§14.2) and keeps CL-4 out.
- **`changes self` marks the method that mutates** (survey F8, Swift's `mutating`):

  ```mz
  record counter
    field count: int

    fn bump changes self
      self.count = self.count + 1
    end fn bump
  end
  ```

  The words follow the signature, where a return type would stand, and a `changes self` method
  has no return type, so a mutating call never also produces a value. It is called only as a
  statement, `c.bump()`, on a `var`, as `xs.push(v)` is (§9.2), or, inside another
  `changes self` method, on `self` or a field of it (`self.inner.bump()`), so mutating methods
  compose: on a `let` it is `MZ0960` with the `exact` fix `var`, on `self` in a method without
  `changes self` it is `MZ0971`, and as a value or with a return type it is `MZ0976`, with no fix. So a
  line that changes a value always starts with that value's name. There is no `inout`
  parameter: only the receiver can change. It lowers to `&mut self`. This answers §20 Q6 for
  records, provisionally.

- A method's name may not equal a field's (`MZ0970`), so `p.x` and `p.x()` never both exist.
- Methods are designed for records wherever records live, and are built for programs first.
  Methods on enums, beside their static columns, are not in M1.

### 11.3 Generics, interfaces and traits: deferred

The owner deferred generics past M1 (issue #69, 2026-10-07), and this RFC defers interfaces and
traits with them, since an interface without a generic function to accept it has nothing to do.
`fn f<T>(…)` and `class`, `interface`, `trait` and `impl` are `MZ0972`, with no fix; the `say`
says the feature is deferred and names what exists (records, methods, named functions). A type
parameter written without the angle brackets (`fn f(x: T)`) is `MZ0701`, an unknown type, as it
is today.

C8's "Done when" asks for "a record has a method; a generic function works for two types; an
interface is satisfied". Wave 1 meets the first clause only, so **C8 cannot turn ✅ at M1** as the
tracker defines M1 ("all of Tier 1 ✅"). That conflict is the owner's to resolve (§20, Q8), and
§20 Q21 proposes the split: C8 for records and methods in M1, a new row for generics and
interfaces after it. This RFC does not change the tracker's row.

When they come, the survey's design holds (F25, F26): every type parameter names an interface,
with no specialisation; an interface is nominal, declared in the record's own block, and a
missing method is reported at the record with a stub fix. Structural typing, computation in
types, extension methods and inheritance stay rejected (survey R4–R7).

### 11.4 `always`: invariants checked when a value is built — _survey F33, RFC-0010 §3.3_

```mz
record span
  field low: int
  field high: int

  contract
    always low <= high
  end
end
```

- **A record's `always` clause holds for every value of it.** It is a `bool` expression of §3
  over the record's fields, named bare as RFC-0010's subjects are, with literals, operators and
  built-in methods. It may not call a user `fn` or method or use `try`, so checking it prints
  nothing and returns nothing (`MZ0975`). RFC-0010's predicate words are §3.3's:
  `count at_least 0` is `MZ0910` with the `exact` fix `count >= 0`, as in any expression. The
  `contract` block is last in the record, after its methods.
- **It is checked after every construction** (a record literal and a `with`), **after every field
  assignment** from outside the record, **and at the end of every `changes self` method**, not
  after each line inside one, so a method may pass through a broken state on its way to a good
  one (Eiffel's rule). A broken invariant **traps** (§4.3): `MZ0991`, naming the record, the
  clause and the position of the line that built or changed the value, and exit 101.
- **It is checked in every build.** RFC-0010 §4.3 lowers `always` to `debug_assert!`; in a
  program that would make a debug and a release build of one program behave differently, which
  §13.1 rules out. This amends RFC-0010 §4.3 for programs (§20, Q25).
- **What `mz check` sees, it reports.** A literal construction whose fields are all literals and
  whose `always` folds to `false` is `MZ0975` at check time, as a constant fault is `MZ0915`.
- RFC-0010 §3.3 says `always` is never the validation step for data crossing a boundary, and
  that stands: P2's JSON reader rejects a value that breaks an invariant as it rejects a wrong
  shape, and the trap is for a program that builds one itself.

### 11.5 Enums with payloads — _survey F10, F11; RFC-0007 D1_

```mz
enum shape
  circle(radius: float)
  rect(width: float, height: float)
  dot
end

fn area(s: shape): float
  match s
    case circle
      return 3.14159 * s.radius * s.radius
    case rect
      return s.width * s.height
    case dot
      return 0.0
  end
end fn area
```

**One `enum` construct holds both static columns and per-value data**, which answers RFC-0007
D1's open question (§20, Q24). A variant may declare **payload fields** in parentheses after its
name, typed as a function's parameters are; its columns, if the enum has any, follow as today
(`circle(radius: float) label "round"`). A columns-only enum is unchanged.

- **Construction is a record literal**: `circle(radius = 1.0)`, or `shape.circle(radius = 1.0)`
  where the bare name is ambiguous. Every field by name, in order, with `MZ0808` and its fixes
  (§11.1). A payload variant written bare, with no fields, is `MZ0808`.
- **A `case` names a variant and never binds a name** (survey F9). Inside `case circle` of
  `match s`, with `s` a name or a path, `s` is narrowed to that variant and `s.radius` reads its
  field, as `when x is not none` narrows an option (§8) and as TypeScript narrows a
  discriminated union. `when s is circle` narrows its branch the same way. Reading a payload
  field anywhere else is `MZ0973`, with a `guess` fix wrapping the line in `when s is circle`.
  When the `match` is on a call or another expression, there is nothing to narrow, and the
  `say` of `MZ0973` names the repair, binding it with `let` first. `case ok <name>` on a result
  (§12.2) keeps its binding, because a result most often comes straight from a call.
- `s is circle` with a payload variant on the right is a variant test, ignoring the fields;
  `a is b` between two `shape` values compares variant and fields.
- A payload enum has equality, a text form (`circle(radius = 1.0)`, §3.8) and a JSON form
  (`{"circle": {"radius": 1.0}}`, §2: the shape `result` already has, so one rule covers both).
  It is not a map key (`MZ0964`).
- A payload field's type is any type a record field may have. A payload whose type holds its own
  enum, directly or through `option`, is `MZ0973` in M1: Rust would need a `Box`, and recursive
  data is a design of its own. Through `list` it is legal, since `Vec` is already indirect.
- **`<enum>.variants()`** is the list of an enum's variants in declaration order (survey F11,
  Swift's `allCases`), on an enum with no payload variant (`MZ0973` otherwise, since a payload
  variant is not a value by itself). The survey names it `all`; this RFC does not, because
  `xs.all(f)` is a fold (§9.4), and one word would mean two things by receiver.

It lowers to a Rust enum with struct variants, each field owned, and the column accessors of
RFC-0001 §5 beside it; the narrowed reads lower to the fields `match` binds (§14.2).

## 12. Errors — _C9, CL-3_

### 12.1 `result(T, E)`

A function that can fail says so in its return type:

```mz
enum parse_problem
  empty      say "no digits"
  not_number say "not a number"
end

fn parse_age(raw: text): result(int, parse_problem)
  when raw.trim() is ""
    return error(empty)
  end
  let n = raw.trim().parse_int()
  when n is none
    return error(not_number)
  end
  return n
end fn parse_age
```

- **`return v` returns success**, wrapped implicitly, as an `option(T)` takes a `T` with no
  `some` (RFC-0008 §5). **`return error(e)` returns failure.** The wrapper is needed on the
  failure side only, because `result(text, text)` must still be unambiguous. Rust's `Ok(v)` and
  `return ok(v)` are `MZ0952` with the `exact` fix `v`; `Err(e)` gets `error(e)`; `throw e` and
  `raise e` get `return error(e)`, `exact` when `e` has the error type.
- **`return r`, where `r`'s type is the function's own result type, returns `r` as it is**: its
  success as success, its error as error. `return parse_age(a)` in a function returning
  `result(int, parse_problem)` passes the callee's result through, which is what Rust's
  `return parse_age(a)` does and what a reader expects. It lowers to `return r;`, never to
  `return Ok(r);` (§14.2). The rule is decided by types, and is never ambiguous: the success type
  `T` can never itself be a result (§2), so a value of the function's result type cannot also be
  a `T` to wrap. A result whose type differs from the function's (another success or error type)
  is `MZ0950`, as §12.3 says.
- **`return try r`, where `try r` is the whole returned value and `r`'s type is the function's
  own result type, is `MZ0954`**, with the `exact` fix deleting `try`. It unwraps a success only to wrap it again, and an error propagates either way,
  so both lines are the same program and the pass-through is the one form. (Rust's clippy flags
  `Ok(r?)` for the same reason.) `return try r + 1` is not that case, and neither is a `try r`
  whose success type differs from the function's (`return try g()` with `g` returning
  `result(int, e)` in a function returning `result(option(int), e)`, where the `int` is then
  wrapped as an option); both are legal.
- `E` is any type with a text form (§3.8). An enum is the expected choice, because a `match` on
  it is exhaustive, and its columns can carry the message. `result(none, E)` is a function that
  produces nothing but can fail; it returns success with a bare `return`, or by reaching
  `end fn` (§6.2's one exception).
- **There are no exceptions** (RFC-0007 D5: errors are values). `try … catch`, `try … except`
  and `try … finally` blocks are `MZ0952`, with no fix; the `say` names `match` on the result.

### 12.2 Handling and propagating

```mz
fn describe(raw: text): text
  match parse_age(raw)
    case ok age
      return "age {age}"
    case error problem
      return "rejected: {problem.say}"
  end
end fn describe

fn total_age(a: text, b: text): result(int, parse_problem)
  return try parse_age(a) + try parse_age(b)
end fn total_age
```

- **Handling is `match`** with `case ok <name>` and `case error <name>` (§7.2). Both cases are
  required (`MZ0930`).
- **Propagation is the prefix `try`**: `try e` is `e`'s success value, and when `e` is an error it
  returns that error from the enclosing function at once. The enclosing function must return a
  `result` with the same error type, and `e` must be a `result` (`MZ0951` otherwise). When the
  error types differ, the conversion is written, never implicit: see `via` below.
- **`try e via f` converts the error on its way out** (survey F15). `f` is a named `fn` of the
  program taking `e`'s error type and returning the enclosing function's error type, so
  `try parse_age(raw) via age_problem` returns `error(age_problem(p))` when `parse_age` fails
  with `p`. Rust's `From` conversions happen out of sight, at the `?`; Go's `fmt.Errorf` wraps
  by hand at every call. `via` is the one visible form, and `f` is named for the reason §6.4
  gives. `via` belongs to its `try` and binds with it at §3.5's level 3, so
  `try g() via f + 1` reads `(try g() via f) + 1`. `MZ0951`'s fix, when the error types differ,
  is `via <f>`: `exact` when exactly one `fn` in the program has that signature, a `guess` when
  several do, and absent when none does (the `say` names the signature to declare). Other
  misuses of `via` are `MZ0955`: `via` without `try`, an `f` that is not a named `fn` or whose
  signature does not fit (no fix), and `via` where the two error types already agree (`exact`,
  deleting `via f`). The word is open (§20, Q27).
- `try` is Zig's and Swift's word for exactly this. Rust's postfix `?` is `MZ0952` with the
  `exact` fix moving it to a prefix `try`: `?` is a symbol, and the lexer already reserves it to
  say "optional types are `option(T)`" (`MZ0104`). The collision is with TypeScript's and
  Python's block `try`, recorded above: a `try` at the end of a line, opening a block, is the
  `MZ0952` that names `match`.
- `try` binds tighter than arithmetic and looser than postfix (§3.5): `try parse_age(a) + 1` is
  `(try parse_age(a)) + 1`, and `try f(x).y` applies `.y` to a result, which is `MZ0950` with the
  `exact` fix `(try f(x)).y`.

### 12.3 The unhandled-error diagnostic

**A `result` must be matched or propagated.** Every other use of one is `MZ0950`:

- discarded, as a call statement whose value nothing reads (Go's `_ = err` cannot be written;
  there is no `_`);
- used where its success type is expected (`let n = parse_age(s)` then `n + 1`);
- printed or interpolated, since a result has no text form (§3.8);
- compared, passed as an argument, stored in a field or a collection, or returned from a function
  whose return type is not that same result type (§12.1's pass-through needs the exact type);
- written as the success or error type of another result (§2).

A `let` may hold a result, so that the `match` can follow on its own line; the `let` is then the
value that must be matched or propagated before its block ends.

`MZ0950`'s fix is a `guess` inserting `try` where the enclosing function returns a result with the
same error type (or `try … via f` where §12.2's `via` fix applies). Otherwise it is a `guess`
inserting a `match` stub on its own line, with `case ok <name>` and `case error <name>` and
empty bodies for the author to fill (survey F16), which the next `mz check` reports until they
are filled. Propagating or handling changes what the function does, so neither fix is ever
`exact`. A returned result of the function's own type is not `MZ0950` at all (§12.1), so the fix
never offers `return try r`. This is C9's "an unhandled error is a diagnostic". `.unwrap()` and `.expect(…)` are
`MZ0952` with no fix: there is no way to turn an error into a halt on purpose, because a trap is
for faults the author did not foresee (§4.3), not for errors the type system already named.

### 12.4 `error(e)` and other misuses

`error(e)` in a function that does not return a result, or with an `e` of the wrong type, is
`MZ0953`.

### 12.5 An error out of `main`

`fn main` may return `result(none, E)`, so that `try` can be used in it. When `main` returns an
error, the program writes `error: <the error's text form>` to standard error and exits with
status **1**. Under `mz run --agent` that is one NDJSON line with code `MZ0992`. A program whose
`main` returns nothing exits 0 when `main` ends.

## 13. The entry point and `mz run` — _C10, P4_

### 13.1 `mz run`

```text
mz run <file.mz>              check, lower, build with cargo, run; the program's output passes through
mz run --agent <file.mz>      the same, with diagnostics, a trap or main's error as NDJSON on stderr (not built yet)
mz run --release <file.mz>    build the release profile
mz build <file.mz> --out <dir> write the program's Cargo package, as for a service (RFC-0011 §8.1)
```

1. **Check.** `mz run` runs `mz check` first. On any error it prints the diagnostics, exits 3 and
   does not build. The loop summary line (RFC-0001 §4.4) is printed as for `mz check`.
2. **Lower.** It writes a Cargo package (§14) into a cache directory that belongs to the user
   and is keyed by the source file's path: `<name>-<path hash>/` under `$MZ_CACHE_DIR`, or
   `mzizi/mz-run/` under `$XDG_CACHE_HOME` or `~/.cache`. With none of them set, `mz run` exits
   2 and names the three; it never falls back to the system's temporary directory. Per user, because a directory in a shared temporary directory let another account plant
   a `build.rs` that `cargo` would run. Keyed by path, not by a hash of the generated text,
   because a directory per content hash made every edit a cold build; in one directory per
   source file, Cargo's own fingerprint rebuilds exactly what changed, and the generated code,
   which embeds each trap's file, line and column (§14.3), is rewritten on every run, so moving
   a line still rebuilds and a trap never names a stale line. (This draft first keyed the
   directory by the generated text in the temporary directory; §18.6 records the change.) The
   package is generated output, never committed.
3. **Build.** `cargo build --offline --quiet` (with `--release` when asked). The package has **no
   dependencies**, so `--offline` always works and a run needs no network: not even crates.io's
   index, unlike a service. A missing `cargo` is a usage problem, exit 2. Lowered code that
   `rustc` rejects is a compiler bug, by construction (P5): `mz run` reports it as `MZ0990`, with
   the `.mz` construct it came from and `rustc`'s first message, and exits 3. (As built,
   `MZ0990` names the program and quotes `rustc`'s first message; naming the construct is not
   built yet, §18.6.)
4. **Run.** It runs the binary with the terminal's standard input, output and error, and exits
   with the program's own status, or 128 plus the signal's number when a signal killed it, the
   shell's convention. Under `--agent`, the summary line gains `"ran": true` or `false`,
   `"exit": <status>`, and `"signal": <n>` when a signal killed the program.

**Standard output belongs to the program.** Everything `mz run` itself writes goes to standard
error, in both modes: the diagnostics of step 1, the loop summary line, `MZ0990`, and the lines
the generated program writes for a trap (`MZ0991`), for `main`'s error (`MZ0992`) and for a
runtime failure (`MZ0993`). **This is a change from `mz check --agent`, which writes its NDJSON to
standard output** (`main.rs` today). `mz run --agent` uses the same NDJSON objects and the same
summary line, on the other stream, so that a program's printed output and the diagnostics never
interleave on one stream. Standard error may also carry lines that are not NDJSON: Rust's own
message before a stack-overflow abort, or the panic message before `MZ0993`. The benchmark
runner calls only `mz check --agent` today and needs no change until it calls `mz run`. When it
does, it has to read diagnostics from standard error, keep the lines that parse as JSON objects
with a `code` key, and score standard output as the program's output.

**The generated `main`** (§14.2) is the only Rust code that runs before or after the program's
own, and it handles every outcome without `unwrap` or `expect`:

1. It spawns `mz_main` on a thread with `std::thread::Builder::new().stack_size(64 << 20)`, and
   matches the `io::Result` that `spawn` returns. On `Err`, it writes
   `mz: runtime MZ0993: could not start the program's thread: <the error>` to standard error and
   exits **70**.
2. It matches `join()`. `Ok` carries what `mz_main` returned: nothing, or `Ok(())`, exits 0, and
   `Err(e)` writes §12.5's line and exits 1. A trap never returns here, since `mz_trap` exits
   the process itself (§14.3).
3. `join()` returning `Err` means the thread panicked. The lowering emits no `panic!`, `unwrap`,
   `expect` or indexing (§14.3), so a panic comes from the standard library or the runtime, and
   is a compiler or runtime bug by the same reasoning as `MZ0990`. The generated `main` writes
   `mz: runtime MZ0993: the program panicked; this is a bug in mz, not in the program` and exits
   **70**, `EX_SOFTWARE` in BSD's `sysexits.h`. Rust's panic hook has already written its own
   message to standard error. No panic can exit 101, because a panic on the spawned thread does
   not end the process.
4. A failure Rust handles by aborting is a signal death, which no generated code can catch:
   stack overflow (§4.3) and allocation failure both raise `SIGABRT`, so `mz run` exits 134.

**Every exit status of `mz run` means one thing**, so a script without `--agent` can still tell
them apart:

| Status | Means                                                                         |
| ------ | ----------------------------------------------------------------------------- |
| 0      | the program ran and `main` ended                                              |
| 1      | the program ran and `main` returned an error (§12.5)                          |
| 2      | a usage or I/O problem: bad arguments, an unreadable file, no `cargo`         |
| 3      | the program did not compile: `mz check` errors, or `MZ0990`; it did not run   |
| 70     | the runtime failed: a Rust panic or a thread that would not start (`MZ0993`)  |
| 101    | the program ran and trapped (§4.3)                                            |
| 128+n  | a signal `n` killed the program: 134 for stack overflow or allocation failure |
| 141    | the program ran and its standard output was closed (§14.2, `print`)           |

141 is also 128 plus `SIGPIPE`'s 13, and the overlap is deliberate: Rust programs ignore
`SIGPIPE`, so the lowered program exits 141 itself on a closed pipe, and a shell reading the
status sees the meaning it already knows.

`mz run` departs here from the other commands, where 1 means "errors": 1 belongs to the program,
because a program that fails conventionally exits 1. A program has no way to choose its own exit
status in M1 (`exit(n)` is P2's).

Semantics do not depend on the profile: traps are explicit checks in the lowered code (§14.3), not
`overflow-checks`, so a debug and a release build of one program print the same output and stop
at the same trap. The release profile still sets `overflow-checks = true`, as a second net.

`mz run` builds with `cargo` because the owner chose it (issue #69): "lowered Rust is the source
of truth." The in-process evaluator stays, for `mz contract` and `mz test` (§15), and §15.3 says
how the two are kept in agreement.

## 14. Lowering to Rust

### 14.1 Value semantics, and no ownership in the source — _CL-4, CL-5, P10_

Every Mzizi value is lowered to an **owned** Rust value. No Mzizi construct lowers to a Rust
reference in a signature, and no lifetime is ever emitted. The rules:

- Each Mzizi type `T` lowers to one owned Rust type, which derives `Clone`, `PartialEq` and
  `Debug`, and `Ord` where `T` is a key type (§2). `int`, `float`, `bool` and enums are `Copy`.
- **A read of a binding whose type is not `Copy` is a `.clone()`** unless it is the receiver of a
  method that only reads (`length`, `get`, `in`, a record method), which borrows. `let b = a`
  lowers to `let b = a.clone();`. Arguments are passed by value. A later pass may turn a clone on
  a binding's last use into a move; that is an optimisation, never a change of meaning, and it is
  not in M1.
- `var` lowers to `let mut`; `let` to `let`.
- `for each x in xs` lowers to `for x in xs.clone()`, which is what makes §7.3's "iterates the
  value it had when the loop began" true.

The cost is copies. A program that passes a large list through deep recursion copies it at each
call. That is the price of keeping CL-5 out of the source (RFC-0001 §1.8), paid in the generated
code rather than by the author; a reference-counted copy-on-write representation is the known
remedy, and is not in M1 (§20, Q15). Nothing in this RFC claims the lowered code is fast.

### 14.2 What each construct emits

| Mzizi                                             | Rust                                                                                                                                                                         |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `program p`                                       | a package `mz-p`: `Cargo.toml` (edition 2024, no dependencies, its own `[workspace]`) and `src/main.rs`                                                                      |
| `fn main`                                         | `fn mz_main()` (or `-> Result<(), E>`), run by the generated `main` of §13.1 on a 64 MiB thread                                                                              |
| `fn f(a: int): int`                               | `fn f(a: i64) -> i64`                                                                                                                                                        |
| `f(a, b = e)`, a default left out                 | `f(a, e, <the default's literal>)`: labels erased, arguments already in declaration order, each omitted default written in (§6.5)                                            |
| `int`, `float`, `bool`, `text`                    | `i64`, `f64`, `bool`, `String`                                                                                                                                               |
| `list(T)`, `option(T)`                            | `Vec<T>`, `Option<T>`                                                                                                                                                        |
| `map(K, V)`, `set(K)`                             | `std::collections::BTreeMap<K, V>`, `BTreeSet<K>`                                                                                                                            |
| `result(T, E)`, `result(none, E)`                 | `Result<T, E>`, `Result<(), E>`                                                                                                                                              |
| `enum` with columns                               | a fieldless `enum` and one `match` accessor per column (RFC-0001 §5)                                                                                                         |
| `record`                                          | a `struct` with the same fields in the same order                                                                                                                            |
| record methods                                    | an `impl` block; each method takes `&self`, and a `changes self` method `&mut self`                                                                                          |
| `point(x = 1.0, y = 2.0)`                         | `Point { x: 1.0, y: 2.0 }`, then the record's `always` check when it has one (§11.4)                                                                                         |
| `p with (x = e)`                                  | `{ let mut mz_w = p.clone(); mz_w.x = e; mz_w }`: the base first, then each field in order (§6.3), then the `always` check on `mz_w`                                         |
| a record's `always` clauses                       | one generated `fn mz_always(&self, at: &MzAt)` that calls `mz_trap` when a clause is false, called at each site §11.4 lists                                                  |
| `v.to_json()`                                     | a generated writer per type, with no crate: `mz_json(&v)` (§11.1)                                                                                                            |
| an `enum` with payloads                           | a Rust `enum` with struct variants, `Circle { radius: f64 }`, and the column accessors beside it                                                                             |
| `case circle` in `match s`, reading `s.radius`    | `match s.clone() { Shape::Circle { radius: mz_p1 } => { … } … }`, every narrowed read of `s.radius` lowered to `mz_p1.clone()`, so `s` itself stays whole inside the arm     |
| `shape.variants()`                                | an associated `const MZ_VARIANTS: [Shape; n]` in `impl Shape`, so each enum has its own; `Shape::MZ_VARIANTS.to_vec()`                                                       |
| `let` / `var` / assignment                        | `let` / `let mut` / `=`                                                                                                                                                      |
| `a + b` on `int`                                  | `mz_add(a, b, &AT_12_9)`: `checked_add`, and a trap when it is `None` (§14.3)                                                                                                |
| `a + b` on `float`                                | `a + b`                                                                                                                                                                      |
| `a / b`, `a % b` on `int`                         | `checked_div`, `checked_rem` through the same trap helper                                                                                                                    |
| `is`, `is not`, `<`, …                            | `==`, `!=`, `<`, … (on text, `<` compares by scalar value, which `String`'s `Ord` does)                                                                                      |
| `and`, `or`, `not`                                | `&&`, `\|\|`, `!`                                                                                                                                                            |
| `x in xs` / `k in m` / `s.contains(t)`            | `xs.contains(&x)` / `m.contains_key(&k)` / `s.contains(t.as_str())`                                                                                                          |
| `"a {x} b"`                                       | `format!("a {} b", mz_text(&x))`, through one generated `MzText` trait (§3.8)                                                                                                |
| `print(v)`                                        | `mz_print(&v)`: `writeln!` on a locked standard output, its result checked (exit 141 on a closed pipe)                                                                       |
| `when` / `else when` / `else`                     | `if` / `else if` / `else`                                                                                                                                                    |
| `when` as a value                                 | `let x = if c { a } else { b };`                                                                                                                                             |
| `match` as a value                                | `let x = match k { K::A => a, _ => b };`, `_` only for the `else` case                                                                                                       |
| a `T` where `option(T)` is expected               | `Some(e)`                                                                                                                                                                    |
| `when p is none … else …`, `p` a name or path     | `match p.clone() { None => { … } Some(mut mz_n1) => { … } }`: inside the narrowed branch, every read of `p` lowers to `mz_n1`                                                |
| `when p is not none … [else …]`                   | `if let Some(mut mz_n1) = p.clone() { … } else { … }`, reads of `p` again lowered to `mz_n1`                                                                                 |
| a guard, `when p is none` that exits              | `let Some(mut mz_n1) = p.clone() else { <the guard's body, which ends in return, break or continue> };`, then as above to the block's end                                    |
| `x otherwise d`                                   | `match x.clone() { Some(mz_v) => mz_v, None => <d> }` (`=> Some(mz_v)` when `d` is an `option(T)`): `d` is evaluated only in the `None` arm (§8.1)                           |
| `match` on an enum or `bool`                      | `match`, with no `_` arm when the cases are exhaustive, so `rustc` re-checks exhaustiveness                                                                                  |
| `match` on `int` / `text`                         | `match` / `match s.as_str()`, with `else` as `_`                                                                                                                             |
| `match` on a result                               | `match r { Ok(name) => …, Err(name) => … }`                                                                                                                                  |
| `for each x in xs`                                | `for x in xs.clone()`                                                                                                                                                        |
| `range(a, to = b)` as a loop source               | `a..b`, with no list built                                                                                                                                                   |
| `while` / `break` / `continue`                    | `while` / `break` / `continue`                                                                                                                                               |
| `return v` in a result function                   | `return Ok(v);`, when `v` has the success type                                                                                                                               |
| `return r`, `r` of the function's own result type | `return r;`: passed through, never wrapped (§12.1)                                                                                                                           |
| `return error(e)`                                 | `return Err(e);`                                                                                                                                                             |
| `try e`                                           | `e?`                                                                                                                                                                         |
| `try e via f`                                     | `e.map_err(f)?`                                                                                                                                                              |
| `xs[i]`, `m[k]`                                   | `usize::try_from(i).ok().and_then(\|u\| xs.get(u)).cloned()`, `m.get(&k).cloned()`: an `Option`, never Rust indexing                                                         |
| `xs[i] = v`                                       | a generated helper that replaces the element or traps (§4.3)                                                                                                                 |
| `xs.map(f)`                                       | `xs.iter().cloned().map(f).collect::<Vec<_>>()`                                                                                                                              |
| `xs.filter(f)`                                    | `xs.iter().cloned().filter(\|x\| f(x.clone())).collect::<Vec<_>>()`: Rust's `filter` passes `&T`                                                                             |
| `xs.fold(init, step = f)`                         | `xs.iter().cloned().fold(init, f)`                                                                                                                                           |
| `count`, `any`, `all`, `first`                    | `iter()` with `filter(…).count()` (as `i64`), `any`, `all`, `find(…).cloned()`, each calling `f` on a clone                                                                  |
| `xs.sum()`                                        | a loop through the `int` trap helper; on `float`, a loop adding from `0.0`, never `iter().sum()`, whose empty sum is `-0.0` (checked with `rustc` 1.97)                      |
| `xs.sort_by(f)`, `xs.group_by(f)`                 | a clone sorted with the stable `sort_by_key`; a `BTreeMap` built by pushing in order                                                                                         |
| `a.wrapping_add(b)` and the rest                  | `i64::wrapping_add` and the rest                                                                                                                                             |
| `s.length()`, `s[i]`, `s.slice(a, to = b)`        | `chars()`-based helpers, counting scalar values (§10); `s[i]` and `slice` return an `Option`                                                                                 |
| `s.find(t)`                                       | a helper walking `s.char_indices()`, testing `s.get(b..)` with `starts_with`; it returns the scalar index, never `str::find`'s byte offset                                   |
| `x.pow(n)` on `int`                               | a helper over `checked_pow`, which takes a `u32`: it traps on `n < 0` or overflow, and above `u32::MAX` answers `x` of `0`, `1` and `-1` by rule (every other `x` overflows) |
| `x.pow(n)` on `float`                             | `x.powf(n as f64)`: `as` from `i64` to `f64` rounds to nearest, which is `to_float` (§4.4)                                                                                   |
| `example` clauses                                 | one `#[test]` each in a `#[cfg(test)]` module (RFC-0010 §5)                                                                                                                  |

Names:

- **The keyword list is the Rust Reference's**, for edition 2024: the strict, reserved and weak
  keywords (`type`, `move`, `ref`, `loop`, `const`, `static`, `pub`, `as`, `box`, `final`,
  `gen`, `yield`, `macro`, `override`, `union`, …), taken whole rather than picked from, so a
  word nobody thought of cannot reach `rustc` bare. A Mzizi name on that list is emitted as a
  raw identifier (`r#type`), except the four Rust cannot write raw (`crate`, `self`, `super`,
  `Self`), which are emitted as `mz_kw_crate` and so on.
- Records and enums are emitted in PascalCase. One whose PascalCase collides with a prelude name
  (`Vec`, `String`, `Option`, `Result`, `Box`) is emitted as `MzUser` plus the name.
- **The prefix `mz_` belongs to the lowering.** Generated helpers and types are `mz_…` and
  `Mz…`, and no Mzizi name of any kind (binding, parameter, `fn`, record, enum, variant or field)
  may start with `mz_` (`MZ0921`), so a record named `mz_at` cannot collide with the generated
  `MzAt`.
- `print` writes with `writeln!` on a locked standard output and checks the result, rather than
  using `println!`, which panics when the output is closed (`mz run prog.mz | head -1`). A failed
  write stops the program with exit status 141, the shell's status for a closed pipe, and no
  message, so a closed pipe is never a panic and never looks like a trap.

### 14.3 What the generated code never holds

As in a service (RFC-0011 §5): no `unwrap`, no `expect`, no `panic!`, no Rust indexing (`v[i]`)
and no `unsafe`. Every trap goes through one generated function,
`fn mz_trap(at: &MzAt, what: &str) -> !`, which writes §4.3's line to standard error and calls
`std::process::exit(101)`. `MzAt` holds the `.mz` file, line, column and the expression's
canonical text, as constants. The lowering's tests check the generated text for each of those
absences, as `compiler/tests/lower.rs` does for a service.

## 15. Contracts and tests

### 15.1 Contracts on functions and programs

RFC-0010 §3.1's function contract applies, with §6's signature in place of its `take` / `give`
placeholder lines. With more than one parameter, an `example` names each, in signature order:

```mz
fn add(a: int, b: int): int
  return a + b
  contract
    example a 2 b 3 returns is 5
    example a 0 b 0 returns is 0
  end
end fn add
```

The `contract` block is not a statement, so it may follow the last `return` without being
`MZ0907`. A program's own `contract` block takes one more subject, **`output`**: the text `main` writes to
standard output. `example output is "…"` and `example output contains "…"` run `main` with no
input. RFC-0010 §8's `MZ0613` applies to a program with no evaluated clause, as a warning.

An `ensure` that compares `returns` with a parameter (`returns at_least b`) is a relational
property, which RFC-0010 §11 leaves open (its question 3, pending G1.3). This RFC does not settle
it, so the example above uses `example` clauses only.

**In-process evaluation is bounded.** `mz contract` evaluates a clause by running `main` or the
`fn` in process, and a program may loop forever (`while true` with no `break` is legal, §6.2), so
the evaluator counts steps: each statement executed and each call made is one. A clause that
spends **10,000,000 steps** without finishing fails with `MZ0994`, which names the clause and the
bound, and evaluation moves to the next clause. Call depth is bounded too, at **10,000** nested
calls, and exceeding it is `MZ0994` as well; the evaluator runs each clause on a thread whose stack
is sized for that depth, so a deep recursion fails the clause instead of aborting `mz`. The bound is a constant of the evaluator, not a
setting, so a clause passes or fails the same way on every machine; a timer would not. `mz run`
has no bound: a program that runs forever is the author's to stop. The number is a proposal for
Wave 0 to confirm against `examples/`.

### 15.2 `mz test` — _T4, Wave 2_

```mz
test "parse_age rejects an empty string"
  match parse_age("")
    case ok age
      expect false
    case error problem
      expect problem is empty
  end
end
```

A `test "<description>"` block holds function-body statements and `expect <bool expression>`
lines. `mz test <file.mz>` runs every test block; a failed `expect` is a diagnostic quoting the
expression and its operands' values, in the same NDJSON protocol. A test is lowered to a
`#[test]` in the generated package and run by `cargo test`, so tests run against the lowered
Rust, the source of truth. It is Wave 2's to build, and its exact codes are decided there.

### 15.3 One semantics, two implementations — _HD-5, for programs_

The owner's default keeps the in-process evaluator for `mz contract` and lowers for `mz run`. Two
implementations of one semantics is HD-5 (RFC-0011 §0) again. The same answer applies: every
`example` clause is evaluated by `mz contract` in process **and** lowered to a `#[test]` that CI
runs, and CI runs every `examples/*.mz` program through `mz run`, comparing its standard output
with a committed `examples/<name>.expected` file. A disagreement between the evaluator and the
lowering is a failed build. That is tested agreement on those cases, never proven agreement
(RFC-0010 C-4).

## 16. Diagnostics

The hundreds digit keeps its meaning (RFC-0008 §6, RFC-0011 §9). **`MZ09xx` is new: programs and
the core language.** `diagnostic.rs` and every RFC were checked for codes in use: `MZ01xx` to
`MZ08xx` are taken, and nothing uses `MZ09xx`. The tens digit groups the codes. When `MZ09xx`
fills, the next family is `MZ10xx`.

| Code     | Tool          | Means                                                                                                                                                                                                                                                                                                            |
| -------- | ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MZ0901` | `mz check`    | a `program` line without a name, or a block a program cannot hold (`view`, `prop`, `route`, `emit`)                                                                                                                                                                                                              |
| `MZ0902` | `mz check`    | no `fn main`, a second one, `main` with parameters, or a return type other than nothing or `result(none, E)`                                                                                                                                                                                                     |
| `MZ0903` | `mz check`    | a malformed signature: `->` (`exact` `:`), `def` / `function` / `func` (`exact` `fn`), `()` on no parameters (`exact`), `: none` / `: void` (`exact`), an untyped parameter, a non-literal default or one on the first parameter, a parameter with two names                                                     |
| `MZ0904` | `mz check`    | two `fn`s, or a `fn` and a record or enum, with one name; a parameter named twice                                                                                                                                                                                                                                |
| `MZ0905` | `mz check`    | a call with too many arguments, a missing argument that has no default, or an argument of the wrong type; the `say` quotes the signature                                                                                                                                                                         |
| `MZ0906` | `mz check`    | a path through a function with a return type that reaches `end fn` without `return`; `exact` fix when the last line is a value of that type                                                                                                                                                                      |
| `MZ0907` | `mz check`    | a statement after `return`, `break` or `continue` that can never run; `exact` fix deletes it                                                                                                                                                                                                                     |
| `MZ0908` | `mz check`    | a `return` that does not fit its function: a value where none is returned, none where one is, or the wrong type                                                                                                                                                                                                  |
| `MZ0909` | `mz check`    | a `fn` used as a value outside `map` / `filter` / `fold`, a function argument whose signature does not fit, or a lambda                                                                                                                                                                                          |
| `MZ0910` | `mz check`    | an operator spelt from another language: `==` `===` `!=` `!==` `&&` `\|\|` `!`, `x not in y`, `not a is b`, `at_least` in an expression — `exact` fixes; `a // b` (`guess` `a / b`, §4.1); `**` (`guess` `x.pow(n)`, built in Wave 1). `\|\|` or `or` with an option on the left is `MZ0938` instead, never both |
| `MZ0911` | `mz check`    | a comment Mzizi lacks: `//`, `#`, `/* … */` on one line — `exact` fix `##`                                                                                                                                                                                                                                       |
| `MZ0912` | `mz check`    | operands of the wrong type: `int` with `float` (`exact` on a literal), `+` on text (`exact` to interpolation on names and literals), unlike types compared                                                                                                                                                       |
| `MZ0913` | `mz check`    | a chained comparison (`guess`), or `and` mixed with `or` without parentheses (`exact`)                                                                                                                                                                                                                           |
| `MZ0914` | `mz check`    | a malformed number: `1.`, `.5`, `1_000` (`exact`), `0x10`, `1e3`, a float literal too large for `f64`; a `.` after digits is a method call only before a letter or `_` (§3.1)                                                                                                                                    |
| `MZ0915` | `mz check`    | a fault visible in constants: division by a literal `0`, a literal negative index (`guess`), a literal negative `int` exponent, a folded overflow, `split("")`                                                                                                                                                   |
| `MZ0916` | `mz check`    | an expression statement whose value nothing reads (other than a `result`, which is `MZ0950`)                                                                                                                                                                                                                     |
| `MZ0917` | `mz check`    | a line or expression a function body cannot read: a value expected and something else found, a missing `)`, an unknown escape, a second `else`, or tokens left over on a statement line (claimed by Wave 0)                                                                                                      |
| `MZ0918` | `mz check`    | `+=`, `-=`, `*=`, `/=`, `++`, `--` — `exact` fix `x = x + 1`                                                                                                                                                                                                                                                     |
| `MZ0919` | `mz check`    | a form this RFC designs that the compiler does not build yet (lists, methods on text, records, an enum column, a program's `contract`, …), named, reported once, and its block skipped; each wave that builds a form retires it from this code (claimed by Wave 0)                                               |
| `MZ0920` | `mz check`    | a name used before its binding, or after its block ended; `guess` fix declares a `var` before the block                                                                                                                                                                                                          |
| `MZ0921` | `mz check`    | a name a program reserves: a record, enum or variant name, a built-in function's name (`print`, `range`), a contextual word where §1 bans it, or any name starting with `mz_` (shadowing is `MZ0713`)                                                                                                            |
| `MZ0922` | `mz check`    | assignment to a `let` (`exact` fix `var`), a parameter or a loop binding (`self` is `MZ0971`)                                                                                                                                                                                                                    |
| `MZ0923` | `mz check`    | assignment to an unbound name: inserts `let` or `var` (`exact` only with no near name and no read after the block), or the nearest name; `MZ0920` instead when read after the block (§5.1)                                                                                                                       |
| `MZ0924` | `mz check`    | a warning: a `var` never reassigned or mutated — `exact` fix `let`                                                                                                                                                                                                                                               |
| `MZ0925` | `mz check`    | a binding form from another language: `const` `val` `auto` (`let`), `let mut` `mut` (`var`), `x := e` — `exact`                                                                                                                                                                                                  |
| `MZ0926` | `mz check`    | a binding with no value                                                                                                                                                                                                                                                                                          |
| `MZ0927` | `mz check`    | a call's labels (§6.5): a positional argument after the first (`exact`), `name: v` (`exact` `name = v`), a label on the first argument (`exact` delete), an unknown label (nearest name), labels out of order, a label twice                                                                                     |
| `MZ0928` | `mz check`    | a `let` or `var` nothing reads, or a `use` line nothing needs (§5.1): the fix deletes the binding and every assignment to it, `exact` when each deleted value is a literal, name, path or bracket literal, a `guess` otherwise. Parameters and loop bindings are exempt                                          |
| `MZ0930` | `mz check`    | a `match` that misses a case: enum variants (named), `ok` or `error`, or `else` on `int` / `text`                                                                                                                                                                                                                |
| `MZ0931` | `mz check`    | a case that can never be reached: listed twice, after `else`, or `else` when every variant is covered (`exact` fix deletes it)                                                                                                                                                                                   |
| `MZ0932` | `mz check`    | a `when` or `match` used as a value where it cannot be, without `else` or exhaustiveness, with a branch that is not one value line, or with branches of different types                                                                                                                                          |
| `MZ0933` | `mz check`    | a conditional spelt from another language: `elif`, `else if` (`else when`), `switch` (`match`), `default:`, `case _`, `_ =>` (`else`) — `exact`                                                                                                                                                                  |
| `MZ0934` | `mz check`    | a loop spelt from another language: `for x in xs`, `for (const x of xs)`, `loop`, `range(n)` (`exact`), C-style `for`, `do … while`                                                                                                                                                                              |
| `MZ0935` | `mz check`    | `break` or `continue` outside a loop                                                                                                                                                                                                                                                                             |
| `MZ0936` | `mz check`    | an `else when` chain over one enum's variants (§7.1, CL-8): `guess` fix to `match`                                                                                                                                                                                                                               |
| `MZ0937` | `mz check`    | block punctuation from another language: a trailing `:` or `{`, a `}` (`exact`: the closer the open block needs), `} else {`                                                                                                                                                                                     |
| `MZ0938` | `mz check`    | a default spelt from another language: `??`, `.or(d)`, `.unwrap_or(d)`, `.get_or(d)`, `or` / `\|\|` after an option (`exact` `otherwise`); `otherwise` after a value that is not an option (`exact` delete) (§8.1)                                                                                               |
| `MZ0939` | `mz check`    | an option escape hatch the survey rejected: `x?.f`, a force-unwrap `x!`, `.unwrap()` or `.expect(…)` on an option; no fix, the `say` names `when x is not none` and `otherwise`                                                                                                                                  |
| `MZ0950` | `mz check`    | an unhandled `result`: discarded, used as its success type, printed or compared, returned where the type differs; a result inside a result; `guess` fix inserts `try` (or `try … via f`), or else a `match` stub                                                                                                 |
| `MZ0951` | `mz check`    | `try` that cannot propagate: not in a result function, a different error type (fix `via <f>`: `exact` when one `fn` fits, `guess` when several), or not on a result                                                                                                                                              |
| `MZ0952` | `mz check`    | an error idiom from another language: `Ok(v)` / `ok(v)` (`v`), `Err(e)` (`error(e)`), `throw` / `raise`, postfix `?` (`try`) — `exact`; a `try` block, `.unwrap()`, `.expect()` with no fix                                                                                                                      |
| `MZ0953` | `mz check`    | `error(e)` outside a result function, or with an `e` of the wrong type; a bare `ok` or `error` variant in a result function (`exact`: qualify it)                                                                                                                                                                |
| `MZ0954` | `mz check`    | `return try r` with `try r` the whole value and `r` of the function's own result type: `exact` fix deletes `try` (§12.1)                                                                                                                                                                                         |
| `MZ0955` | `mz check`    | a misused `via` (§12.2): without `try`, naming something that is not a `fn`, a `fn` whose signature does not fit; `via` where the error types already agree (`exact` delete)                                                                                                                                     |
| `MZ0960` | `mz check`    | a mutation of something that is not a `var` — `exact` fix `var` when it is a `let`                                                                                                                                                                                                                               |
| `MZ0961` | `mz check`    | a bracket literal that cannot be typed: `[]` with no context, mixed element types, entries mixed with elements, a repeated map key                                                                                                                                                                               |
| `MZ0962` | `mz check`    | an operation spelt another way: `len(x)`, `.len()`, `.size()`, `.length`, `.append`, `.strip()`, `str(x)`; `.contains` on a collection; `t in s` on text (`s.contains(t)`); `is []`, `length() is 0`, `.is_empty()` (`is none`) — `exact` on names and paths                                                     |
| `MZ0963` | `mz check`    | a tuple — name a record                                                                                                                                                                                                                                                                                          |
| `MZ0964` | `mz check`    | a map key or set element type with no order                                                                                                                                                                                                                                                                      |
| `MZ0970` | `mz check`    | a method declaration problem: `self` as a parameter (`exact`), `this.` (`exact` `self.`), a method named like a field, `self` outside one                                                                                                                                                                        |
| `MZ0971` | `mz check`    | assignment to `self` or a field of it, or a `changes self` call on either, in a method without `changes self`; `guess` fix adds `changes self` (§11.2)                                                                                                                                                           |
| `MZ0972` | `mz check`    | a declaration deferred past M1: type parameters, `class`, `interface`, `trait`, `impl`                                                                                                                                                                                                                           |
| `MZ0973` | `mz check`    | an enum payload misused (§11.5): a payload field read where the variant is not narrowed (`guess`: wrap in `when s is <variant>`), a payload that holds its own enum, `variants()` on an enum with payloads                                                                                                       |
| `MZ0974` | `mz check`    | a misused `with` (§11.1): not on a record, an unknown field (nearest name), a field named twice, `with ()` (`exact` delete), fields out of order (`exact` on literals, names, paths)                                                                                                                             |
| `MZ0975` | `mz check`    | a record's `always` clause that is not a `bool`, calls a user `fn` or method, or uses `try`; a literal construction its `always` folds to `false` (§11.4)                                                                                                                                                        |
| `MZ0976` | `mz check`    | a `changes self` method with a return type, or called where a value is expected (§11.2)                                                                                                                                                                                                                          |
| `MZ0980` | `mz check`    | a print spelt from another language: `print(a, b)` (`exact` to one interpolated text), `console.log`, `println!`, `fmt.Println`, `puts`, `print x`                                                                                                                                                               |
| `MZ0990` | `mz run`      | the lowered code did not compile: a compiler bug, reported with the `.mz` construct (not built yet: it names the program, §18.6) and `rustc`'s first message                                                                                                                                                     |
| `MZ0991` | `mz run`      | a trap (§4.3): integer overflow, division by zero, a negative `int` exponent, `xs[i] = v` out of range, a float out of `int`'s range, a broken `always` invariant, a non-finite float in `to_json()`                                                                                                             |
| `MZ0992` | `mz run`      | `main` returned an error (§12.5)                                                                                                                                                                                                                                                                                 |
| `MZ0993` | `mz run`      | the runtime failed: the lowered program panicked, or its thread could not start; exit 70 (§13.1)                                                                                                                                                                                                                 |
| `MZ0994` | `mz contract` | an in-process evaluation spent its 10,000,000 steps, or went 10,000 calls deep, without finishing (§15.1)                                                                                                                                                                                                        |

`MZ0929`, `MZ0940`–`MZ0949`, `MZ0956`–`MZ0959`, `MZ0965`–`MZ0969`, `MZ0977`–`MZ0979`,
`MZ0981`–`MZ0989` and `MZ0995`–`MZ0999` are left free, for the waves to claim within their
group. The survey amendment (§21) took `MZ0927`, `MZ0928`, `MZ0938`, `MZ0939`, `MZ0955` and
`MZ0973`–`MZ0976` from this list.

The survey amendment widens `MZ0962` without changing its row: `.get(i)` and `.get(k)` (`exact`
`xs[i]`, §3.7), `count()` with no argument (`exact` `length()`), and on a list `find(f)`,
`some(f)`, `every(f)` and `sorted` (`exact` to §9.4's folds). `MZ0927`'s reordering fix is `exact`
only when every argument is a literal, a name or a path (§6.5).

**Existing codes reused, so one kind of mistake keeps one code wherever it is made:** `MZ0101`
(a camelCase name, now also in function bodies), `MZ0103` (an `int` literal too large, including `int`'s minimum written as one, §3.1), `MZ0104`
(a character Mzizi does not use, for what is left after the operators above), `MZ0105` (a type
spelt with symbols), `MZ0106` (a spread, with the `exact` fix `p with (…)` in a program, §11.1), `MZ0407` (`if`, now also in function bodies), `MZ0701`
(an unknown type), `MZ0704` (a duplicate record, field or enum), `MZ0207` and `MZ0208` (an `end fn` with the wrong name, or a bare `end` closing a `fn`, §1), `MZ0308` (a record body line that is neither a field nor, in a program, a method), `MZ0707` (a name bound nowhere),
`MZ0708` (no such field, variant, column, **or method**: the nearest-name fix covers methods too),
`MZ0710` (an un-narrowed option, now also in `print` and `{…}`), `MZ0713` (shadowing, now also in function bodies, §5.3), `MZ0711` (a value of the wrong kind, including `for each` over
a map), `MZ0712` (a condition that is not a `bool`: truthiness), `MZ0714` (a malformed `{…}`),
`MZ0808` (a record literal or a payload variant's, with §11.1's fixes), `MZ0964` (also a
`sort_by` key or a record as a key), `MZ0613` (no evaluated contract clause), and RFC-0010's contract
codes.

All follow RFC-0001 §4: `say` at most 200 characters and quoting the source, deterministic order,
and one diagnostic per true error. Three rules keep the last true in expressions:

- **A sub-expression that fails is the error type** from then on, and adds no further
  diagnostic: `let n = undefined_name + 1` is one `MZ0707`, and every later use of `n` is
  silent, as RFC-0008 §6 already does for an unknown type.
- **A spelling fix repairs the tokens in place**, as `MZ0105` does: `a == b` reports `MZ0910` and
  the parser sees `a is b`, so the line produces no second diagnostic.
- **No two `exact` fixes overlap** (RFC-0008 §6), so `mz fix` applies them all in one pass. Where
  two mistakes share text (`if x == 1 {`), the outer fix stays `exact` and the inner ones are
  folded into it: one diagnostic, one fix, `when x is 1`.

**The idioms a small model brings, and their fixes** (RFC-0001 §4.7, extended): every `exact`
fix in the table above is one that applying blind produces the only program that could have
compiled, in RFC-0008 §5.2's sense. A fix that changes behaviour (inserting `try`) or chooses
between programs (renaming a shadowing binding) is a `guess`, always.

## 17. Canonical form

RFC-0001 §3's rule: one rendering of every program, and the compiler owns it. `mz fmt` (T2) does
not exist yet, so this is the form `mz fmt` will produce and that examples and guides must use.

| Construct          | Canonical form                                                                                               |
| ------------------ | ------------------------------------------------------------------------------------------------------------ |
| a program          | `program <name>` … `end program <name>`; two-space indent; one blank line between top-level blocks           |
| order              | doc, `use`, `enum` / `record`, `fn main`, other `fn`s in source order, `test`, `contract` (§1)               |
| a signature        | `fn name(a: int, b: text): int`: no space before `(` or `:`, one after `,` and `:`; no `()` on no parameters |
| a function's end   | `end fn <name>`                                                                                              |
| a binding          | `let x = e`, `var x = e`; an annotation only where the value does not fix the type                           |
| an assignment      | `x = e`, `x.f = e`, `xs[i] = e`                                                                              |
| binary operators   | one space either side; unary `-` with no space; `not` and `try` followed by one space                        |
| parentheses        | only where precedence needs them, and always where `and` meets `or`                                          |
| calls and methods  | `f(a, b = e)`, `x.m(a)`: no space before `(`, one after each `,` and around `=`                              |
| a default          | `port: int = 80`, one space either side of `=`                                                               |
| `with`             | `p with (x = 1.0)`, one space either side of `with`                                                          |
| `otherwise`        | `x otherwise d`, one space either side of `otherwise`                                                        |
| `changes self`     | `fn bump changes self`, after the parameters, where a return type would stand                                |
| literals           | `[1, 2, 3]`, `["a": 1]`, `[]`; floats with no trailing zeros past the first (`1.0`, `1.5`, not `1.50`)       |
| a record value     | `point(x = 1.0, y = 2.0)`, fields in declaration order                                                       |
| `when`             | `when`, `else when`, `else` and `end` at one indent; branch bodies two deeper                                |
| `match`            | `match` and `end` at one indent; `case` and `else` two deeper; case bodies two deeper again                  |
| a loop             | `for each x in e` / `while c`, body two deeper, bare `end`                                                   |
| a record's methods | after its fields, one blank line between them; its `contract` (with `always`) last                           |
| a contract         | last inside its `fn`, or last inside the program                                                             |
| text               | `"…"` with the escapes of §3.1; `{e}` with no spaces inside the braces                                       |

## 18. Implementation plan

The waves are issue #69's. Each row is one pull request, on its own branch, targeting `staging`.
Each one meets its tracker row's "Done when", updates that row and the CHANGELOG, adds an
`examples/*.mz` program with its `.expected` output, gives every construct it adds its codes and
`exact` fixes, and passes the AGENTS.md checks. Each wave's features register their language-harness
entries as part of being built (RFC-0012 §1.2): a feature is not built until its entry, codes and
tested examples are in `compiler/src/harness.rs`. None claims a measured result. A row turns ✅ only
when its "Done when" test is on `main` and green (`LANGUAGE-TRACKER.md`).

### 18.1 Wave 0, the foundation slice (serial)

One pull request that every later wave builds on. It establishes the module layout, the way
`service` did:

- `compiler/src/program.rs`: the program tree and its checker (types, scope, paths).
- `compiler/src/parse/program.rs`: the parser for the `program` block, `fn`s and statements.
- `compiler/src/expr.rs`: the expression tree, its parser (precedence, §3.5) and its type rules,
  kept apart from `program.rs` so that components and services can adopt expressions later.
- `compiler/src/run.rs`: the lowering of a program (§14) and `mz run` (§13).
- `compiler/src/eval.rs`: the in-process evaluator `mz contract` uses for a program's `example`
  clauses (§15.3).
- The lexer gains the operator tokens, `[` `]`, escapes and float literals. In a component or a
  service, an operator token where the grammar has none is still `MZ0104`, with today's text, so
  no existing diagnostic changes.

These names are proposals; the pull request fixes them and says so here.

**In the slice:** `program`, `fn main`, `fn` with parameters and a return type, `let`, `var` and assignment, `int` and
`bool`, `+ - * / %`, unary `-`, `is`, `is not`, `< <= > >=`, `and`, `or`, `not`, calls,
recursion, `when` / `else` as statements, `return`, `print` of text literals with `{name}` and
`{call(…)}` interpolation, integer traps (§4.3), the lowering, `mz run`, `mz build` for a program,
and CI running `examples/*.mz` programs through `mz run` against `.expected` files. Codes:
`MZ0901`–`MZ0908`, `MZ0910`, `MZ0912` (`int` and `bool`), `MZ0915`, `MZ0916`, `MZ0920`–`MZ0926`,
`MZ0980`, `MZ0990`, `MZ0991`, `MZ0993` (the generated `main`, §13.1) and `MZ0994` (the evaluator's
step bound, §15.1), with `MZ0713` and `MZ0208` reused in function bodies.

| Row | Done when (`LANGUAGE-TRACKER.md`), and the test that shows it                                                                                                                                             |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C10 | `mz run hello.mz` prints output: `examples/hello.mz` prints `hello, world`, and CI's run of it matches `examples/hello.expected`                                                                          |
| C3  | one function calls another with arguments and returns a typed value; recursion works: an example in Wave 0's subset (§1's `fib`, without loops) prints `fib(9) is 34`, and `MZ0905` / `MZ0906` have tests |
| C2  | a value can be named and reused; the resolver reports use before binding: `let` in the example, and `MZ0920`, `MZ0713` and `MZ0921` triggered by tests                                                    |

C1 and C5 become 🟡 after this slice (integers only). The P3, P4 and P10 rows gain evidence, and
stay 🟡 or 📝 until Tier 1 lowers whole.

### 18.2 Wave 1 (parallel, on the foundation)

| PR      | Builds                                                                                                                                                                                         | Done when, and its test                                                                                                                                                                                |
| ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| C1 + C5 | `float` and §4 whole, interpolation of any expression, §3.8's text forms, the rest of §3.5, `MZ0911`, `MZ0913`, `MZ0914`, `MZ0918`                                                             | C1: an example computes and prints a value from an expression, and `mz check` types it. C5: `float` exists, and tests pin overflow (exit 101, `MZ0991`), integer and float division by zero, and `nan` |
| C4      | `else when`, `match` and exhaustiveness, the variant-chain check (`MZ0936`), `when` / `match` as values, `for each` and `while` in function bodies, `break`, `continue`, early `return`        | all four work in a function body, and a non-exhaustive `match` is `MZ0930`; `MZ0930`–`MZ0937` each have a test                                                                                         |
| C9      | `result`, `error(…)`, `try`, `case ok` / `case error`, `main` returning a result, `MZ0950`–`MZ0954`, `MZ0992`. Rebases on C4 for `match`                                                       | a function returns an error that its caller handles with `match` and another propagates with `try`, and an unhandled error is `MZ0950`                                                                 |
| C7      | bracket literals, `map`, `set`, option indexing (§3.7), §9.2's operations, `in`, `map` / `filter`, §9.4's eight folds, `range`, `MZ0909`, `MZ0960`–`MZ0964`; `MZ0105` moves to the type parser | `map(K, V)` exists, and an example builds a list and a map and transforms them in a function                                                                                                           |
| C8      | §11 whole: construction (`MZ0808`), `with`, equality, text and JSON, methods, `self`, `changes self`, `always`, enum payloads and `variants()`, `MZ0970`–`MZ0976`. Rebases on C4 for `match`   | **a record has a method**. The row stays 🟡: its generic-function and interface clauses are deferred (§11.3; §20 Q8, Q21)                                                                              |

C7's tests also show that `xs[i]` out of range is `none` and each fold on an empty and a non-empty
list; `MZ0105`'s tests stay unchanged. C8's also show that a missing field is `MZ0808`, that `with`
leaves its source unchanged, and that a broken `always` exits 101.

All five touch `expr.rs`. The order they merge in is the owner's; each rebases on whatever merged
before it, and none rewrites another's codes.

**C7 and C8 are built after the survey amendment (§21)**, so their pull requests build the
amended design directly. Rows that were built, or in flight, before it take the amendment as
follow-up pull requests, each on top of its row's work, so that no in-flight branch has to
change course:

| Follow-up   | Builds                                                                                                                                                                                     | Done when, and its test                                                                                                                                                                                |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| C3 labels   | §6.5's labels and literal defaults, `MZ0927`; `MZ0903` and `MZ0905` lose Wave 0's "a named argument" text                                                                                  | a call with a default left out and one with labels both run; `mz fix` turns `area(3.0, 4.0)` into `area(3.0, height = 4.0)`, and the result checks clean                                               |
| C2 unused   | `MZ0928`, with its `exact` and `guess` deletions                                                                                                                                           | an unread `let` is `MZ0928`, `mz fix` deletes it, and an unread loop binding or parameter is not reported                                                                                              |
| C4 options  | §8 whole: `option(T)` in function bodies, `when x is not none`, the guard that narrows the rest of its block, `otherwise` (§8.1), `MZ0938`, `MZ0939`; the draft's `x.or(d)` is never built | a guard `when x is none … return … end` narrows `x` for the rest of its block; `otherwise` evaluates its right side only on `none` (a test with a trapping default); and `x ?? d` gets the `exact` fix |
| C9 `via`    | `try … via f`, `MZ0955`, `MZ0951`'s `via` fix and `MZ0950`'s `match` stub                                                                                                                  | a function propagates an error of another type through `via`, and `mz fix` inserts `via` when one `fn` fits                                                                                            |
| C5 wrapping | `wrapping_add`, `wrapping_sub`, `wrapping_mul`, and `a // b` as `MZ0910`                                                                                                                   | the three wrap at `int`'s limits without a trap                                                                                                                                                        |

Where the row's own pull request has not opened when this amendment is accepted, it builds the
follow-up itself instead. No committed example calls a function with two or more arguments
(`examples/hello.mz` and `examples/fib.mz` were checked), so the C3 follow-up changes no example;
the tests in `compiler/tests/program.rs` that call `add(a: int, b: int)` positionally move to
labels in that pull request.

### 18.3 Wave 2

| PR  | Builds                                                   | Done when, and its test                                                                                       |
| --- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| C6  | §10's text operations, with their lowering table         | the operations exist with tests: one test per method, including the scalar-value cases and `parse_int`'s rule |
| T4  | `test` blocks, `expect`, `mz test`, lowered to `#[test]` | ordinary test functions run with `mz test`                                                                    |

Whatever Tier 1 has left after Wave 1 is listed in #69 and lands here.

### 18.4 What CI gains

The `compiler` job's `mz check` and `mz contract` loops already cover every `examples/*.mz` file,
so a program is checked and its contract evaluated with no change. The `lowering` job gains one
step: for each `examples/*.mz` file that holds a program, `mz run` it and compare its standard
output with `examples/<name>.expected`. Since a program's package has no dependencies, that step
needs no network.

### 18.5 What the language harness gains

Every construct and code here reaches an agent through the language harness (RFC-0012, H1), and
through `benchmarks/prompts/mzizi-guide.md` until the language harness serves it. This RFC does not change the
guide, because nothing it describes exists. The pull request that makes a construct real changes
the guide, re-measures it in `BUDGET.md`, and says that no benchmark was re-run.

### 18.6 Implementation record

Updated by each pull request that implements part of this RFC.

**Wave 0, the foundation slice** (Refs #69; stacked on this RFC's pull request). Built and tested
in `compiler/tests/program.rs`; nothing measured.

- **Modules.** §18.1's proposals, with one change: the expression parser lives with the
  statement parser in `compiler/src/parse/program.rs`, because both read one token cursor;
  `compiler/src/expr.rs` holds the expression tree, its canonical text (§17), constant folding
  and the operator typing table. `compiler/src/program.rs` is the tree and the checker,
  `compiler/src/run.rs` the lowering and `mz run`. There is no `eval.rs`: a program's
  `contract` block is not built, so `mz contract` has no clause to evaluate (§15.1, §15.3).
- **The lexer** reads operators and string escapes only in a file whose first line is
  `program …`, so a component or a service lexes exactly as before. `[` `]` and float
  literals are not lexed yet.
- **Built:** `program`, `fn main`, `fn` with typed parameters and a return type, calls and
  recursion, `let`, `var`, assignment, `when` / `else` as statements, `return`, `print`,
  `int`, `bool` and `text` values, `+ - * / %`, unary `-`, `is`, `is not`, `< <= > >=`, `and`,
  `or`, `not`, `{expr}` interpolation and §3.1's escapes, integer traps (§4.3), the lowering
  (§14), `mz run` (with `--release`) and `mz build` for a program, and CI's `mz run` of
  `examples/*.mz` programs against `examples/<name>.expected` (§18.4).
- **Codes emitted:** `MZ0901`–`MZ0910`, `MZ0911` (a line that starts with `//` or `#` only),
  `MZ0912`, `MZ0913` (chained comparison only),
  `MZ0915`, `MZ0916`, `MZ0918`, `MZ0920`–`MZ0926`, `MZ0937` (a trailing `:` only), `MZ0980`,
  `MZ0990` and `MZ0991`, beyond §18.1's list where an idiom needed its code to stay one
  diagnostic. **Claimed** from §16's free range: `MZ0917` (a line a function body cannot read)
  and `MZ0919` (a designed form not built yet), now in §16's table.
- **Where the code departs from this text, the code is the fact.** §13.1 step 2's cache is
  per user, `<name>-<path hash>` under `$MZ_CACHE_DIR`, or `mzizi/mz-run/` under
  `$XDG_CACHE_HOME` or `~/.cache`, with no fallback to the system's temporary directory: a shared temporary directory let
  another account plant a `build.rs` that `cargo` would run, and a directory per content hash
  made every edit a cold build. Keyed by the source path, Cargo's fingerprint decides what an
  unchanged program skips. §4.1's rule is read exactly: `int` minimum `% -1` is 0 and does not
  trap; only a zero divisor traps a remainder. `mz run` prints warnings to standard error
  before it runs, and a `cargo` failure that is not `rustc` rejecting the lowered code exits 2,
  not `MZ0990`.
- **Nesting is capped, as for components and services (RFC-0001 §4.7 item 8), with `MZ0411`
  once per file.** A program's blocks and expressions share one budget of 32 levels: the
  program, the `fn`, each `when` (and `else when`), and each expression read inside
  another: a statement's value, a `(`, a call's arguments, an interpolation, a `not`, a
  prefix `-`. An expression's tree of binary operators may be 64 deep, so a chain such as
  `1 + 1 + …` or `a < b < …` may be about 64 long. Half the cap a component gets, because each level costs
  the parser and the checker several frames: in a debug build a level took 12 to 16 KiB of
  stack, and the robustness tests hold every case to a 1 MiB stack. Past the cap a `when` is
  skipped to its `end`, and the rest of an expression's line is not read, both without
  recursion. 100,000 nested parentheses, `not`s or prefix `-`s, and 5,000 nested `when`s,
  each give one `MZ0411` (`compiler/tests/robustness.rs`).
- **One diagnostic per true error, in three more places.** A keyword written as a
  parameter's or a function's name (`fn twice(match: int)`, `fn nothing`) is one `MZ0903`:
  the parameter still counts toward the function's arity and the function is still
  declared, so a call is not also `MZ0905`, and a use of the keyword as that name is not
  also `MZ0917`. An `int` literal too large (`MZ0103`) reads as an error value, not as a
  missing one. `MZ0911` is built for a line that starts with `//` or `#`, with the `exact`
  fix `##` (a `guess` when code follows the marker with no space, as in a `#!` shebang or `#[inline]`), and a file that opens with one is still read as a program; `/* … */` and a
  comment after code on the same line are not built.
- **Not built:** `mz run --agent` (it exits 2 and says so), a trap or `MZ0990` as NDJSON,
  `MZ0990` naming the `.mz` construct (it names the program and quotes `rustc`'s first
  message), the guide change of §18.5, and everything outside §18.1's list.

**Wave 1, C1 + C5: numbers and the rest of §3 that needs no collection** (Refs #69; stacked
on Wave 0's pull request). Built and tested in `compiler/tests/program_numbers.rs` and
`compiler/src/numbers.rs`; nothing measured.

- **Modules.** `compiler/src/numbers.rs` holds §4.4's method table, the `int` folding of
  `abs`, `min`, `max` and `pow`, and `numbers/float_text.rs`, §3.8's float algorithm. That
  one file is compiled into `mz` (where its unit tests run) and emitted verbatim into every
  lowered `main.rs`, so a float's text in a diagnostic and in a program's output come from
  one implementation. The `int` `pow` rule is not shared that way: the checker's
  `numbers::int_pow` (for `MZ0915`) and the lowering's `mz_pow` (for the trap) are two
  copies, each pinned by its own tests.
- **Built:** `float` literals, the type and its IEEE 754 arithmetic (§4.2); §4.4 except the
  `wrapping_*` methods (the C5 wrapping follow-up, §18.2), as methods (postfix, §3.5 level 2); §4.3's `pow`, `abs`, negative-exponent and `to_int`
  traps; §3.8's float text form; `MZ0912` for `int` with `float`; `MZ0913` for `and` mixed
  with `or`; `MZ0914`; `MZ0911` for `/* … */` and for a comment after code; `MZ0910` for
  `not a is b`; `MZ0962` for `str(x)`, `x.to_string()` and Python's free numeric functions;
  `MZ0708` for a method a number does not have; `MZ0905` for a method's arguments; and
  `MZ0915` for a literal negative `int` exponent and a folded `pow` or `abs` overflow.
  Methods on numbers and `float` are retired from `MZ0919`; methods on `text` are not (§10).
- **Where the code departs from this text, the code is the fact.**
  - `**` is lexed in a program and is `MZ0910`, with the `guess` fix `x.pow(n)` (a `guess`:
    `pow` takes an `int` exponent and `**` any number). The tree reads it as `pow`, so the
    line is still type-checked, and `a ** b ** c` is one diagnostic. A `float` exponent is
    then `MZ0905`: a whole-number literal gets the `guess` `int` (`2.0` to `2`), `0.5` the
    `guess` `x.sqrt()`, and any other exponent no fix, since no `int` means the same.
  - `MZ0911` for a comment after code (`x = 1 // note`, `# note`, `/* note */`) has a
    `guess` fix that moves the comment to a line of its own above the code, since after code
    `//` may also be Python's floor division. An unclosed `/*` has no fix. Code after a
    `/* … */` that closes on its line (`/* temp */ let x = 1`) is still read.
  - `x.to_string()` is `MZ0962` on any value with a text form, with the `exact` fix
    `"{x}"`, or `x` itself on a `text`.
  - `MZ0914` also covers a float literal too large for `f64` (no fix); §3.1 names no code
    for it.
  - `not a is b` is `MZ0910` only without parentheses: `not (a is b)` says what it means
    and is left alone. `!a == b` is not this case, because `!` binds to `a` in the languages
    that write it; it gets its two `MZ0910`s as before.
  - `x.min(y)` and `x.max(y)` on floats are Rust's `f64::min` and `f64::max`, so
    `nan.min(1.0)` is `1.0`. §4.4 does not say what `min` does with `nan`; this is §20's to
    decide.
  - `f.to_int()` on a float literal out of `int`'s range is not `MZ0915` at check time;
    it traps when it runs. §16 lists no such constant fault.
  - `str(x)` on a `text` has the `exact` fix `x` itself, since `"{"x"}"` cannot be
    written (§3.6). `String(x)` reaches the checker as the lexer's `string`, so it is
    `MZ0101` and `MZ0962`, as `String` as a type already was.
  - A float literal's canonical text (§17) is always plain decimal, never an exponent, so
    it reads back as the same literal: `1000000000000000000000.0`, not `1.0e21`.
  - §3.8 does not say how a tie breaks. When a float's exact value is halfway between two
    shortest digit strings, its text form takes the one Rust's `{:e}` gives (the upper);
    JavaScript and Python take the even one (`608898711247163.25` prints
    `608898711247163.3` here, `608898711247163.2` there). Both read back as the same
    value. A diagnostic or trap quotes a float literal in this canonical form, not as
    written (`0.30000000000000001 + 1` is quoted as `0.3 + 1`).
  - Python's `round(x)` and `pow(a, b)` are `MZ0962` with a `guess` fix, not an `exact`
    one, since `.round()` and `int.pow` differ from them (ties to even; a negative
    exponent). `pow` is `exact` only when the exponent is a non-negative `int` literal.
    `str(x)` on a value with no text form has no fix. `MZ0914` quotes at most 20 characters
    of a long literal, so its `say` stays within RFC-0001's 200.
  - `MZ0913`'s fix, and `not a is b`'s, fold the `exact` idiom fixes inside them (`&&`,
    `==`, a nested `MZ0913`) into one fix, so two `exact` fixes never overlap; a `guess`
    inside makes the covering fix a `guess`.
- **Not built:** `in`, indexing, `[ … ]` and the collection forms of §3 (C7), text methods
  (C6), records and their text form (C8), and the guide change of §18.5.

**Wave 1, C4: control flow in function bodies** (Refs #69; #89, after Wave 0, #80,
and C1 + C5, #83; on `main` since 2026-10-08, #91).
Built and tested in `compiler/tests/program_control.rs`, with `examples/control.mz` run in CI
against `examples/control.expected`; nothing measured.

- **Modules.** Each of Wave 0's modules gains a child for §7: `parse/program/control.rs`
  (the parser), `program/control.rs` (the checker, and which statements end a path) and
  `run/control.rs` (the lowering). The trees gain `StmtKind::Match`, `For`, `While`, `Break`
  and `Continue`, an `else_whens` list on `StmtKind::When`, and `ExprKind::Variant`, `When` and
  `Match`; `Ty` gains `Enum`, which carries its enum's name interned once per process.
- **Built:** `else when` (§7.1), `match` over an enum, `int`, `text` or `bool` (§7.2),
  `for each` over `range(a, to = b)`, `while`, `break` and `continue` (§7.3), `when` and `match` as
  the value of a `let`, `var`, assignment or `return` (§7.4), early `return` from any of them,
  and a program's `enum` (§1), whose variants are written bare or as `<enum>.<variant>`, print
  as their names and order by declaration. Codes emitted: `MZ0930`–`MZ0936`, `MZ0937` on
  every new block line, and `MZ0927` for `range`'s label alone (below).
- **Retired from `MZ0919`:** `while`, `for each`, `match`, `else when`, `break`, `continue`,
  `loop` (now `MZ0934`) and a program's `enum`.
- **Where the code departs from this text, the code is the fact.**
  - _A program's `enum` lists one variant name per line and closes with a bare `end`_, as a
    component's does; a variant with columns (§14.2's accessor table) is `MZ0919` (C9, below,
    builds columns). Rust's
    `end enum x` echo is `MZ0206`, `exact` to `end`. A variant may be named `ok`, `error`,
    `float`, `map`, `set` or `result` (§1); the other contextual words, the built-in names and
    an `mz_` prefix are `MZ0921`, and a variant listed twice is `MZ0704`, `exact` deleting
    its line.
  - _A bare variant resolves against the type expected where it stands_ (RFC-0008 §5): the
    other side of a comparison, a `match`'s value, an annotated `let` or `var`, the `var` an
    assignment changes, the function's return type, and the parameter an argument fills.
    Everywhere else (an unannotated `let`, a branch of a block used as a value, an
    interpolation, `print`) it must belong to exactly one enum; one two enums share is
    `MZ0708` there, with a `guess` fix naming its enum. When both sides of a comparison are
    bare variants, one that belongs to exactly one enum is the other side's expected type,
    in the checker and the lowering alike: with `light { blue, amber }` and
    `color { red, blue }`, `amber is blue` is `light.blue`. Two that both enums share
    (`blue is blue`) are one `MZ0708`, on the left, since naming its enum settles the right.
    `MZ0708`'s `say` lists each variant once, even one listed twice (`MZ0704`), and so does
    `MZ0930`'s.
  - _`for each` iterates `range(a, to = b)` only_, because lists are C7's. `range` anywhere else
    is `MZ0919`, and `for each` over an `int` is `MZ0711` with the `guess` fix
    `range(0, to = n)`. `range`'s `to` is the one label the parser reads, and the canonical
    text writes it; §6.5's labels are not built, so the positional `range(a, b)` is still
    read, and lowers the same, until the labels pull request makes it `MZ0927`. `to: b` is
    already `MZ0927`, §16's `name: v`, with the `exact` fix `to = b` and no second
    diagnostic for the `:`. The `int` guess is not written over a source the lexer cut short:
    after a character it dropped (`MZ0104`, as `[` is until C7) or a type it respelt
    (`MZ0105`), the source is an error value, so `for each k in [1, 2]` is its two `MZ0104`s
    alone, and TypeScript's `for (const k of [n])` is `MZ0934` with no rewrite.
  - _A `case` lists literals (an `int`, a negative `int`, a `text` with no interpolation, a
    `bool`) or variants._ Any other value is `MZ0917`; one of another type is `MZ0912`. Every
    `MZ0931` has an `exact` fix: a value listed twice loses that value, a case whose every
    value an earlier case takes and a case after `else` lose their lines, as §7.2's covered
    `else` does. A case value already reported (a misspelt variant) suspends the coverage
    verdict, so the `match` is not also `MZ0930` for the same mistake.
  - _A `match` without `else` ends a path when its cases all do._ One that misses a case is
    `MZ0930` alone: it is neither `MZ0906` (a path without `return`) nor a reason to call the
    line after it unreachable (`MZ0907`).
  - _`MZ0936` reads `<name> is <variant>` only_: `<e> in [<variants>]` waits for lists (C7)
    and a path through a field waits for records (C8). Its `guess` fix rewrites the chain in
    canonical form (§17), so a `##` comment inside the chain is not kept. A `when` used as a
    value (§7.4) is read the same way, and its fix is a `match` used as a value, from `when`
    through the `end`.
  - _`MZ0924` waits while lines of a function were skipped unread_ (a `do … while`, a
    C-style `for`, a form `MZ0919` names, a block past the nesting cap): those lines may
    assign the `var`, so neither the warning nor its `exact` fix to `let` is given until they
    are rewritten.
  - **`MZ0933` also repairs Rust's `_ => <statement>` on one line**, moving the statement to
    the line after `else`. A `match` on a `result` (`case ok <name>`) is C9's.
  - _`MZ0934`'s TypeScript fix_ covers `for (const|let|var x of xs)`, `exact`, and the same
    rewrite of `for (… in xs)`, a `guess`, since TypeScript's `in` iterates keys. A C-style
    `for (…; …; …)` and a `do` line have no fix; the block is skipped to its `end`, or to
    `do`'s `while` line, and never past `end fn`, so a loop written with braces costs no
    more than its own lines.
  - _A `break` or `continue` outside every loop is `MZ0935` alone_: it ends no path, so the
    line after it is not `MZ0907`.
  - _`MZ0937`'s `{` and `}`_ are not built: the lexer still reports a brace as `MZ0104`.
  - _Nesting (§18.6, Wave 0):_ each `while`, `for each` and `match`, and each `when` or
    `match` used as a value, is one level of the program's 32; an `else when` is none, since
    the chain is flat. Wave 0 counted each `else when` as a level, so a chain of about 30
    links was `MZ0411`; 10,000 links now check and lower on a 1 MiB stack.
  - _Lowering:_ `while true` is `loop`, so `rustc` agrees that a `while true` no `break`
    leaves ends every path. An enum's PascalCase name that is a Rust prelude or derive name
    (`Vec`, `Option`, `Copy`, …) gets `MzUser` before it, and two names that PascalCase to one
    (`a1` and `a_1`) are told apart by a numbered `MzUser` name.
  - _A `match` over a `float` is `MZ0711`_, naming `when`: §7.2 lists an enum, an `int`, a
    `text` and a `bool`, and a float literal in a `case` of an `int` `match` is `MZ0912`.
    `float` values otherwise work in every C4 form (#83 merged first).
  - _A `fn` named like a variant is `MZ0921`_ at the `fn` (§16's "a variant name"), and a
    bare use of the name still reads as the variant. A name bound elsewhere in the function
    (a block that ended) is never read as a variant, so it is `MZ0920`, not `MZ0708`.
  - _A `for each` binding read after its loop is `MZ0920` with no fix_: the `var` its
    block-ended fix would insert before the loop clashes with the loop's binding.
  - _A bare `default` alone on a branch line of a `match` used as a value is that branch's
    value_ (a binding may be named so); `default:` is still `MZ0933`. A case written after
    the `else` stays after it in canonical text (§17), so `MZ0936`'s rewrite never makes it
    run.
- **The language harness** (RFC-0012 §1.2): each construct above has a feature entry in
  `compiler/src/harness.rs`, with a runnable example, and `MZ0930`–`MZ0936` each have a code
  entry with a trigger. `Ty` is not a `listed_enum!`, since `Ty::Enum` carries a name:
  `Ty::ALL` lists the built-in types by hand, and an enum is not a surface type there; the
  `enum` entry registers it.
- **Not built:** `for each` over a list or a map, `x in [variants]`, a `match` on a result
  and enum columns in a program (both since built by C9, below), block braces, §18.2's
  "C4 options" follow-up (§8 whole:
  `option(T)` in function bodies, `when x is not none`, guards, `otherwise`, `MZ0938`,
  `MZ0939`), and the guide change of §18.5.
- **One `match` for C9 to extend (#87).** C9's pull request built its own `match` on a
  result (`ResultMatch`, in `program/errors.rs`) and its own program `enum` with columns.
  This one was meant to absorb both on whichever rebased second, and C9 did so (below): a result `match` is a
  `StmtKind::Match` whose scrutinee has a result type, whose cases are two more keys
  (`ok`, `error`) in `program/control.rs`'s coverage check, with the universe `{ok, error}`
  and no `else` allowed, so `MZ0930` and `MZ0931` come from one place; each `Arm` gains the
  name its case binds. C9's `EnumDecl`, which has columns, replaces this one's, and the
  lowering of both is one fieldless Rust `enum`. Until one of the two pull requests rebases
  on the other, the two `match`es and the two `enum` declarations coexist only across
  branches, never in one tree.

**Wave 1, C9: errors (§12)** (Refs #69; PR #87, after C4, #89, and the language
harness, #88; on `main` since 2026-10-08, #91). Built and tested in `compiler/tests/program_errors.rs`, with
`examples/errors.mz` run through `mz run` against `examples/errors.expected`; nothing
measured.

- **One `match` and one `enum`** (owner direction: the language has one of each). A
  `match` on a result is C4's `StmtKind::Match` (and `ExprKind::Match` used as a value) over
  a result-typed value: each `Arm` carries the name its case binds (`Arm::binding`), and
  `program/control.rs`'s coverage check gains the keys `ok` and `error`, with `{ok, error}`
  as the universe, so `MZ0930` and `MZ0931` come from one place, with C4's messages naming
  `case ok` and `case error`. C9's `EnumDecl`, which has columns, replaced C4's; the enum is
  still C4's one fieldless Rust `enum`, with C4's Rust names, and C9 adds an accessor per
  column. The "non-result `match` is not built" path (`MZ0919`, `MZ0711`) is gone.
- **Modules.** `program/errors.rs` (the checker's half of §12, and the enum's tree types),
  `parse/program/errors.rs` (columns, `result(…)`, prefix `try`, an enum value's column and
  `MZ0952`), `run/errors.rs` (columns, `Result`, `?` and `mz_main_result`) and `intern.rs`
  (a result's two types, interned so `Ty` stays `Copy`; an enum's name is C4's
  `expr::intern`).
- **Built:** a variant's literal columns (`say "…"`, an `int` or a `bool`), every variant
  with the same ones, read with a dot (`problem.say`); `result(T, E)` and `result(none, E)`
  as a return type or a `let`'s; `return error(e)`; `return r` passing a result of the
  function's own type through; prefix `try` (§3.5 level 3); a `match` on a result with
  `case ok <name>` and `case error <name>`, both required, as a statement or as a value;
  `main` returning `result(none, E)`. The lowering of §14.2's rows for them:
  `Result<T, E>`, `Ok(v)`, `Err(e)`, `?`, `Ok(v)` and `Err(e)` patterns in C4's Rust
  `match`, one accessor method per column, and no `unwrap`, `expect`, `panic!` or `unsafe`.
- **Codes emitted:** `MZ0950` (discarded, used as its success value, compared, printed,
  interpolated, passed, assigned, a condition, returned where the type differs, a result
  parameter, a result in a result, a result in a `var`, and a `let` whose result nothing
  reads before its block ends; the `guess` fix `try` where the function returns the same
  error type), `MZ0951`, `MZ0952` (`Ok(v)` and `ok(v)`, `Err(e)` and `err(e)`, postfix `?`,
  `throw e` and `raise e`, all with their fixes; a `try` block, `.unwrap()` and `.expect(…)`
  with none), `MZ0953`, `MZ0954`, and `MZ0992`; `MZ0930`, `MZ0931` and `MZ0917` for a
  `match` on a result. Existing codes in their RFC-0001 meaning for columns: `MZ0302`,
  `MZ0303`, `MZ0704`, `MZ0711`. No code is claimed, and none clashes with the codes the
  survey amendment took (`MZ0927`, `MZ0928`, `MZ0938`, `MZ0939`, `MZ0955`,
  `MZ0973`–`MZ0976`).
- **The language harness** (RFC-0012 §1.2): `compiler/src/harness.rs` gains a feature
  entry for `result`, `error`, `match on a result`, `main returning a result` and
  `postfix ?` (the lexed spelling, whose entry is the `exact` fix target), the prefix
  operator `try` (derived from `UnOp`), the `enum` entry's columns, and a code entry for
  each of `MZ0950`–`MZ0954` (each with a trigger) and `MZ0992` (none: `mz check` cannot
  report it, so `compiler/tests/harness.rs`'s list of untriggered codes grows to
  `MZ0990`, `MZ0991`, `MZ0992`). `examples/errors.mz` and two more programs are its
  runnable examples, run with their output compared.
- **Where the code departs from this text, the code is the fact.**
  - _In a `match` on a result, `else` is not allowed_ (§12): any `else` is `MZ0931`, whose
    `exact` fix deletes it, and a `case error` written after it still counts, so
    `case ok x` / `else` / `case error e` loses only its `else`. A missing case is still
    `MZ0930`, at the matched value as for every `match`. Everywhere else C4's rule holds: a
    case after `else` is unreachable.
  - _A case line is `case ok <name>` or `case error <name>`_. The parser reads the second
    word as the binding only when no enum of the program has it as a variant, since no
    binding may take a variant's name (`MZ0921`): with `enum status { ok, error, … }`,
    `case ok error` still lists two variants. Any other case line in a `match` on a result
    is one `MZ0917`, and the names it lists are quiet in its arm; a binding in a `match`
    that is not on a result is one `MZ0917` too. `case ok` binds no name when the success
    is `none`, and with no name on a success that is a value it is `MZ0917`.
  - §12.5's line reads `mz: error MZ0992: main returned an error: <text form>`, not
    `error: <text form>`, so it names its code as a trap's line does (`mz: trap MZ0991 …`).
    The generated `mz_main` writes it and exits 1 itself, around the program's `main`
    (lowered as `mz_main_result`), so the generated `main` of §13.1 is unchanged rather than
    matching `Ok(Err(e))` from `join()`.
  - A bare variant resolves as C4 resolves it, against the type expected where it stands;
    `error(e)`'s argument is read against the function's error type. A bare `ok` or `error`
    variant in a function that returns a result is `MZ0953`, with the `exact` fix naming its
    enum.
  - `throw e` and `raise e` get an `exact` fix only when the parser can see that `e` has the
    function's error type: a variant of its error enum, bare or qualified, or a literal of
    its error type. Otherwise the fix is a `guess`. `throw Error(e)` is fixed to
    `return error(e)`, not `error(error(e))`, and `throw new Error("bad")`, which is not one
    expression, is one `MZ0952` with no fix.
  - `var r = f()` holding a result is `MZ0950`, with the `guess` fix `let`, and not also
    `MZ0924`: §12.3 lets a `let` hold a result, and a `var` that can be reassigned before the
    `match` would hide which result was matched.
  - `error(e)` is a value of the function's own result type, so it may be bound (`let r =
error(e)`) as well as returned.
  - A `try` block is recognised by `try` ending its line (after a `:` or a `{`, which is not
    a token); the lines indented past it, and `catch`, `except` and `finally` lines, are
    skipped as part of the one `MZ0952`.
  - In a program, `?` is lexed as an operator, so the parser can repair a postfix `?`, whose
    `exact` fix writes `(try f(x)).y` before a dot, since a dot binds tighter than `try`.
    After a type (`int?`) it is still `MZ0104`, with the component's text. A chain of
    postfix `?`s and `.name`s is capped at 64 links, with `MZ0411` past it.
  - A dot without parentheses after a value is an enum's column; `<enum>.<variant>` is C4's
    `ExprKind::Variant`. On a number it is a method written without its `()`, `MZ0962` as
    Wave 1's numbers have it. A method called on an enum is `MZ0708`, and on a result
    `MZ0950`, with the `guess` `(try r).name(…)`.
  - A result-typed last line of a function that must return is `MZ0906` only, not also
    `MZ0950`, and `MZ0906`'s `exact` fix `return` also applies when the line's type is the
    function's success type. An unknown return type is `MZ0701` alone, not also `MZ0906`.
  - A bare `result` with no types is `MZ0306`, naming the two types, not `MZ0919`.
  - _An unread result waits while lines of its function were skipped unread_ (a `try`
    block, a form `MZ0919` names), as `MZ0924` does: those lines may read it.
  - _Columns are compared with every column any variant has_, so a variant missing one is
    the variant reported (`MZ0303`), and a variant whose line stopped at a column with no
    literal (`MZ0302`) is not compared.
  - _A result used as a condition_ gets `MZ0950`'s `guess` `try` only when its success is a
    `bool`; a result bound by `let x: int = …` is `MZ0950`, not a type mismatch.
- **Not built:** converting one error type to another on `try` (`via`, `MZ0955`), `MZ0950`'s
  `match`-stub fix where `try` does not fit (§12.3; `MZ0950` has no fix there), both the
  "C9 via" follow-up of §18.2; `mz run --agent`'s NDJSON line for `MZ0992`; and the guide
  change of §18.5.

## 19. What this RFC does not claim

- That any of it is built. §18.6 records what lands, pull request by pull request.
- That Mzizi programs are easier for a model to write than Python, TypeScript, Go or Rust
  programs. That is what the public suites (MultiPL-E, EvalPlus) would measure once Tier 1 exists
  (`LANGUAGE-TRACKER.md`, "The measurement"), and nothing has run. Each `exact` fix for a prior's
  idiom removes a round trip by design; whether it helps a model is the measurement.
- That the lowered code is fast, or that value semantics costs nothing (§14.1).
- That traps are the right answer for overflow, division by zero, an assignment out of range and
  a broken invariant. They are this design's answer, and §20 Q2 asks the owner.
- That the survey's features help a model write Mzizi. The survey (§21) records what ten
  languages chose and what failure modes were reported for them; its measured findings were
  measured on those languages, not on Mzizi, and none of the amendments has been tried by a
  model.
- That the evaluator and the lowering agree beyond the cases CI runs both on (§15.3).

## 20. Open questions for the owner

Each is a decision this RFC made provisionally so that the design is whole. **Each needs the
owner's yes, no or change before the wave that builds it.**

1. **Text concatenation.** Interpolation only, with `a + b` on text an `exact`-fixed `MZ0912`
   (§3.6)? Or `+` on text as a second form? This RFC chose one form.
2. **Traps.** Integer overflow, integer division by zero and out-of-range indexing stop the
   program with exit 101 (§4.3). That is a halt reachable from surface code, which RFC-0007 G2.5
   rules out for panics. Accept traps as distinct from panics, or require a `result` from every
   fallible operation? And what does a trap do inside a service handler?
3. **`break` and `continue`.** In M1 (§7.3), or left out, with early `return` as the only way out
   of a loop?
4. **`end fn <name>`.** The echo on every function (§1). Component `fn`s close with a bare `end`
   today: move them to the echo with an `exact` fix, or keep two closers for one construct?
5. **Equality as `is`.** `is` and `is not`, with `==` an `exact`-fixed `MZ0910` (§3.3), rather
   than adding `==`.
6. **Mutability.** `let` and `var`, with assignment to a `var` only (§5.1). The survey amendment
   answers the second half provisionally: a method changes its receiver only when it says
   `changes self`, is called as a statement, and only on a `var` (§11.2). Accept, or keep every
   method read-only?
7. **No lambdas in M1.** `map`, `filter` and `fold` take a named `fn` (§6.4). Add a lambda form
   for M2, or never?
8. **C8 and M1.** Q21 makes this a concrete proposal. C8's "Done when" includes a generic function
   and an interface, which the owner deferred past M1. Split C8 into a methods row (in M1) and a
   generics row (after it), or change M1's definition from "all of Tier 1 ✅"?
9. **Map and set order.** Key order (§9.2), lowering to `BTreeMap`, rather than insertion order,
   which needs a crate. And what JSON form a non-finite `float` takes (§2).
10. **Integer division.** Truncation toward zero (§4.1), Rust's rule, against Python's floor.
11. **Text length.** Unicode scalar values (§10), against bytes (Rust's `len`) or grapheme
    clusters (what a person counts).
12. **Contextual words or keywords.** The words of §1 are contextual, as RFC-0011's were, so
    `KEYWORDS` stays at 23. `and`, `or`, `let`, `var`, `return`, `while`, `try` and `self` could
    be keywords instead; for a reader the difference is nil, for the lexer it is simpler. And
    the bans §1 keeps have a cost RFC-0011 did not pay: no binding or `fn` may be named `ok` or
    `error`, and no record or enum `map`, `set`, `float` or `result`, so a model that writes
    `let error = …` or `record result` gets `MZ0921`. §1 already allows the type names as
    bindings and `ok` / `error` as variants, the commonest cases. Are the remaining bans worth
    their round trip?
13. **The positive option form in views.** A function body narrows on `when x is not none` (§8).
    Should views accept it too, amending RFC-0008 §4?
14. **Handler conditions.** RFC-0011's handler conditions stay a sub-grammar. Should handlers take
    §3's expressions once they exist, and should `at_least` / `at_most` then retire from them?
    Two divergences wait on the answer. `in` with one text literal on its right is equality
    membership in a handler and, in a function body, a diagnostic pointing to `s.contains(t)`
    (§3.3); and a record value is `problem error "x"` in a handler and `problem(error = "x")` in
    a program (§11.1). Should handlers move to the program forms, each with an `exact` fix?
15. **Copies.** Value semantics lowers to clones (§14.1). When does a copy-on-write
    representation become worth its complexity: before the public suites run, or after?
16. **Recursion depth.** A 64 MiB stack, and an abort past it (§4.3). Is a depth limit that traps
    worth the code it needs in the lowering?
17. **The file name.** RFC-0007 D3 already proposes "one top-level declaration per file,
    name-identical", generalising RFC-0001 §7.4, and §1 cites it. This RFC applies it to a
    program, so `program hello` lives in `hello.mz`. The owner is asked only to confirm that D3
    covers programs.
18. **Program-level constants.** A program has no constants: a value used in several functions is
    a zero-parameter `fn` (§6.1). Is that enough for M1, or should `let` be legal at program
    level (RFC-0007 G1.5's derived values)?
19. **`map`, `filter` and `fold` beside loops.** They are a second form for what `for each` and
    `push` already do, the iterator-chain vs loop duality RFC-0001 §1.2 excluded (§9.3). Keep
    both in M1, as this RFC does, or keep loops only and leave the three to P2, which would
    leave C7's "map, filter, fold" unmet in M1? §9.4's named folds widen the same duality
    (`xs.count(f)` beside a counting loop), so the answer covers them too.
20. **C6 and P2.** C6's "Done when" says the text operations exist "in the standard library (P2)
    with tests". §10 builds them as built-in methods with tests, before P2 exists. Do built-in
    methods meet C6, or does C6 wait for P2? If the first, the tracker's wording should change
    to say so, by the owner's decision, before C6 turns ✅.
21. **Split C8.** Q21 to Q29 come from the survey amendment (§21); this one is the concrete
    proposal Q8 asks for. The owner deferred generics ("methods for now", issue #69), and C8's
    "Done when" still names a generic function and an interface, so C8 cannot turn ✅ in M1 and M1
    ("all of Tier 1 ✅") cannot be reached. Proposed: C8 becomes "**Records and methods**", Done
    when "a record is built by field name and copied with `with`; a record has a method; a broken
    `always` invariant is reported", in M1; and a new row, "**Generics and interfaces**", Done when
    "a generic function works for two types; an interface is satisfied", outside Tier 1 and after
    M1, carrying survey F25 and F26. This RFC does not edit `LANGUAGE-TRACKER.md`; the owner's
    answer does, in its own pull request.
22. **Indexing returns an option** (§3.7, survey F2). This reverses the draft's trapping `xs[i]`
    and drops `.get`. The cost is an `otherwise` or a guard wherever the author knows the index is
    in range, the commonest case in loops over `range(0, to = xs.length())`. Accept, or keep the
    draft's two forms?
23. **Labels on every call** (§6.5, survey F3). Every argument after the first is labelled,
    built-ins included (`range(0, to = n)`, `xs.fold(0, step = add)`), and labels follow the
    declaration order. A two-argument call is longer than in any incumbent but Swift. Accept;
    exempt built-ins; or allow labels in any order and give up a reformat that never changes
    evaluation order?
24. **Enum payloads** (§11.5, RFC-0007 D1). One `enum` construct for columns and payloads; a
    `case` narrows instead of binding; JSON as `{"variant": {…}}`. Accept, or keep payloads
    out of M1 and leave D1 open?
25. **`always` in every build** (§11.4). This amends RFC-0010 §4.3, which checks `always` in
    debug builds only, and makes a broken invariant a trap, which Q2's question about traps
    covers too. Accept for programs, and should services follow?
26. **The fold set** (§9.4). Closed at the survey's eight. `min`, `max`, a key-less `sort` and a
    descending sort are not in it, so `xs.sort_by(f)` needs a key `fn` even on `list(int)`. Add
    any of them, or keep the eight?
27. **`via` and where `try` may stand** (§12.2). `via` names an error conversion; is the word
    right? And the survey (its §4.1 item 4) proposed that `try` only lead a line or a binding,
    after the Go team's lesson that `try()` hid control flow inside expressions; this RFC keeps
    `try` as a prefix anywhere in an expression (`try a() + try b()`), because it is a visible
    word, not a call. Keep, or restrict it?
28. **Unused bindings are errors** (§5.1, survey F21, Go's rule). An error blocks `mz run` on a
    half-written function; a warning would not. Error, as proposed, or warning?
29. **Field assignment beside `with`** (§11.1). `q.x = 3.0` on a `var` and `q with (x = 3.0)`
    both make a changed record, one as a statement and one as a value. Keep both, or keep `with`
    only and make field assignment `MZ0960` with an `exact` fix to `q = q with (x = 3.0)`?

## 21. Reconciliation with the top-10 language survey

`design/LANGUAGE-SURVEY.md` (PR #75, not merged yet; design input, nothing measured) lists
what RFC-0013 must design (its §3.1, F1–F26) and decide (its §4.1). This table compares each row
with this RFC as it now stands. "Kept" means the RFC already had the survey's design before the
amendment; "amended" means this amendment added or changed it; "departs" means the RFC chose
otherwise, for the reason given.

| Survey row                            | Where here     | Status                                                                                                                                                                                                                                                                |
| ------------------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F1 collection transforms, named folds | §9.3, §9.4     | amended: the eight folds; the form is methods taking a named `fn` (the survey left it open)                                                                                                                                                                           |
| F2 safe indexing, a deterministic map | §3.7, §9.2     | amended: `xs[i]` and `m[k]` return an option, `.get` is gone; key order was kept                                                                                                                                                                                      |
| F3 named arguments and defaults       | §6.5           | amended. Departs in two places: labels in declaration order, so a reformat never changes evaluation order; an unknown label's fix is `exact` only where one label fits (§5.2's nearest-name rule, or one unfilled parameter of the value's type), a `guess` otherwise |
| F4 interpolation only                 | §3.6           | kept                                                                                                                                                                                                                                                                  |
| F5 records with value equality        | §11.1          | amended: equality and the text form were there; `to_json()` added; hashing has nothing to attach to, since maps are ordered                                                                                                                                           |
| F6 construction by field name         | §11.1          | amended: declaration order and positional construction's `exact` fix. Departs: a missing field has no `exact` fix, because inventing its value is R9's zero value                                                                                                     |
| F7 copy-and-update                    | §11.1          | amended: `p with (x = v)`, in the record literal's own syntax, where the survey's placeholder was `entry with version "2.0"`                                                                                                                                          |
| F8 methods, `changes self`            | §11.2          | amended: `changes self`, statement-only, on a `var`                                                                                                                                                                                                                   |
| F9 exhaustive `match`                 | §7.2           | kept. The survey's open point 5: no guards in M1 (the Python and C# passes, against Java's), and `else` stays legal on an enum without a warning, see below                                                                                                           |
| F10 enum payloads                     | §11.5          | amended: one construct (D1); a `case` narrows rather than binds                                                                                                                                                                                                       |
| F11 closed literal sets, `all`        | §11.5          | amended, renamed: `variants()`, because `all(f)` is a fold                                                                                                                                                                                                            |
| F12 no null                           | §8             | kept                                                                                                                                                                                                                                                                  |
| F13 flow-sensitive narrowing          | §8             | kept (positive form and guard). Departs: `MZ0710` has no `exact` fix, because the guard's branch is the author's                                                                                                                                                      |
| F14 one default word                  | §8.1           | amended: `otherwise`, replacing `x.or(d)`, evaluated only on `none`                                                                                                                                                                                                   |
| F15 errors as values, one word        | §12            | kept `try`; amended with `via` for an explicit error mapping                                                                                                                                                                                                          |
| F16 an ignored result is an error     | §12.3          | kept; amended with the `match`-stub fix                                                                                                                                                                                                                               |
| F17 integers that never wrap silently | §4.1, §4.4     | kept; amended with the named `wrapping_*` methods and `a // b` as `MZ0910`. `%` stays the remainder's spelling (§3.2's case for symbols)                                                                                                                              |
| F18 floats and decimals               | §4.2, §4.4     | kept: `float` named; `decimal` decided against for M1 (P2's `math`)                                                                                                                                                                                                   |
| F19 local inference                   | §5.1, §6.1     | kept. Departs: a redundant annotation is left to `mz fmt` (T2), not reported by `mz check`, since it is not an error                                                                                                                                                  |
| F20 functions as values, no lambdas   | §6.4           | kept                                                                                                                                                                                                                                                                  |
| F21 unused bindings are errors        | §5.1           | amended: `MZ0928`, exempting parameters and loop bindings                                                                                                                                                                                                             |
| F22 compile-time evaluation           | §4.1, §15.1    | departs: `mz check` folds constants (`MZ0915`, `MZ0975`) but never runs a function; `example` clauses run in `mz contract`, under its step bound, so a check is always fast                                                                                           |
| F23 Unicode-correct text              | §10            | kept: scalar values (§20 Q11); amended so `s[i]` and `s.slice` return an option                                                                                                                                                                                       |
| F24 a fast edit-run loop              | §13            | kept                                                                                                                                                                                                                                                                  |
| F25, F26 generics and interfaces      | §11.3          | kept deferred; the survey's design recorded for when they come; the tracker conflict is §20 Q21                                                                                                                                                                       |
| F33 `always` invariants (Tier 2 row)  | §11.4          | amended for records: checked at construction, in every build                                                                                                                                                                                                          |
| R1–R12 rejected features              | §8.1, §11, §16 | kept rejected: R1–R2 `?.`, `!`, `.unwrap()` (`MZ0939`, `MZ0952`); R3 §14.1; R4–R7 §11.3; R8 no form; R9 §11.1; R10 `end` blocks; R11 §12.1; R12 a `with` line (§11.1)                                                                                                 |

**Where the survey's own sources disagreed, and what this RFC picked:**

- **The default word** (survey §4.1 item 3): `otherwise`, not `or`, because `or` is §3.4's
  boolean operator.
- **The fold form** (item 1): methods taking a named `fn`, the one form consistent with no
  lambdas that adds no third loop.
- **Guards in `case`** (item 5): none in M1. A guarded case does not cover its variant, so the
  checker would have to prove guards exhaustive or give up `MZ0930`'s precision.
- **`else` over an enum** (item 5): legal, and no warning. A `match` that picks one variant out of
  twelve would otherwise list eleven, which pushes authors back to the `when` chain `MZ0936`
  rejects, and a warning that fires on correct code teaches authors to ignore warnings.
- **The record syntax** (item 8): the survey suggested one `name value` style shared with
  contract examples; this RFC uses `name = value` inside parentheses for calls, records and
  `with` alike (§6.5's last paragraph says why), and contract examples keep `name value`.
- **The propagation word** (item 4): `try`, the five-pass majority; whether it may stand inside
  an expression is §20 Q27.
- **Enum payloads** (item 6): one construct, §20 Q24.
- **Numbers** (item 7): already decided in §4, unchanged: overflow traps in every build,
  division truncates, no `decimal` in M1.
- **Text** (item 9): interpolation of any expression without a string literal inside, and scalar
  values, both unchanged.
- **Generics** (item 10): deferred, with the tracker conflict put to the owner as §20 Q21.
