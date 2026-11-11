# RFC-0018 — Function values: a function type, a named `fn` as a value, and inline functions that capture nothing

**Status:** draft for review. **Nothing in this RFC is implemented** until a wave in §8 says so.
Every form below is designed, not built: `mz check` today answers `MZ0909` to a function used as a
value outside `map`, `filter` and the folds. Each wave names the tests that would make a form
true, and the tracker's row K5 stays ❌ until a wave's tests are on `main`.
**Author:** the machine author (Claude), for the owner, on issue
[#153](https://github.com/mzizi-dev/mzizi/issues/153) (stream 2, the server components).
**Scope:** reopens RFC-0013 §20 Q7 (owner decision, 2026-11-07: "Non-capturing function values
first (map, filter, folds). Captures are a later step and need their own design"). It settles a
function type, a named `fn` as a value, a call through a value, and a non-capturing inline
function. It defers **captures** (a function that reads a local of the code around it), function
values in records and collections, and function values as results, each to a later RFC.

> **Amends RFC-0013 §6.4**, which says "There are no lambdas, closures or function types in M1" and
> makes a bare `fn` name anywhere but `map`, `filter` and `fold` an `MZ0909`. §2 below narrows
> that. RFC-0013 carries a dated note pointing here; its text is otherwise unchanged. **Nothing
> takes effect until a wave in §8 is on `main`.**

---

## 0. Why: what a component met

Writing the first server components in Mzizi (`examples/components/`, tracker K5) found three places a
function value is the natural form and the language has none:

1. A **higher-order helper of the author's own**. `map` takes a named `fn`, but a user `fn` cannot,
   so a validator that applies "a check" to each field, or a retry that applies "an attempt", is
   written once per check.
2. A **one-use predicate**. The circuit breaker keeps the failures inside a window. `filter` would
   say it, but its function takes one element and the window needs `now` and `window_ms`: that is a
   capture, and §7 leaves it out. Without it the breaker uses a `for each` loop with a `push`. This
   RFC does not remove that workaround; it removes the other two (§0.1, §0.3).
3. A **named helper for a three-line predicate**, which RFC-0013 §6.4 already calls "a real cost:
   a one-use helper takes three lines". Wave 2 answers it only for functions that read their own
   parameters.

None of this is measured. RFC-0013 §6.4's reasons (a named function is a unit a contract attaches to
and an IR node an agent patches by name) still hold, and §5 keeps them: a function value that is a
bare name stays a name.

## 1. The function type

```mz
fn apply_twice(f: fn(int): int, x: int): int
  return f(f(x))
end fn apply_twice
```

- **Spelling.** `fn(<types>): <type>`, the signature of a `fn` declaration without names, and
  `fn(<types>)` for a function that returns nothing. It adds no keyword (`fn` is read by position,
  as `program` and `record` are, so `KEYWORDS` in `compiler/src/lex.rs` stays at 23). A
  parameter's type is any type a `fn` parameter has. A default value, a label and `changes self`
  are not part of a type.
- **Where it may stand (wave 1).** The type of a `fn` **parameter**, and the declared type of a
  `let` or `var`. Everywhere else (a record field, an element of `list`, `map`, `set` or `option`,
  an `option`'s or `result`'s payload, a `fn`'s **return type**) it is `MZ0977`, with a `say`
  naming the position and the repair: pass the function as an argument, or declare a named `fn`.
  The restriction is the whole of why wave 1 needs no `Box`, no lifetime and no equality on
  functions (§4).
- **Two function types are equal** when their parameter types and return type are equal. There is
  no subtyping, and no conversion between an `int` parameter and a `float` one (RFC-0013 §4).

## 2. A named `fn` is a value

```mz
fn double(x: int): int
  return x * 2
end fn double

fn main
  print(apply_twice(double, 5))
  let g: fn(int): int = double
  print(g(1))
end fn main
```

- **A bare `fn` name is a function value where a function type is expected**: an argument to a
  parameter of function type, and the right side of a `let` or `var` that declares one. A `let`
  with no declared type may infer it from a bare name (`let g = double` is `fn(int): int`).
- **Elsewhere it is still `MZ0909`**, as RFC-0013 §6.4 says: a `return`, an operand, a record field,
  a collection element, an argument to a method that takes none. The zero-parameter case keeps its
  `exact` fix appending `()`.
- **An argument that does not fit** is `MZ0909`, as for `map` today, and its `say` quotes the
  signature expected and the signature found.
- **A method is not a value** (`p.norm` is `MZ0909`): its receiver is part of the call.
- **A parameter or local of function type is a name like any other.** It may not be assigned a
  value of another function type (`MZ0905`, as for any binding), and a `var` of function type may
  be assigned another function of the same type.
- **Recursion and ordering** are as RFC-0013 §6.3: names resolve across the whole program.

## 3. Calling a function value

`f(x)` calls a binding `f` of function type, with the arguments `f`'s type gives, by position. It is
a call like any other for `MZ0905` (the wrong count or type of an argument, quoting the type) and
`MZ0950` (a result nobody handles). A **named argument** or a default on a function value is
`MZ0905`: a type has no labels (§1). `f` shadows nothing: RFC-0013 §5's no-shadowing rule already
stops a local from taking a `fn`'s name (`MZ0921`).

## 4. What a function value is not (wave 1)

- It has no equality, no text form and no JSON form: `f is g`, `print(f)` and `"{f}"` are `MZ0977`.
  Two paths to one function, or two functions with one body, would otherwise differ by an accident of
  the lowering.
- It is not a map key, a set member or a field, for the same reason (§1).
- It carries nothing. A value is the name of one `fn` of the program, so it is one machine address
  and `Copy`: passing it moves no ownership, and RFC-0001 §1.8's "no borrows at the surface" holds
  without change.

## 5. Inline functions that capture nothing (wave 2)

```mz
print(apply_twice(fn(x: int): int = x * 2, 5))
print(primes.filter(fn(n: int): bool = n % 2 is 1))
```

- **Form.** `fn(<name>: <type>, …): <type> = <expression>`: one expression, on one line, no block.
  A body that needs a statement is a named `fn` (RFC-0013 §6.4's reason, kept). `=` is the one
  token: Mzizi has no `==` (`is`, RFC-0013 §3), so the form cannot be misread, and the other
  languages' `x => …`, `|x| …` and `lambda x: …` stay `MZ0909` with a `say` naming this form.
- **It reads only its parameters, literals, and the `fn`s of the program.** A reference to a local,
  a parameter or a `var` of the code around it is **a capture**, `MZ0978`, with no fix, and a `say`
  that names the two repairs: pass the value as a parameter (a named `fn` of one more parameter),
  or wait for captures. This is checked by name resolution, before lowering, so no `rustc` message
  reaches the author (RFC-0014 T2-7).
- **It is a value of function type** and may stand wherever a bare `fn` name may (§2), and as the
  argument of `map`, `filter`, `count`, `any`, `all`, `first`, `fold`, `sort_by` and `group_by`,
  where RFC-0013 §9 takes a named `fn` today.
- **It may call a named `fn`, and itself not at all** (no name to call), so a recursion is a
  named `fn`.
- **Canonical form** (RFC-0013 §17) writes it with single spaces and no parentheses around the
  body.

## 6. Lowering

| Mzizi                              | Rust                                                                                         |
| ---------------------------------- | -------------------------------------------------------------------------------------------- |
| `fn(int): bool` (parameter, `let`) | `fn(i64) -> bool`, a function pointer                                                        |
| `fn(text, int)`                    | `fn(String, i64)`                                                                            |
| `apply_twice(double, 5)`           | `apply_twice(double, 5)`; a function item coerces to the pointer                             |
| `f(x)` on a binding                | `f(x)`                                                                                       |
| `fn(x: int): int = x * 2` (wave 2) | `\|x: i64\| -> i64 { x * 2 }`, which coerces to `fn(i64) -> i64` because it captures nothing |

A closure that captured would not coerce, so the pointer type is also `rustc`'s own check of §5's
rule: the front-end rule is what an author sees, and a `MZ0990` here is a compiler bug. A trap in
the body names the `.mz` line, as RFC-0013 §4.3 does for any function. The pointer type has no
lifetime, which is why §1 has no function-typed result in wave 1.

## 7. Captures: not here

A function that reads a local of the code around it needs a decision this RFC does not make: what
is captured (a copy, as every Mzizi value is owned, RFC-0001 §1.8), whether a captured `var` is a
copy at creation or a shared cell, and how a function value with a captured environment is typed
(it is no longer a pointer, so function types would change). It is the next RFC, and the breaker's
window filter is its first test. Until then every capture is `MZ0978`.

## 8. Waves, and what makes each true

| Wave | Builds                                                                                                                            | Tests that would make it true                                                                                                                                                                               |
| ---- | --------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | the function type in parameters and `let`/`var`, a bare `fn` name as a value, a call through a value, `MZ0977`, `MZ0909` narrowed | `examples/fn_values.mz` runs through `mz run` against its `.expected`; one test per diagnostic in `compiler/tests/program_fn_values.rs`; the harness entry `fn_type` and codes in `compiler/src/harness.rs` |
| 2    | the inline form of §5, `MZ0978`, the inline form accepted by `map`, `filter` and the folds                                        | a test per method with an inline function; a capture of a parameter, a `let` and a `var` each `MZ0978`; the canonical form                                                                                  |
| 3    | later, by a decision: function values in records and collections, and as results                                                  | not designed here                                                                                                                                                                                           |

Each wave updates the tracker's K5 row with its evidence in the same pull request. K5 turns ✅ only
when waves 1 and 2 are on `main` and a component uses them where it had a workaround. The window
filter of the breaker stays a loop until captures exist, and says so.

## 9. Diagnostics

| Code     | Phase      | Meaning                                                                                                                                                             |
| -------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MZ0909` | `mz check` | (narrowed) a function used as a value where no function type is expected, an argument that does not fit its type, a method as a value, or another language's lambda |
| `MZ0977` | `mz check` | a function type where wave 1 does not allow one (§1), or a function value compared, printed or interpolated (§4)                                                    |
| `MZ0978` | `mz check` | an inline function that reads a local, a parameter or a `var` of the code around it (§5); wave 2                                                                    |

## 10. Open questions for the owner

1. **The inline form's spelling** (§5): `fn(x: int): int = x * 2`, or a spelling that borrows an
   existing Mzizi word? It is wave 2, so wave 1 does not wait on it.
2. **Return types** (§1): wave 3 allows a function-typed result only with a decision on whether a
   function value may outlive its call, which is a captures question (§7).
