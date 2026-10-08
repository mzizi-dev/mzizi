# Changelog

Every change to Mzizi's behaviour, diagnostics, language, charter, RFCs or benchmarks,
newest first. The format is [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The
compiler has no releases (`compiler/Cargo.toml` is `0.0.0`), so sections are dated by the
day their pull requests merged into `main`, not versioned. [`AGENTS.md`](./AGENTS.md),
"Changelog", says when a pull request adds an entry.

**Nothing listed here has been measured against the charter's kill criterion.** The two
pilots below are pilots, and each entry says what they showed. Entries separate what is
tested (`cargo test`, gated in CI) from what is designed (an RFC).

Entries from 2026-08-23 to 2026-09-29 were backfilled on 2026-09-30 from `git log` and
the merged pull requests. The commits dated 2026-08-23 predate this repository: they came
across from `mzizi-lang/` in the subtree split that created it (`MIGRATION.md` §3), and
carry no pull request number.

## [Unreleased]

### Added

- **Text methods in a `program`, the part of C6 that needs no option or list** (RFC-0013 §10, tracker row C6, now 🟡; Refs #69). `s.length()`, counted in Unicode scalar values as §10 chooses (`"héllo".length()` is `5`, `"日本語".length()` is `3`), `s.contains(t)`, `s.starts_with(t)`, `s.ends_with(t)`, `s.trim()`, `s.to_upper()`, `s.to_lower()` (Unicode's full case mappings: `"straße".to_upper()` is `"STRASSE"`), `s.replace(old, by = new)` and `s.repeat(n)`. They lower to Rust through `chars().count()` and `str`'s own methods, none of which cuts a character, and the generated code holds no `unwrap`, `expect`, `panic!` or `unsafe`. New modules: `compiler/src/text.rs` (the method table), `compiler/src/text/ops.rs` (the helpers, emitted verbatim into every lowered program) and `compiler/src/program/text.rs` (the checker).
- **Traps:** `s.repeat(n)` with a negative `n`, or with a result too long for a text to hold, stops the program with `MZ0991` (`negative repeat count`, `text too long`) and exit 101. RFC-0013 §4.3's table did not list these; §18.6 records them.
- **Diagnostics,** all existing codes: **`MZ0962`** for other languages' spellings, with `exact` fixes: `len(s)` to `s.length()` (Python counts scalar values too); `upper()`, `lower()`, `to_uppercase()`, `toUpperCase()`, `toLowerCase()`, `startswith`, `endswith` and `includes` to the Mzizi name; `replaceAll(a, b)` to `replace(a, by = b)`; and emptiness asked as `s.is_empty()`, `not s.is_empty()`, `s.length() is 0`, `is not 0` or `> 0` (with `len(s)`, `s.len()`, `s.count()` or `s.length` in place of `s.length()`) to `s is ""` or `s is not ""` (§3.3), in one pass, with `len(s) == 0`'s `MZ0910` folded into that one fix. `substring(a, b)` and `indexOf(t)` get a `guess` naming `slice` and `find`. `s.len()`, `s.size()`, `s.count()` and `s.length` get the fix `s.length()` as a `guess`, and `strip()` gets `trim()` as a `guess`: Rust's `len` counts bytes, JavaScript's `length` counts UTF-16 units and Python's `strip` also strips U+001C to U+001F, so each fix may change what the program means. **`MZ0927`** for `s.replace(a, b)` without its label (`exact`: insert the `by =` label) and for `by: b` or a misspelt label such as `with = b` (`exact` `by = b`). **`MZ0915`** for a literal negative `repeat` count. **`MZ0708`** for a method text does not have, with the nearest name as a `guess`. **`MZ0905`** for a text method's arity or argument types. A camelCase method name is one diagnostic, not also `MZ0101`.
- **Not built, and still `MZ0919`:** the text methods that return an option or a list (`s.slice(a, to = b)`, `s.find(t)`, `s.parse_int()`, `s.parse_float()`, `s.split(sep)`, `s.chars()`), each naming what it returns, until C7's lists and options exist; also `s[i]` and `t in s`. `MZ0919`'s `say` now names those six instead of "methods on `text`". The row stays 🟡 until they land and until the owner decides whether built-in methods meet its "Done when" (RFC-0013 §20 Q20). §18.5's guide change is not made.
- **The language harness registers the nine methods** (RFC-0012 §1.2): one method entry each, typed from the checker's table, with `examples/text.mz` as the runnable example; the `text` type lists them, and the `say` texts of `MZ0708`, `MZ0915`, `MZ0919`, `MZ0927`, `MZ0962` and `MZ0991` cover them. `mz harness definition` has 146 entries (from 137; method entries from 11 to 20).
- **`examples/text.mz`** trims, measures, cases, tests, replaces and repeats text, with `Zürich`, `日本語` and `straße`; its output is `examples/text.expected`, which CI's `lowering` job compares with `mz run`'s.
- **Tested, not measured:** 18 tests in the new `compiler/tests/program_text.rs` (one per method, run through `mz run` on ASCII, accented Latin, CJK, an emoji and a two-scalar flag; each idiom's `exact` fix applied as `mz fix` applies it and the result checked again; the lowered text; the `repeat` traps' exit 101; and that every method in the table has a lowering), and 3 unit tests in `text.rs`. `cargo test` in `compiler/` goes from 513 to 534 tests (644 to 665 in the workspace, in 25 suites with tests, from 24). No benchmark ran, and nothing here says a model handles text better in Mzizi than in another language.

### Added — collections in a `program`: `list(T)`, `map(K, V)` and `set(K)`, RFC-0013 §9, C7

- **Collections in a `program`: `list(T)`, `map(K, V)` and `set(K)` (RFC-0013 §2, §3.7, §8.1, §9, LANGUAGE-TRACKER C7; Refs #69).** Built: bracket literals (`[1, 2]`, `["a": 1]`, `[]`), which take their type from where they stand, so `[3, 1, 3]` is a set where a `set(int)` is expected; `xs[i]` and `m[k]`, which return an option, `none` outside the list or for a missing key, read with **`x otherwise d`**, whose default runs only on `none` (§3.5 level 6, right-associative); `xs[i] = v`, which traps out of range (`MZ0991`, exit 101), and `m[k] = v`; `x in c`; a collection's emptiness, `c is none` and `c is not none`; `for each` over a list, iterating the value the list had when the loop began; `range(a, to = b)` as a list anywhere; §9.2's methods `length`, `slice`, `keys`, `values`, `to_list`, `map`, `filter`, `join`, `push`, `insert` and `remove`; and §9.4's eight named folds `count`, `sum`, `any`, `all`, `first`, `fold(init, step = f)`, `sort_by` and `group_by`, each taking a named `fn` whose signature must fit. Maps and sets iterate and print in key order. §3.8's text forms for collections (`[1, 2]`, `["a": 1]`, text quoted inside, an option as its value or `none`). Lowered to `Vec`, `BTreeMap`, `BTreeSet` and `Option`, through helpers that never index with `[…]` and emit no `unwrap`, `expect`, `panic!` or `unsafe`; values stay values (§14.1): `let b = a` copies, and a `fn` never changes its caller's list.
- **Diagnostics.** C7's codes: **`MZ0909`** (a function argument that is not a named `fn`, or whose signature does not fit, quoting both; a lambda, `x => …`, `(x) => …`, `lambda x: …`, with no fix), **`MZ0960`** (a mutation of something that is not a `var`; `exact` fix `var` on a `let`), **`MZ0961`** (a bracket literal that cannot be typed: `[]` with nothing to say what it holds, mixed element types, entries mixed with elements, a map key written twice), **`MZ0962`** on a collection (`len(xs)`, `.len()`, `.size()`, `.count()`, `.length`, `.append`, `.contains`, `.includes`, `.has`, `.get(i)`, `is []`, `.length() is 0`, `.is_empty()`, `some`, `every`, `sortedBy`, `sorted(xs, key = f)` with a function of the program as the key, and a length compared with 0 (`len(xs) == 0` and the rest, straight to `xs is none`), each with its `exact` fix on names and paths; `find`, with the `guess` `first`; `.reduce(f, init)` with the `guess` `fold`; `t in s` on text, `exact` `s.contains(t)`, C6's method), **`MZ0963`** (a tuple, no fix) and **`MZ0964`** (a map key, set element or `sort_by` / `group_by` key with no order). Reused: **`MZ0710`** (an option used as its value; registered in the language harness and off the pending list), `MZ0105` (`[T]` and `T[]` as a type, now reported by the program's type parser, since `[` is a list literal there; a component's and a service's `MZ0105` are unchanged), `MZ0711` (`for each` over a map or a set, `exact` `m.keys()` / `s.to_list()`), `MZ0712` (`when xs`, `guess` `xs is not none`), `MZ0910` (Python's `x not in xs`, `exact` `not x in xs`), `MZ0912` (`otherwise` on a value that is not an option, `is none` on one that is not a collection) and `MZ0915` (a literal negative index, `guess` `xs[xs.length() - 1]`). Every new type, operator (`in`, `otherwise`, indexing, `is none`), method, statement form and code is registered in the language harness with runnable examples: `mz harness definition` goes from 146 to 181 entries and from 70 to 75 diagnostic codes, with 50 pending (from 51); a collection's `length` has the entry `collection length`, since C6's text `length` has `length`. The harness's precedence levels now match RFC-0013 §3.5's table, since `otherwise` takes level 6: comparison is 7, `not` 8, `and` 9 and `or` 10 (were 6 to 9).
- **`examples/collections.mz`**, which builds a list, a map and a set and transforms them in functions, runs in CI through `mz run` against `examples/collections.expected`.
- **Tested, not measured:** 27 tests in the new `compiler/tests/program_collections.rs` (the parse; every C7 code and each reused one, with each `exact` fix applied by `mz fix`'s function and the result checked again; the lowered text; and `mz run` end to end: the example, each fold on an empty and a non-empty list, an index out of range as `none`, value semantics, key order and text forms, an `otherwise` default that traps only when it runs, and `xs[i] = v` out of range and an `int` `sum` overflow each exiting 101 with `MZ0991`), and one more in `compiler/tests/robustness.rs`: 100,000 nested bracket literals, list types written either way, map types, `otherwise`s or indexes each give exactly one `MZ0411` on a 1 MiB stack, and a 100,000-element list literal and a 100,000-entry map literal check and lower. `cargo test` in `compiler/` went from 534 to 562 tests on `staging` with C6 (665 to 693 in the workspace, in 26 suites). No benchmark ran, and nothing here says a model writes collection code better in Mzizi than in any other language.
- **Not built, and left to named follow-ups:** `option(T)` written as a type, `none` as a value, narrowing with `when x is not none` and its guard, and `MZ0938` / `MZ0939` (§18.2's "C4 options"; `MZ0919` names them); indexing text, and C6's text methods that return an option or a list (`slice`, `find`, `parse_int`, `parse_float`, `split`, `chars`), which still report `MZ0919` and are now unblocked for a follow-up; records (C8); §6.5's labels other than `range`'s `to`, `slice`'s `to` and `fold`'s `step`; and the guide change of §18.5. RFC-0013 §18.6 records the split and where the code departs from the RFC's text.
- **Fixed after an independent review of #99, before merge.** Each fix has a test.
  - `sorted(xs, reverse = true)` no longer gets the `exact` fix `xs.sort_by(true)`. Only the label `key` is a sort key, and only a function of the program is `exact`. A call now records whether its labelled argument was written with its label.
  - `len(xs) == 0`, `xs.count() is 0`, `xs.size() > 0` and the other spellings of a collection's length compared with 0 are fixed straight to `xs is none` / `xs is not none`, so one `mz fix` pass converges. A test runs `mz fix` once on the shipped binary, then `mz check`, and expects a clean result.
  - `xs.is_empty() is true` is fixed to `(xs is none) is true`, which checks, instead of a chained comparison, which did not.
  - `range(0, to = 9223372036854775807)` as a list panicked with "capacity overflow", and `range(0, to = 100000000000)` aborted with exit 134. A list too long to hold is now `MZ0991` (`a list too long to hold in memory`, exit 101). Its length is computed with checked arithmetic and reserved with `try_reserve_exact`. `xs.map(f)` reserves its result the same way.
  - `.find(f)`'s fix `first` is a `guess`, not `exact`, since `first` returns an option.
  - RFC-0013 §18.6 records three gaps the review found that are not changed here: a map key equal only at run time keeps the last value; `range` and `slice` are accepted without the `to` label; and `xs[0].push(1)` gets `MZ0710` suggesting `otherwise`.

### Changed — C8 is records and methods; generics and interfaces are a new row, P12 (2026-10-08)

- **`LANGUAGE-TRACKER.md` splits C8**, by the owner's decision of 2026-10-08 on RFC-0013 §20 Q21 (#69). C8 becomes "User types: records and methods", done when a record is built by field name and copied with `with`, a record has a method, and a broken `always` invariant is reported, each with its language-harness entries; it stays 🟡, and is part of M1. The new row **P12, "Generics and interfaces"** (❌), carries the deferred clauses ("a generic function works for two types; an interface is satisfied") and comes after M1. RFC-0013 §11 and §20 Q21 record the decision. Nothing is built by this change.

### Fixed — the tracker's H1 row says the 70 harness codes are on `main` (2026-10-08)

- **`LANGUAGE-TRACKER.md` row H1** still said "70 diagnostic codes on `staging` … 66 on `main`" and "51 codes (55 on `main`)" after the 2026-10-08 release (#96) carried #93 to `main`. It now says 70 codes and 51 pending, with `MZ0301`, `MZ0302`, `MZ0303` and `MZ0704` registered by #93. Read from `mz harness definition` on `main` (`dc156c5`): 137 entries, 70 codes, 51 pending. Nothing else changes.

## 2026-10-08

### Changed

- **`LANGUAGE-TRACKER.md` marks C1, C2, C3, C4, C5, C9 and C10 ✅** now that the 2026-10-08 release (#91) put Wave 0 (#80), C1 + C5 (#83), C4 (#89), C9 (#87) and the language harness (#88) on `main`. Each row's "Done when" was checked on `main`'s tree (be88017): `cargo test --workspace` passes 643 tests in 24 suites (512 in the compiler crate), including `program.rs`, `program_numbers.rs`, `program_control.rs`, `program_errors.rs` and `harness.rs`; `mz check` and `mz run` of `examples/hello.mz`, `fib.mz`, `numbers.mz`, `control.mz` and `errors.mz` print exactly their `.expected` output; and `mz harness entry` answers for every construct and code the rows name. The rows keep their "Not yet" and "Not built" lists. **P4 and H1 stay 🟡**: P4 because collections, text methods and records do not exist, so a general program cannot be written yet, and H1 because the plugin host and the generated skills are not built. The "Where Mzizi stands today" section now summarises what a `program` can do on `main`, the comparison table's expressions, numbers, error handling and compiles rows say what a `program` has, and H1's count of diagnostic entries is corrected from 60 to 66. `README.md`, `AGENTS.md` and RFC-0013 §18.6 lose their stale "on `staging`" and "no results" wording. Documentation only: no code changes, and nothing is measured.

### Fixed

- **The language harness registers every code a `program` can raise** (RFC-0012 §1.2; Refs #69). Four shared codes a program's enums raise were still on the pending list: **`MZ0301`** (an enum line that is not a variant, or, in a program, an `enum` line with no name), **`MZ0302`** (a column with no literal value), **`MZ0303`** (a variant missing a column another variant has) and **`MZ0704`** (an enum, a variant or a column declared twice; the repeated variant's line has an `exact` fix that deletes it). Each now has an entry with a program trigger, its `say` text and its fix kinds as raised (`MZ0704`: none or `exact`; the other three: none), and the `enum` entry names them. They leave `PENDING_CODES` and the test's frozen snapshot. `mz harness definition` now has 137 entries, 70 diagnostic codes (45 `MZ09xx` and 25 shared, from 21) and 51 pending codes (from 55). The cause: `every_program_code_is_registered_not_pending` in `compiler/tests/harness.rs` looked only at `MZ09xx` codes. It now scans every file a program passes through, from the lexer to `mz run` (`PROGRAM_SOURCES`: `lex.rs`, `expr.rs`, `numbers`, `parse/program`, `program`, `run` and `main.rs`), fails on any code written there, outside its unit tests, that has no entry, is pending, or whose entry's kinds leave out `program`, and checks that every `MZ09xx` code in the compiler lies inside that scan. Re-pending `MZ0303` makes it fail with "MZ0303 (in program/errors.rs) is pending, and a program can raise it". No checker message changes here. RFC-0012 §2, `AGENTS.md` and `LANGUAGE-TRACKER.md` (H1) give the new counts and the stronger rule.
- **The harness's `say` and teaching texts no longer describe a language C4 and C9 changed** (RFC-0012 §1.1; Refs #69). Read against the checker, line by line: **`MZ0919`**'s say listed "an enum variant with columns" as not built, and columns are built (C9); it now names exactly what still reports `MZ0919`: lists, maps, sets and options (`none` among them), `range` off a `for each` line, methods on `text`, a `record`, a `use` line, a `test` block, and a `contract` block in a program or on a `fn`. **`MZ0902`**'s say called any return type on `main` an error; `main` may return `result(none, E)`. The **`program`** entry said a program "holds `fn`s and nothing else" with a `main` of "no return type"; it now holds `fn`s and `enum`s, `main` may return `result(none, E)`, and its grammar shows both. Also corrected: **`MZ0708`** (a variant or column an enum lacks, a bare variant two enums share), **`MZ0711`** (a column whose type differs between variants), **`MZ0904`** (a `fn` and an `enum` with one name), **`MZ0921`** (a binding named like an enum or a variant, a `fn` named like a variant) and **`MZ0922`** (assignment to a `for each` binding) each left out a case C4 or C9 added; `print`'s and the text literal's types left out an enum's variant, which has a text form, and say a result has none; `names` leaves out enum and variant names; and `match` says it takes a result too. Two checker messages a program reports said the same stale thing and are corrected: `MZ0919`'s tail ("a program has `fn`, `let`, `var`, `when`, `return`, `print`, …" now lists `enum`s, `match`, loops and results) and `MZ0901`'s for statements outside a `fn` ("a program holds `fn`s and `enum`s"). The codes, spans and fixes are unchanged.
- **A test holds `MZ0919`'s list to the checker** (`every_form_mz0919_names_is_still_not_built` in `compiler/tests/harness.rs`): the forms its say names after the colon must match a table of programs, one or more per form, in order; each program must still report `MZ0919` and nothing else; and the number of places in the program's source that report `MZ0919` (nine) is pinned, so a new one must be named in the say before the count is raised. When a wave builds a form, its programs stop reporting `MZ0919`, and the test fails until the say stops calling it unbuilt; putting "an enum variant with columns" back, with its program, fails it. Nothing cheaper and robust compares the rest of the hand-written text with the checker, so `say` and `teach` texts otherwise stay written by hand and checked by reading the code (RFC-0012 §2, `AGENTS.md`). `cargo test` in `compiler/` goes from 512 to 513 tests (643 to 644 in the workspace).

### Added — `result`, `error` and `try`: errors in a program, RFC-0013 §12, C9 (2026-10-08)

- **Errors in a `program`** (RFC-0013 §12, tracker row C9; Refs #69; on `staging` after C4, #89, and the language harness, #88). A function that can fail returns `result(T, E)`, or `result(none, E)`, and fails with `return error(e)`; `return v` returns success, and `return r` passes a result of the function's own type through. A caller handles a result with C4's `match` and `case ok <name>` / `case error <name>`, as a statement or as a value, or propagates its error with a prefix `try` (`return try check_age(a) + try check_age(b)`). `fn main` may return `result(none, E)`; when it returns an error, the program writes `mz: error MZ0992: main returned an error: <the error's text form>` to standard error and `mz run` exits 1. An enum's variants may carry literal columns, as a component's do (`negative say "is below zero"`), read with a dot (`problem.say`). New modules: `compiler/src/program/errors.rs`, `compiler/src/parse/program/errors.rs`, `compiler/src/run/errors.rs` and `compiler/src/intern.rs`.
- **One `match` and one `enum`** (owner direction). A `match` on a result is C4's `match` over a result-typed value: each case carries the name it binds, and C4's coverage check gains the keys `ok` and `error`, so `MZ0930` and `MZ0931` have one implementation, whose messages now name `case ok` and `case error`. In a `match` on a result, `else` is not allowed: it is `MZ0931`, whose `exact` fix deletes it, keeping a `case error` written after it. `MZ0930` points at the matched value, as for every `match`. C9's enum declaration, with columns, replaced C4's; enums keep C4's Rust names, with an accessor method per column. A column on a variant is no longer `MZ0919`.
- **New diagnostics in a program**, RFC-0013 §16's codes: **`MZ0950`**, a result nothing matches or propagates (discarded, used as its success value, a condition, compared, printed or interpolated, passed, assigned, returned where the type differs, a result parameter, a result inside a result, a `let` whose result nothing reads before its block ends, and a result held by a `var`), with the `guess` fix `try` where the function returns the same error type, and the `exact` fix `(try f(x)).y` for `try f(x).y`; **`MZ0951`**, a `try` that cannot propagate (not on a result, in a function that returns none, or a different error type), with no fix; **`MZ0952`**, the error idioms of other languages: `Ok(v)` and `ok(v)` (`exact` `v`), `Err(e)` (`exact` `error(e)`), Rust's postfix `?` (`exact` prefix `try`, one diagnostic for a chain), `throw e` and `raise e` (`return error(e)`, `exact` when `e` visibly has the error type, a `guess` otherwise), and a `try` block, `.unwrap()` and `.expect(…)`, with no fix; **`MZ0953`**, `error(e)` outside a result function or of the wrong type, and a bare `ok` or `error` variant in a result function (`exact`: name its enum); **`MZ0954`**, `return try r` with `r` of the function's own result type (`exact`: delete `try`); and **`MZ0992`** from `mz run`. `MZ0902` accepts `fn main: result(none, E)`. Columns reuse `MZ0302`, `MZ0303`, `MZ0704` and `MZ0711`. In a program, `?` is now lexed as an operator; after a type (`int?`) it is still `MZ0104` with the same text. No component or service diagnostic changes.
- **The lowering** (RFC-0013 §14.2's rows): `result(T, E)` is `Result<T, E>`, `return error(e)` is `return Err(e);`, success is `Ok(v)`, `try e` is `e?`, and a `match` on a result is C4's Rust `match` with `Ok(v)` and `Err(e)` patterns. `main` returning a result is lowered as `mz_main_result`, beside a generated `mz_main` that reports its error and exits 1. The generated code holds no `unwrap`, `expect`, `panic!` or `unsafe`, which a test checks.
- **The language harness registers C9** (RFC-0012 §1.2): feature entries for `result`, `error`, `match on a result`, `main returning a result` and `postfix ?` (the spelling the lexer reads, which the `exact` fix rewrites as `try`), the prefix operator `try`, and the `enum` entry's columns; code entries with triggers for `MZ0950`–`MZ0954`, and `MZ0992`, which only `mz run` reports (the harness test's list of untriggered codes is now `MZ0990`, `MZ0991` and `MZ0992`). Its runnable examples, `examples/errors.mz` and two more, run with their output compared.
- **`examples/errors.mz`**: `check_age` returns an error that `describe` handles with `match` and `total_age` propagates with `try`, and `main` returns `result(none, age_problem)`. Its output is committed as `examples/errors.expected`, and CI's `lowering` job runs it with the other example programs, unchanged.
- **Tested, not measured:** 52 tests in the new `compiler/tests/program_errors.rs` (the parse; every new diagnostic, with each `exact` fix applied and the result checked again; the lowered text; and `mz run` end to end: `examples/errors.mz`, a `main` whose error exits 1 with the `MZ0992` line, and results in every form C4 built), one unit test in `intern.rs`, and one more in `compiler/tests/robustness.rs`: 100,000 prefix `try`s, `.name`s or postfix `?`s on one line, and 5,000 nested `match`es on results, each give exactly one `MZ0411` and stay within a 1 MiB stack. `cargo test` in `compiler/` went from 458 to 512 tests (from 589 to 643 in the workspace, in 24 suites with tests). No benchmark ran, and nothing here says a model handles errors better in Mzizi than in any other language.
- **Found in review and fixed, each with a test.** Before the rebase: the `exact` fix for a postfix `?` before a dot wrote `try f(x).y`, which reads `.y` off the result, and now writes `(try f(x)).y`; `throw new Error("bad")` was two diagnostics with a fix covering only `throw new`, and is one `MZ0952` with no fix; `throw Error("bad")` was fixed to `return error(error("bad"))` and is now `return error("bad")`; `Ok(…)`, `Err(…)` and `throw` around a value that did not parse built a fix that wrote the placeholder `…` into the file, and add nothing now; a `var` holding a result was `MZ0950` and `MZ0924`, and is `MZ0950` alone; a result-typed last line in a result function was `MZ0950` and `MZ0906`, and is `MZ0906` alone, whose `exact` `return` fix now also fits the success value; `return try g(x)` in a function returning nothing was `MZ0951` and `MZ0908`, and is `MZ0951` alone; an unknown return type was `MZ0701` and `MZ0906` with an `exact` fix, and is `MZ0701` alone. In the rebase onto C4: a `match` on a result used as a value whose branches read the case's name lowered as `()` and failed `rustc` (`MZ0990`), and now runs; a case's name was left unbound when the value matched was already an error, so each use was a spurious `MZ0920`; a malformed result case (`case ok v w`, `case okay v`) was one diagnostic per word and an `MZ0707` per use of its names, and is one `MZ0917`; a case that binds a name in a `match` on an enum was `MZ0917` and `MZ0708`, and is `MZ0917` alone; and a bare `result` was `MZ0919` ("not built") and is `MZ0306`, naming its two types. A `/code-review` of the merged diff then found: a `let` holding a result in a `for each` body was never reported (its scope ended without the check), and is `MZ0950`; `return red` in a function returning `result(color, E)` did not read `red` against `color`, and does; a result parameter's calls each added an `MZ0950` or `MZ0905`, and say nothing more; `try f(1 +).say` built an `exact` fix holding the placeholder `…`, and builds none; an unread result read only on lines the parser skipped was `MZ0950`, and waits as `MZ0924` does; `MZ0953` was missed for a bare `ok` or `error` read against an expected type (an annotated `let`, `error(…)`'s argument), and is reported; a variant missing a column put `MZ0303` on every variant that had it, and now blames the one that lacks it; `let x: int = f()` with `f` a result was `MZ0711`, and is `MZ0950`; and `MZ0950`'s `try` guess on a condition is offered only when the success is a `bool`.
- **Departures recorded in RFC-0013 §18.6**, where the code is the fact: the `MZ0992` line's wording; `else` in a `match` on a result; how the parser tells a case's binding from a second variant; `throw`'s fix `exact` only when the parser can see the error type; a `var` may not hold a result; `error(e)` may be bound by a `let`; how a `try` block is skipped; and the interned result types.
- **Not built, and not claimed:** converting one error type to another on `try` (`via`, `MZ0955`) and `MZ0950`'s `match`-stub fix where `try` does not fit (RFC-0013 §12.3; `MZ0950` has no fix there), both the "C9 via" follow-up; `mz run --agent`; and the change to `benchmarks/prompts/mzizi-guide.md` that RFC-0013 §18.5 asks for, so no benchmark arm knows about results yet.
- `LANGUAGE-TRACKER.md`: **C9 moves from 📝 to 🟡, done in this pull request**: its "Done when" test is `compiler/tests/program_errors.rs` with `examples/errors.mz`, with its harness entries checked by `compiler/tests/harness.rs`, and the row turns ✅ when this reaches `main`. C4's row notes that its `match` covers a result. `README.md`, `AGENTS.md` and `CONTRIBUTING.md` give the new test counts.

### Added — control flow in a program's function bodies: RFC-0013 §7, C4 (2026-10-08)

- **`else when`, `match`, `for each`, `while`, `break`, `continue`, and `when` and `match` used as values, in a `program`'s function bodies** (RFC-0013 §7, Wave 1's C4 in §18.2; Refs #69; built on the foundation slice, #80). An `else when` continues a `when` and shares its `end`; the chain is kept flat, so its length costs no nesting. `match <value>` takes `case` lines, each listing one or more values (`case 6 7`), then an optional `else`, over an enum, an `int`, a `text` or a `bool`. A `when` or `match` may be the whole value of a `let`, a `var`, an assignment or a `return`, each branch one line, an expression. `for each i in range(a, to = b)` counts from `a` up to but not including `b` (the amended RFC's labelled form; `to` is the one label read, `to: b` is `MZ0927` with the `exact` fix `to = b`, and the positional `range(a, b)` is still accepted until §6.5's labels are built); `while <condition>` is the conditional loop; `break` and `continue` act on the innermost loop. A program may now declare an **`enum`**, one variant name per line, closed by a bare `end`; its variants are written bare (`circle`) or with their enum (`shape.circle`), print as their names, and order by declaration. New modules: `compiler/src/parse/program/control.rs`, `compiler/src/program/control.rs`, `compiler/src/run/control.rs`.
- **New diagnostics from RFC-0013 §16:** `MZ0930` (a `match` that misses a case: it names the missing variants, or asks for `else` on an `int` or a `text`; no fix), `MZ0931` (a case that can never run: a value listed twice, a case whose every value an earlier case takes, a case after `else`, or an `else` on a `match` that covers every value, each with an `exact` fix that deletes it), `MZ0932` (a `when` or `match` used as a value inside a larger expression or as an argument, without `else`, not covering every variant, with a branch that is not one value line, or with branches of different types; no fix), `MZ0933` (`elif` and `else if` to `else when`, `switch` to `match`, `default:`, `case _` and `_ =>` to `else`, all `exact`), `MZ0934` (`for x in xs` gets `each`, `for (const x of xs)` becomes `for each x in xs`, `loop` becomes `while true`, `range(n)` becomes `range(0, to = n)`, all `exact`; `for (const k in xs)`, which iterates keys in TypeScript, gets the same rewrite as a `guess`; a C-style `for (…; …; …)` and `do … while`, no fix), `MZ0935` (`break` or `continue` outside a loop) and `MZ0936` (an `else when` chain whose every condition is `<name> is <variant>` of one enum, as statements or as a `when` used as a value, with a `guess` fix rewriting it as a `match`, or a `match` used as a value, in canonical form). `MZ0927` is emitted for one case only: `range(a, to: b)`, `exact` to `to = b`, as one diagnostic. `MZ0937` now also deletes a trailing `:` on a `while`, `for each`, `match` or `case` line. Reused codes reach the new forms: `MZ0708` (a variant its enum does not have, with the nearest as its fix; a bare variant two enums share where no type is expected, with a `guess` fix naming its enum: a comparison, a `case`, an annotated binding, an assignment, a `return` and an argument each read a bare variant against the type they expect, and in a comparison of two bare variants the one that belongs to one enum is that type, so `amber is blue` reads `blue` as `light.blue`; the `say` names each variant once, even one listed twice), `MZ0704` (an enum or a variant declared twice; a repeated variant's line is deleted, `exact`), `MZ0711` (`for each` over an `int`, with the `guess` fix `range(0, to = n)`, never written over a source the lexer cut short, so `for each k in [1, 2]` is its two `MZ0104`s alone, and a `match` over a `float`, which takes no cases: compare a float with `when`), `MZ0712` (a `while` condition that is not a `bool`), `MZ0906` and `MZ0907` (a `match` whose every case returns, and a `while true` no `break` leaves, end a path; a line after `break` or `continue` never runs), `MZ0921` (a binding or a `fn` named after an enum or a variant), `MZ0920` (a `for each` binding read after its loop, with no fix, since a `var` of that name would clash with it), `MZ0922` (assignment to a `for each` binding), `MZ0924` (not reported in a function with lines skipped unread, such as a `do … while`, a C-style `for` or a block past the nesting cap, since those may assign the `var`) and `MZ0411` (each loop and `match` is one level of the program's nesting cap).
- **Retired from `MZ0919`:** `while`, `for each`, `match`, `else when`, `break`, `continue`, `loop` (now `MZ0934`) and a program's `enum`. Still `MZ0919`: an enum variant with a column, `range(a, to = b)` anywhere but a `for each` line (it is a list, C7's), lists, methods on text, records and a program's `contract`.
- **The lowering** (RFC-0013 §14.2): an enum is a fieldless Rust `enum` deriving `Copy`, `Eq` and `Ord`, with a text form that writes the variant's name; `when` / `else when` / `else` is `if` / `else if` / `else`; a `match` on an enum or a `bool` has no `_` arm unless it has an `else`, so `rustc` checks exhaustiveness again; a `match` on a `text` matches its `&str`; `for each i in range(a, to = b)` is `for i in a..b`, building no list; `while true` is `loop`; a `when` or `match` used as a value is a Rust `if` or `match` expression. The generated code still holds no `unwrap`, `expect`, `panic!` or `unsafe`.
- **`examples/control.mz`** prints FizzBuzz from 1 to 15, matches over an enum, an `int` and a `text`, and uses `when` and `match` as values, `while true` with an early `return`, `break`, `continue` and a Collatz count; its output is committed as `examples/control.expected`, which CI's `lowering` job compares through `mz run`.
- **Tested, not measured:** 53 tests in the new `compiler/tests/program_control.rs` (parse, every new code with its `exact` fix applied and the result checked again, `MZ0936`'s `guess` applied and checked, for a chain of statements and for a `when` used as a value, the lowered text, `mz run` with exact standard output, a trap inside a `while true` and inside a `match` used as a value each exiting 101 with `MZ0991`, and 40 nested loops and `match`es as one `MZ0411`) and one more in `compiler/tests/robustness.rs` (5,000 nested `while`s, `for each`es, `match`es, or `when`s around a `when` used as a value, are each one `MZ0411`; 27 nested `for each`es check clean and lower on a 1 MiB stack; a 10,000-link `else when` chain checks and lowers on it). `cargo test` in `compiler/` went from 403 to 458 tests on `staging` with the language harness, #88 (from 534 to 589 in the workspace, in 23 suites). No benchmark ran, and nothing here says a model writes these forms better than in any other language.
- **Not built, and not claimed:** `for each` over a list or a map (C7), `x in [variants]` in a chain `MZ0936` reads, a `match` on a `result` (C9; #87 builds its own, and RFC-0013 §18.6 says how this `match` is meant to take it over on rebase), §8's options in function bodies (§18.2's "C4 options" follow-up), block punctuation `{` and `}` (`MZ0937`'s other half), and enum columns in a program. `benchmarks/prompts/mzizi-guide.md` is unchanged. RFC-0013 §18.6 records where the code departs from the RFC's text.
- **The language harness registers C4** (RFC-0012 §1.2, the owner's rule that a row is done only with its harness entries): `compiler/src/harness.rs` gains a feature entry each for `else when`, `match`, `for each` (with `range`), `while`, `break`, `continue`, `when or match as a value` and a program's `enum`, each with a runnable example and its expected output (`examples/control.mz` among them), and a code entry with a trigger for `MZ0930`–`MZ0936` and `MZ0927` (60 codes registered, from 52). `statement_entry` and `expression_entry` name an entry for every new statement and expression form; `when`'s entry no longer says `else when` is unbuilt, and `MZ0919`'s say no longer lists the forms C4 retired. `Ty` is no longer declared with `listed_enum!`, because `Ty::Enum` carries its enum's name: `Ty::ALL` lists the built-in types by hand, and an exhaustive `match` in `Ty::listed` stops a new variant compiling until it is listed. An enum is the program's own type, so it is not a surface type in the harness's type list.
- `LANGUAGE-TRACKER.md`: **C4 is 🟡, done in this pull request**: its "Done when" test is `compiler/tests/program_control.rs`, with its harness entries checked by `compiler/tests/harness.rs`, green on the branch, and the row turns ✅ when it reaches `main`.

### Added — the language harness, first slice: `mz harness` and one entry per built feature (2026-10-07)

- **Tested** (`cargo test`, gated in CI; 403 tests in the compiler crate, 18 of them in the new `compiler/tests/harness.rs`; 534 in the workspace). **`compiler/src/harness.rs`** is the language harness's registry ([RFC-0012](./design/RFC-0012-harness.md) §1.1), with no new dependency. It registers:
  - `program`, and everything RFC-0013 builds so far: `int`, `float`, `bool` and `text`; the 13 binary and 2 prefix operators; the 11 numeric methods (`to_float`, `to_int`, `round`, `floor`, `ceil`, `sqrt`, `is_nan`, `abs`, `min`, `max`, `pow`); `fn`, `let`, `var`, assignment, `when`, `return`, expression statements, `print`, text literals, comments and names;
  - 52 diagnostic codes, each with its RFC section, severity, `say` text, fix kinds and a trigger: each `MZ09xx` a program can raise (`MZ0901`–`MZ0926`, `MZ0937`, `MZ0962`, `MZ0980`, and `mz run`'s `MZ0990` and `MZ0991`) and the 21 shared codes a program reuses (`MZ0101`–`MZ0106`, `MZ0204`–`MZ0208`, `MZ0306`, `MZ0310`, `MZ0407`, `MZ0411`, `MZ0701`, `MZ0707`, `MZ0708`, `MZ0711`, `MZ0712`, `MZ0714`);
  - the nine `mz` commands.
  - `component` and `service` are kind-level entries only, and their 55 codes are on a sorted pending list that a test freezes, so it can only shrink.
- **What has one source, and what is a parallel copy held by tests.** One source:
  - `mz` dispatches on the registry's command table, and its usage message is generated from it;
  - the program parser accepts exactly the surface types' names (`Ty::from_name`);
  - each operator's spelling, precedence and operand types, the operator and type lists (`ALL`, now generated with each enum by one macro in `compiler/src/expr.rs`) and each method's signature are read from the checker's own tables.

  Parallel copies, which the checker does not read: each code's severity, `say` and fix kinds, and each feature's grammar, teaching text and examples. `compiler/tests/harness.rs` fails when:
  - the compiler source holds a code with no entry and not pending, or an entry names a code nothing emits;
  - a trigger stops reporting its code at its severity with a declared fix kind;
  - an example stops checking with no diagnostic, or a program example stops printing its stated output under `mz run` (skipped, and said so, without `cargo`);
  - an operator the lexer reads has no entry, or a rejected spelling is not reported with the code its entry names;
  - a surface type or numeric method has no entry.

  Each code's `say` text and each feature's grammar and teaching text are written by hand and not compared with the checker; only a length cap on `say` is tested. Fix kinds are recorded as raised, so an agent can still receive a `guess` (an overlapping `exact` fix, demoted) on a code whose entry lists only `exact`.

  In a debug build every report `check`, `check_contract`, `check_program` and `check_with_ast` return is checked against the registry, before overlapping fixes are demoted. A code with no entry, another severity, or a fix kind its entry does not declare panics; a release `mz` skips the check. That check found six wrong declarations in the full test suite (`MZ0101`, `MZ0707` and `MZ0708` carry `exact` and `guess` fixes; `MZ0905` a `guess`; `MZ0910` and `MZ0911` sometimes none), now corrected. Statement and expression forms, operators and types are matched exhaustively, so a new one does not compile until it names its entry.

- **`mz harness version`** prints `{"protocol":1,"language":"phase-0, RFC-0013 wave 0 and wave 1 numbers","definition_sha256":…}`. No crate version changes; the definition's SHA-256 pins its content. **`mz harness definition [--agent]`** prints every entry as deterministic, hand-serialised JSON, indented or on one line. **`mz harness entry <name> [--agent]`** prints one entry (`mz harness entry is not`, `mz harness entry MZ0905`). They exit 0, or 2 for a usage problem or an unknown name.
- **Rule** (owner, 2026-10-07: "The harness work should be part of the build"): a pull request that adds or changes a language feature adds or updates its harness entry in the same pull request. It is written into AGENTS.md, CONTRIBUTING.md, CLAUDE.md, `LANGUAGE-TRACKER.md`'s preamble (a row's "Done when" includes its entries) and RFC-0013 §18. H1's notes say what exists; H1 stays 🟡.
- **Designed, not built:** the plugin host and `mz harness plugins`; generating the skills and the benchmark guides from the definition; the checker reading its rules from the registry rather than being checked against it (RFC-0012 §1, §4, §7). No benchmark was run. Whether an agent taught by the language harness does better is a hypothesis the benchmark is designed to test (RFC-0012 §9, question 9), not a result.

### Changed — RFC-0012 amended: the language harness, the spine of the language (2026-10-07)

- **Design only; nothing is measured.** [RFC-0012](./design/RFC-0012-harness.md) is renamed **the language harness** throughout, from the owner's direction of 2026-10-07 (quoted there, lightly edited): every feature of the language is defined once, as a harness entry inside the compiler, and the checker's tables where practical, `mz harness definition`, the agent skills and the benchmark guides are generated from or checked against those entries. `benchmarks/harness/` keeps its name, the benchmark harness.
- §1 is restated as **the spine**, with **§1.1, the harness entry** (name and kind, grammar, a one-paragraph `teach` text, type rules, diagnostic codes with `say` text and fix kind, and tested examples) and **§1.2, the registration rule**: a pull request that adds or changes a feature adds or updates its entry, and a test fails when the compiler emits a code or builds a construct with no entry. §4.1 now says first-party features register through the same interface as third-party plugins, which may add and may not change the language's meaning.
- §5 designs `mz harness version`, `mz harness definition [--agent]` and `mz harness entry <name>`; §7 says the skills and the benchmark guides are generated from the definition; §9 decides question 3 (the definition comes from the compiler's own registered entries, not a hand-written file) and adds question 9: whether an agent taught by the language harness does better is a hypothesis for the benchmark to test, not a result.
- AGENTS.md, README.md, `LANGUAGE-TRACKER.md` (H1 and H2's wording; H1 stays 🟡, H2 stays 📝) and RFC-0013 §18.5 use the new name. Refs #69.

### Added — `float`, numeric methods and the rest of C1 in programs: RFC-0013 §3 and §4 (2026-10-07)

- **`float` in a `program`** (RFC-0013 §4.2, Wave 1's C1 + C5 of §18.2; Refs #69). Literals are digits, `.`, digits (`1.5`, `0.25`); `float` is a type for bindings, parameters and returns. Arithmetic is IEEE 754 binary64 and never traps: `1.0 / 0.0` is `inf`, `0.0 / 0.0` is `nan`, `nan is nan` is `false`, and `%` is the truncated remainder (`-5.0 % 3.0` is `-2.0`). `int` and `float` never mix: `1 + 1.5` is `MZ0912`, with the `exact` fix `1.0` on an `int` literal and the `guess` `x.to_float()` on anything else, in arithmetic, comparisons and `min` / `max`.
- **The numeric methods of §4.4**, the first methods a program has: `to_float` on `int`; `to_int`, `round` (half away from zero), `floor`, `ceil`, `sqrt` and `is_nan` on `float`; `abs`, `min`, `max` and `pow(n)` (an `int` `n`) on both. Methods are postfix (§3.5 level 2), so `-2.pow(2)` is `-(2.pow(2))`, and `2.pow(10)` and `1.0.to_int()` lex as §3.1 says. A method a number lacks is `MZ0708` with a nearest-name or conversion fix (`n.sqrt()` on an `int`: `guess` `n.to_float().sqrt()`); wrong arguments are `MZ0905`; a method without parentheses is `MZ0962` (`exact` `()`); methods on `text` stay `MZ0919` (C6's).
- **A float prints in §3.8's form**: the shortest digits that read back, plain between `1e-6` and `1e21` (`1.0`, `0.30000000000000004`, `0.000001`), otherwise `1.0e21`, `1.5e-7`, and `inf`, `-inf`, `nan`, `-0.0`. The function is `compiler/src/numbers/float_text.rs`, compiled into `mz` and emitted verbatim into every lowered program.
- **Traps (§4.3), each `MZ0991` with exit 101:** `pow` and `abs` on `int` overflow, a negative `int` exponent, and `f.to_int()` on `nan`, an infinity or a value outside `int`. At check time, `MZ0915` adds a literal negative `int` exponent (`2.pow(-1)`) and a folded overflow (`2.pow(64)`, `(-9223372036854775807 - 1).abs()`).
- **New diagnostics in a program:** `MZ0913` for `and` mixed with `or` without parentheses (`exact`: the parentheses §3.5 implies; `&&`, `||` and `==` inside are folded into the one fix); `MZ0914` for a malformed number (`1.` and `.5`, `exact` `1.0` / `0.5`; `1_000`, `exact` `1000`; `0x10`, `0b1`, `1e3`, and a float literal too large for `f64`, no fix); `MZ0911` for `/* … */` on a line of its own (`exact` `##`) and for `//`, `#` or `/* … */` after code or before it on the line (`guess`: the comment moves to a line above; code after a closed `/* … */` is still read); `MZ0910` for `not a is b` (`exact` `a is not b`) and for Python's `**` (`guess` `x.pow(n)`; with the exponent `0.5`, `MZ0905` adds the `guess` `x.sqrt()`, and any other fractional exponent gets no fix); `MZ0962` for Python's free functions `float(x)`, `int(x)`, `abs(x)`, `round(x)`, `min(a, b)`, `pow(a, b)` and the rest (`exact` method calls, except a `guess` for `round(x)`, whose ties differ from Python's and JavaScript's, and for `pow` unless the exponent is a non-negative `int` literal), and for `str(x)`, `String(x)` and `x.to_string()` on any value with a text form (`exact` `"{x}"`, or the value itself when it is already `text`; no fix on a value with no text form). `MZ0914` quotes at most 20 characters of a long literal. `MZ0712`'s fix on a `float` condition is `x is not 0.0`, and `f64` / `double` as a type get `MZ0701`'s `float`.
- **`examples/numbers.mz`** computes areas, a mean, a hypotenuse, `0.1 + 0.2`, the infinities and `nan`, and prints them; `examples/numbers.expected` holds its output, and CI's existing `mz run` step compares the two.
- **Tested, not measured:** 23 tests in the new `compiler/tests/program_numbers.rs` (the parse, every new diagnostic with its `exact` fix applied and re-checked, the lowered text with no `unwrap`, `expect`, `panic!` or `unsafe`, and `mz run` with exact standard output, standard error and exit status for IEEE 754 cases and seven traps) and 3 unit tests in `compiler/src/numbers.rs`; `cargo test` in `compiler/` went from 359 to 385 tests (from 490 to 516 in the workspace, in 21 suites). No benchmark was re-run, and `benchmarks/prompts/mzizi-guide.md` does not describe any of this yet (§18.5).
- **Not built:** `in`, indexing and collections (C7), text methods (C6), records and their methods (C8), a decimal type, exponent literals (§3.1, P2), and the `wrapping_*` methods of §4.4 (the C5 follow-up, §18.2). `nan.min(1.0)` is `1.0`, as Rust's `f64::min` gives; RFC-0013 does not say, and §18.6 records it.
- `LANGUAGE-TRACKER.md`: **C1 and C5 meet their "Done when" on this branch** and stay 🟡 until it reaches `main`, as the tracker's legend says.

### Added — `benchmarks/perf/`, a performance suite against hand-written Rust (2026-10-07)

- **`benchmarks/perf/`** (Refs #69, the owner's decision of 2026-10-07: "Keep the overflow checks and remove them where the compiler can prove they are unnecessary … Build the benchmark"). Four programs in RFC-0013's foundation slice, `fib`, `ackermann`, `collatz` and `interpolate`, each with its exact output committed as `programs/<name>.expected` and a hand-written, std-only Rust reference in `rust/src/bin/`, built twice from one package: `--release` with Rust's release defaults (overflow unchecked) and `--profile release-checked` with `overflow-checks = true`, so overflow on `+`, `-`, `*` and unary `-` panics with exit 101, as Mzizi's lowering traps with exit 101 (`MZ0991`). That is the extent of the match: Rust panics where Mzizi writes one line and calls `process::exit`, Rust panics on `int` minimum `% -1` where Mzizi returns 0, a closed standard output exits 101 in Rust and 141 in Mzizi, and division by zero panics in both Rust profiles; the folder's README lists these, and none of the four programs reaches them. CI's `benchmarks` job runs `cargo fmt --check` and `cargo clippy -D warnings` on that package, which is its own workspace root.
- **`benchmarks/perf/run.sh`** builds each program three ways (`mz build --out` then `cargo build --release --offline`, and the two references), fails unless all three print the `.expected` file, and measures the median of N runs' wall time (default 7, interleaved), max RSS (with GNU `time`), binary size, cold release build time and `mz check` time. It writes `perf.json` (schema `mzizi-perf/1`, in the folder's README) and `perf.md`, both naming the CPU, cores, OS, `rustc`, commit and date. `--check-only` builds and compares and times nothing, and `MZ=<path>` reuses an existing `mz`. It exits 1 when outputs differ or when a binary, `mz check` or GNU `time` exits non-zero during timing (that program is reported and not timed, the rest still are, and `perf.json` and `perf.md` are still written, each failed program with an `error` saying what failed), 2 on usage, a missing tool, or a program name that does not match `^[a-z0-9_]+$` (checked before `mz` is built), and 3 when a build fails. Linux only: it needs bash 4 and GNU `date`.
- **CI's `lowering` job runs `benchmarks/perf/run.sh --check-only --out "$RUNNER_TEMP/perf"`**: correctness only, no timing gate, no network beyond what the job already uses, and its 12 throwaway target directories outside `./target`, so the Rust cache does not save them. CONTRIBUTING.md ("What CI does not check") now lists everything the `lowering` job gates: the example service, `mz run` of every example program, and this suite.
- **Measured, not claimed.** The pull request's body has one measured run, labelled with its machine; no result is committed here, and none is a claim that Mzizi is faster or slower than Rust in general. No pass yet removes an overflow check the compiler can prove unnecessary; that is the owner's goal, and this suite is how it will be measured. `LANGUAGE-TRACKER.md` and `benchmarks/READINESS.md` list the suite as runnable, with CI gating output agreement only, no timing committed, and outside the kill criterion.

### Changed — the pre-commit hook: CI runs its test, a missing rustfmt, and its limits written down (2026-10-07)

- **CI's `compiler` job now runs `scripts/test-pre-commit.sh`**, with the caller's global and system git config ignored. It has 16 cases, all passing locally. The new ones: an entry added and then removed on a branch does not count, a branch cut from a staging commit that touches `CHANGELOG.md` is refused, and a cargo without rustfmt passes with a notice. When `cargo fmt --version` fails, **`.githooks/pre-commit`** now says so, names `rustup component add rustfmt`, and lets the commit through to CI's fmt check. Before, it refused the commit with advice to run a `cargo fmt` that could not run. The hook's changelog rule is unchanged. The hook, AGENTS.md and CONTRIBUTING.md now say it trusts the local `origin/staging` and `origin/main`: when those are stale, or the branch is stacked on another unmerged branch, other people's entries count as the branch's, so fetch first. They also say, as `scripts/install-hooks.sh` does, that the hook runs the checked-out branch's code. Read changes under `.githooks/` before committing on someone else's branch, or use `--no-verify`. Refs #69.

### Added — a pre-commit hook: a changelog entry on every branch, and cargo fmt (2026-10-07)

- **`.githooks/pre-commit`** refuses a commit unless `CHANGELOG.md` is staged, or the branch's changes since it left `origin/staging` or `origin/main` already include it. The hook compares the branch with whichever of those two refs it shares the most recent history with, so a branch cut from `main` is not credited with `main`'s own release commits; with neither ref, only the staged changes count. It has no path exemptions, the same rule as `changelog / entry required`: every change needs an entry. `MZ_NO_CHANGELOG=1` skips the check and prints a notice, the local twin of the `no-changelog` label. When staged files include Rust under `compiler/` or `benchmarks/`, it also runs `cargo fmt --all -- --check` (against the working tree, not the index), and nothing slower. **`scripts/install-hooks.sh`** sets `core.hooksPath` to `.githooks` for a clone and every worktree of it, and says what it replaced. AGENTS.md ("Changelog") and CONTRIBUTING.md say to run it once per clone, and that agents run it before their first commit. Owner rule, 2026-10-07: "We need to always have change log updates." Refs #69.
- **`scripts/test-pre-commit.sh`** tests the hook in a throwaway repository: 12 cases, all passing locally (a commit without an entry is refused, one with it passes, `MZ_NO_CHANGELOG=1` passes, a branch cut from a `main` that has diverged from `staging`, the `origin/main` and no-ref fallbacks, a linked worktree, unformatted Rust refused, formatted Rust passes). CI does not run it. A hook is local and `git commit --no-verify` bypasses it, so the CI job is still the check on the pull request.

### Changed — RFC-0013: amendments from the top-10 language survey, design only (2026-10-07)

- **`design/RFC-0013-core-language.md` is amended from `design/LANGUAGE-SURVEY.md`** (PR #75, not merged yet; Refs #69). Design only: no compiler code changes, no diagnostic is emitted, and nothing was measured. The survey's measured findings were measured on other languages, not on Mzizi. The amendment adds:
  - **Named arguments** (§6.5): the first argument by position, every later one labelled `name = value` with its parameter's name, in declaration order; one name per parameter; defaults are literals only. Built-in functions and methods are labelled too (`range(0, to = n)`, `xs.fold(0, step = add)`). The new `MZ0927` covers label mistakes, with `exact` fixes where only one program fits.
  - **Records** (§11.1, §11.4): construction names every field, in declaration order, with no zero values (`MZ0808`, `exact` fix for positional construction). `p with (x = 3.0)` is a copy-and-update expression (new `MZ0974`). Equality, a text form and `to_json()` come with every record. A record's `always` clauses are checked at every construction and change, in every build, and a broken one traps with exit status 101 (`MZ0991`); a malformed clause, or a literal construction that folds to `false`, is the new `MZ0975` at check time. `changes self` marks the one kind of method that mutates its receiver (new `MZ0976`).
  - **Enum payloads** (§11.5): one `enum` construct for static columns and payloads, a `case` that narrows instead of binding, and `<enum>.variants()` (new `MZ0973`).
  - **Options** (§8): a guard `when x is none … return … end` narrows `x` for the rest of its block. **`otherwise`** is the one default word (`x otherwise d`, with `d` evaluated only on `none`) and replaces the draft's `x.or(d)`. `??`, `.unwrap_or` and the like get the new `MZ0938` with `exact` fixes, and `?.`, `x!` and `.unwrap()` on an option are the new `MZ0939`.
  - **Collections** (§3.7, §9.4): `xs[i]`, `s[i]` and `m[k]` return an option instead of trapping, and `.get` is gone. Eight named folds form a closed set: `count`, `sum`, `any`, `all`, `first`, `fold`, `sort_by`, `group_by`.
  - **Errors and numbers** (§12.2, §4.4, §5.1): `try e via f` maps an error type explicitly (new `MZ0955`), and an unhandled result gets a `match`-stub fix. `wrapping_add`, `wrapping_sub` and `wrapping_mul` are the only ways to wrap. An unread `let` or `var` is the new error `MZ0928`.
- §16's free list is updated to match these nine codes. §18.2 now says C7 and C8 build the amended design, and it adds five follow-up pull requests for rows built or in flight before the amendment (C3 labels, C2 unused bindings, C4 options, C9 `via`, C5 wrapping). §20 grows from 20 to 29 open questions. Q21 proposes splitting `LANGUAGE-TRACKER.md`'s C8 into "records and methods" (M1) and a new "generics and interfaces" row after M1. The tracker's C8 row is unchanged, and changes only when the owner answers. The new §21 compares every survey row (F1–F26, F33, R1–R12) with the RFC and says where the RFC departs from the survey, and why.
- RFC-0001 (§4.7's `MZ0106`), RFC-0007 (D1) and RFC-0010 (§4.3's `always` lowering) each gain a note naming what the amendment proposes for them. None takes effect until the owner accepts RFC-0013.

### Added — `program`, `fn` and `mz run`: RFC-0013's foundation slice (2026-10-07)

- **A `program` file kind with `fn main`** (RFC-0013 §1, Wave 0 of §18.1; Refs #69). A program holds `fn`s: `fn name(a: int, b: text): int` … `end fn name`, with typed parameters, a return type or none, calls and recursion. A function body has `let`, `var` and assignment, `when` / `else`, `return` and `print(v)`. Values are `int`, `bool` and `text`, with `+ - * / %`, unary `-`, `is`, `is not`, `< <= > >=`, `and`, `or`, `not`, parentheses and §3.5's precedence, and `{expr}` interpolation and the escapes `\n \t \" \\ \{ \}` in text. The lexer reads operators and escapes only in a file whose first line is `program …`, so no component or service diagnostic changes. New modules: `compiler/src/program.rs` (tree and checker), `compiler/src/parse/program.rs` (parser), `compiler/src/expr.rs` (expressions), `compiler/src/run.rs` (lowering and `mz run`).
- **`mz check` types a program**, one diagnostic per true error, with these codes from RFC-0013 §16: `MZ0901` (a line a program cannot hold, or statements outside every `fn`), `MZ0902` (`fn main` missing, doubled, with parameters or a return type), `MZ0903` (signature idioms: `->`, `def`/`function`/`func`, `()` on no parameters, `: none`, all `exact`; an untyped parameter or a default value, no fix), `MZ0904`, `MZ0905` (quoting the signature), `MZ0906` (`exact` `return` when the last line is a value of the return type), `MZ0907` (`exact` deletion of lines after `return`), `MZ0908`, `MZ0909` (`exact` `()` on a zero-parameter `fn` used as a value), `MZ0910` (`==`, `!=`, `&&`, `||`, `!`, `at_least`, `at_most`, all `exact`), `MZ0912` (operand types; `+` on text is the concatenation idiom, with the interpolated text as its fix), `MZ0913` (chained comparison only, `guess`), `MZ0915` (division by a constant zero, a constant overflow), `MZ0916`, `MZ0918` (`+=`, `++` and the rest, `exact`), `MZ0920`–`MZ0926` (scope, shadowing, `let` / `var`, Go's `:=`, Rust's `let mut`, all with the fixes the RFC names), `MZ0937` (a trailing `:` on a block line only, `exact`) and `MZ0980` (`print(a, b)`, `console.log`, `println!`, `fmt.Println`, `print x`, all `exact`). Two codes are claimed from §16's free range and recorded in RFC-0013 §18.6: **`MZ0917`**, a line or expression a function body cannot read, and **`MZ0919`**, a form RFC-0013 designs that this slice does not build (`while`, `for each`, `match`, `else when`, `float`, methods, a program's `contract` block), reported once with its block skipped. Existing codes keep their meaning in a program: `MZ0204`–`MZ0208` for closers, `MZ0407` for `if` (`exact` `when`), `MZ0701`, `MZ0707`, `MZ0711`, `MZ0712` (truthiness, with a `guess` question), `MZ0714`. `mz fix` applies the `exact` fixes; `compiler/tests/program.rs` applies each one and checks the result again.
- **`mz run <file.mz>`** checks a program (errors: exit 3, nothing runs), lowers it to a Cargo package with **no dependencies** in `<name>-<path hash>` under `$MZ_CACHE_DIR`, or `mzizi/mz-run/` under the user's cache directory (`$XDG_CACHE_HOME` or `~/.cache`), so no other account can plant files in a package it builds; with none of the three set it exits 2 and names them, and it never falls back to the system's temporary directory; builds it with `cargo build --offline --quiet`, and runs the binary with the terminal's standard input, output and error, exiting with its status; warnings are printed to standard error first. Only `rustc` rejecting the lowered code is `MZ0990`; any other `cargo` failure (an old toolchain, a full disk) exits 2. `--release` builds the release profile. Integer overflow and division or remainder by zero **trap**: one line, `mz: trap MZ0991 at <file>:<line>:<column>: integer overflow in <expression>`, and exit 101; division truncates toward zero and `%` takes the dividend's sign. A closed standard output exits 141. Lowered code that `rustc` rejects is reported as `MZ0990` with `rustc`'s first message, and exits 3. **`mz build <program.mz> --out <dir>`** writes the same package. `mz outline`, `mz ir` and `mz hash` on a program exit 2 and say they do not cover programs yet; `mz contract` on a program exits 0 with no clauses.
- **`examples/hello.mz`** prints `hello, world`, and **`examples/fib.mz`** computes `fib(9)` = 34 and `fib(20)` = 6765 by recursion; their output is committed as `examples/hello.expected` and `examples/fib.expected`. **CI's `lowering` job gains a step** that runs every example program through `mz run` and diffs its standard output against the `.expected` file, with no network and no secrets.
- **A program's nesting is capped, as a component's and a service's are, with `MZ0411` once per file** (RFC-0001 §4.7 item 8; RFC-0013 §18.6). The program parser recursed once per `(`, `not`, prefix `-` and nested `when`, with no limit, so in a debug build 2,000 nested parentheses on one line, 3,000 nested `when`s or 50,000 prefix `not`s aborted `mz check` with a stack overflow, the vulnerability SECURITY.md names. A program's blocks and expressions now share one budget of 32 levels (the program, the `fn`, each `when` and `else when`, and each expression read inside another: a statement's value, a `(`, a call's arguments, an interpolation, a `not`, a prefix `-`), and an expression's tree of binary operators may be 64 deep, so a chain such as `1 + 1 + …` may be about 64 long. That is half a component's cap, because a level of a program cost 12 to 16 KiB of stack in a debug build, measured, and the tests hold every case to a 1 MiB stack. Past the cap a `when` is skipped to its `end` and the rest of an expression's line is not read, both without recursion. `compiler/tests/robustness.rs` gains two tests: 100,000 nested parentheses, `not`s, prefix `-`s, `+`s, chained `<`s and parentheses in an interpolation, and 5,000 nested `when`s, each give exactly one diagnostic, `MZ0411`; and the deepest programs under the cap check with no error and lower on a 1 MiB stack, while one level more is `MZ0411`. Its random cases now also lower every program that checks.
- **One diagnostic per true error in three more places.** A keyword written as a parameter's name (`fn twice(match: int)`) or a function's name (`fn nothing`) is one `MZ0903`; it used to add `MZ0905` at each call, because the function was registered short a parameter, and `MZ0917` at each use. An `int` literal too large is one `MZ0103`, without the `MZ0917` "expected a value" that followed it. **`MZ0911`** is built for a line that starts with `//` or `#`, with the `exact` fix `##` (a `guess` when code follows the marker with no space, as in a `#!` shebang or `#[inline]`), and a file whose first line is such a comment is still read as a program: it was lexed as a component, with no operators, and every `+` became `MZ0104`. `/* … */` and a comment after code on one line are not built.
- **Tested, not measured:** 48 tests in `compiler/tests/program.rs`, 3 unit tests in `expr.rs` and 2 more in `compiler/tests/robustness.rs`; `cargo test` in `compiler/` went from 306 to 359 tests (from 437 to 490 in the workspace, in 20 suites). The seven `mz run` tests build real packages with `cargo` and print `SKIPPED` on standard error when `cargo` is not on the path. No benchmark ran, and nothing here says a model writes Mzizi programs better than any other language.
- **Not built, and not claimed:** a program's `contract` block and the in-process evaluator for it (RFC-0013 §15.1, §15.3), `mz run --agent` (exit 2), traps as NDJSON, `MZ0990` naming the `.mz` construct, `else when`, loops, `match`, `float`, collections, text methods, records in programs and `result`. `benchmarks/prompts/mzizi-guide.md` is unchanged, so no benchmark arm knows about programs yet.
- `LANGUAGE-TRACKER.md`: **C2, C3 and C10 are 🟡, done in this pull request**: their "Done when" tests are in `compiler/tests/program.rs` and CI, and each row turns ✅ when this reaches `main`, as the tracker's legend says. C1 is 🟡 (integers and booleans, no `float`); C4, C5, P3, P4 gain evidence and stay 🟡; P10 moves from 📝 to 🟡. Each row, and a paragraph after "Where Mzizi stands today", says the evidence is in this open pull request and not on `main`. `mz`'s usage line and the `MZ0201` message now name `program`.

### Added — RFC-0013, the core language (Tier 1), a draft for review (2026-10-07)

- **`design/RFC-0013-core-language.md` designs all of Tier 1, `LANGUAGE-TRACKER.md` rows C1–C10, as one RFC** (Refs #69, the owner's "Tiers by rfc, compile to rust, methods for now, go ahead"). It specifies a `program` file kind with `fn main` and `print`; expressions and their precedence, with `is` / `is not` for equality, `and` / `or` / `not`, and interpolation as the one way to build text; `let` and `var`, block scope and no shadowing; function signatures `fn name(a: int): int` closed by `end fn name`, calls and recursion; `else when`, exhaustive `match`, `when` and `match` used as values, `for each`, `while`, `break`, `continue` and early `return` in function bodies; `float` (IEEE 754) beside `int`, whose overflow and division by zero **trap** with exit status 101; `map(K, V)` and `set(K)` in key order, no tuples, and named-function `map` / `filter` / `fold`; text operations counting Unicode scalar values; methods on records with a read-only `self`; `result(T, E)` with `return error(e)`, a prefix `try` and `match`; `mz run`, which lowers a program to a Cargo package with no dependencies and runs it; the lowering of every construct to Rust with value semantics and no lifetimes; the canonical form; and the waves that build it, each with its tracker row's "Done when" test. It reserves the diagnostic range **`MZ09xx`** (unused until now) and names 51 codes, many with `exact` fixes for the idioms small models bring from Python, TypeScript and Rust (`==`, `elif`, `x += 1`, `len(xs)`, `Ok(v)`, `x?`, `console.log`, `{ … }` blocks). Generics and interfaces are deferred past M1, as the owner decided, so §20 asks how C8 and M1 are to be reconciled; it lists 20 open questions for the owner in all.
- **Design only. Nothing in it is implemented, and it measures nothing.** No compiler code changes, no diagnostic is emitted, and no benchmark ran.
- After review, the draft states rules it had left open: `return r` of the function's own result type passes the result through (lowered `return r;`), a result can never be the success type of another (`MZ0950`), and `return try r` is the new `MZ0954` with an `exact` fix deleting `try`. An `else when` chain over one enum's variants is the new `MZ0936` (`guess` fix to `match`). The generated `main` handles a failed thread spawn and a panic explicitly, with the new `MZ0993` and exit status 70, and `mz run`'s table now lists signal deaths (134 for stack overflow). `mz contract`'s in-process evaluation of a program is bounded at 10,000,000 steps (new `MZ0994`). `mz run` writes everything of its own to standard error, unlike `mz check --agent`. Substring search is `s.contains(t)`, not `in`; an un-narrowed option cannot be printed (`MZ0710`); shadowing keeps RFC-0008's `MZ0713`; `MZ0923`'s fix is `exact` only when no near name exists and the name is not read after its block; the float text form is defined digit by digit; and `int` minimum `% -1` traps. The type names `float`, `map`, `set` and `result` may now name bindings, and `ok` / `error` may name enum variants (qualified inside a result function, `MZ0953`). Option narrowing lowers through `mz_`-named copies, so narrowed paths and assignments to a narrowed `var` have a defined Rust shape. Two open questions were added, on `map` / `filter` / `fold` beside loops (Q19) and on whether built-in text methods meet C6 (Q20). Still design only.
- `LANGUAGE-TRACKER.md` names RFC-0013 as design evidence on rows C1–C10, P4, P5 and T4. Following the legend, the ❌ rows with a design are now 📝 (C1, C2, C6, C9, C10, P5); the 🟡 rows (C3, C4, C5, C7, C8, P4, T4) stay 🟡, since part of each already exists. No row is ✅.
- RFC-0001 (§1.2, §1.5), RFC-0008 (§1, §2, §5), RFC-0010 and RFC-0011 each carry a note saying what RFC-0013 proposes to amend in them; the amendments take effect only if the owner accepts it. `README.md`'s RFC table and `design/ROADMAP.md` list it.

### Added — release notes from the changelog, and a changelog entry in every pull request (2026-10-07)

- **Each release to `main` publishes its `CHANGELOG.md` entries as release notes.** `main-release.yml` now runs `.github/scripts/release_notes.py <previous main release> HEAD`. The script compares `CHANGELOG.md` at the two commits entry by entry and prints every entry that is new, under its heading. An entry later moved from `[Unreleased]` into a dated section is not reported twice, and a reflowed one is not reported again; an entry whose words were edited is reported as new. The notes link to `CHANGELOG.md` at the release's tag, and a failed read of `CHANGELOG.md` fails the run rather than publishing wrong notes. A re-run after a failed `gh release create` now creates the missing release instead of stopping at the tag. The output becomes the release notes, followed by GitHub's generated list of merged pull requests over the same range (`--notes-start-tag`). Before, a release carried only that list. Checked by hand: `v0.3.0` → `v0.4.0` gives the 6 entries that landed between them, and the same tag on both sides prints "No CHANGELOG.md entries were added since the previous release." The workflow passes actionlint, zizmor and yamllint. No release has used it yet.
- **The `changelog / entry required` job now applies to every pull request** (owner rule, 2026-10-07: "Change log in every PR that we are doing"). A pull request that changes only `.github/**` or lint configuration used to pass without an entry; now it needs one too. The `no-changelog` label still lets one through, as a person's explicit call, and so does a Dependabot version bump (`PR_AUTHOR`). Checked with `.github/scripts/changelog-entry.sh` on a change to `.prettierrc` alone: exit 1 without an entry, exit 0 with the label or as Dependabot. The job is still not a required check. AGENTS.md and CONTRIBUTING.md say so.

### Added — a survey of the top 10 languages, feature by feature (2026-10-07)

- **`design/LANGUAGE-SURVEY.md`** surveys Python, JavaScript, TypeScript, Java, C#, Go, C, C++, Rust and Swift, checked against the TIOBE Index (September 2026) and the Stack Overflow Developer Survey (2025), and names the languages they rank that it does not cover (SQL, PHP, R, Visual Basic, Kotlin). It lists 63 deduplicated features (9 adopt, 38 improve, 3 already-has, 13 reject), each with a one-line Mzizi design, a one-line Rust lowering and its tracker row; says what RFC-0013 (Tier 1) and the future Tier 2 and Tier 3 RFCs must decide; maps each "do better" goal to an RFC-0009 task family or a proposed new task; and lists the agent failure modes reported for each language with the design choice aimed at each. Refs #69. **Design only:** nothing in it is implemented, and it measures nothing. Linked from `design/ROADMAP.md`.

## 2026-10-07

### Security — deep nesting no longer crashes `mz`, and a file name can no longer write into `mz build`'s output (2026-10-07)

- **Nesting is capped at 64 blocks, with the new error `MZ0411`.** The parser and every pass after it recurse once per nested block. 5,000 nested view elements or handler `when`/`if` blocks aborted `mz check`, `contract`, `outline`, `ir` and `build` with a stack overflow, which SECURITY.md lists as a vulnerability. The first block past that depth now gets `MZ0411`, once per file, and every such block is skipped by counting block openers against `end`s, without recursion. The skip classifies lines with the same rules the parser uses (`view_line_kind`, `handler_line_kind`), so it stops where the parser would: at the matching `end`, a declaration word, `end component`/`end service`, or the next `route`. Blocks it skipped that are still open when it stops go back on the parser's block stack, so the `MZ0204` that follows names every one and its fix inserts exactly the missing `end`s. Measured with the release binary: 100,000 nested rows gave one `MZ0411` in 211 ms, where 5,000 had aborted. The repo's deepest file nests 9 levels. RFC-0001 §4.7 gains item 8.
- **`mz build` escapes the source file's name in the comments it writes.** The `.mz` file's name goes into a `//!` comment in the generated `main.rs` and a `#` comment in its `Cargo.toml`. A file whose name was `x`, a newline, `fn injected() {}`, a newline and `.mz` put `fn injected() {}` into `main.rs` as code; a `[dependencies…]` line would have gone into `Cargo.toml` the same way. Control characters, invisible and bidirectional format characters, and `\` itself are now written as escapes, so the comment reads back one way (`x\nfn injected() {}\n.mz`) and holds none of the bidi overrides rustc rejects in comments. Other text, `café` included, is written as it is. The same escaping now applies to each contract `example`'s text, which `mz build` writes into a `///` comment above its generated test. `compiler/tests/lower.rs` holds it.
- **`compiler/tests/robustness.rs`** (8 tests, no dependencies) holds the compiler to SECURITY.md's "terminates with a diagnostic on every input". It runs `check`, `contract`, `fix` and its re-check, the NDJSON writer, `outline`, the IR and service lowering over:
  - 480 seeded random edits of the repo's `.mz` files;
  - 300 runs of random grammar tokens;
  - 200 runs of random bytes;
  - nesting up to 10,000 deep;
  - 1 MiB lines.

  Each runs on its own thread, with a 1 MiB stack (the smallest main thread `mz` gets, on Windows) and a deadline, and a failure prints its seed. Each case's label is written to `robustness-last-case-<test>.txt` in Cargo's test temp directory before it runs, so a stack-overflow abort still names it. Three further tests hold the skip to the parser's rules. Nesting full of attributes, strings, doc lines, `else`, `nothing` and `match` leaves no error after the skipped block. A short deep block before `end component` is reported once (`MZ0204`), as under the cap, with a fix that inserts exactly the missing `end`s. A deep handler, even one short an `end`, does not swallow the next route. A one-off run with 100 times as many cases on three other seeds (about 144,000 edited files and 150,000 token and byte cases) found no panic or hang.

- **The compiler and the three benchmark crates are `#![forbid(unsafe_code)]`**, library and binary. None had `unsafe` code; now none can gain it without removing the attribute. The runtime `mz build` emits already was.
- Tests: 306 in the compiler crate (was 297), 437 in the workspace (was 428), in 19 suites.

### Changed — the crate descriptions say what the crates do today (2026-10-07)

- **`compiler/Cargo.toml`'s `description`** said the crate was a "front end — lexer, recovering parser, and the agent NDJSON diagnostic protocol". It now lists what `mz` has: the name and type resolver, exact fixes, contract evaluation, the content-addressed IR, and lowering of a `service` to a local Rust + axum package. It also says no component lowers yet. **`benchmarks/runner/Cargo.toml`'s** named only the Mzizi and Dioxus arms. It now names the five arms under `benchmarks/arms`, and says backend episodes are not yet scored with probes. Metadata only: no behaviour changes, and nothing is published (`publish = false`).

### Security — the workflows are pinned, audited and kept current (2026-10-07)

- **Every action in `.github/workflows` is pinned to a commit SHA**, with the ref it was read from in a comment: `actions/checkout` v5.1.0, `dtolnay/rust-toolchain` stable and `Swatinem/rust-cache` v2.9.2 (#62). Every checkout that does not push sets `persist-credentials: false`. Before this, 11 `uses:` lines in `ci.yml` and `changelog.yml` were mutable tags, which the org's Semgrep rule fails as soon as a pull request touches the file.
- **A new `workflow audit` job in `supply-chain.yml` runs zizmor 1.30.1** over the workflows, failing on template injection, unpinned or impostor actions, persisted credentials, over-broad permissions and dangerous triggers. On 2026-10-07 it reported 18 findings on `staging` (12 high, 6 medium) and none after this change. `main-release.yml`'s `workflow_run` trigger is suppressed on its line with the reason: it only checks out a commit pushed to `main`.
- **`.github/dependabot.yml`** proposes GitHub Actions updates to `staging` weekly, a week after each release (`cooldown`). Cargo and npm stay on security updates only, because their pins are deliberate.
- **`.github/CODEOWNERS`** names the owner for `.github/`, `deny.toml` and `SECURITY.md`. It takes effect only where a ruleset requires code-owner review.

### Changed — RFC-0012 §8 surveys prior art (2026-10-07)

- **RFC-0012 (the harness) §8 now compares the design with six outside systems** (Refs #48): the Language Server Protocol and rust-analyzer, Roslyn, the TypeScript language service, `rustc --error-format=json` and `cargo --message-format=json`, MCP servers for languages and toolchains, Unison's codebase manager, and Go's `go/analysis` as a plugin host with static composition. For each it says what the RFC takes and how it differs. It also checks the RFC's claim that the harness is part of the language: none of the six systems specifies its machine interface as part of its language, so the claim is stated as a design goal with three conditions, none of which exists yet. The TypeScript row adds a constraint: a plugin's checks must reach `mz check --agent`, because TypeScript's plugins are never loaded by `tsc`. §9 gains question 8, whether to build an LSP server as a client of the harness. `mzizi-dev/agent-tools`' `docs/rfc-harness-plugins.md` still has not been read in full: this session was refused access to the private repository, and §8 says so. **Design only.** It is a reading of documentation, it measures nothing, and no code changes.

### Changed — the React arm pins from the registry lockfile (2026-10-07)

- **`benchmarks/arms/react/sandbox/package.json` and `package-lock.json` now pin the versions in `mzizi-dev/mzizi-registry`'s `pnpm-lock.yaml` at `3afeb752`**, as RFC-0009 §1 requires (Refs #48). `typescript` 5.9.3 → 6.0.3, `react` 19.3.0 → 19.2.8, `@types/react` 19.3.0 → 19.2.18, `tailwind-merge` 3.7.0 → 3.6.0; `class-variance-authority` 0.7.1 and `clsx` 2.1.1 were already the registry's. The lockfile was regenerated with `npm install`, and all seven packages in it (with `csstype` 3.2.3) match the registry lock's versions and integrity hashes. The versions were copied in by hand; CI does not read the registry.
- **`sandbox/tsconfig.json` drops `baseUrl`**, which TypeScript 6.0 deprecates (TS5101 made every check exit 2). The registry's own `tsconfig.json` has none either.
- **`benchmarks/prompts/react-guide.md` says React 19.2**, not 19.3. Re-counted with the tokenizer `BUDGET.md` names: 2,691 tokens and 10,298 bytes, unchanged; only its SHA-256 in `BUDGET.md` changes. `verify-guide.sh` on it passes under `tsc` 6.0.3, both wrong-on-purpose outputs byte for byte.
- `benchmarks/arms/react/README.md`'s pinned-versions section is rewritten with that provenance, and `benchmarks/READINESS.md` closes the item. No episode has run on the React arm; nothing here is a measured result.

### Added — `match` in a view is `MZ0410`, not a silent element (2026-10-07)

- **`match` inside a view is now the error `MZ0410`** (#54). Mzizi has no `match` (RFC-0001 §1.2); the parser used to read `match size` as an element named `match` with an unchecked tail, so the issue's file passed `mz check` with 0 errors. The diagnostic names the idiom and the form to write instead: “Mzizi has no `match` — write one `when size is x … end` block per variant, with `else` for the rest”. It carries **no fix**: one `match` becomes several `when` blocks, so there is no single replacement, `exact` or `guess` (RFC-0001 §4.3 offers a fix only when one is unambiguous). Its arms are not parsed, since they come in whatever shape the writer knows (`case x` blocks with or without `end`, Rust's `x => …`, an `else`): every line indented past `match` is skipped, then its `end`, and the lexer's diagnostics on those lines are dropped, so the block is one diagnostic and is left out of the tree and the IR. A mistake after the block is still reported, and `case` outside a `match` is still `MZ0402`. Tested: three new tests in `compiler/tests/recovery.rs` (the issue's file verbatim, six arm shapes, and an error after the block); `cargo test --workspace` runs 428 tests (was 425). Views only: in a service handler, `match` is still the generic `MZ0811`.
- **RFC-0001 §4.7's idiom table gains the `MZ0410` row.** `LANGUAGE-TRACKER.md` C4 no longer says `match`/`case` exist inside `view`: they never did, and now `match` is an error there.
- **`benchmarks/prompts/mzizi-guide.md` says `match` is `MZ0410`**, replacing "a `match` line is not always an error". Re-measured in `benchmarks/prompts/BUDGET.md`: 2701 → 2695 tokens (-1.8% → -2.0% against the 2750 budget), 9929 → 9900 bytes; the other three UI guides are unchanged and re-counted to their recorded values. `verify-guide.sh` passes on the guide. No benchmark has been re-run, and this says nothing about any result.

### Fixed — the docs say what `mz contract` and `mz build` do, and stop calling components "the corpus" (2026-10-07)

- **`SECURITY.md` now says what each `mz` command does with an untrusted `.mz` file** (#49). It used to say the compiler "does not execute anything" and that contract bodies "are not evaluated". It now lists each command: `mz check`, `mz outline` and `mz ir` write nothing; `mz fix` rewrites the given file with its `exact` fixes; `mz contract` evaluates a component's contract against its declarations, and runs a `service`'s examples in process, which reads only `file` fixtures that the checker confines to the `.mz` file's directory (`MZ0810`); `mz build` writes a Rust + axum package to `--out` but does not compile or run it. No command makes a network connection. `CONTRIBUTING.md`'s two lines that said contracts are not evaluated, or that `mz contract` executes nothing, now say the same. `benchmarks/README.md` calls the components "UI tasks", not the "Corpus", and `README.md`'s tree describes `examples/` as example components and the example service. `README.md`'s "UI task corpus" (Charter §6 wording) is left for the owner with the charter (#46). Docs only: no behaviour changes.

### Security — a Rust dependency audit in CI (2026-10-07)

- **A new `Supply chain` workflow (`.github/workflows/supply-chain.yml`) audits Rust dependencies with `cargo deny`** (#62). It runs `cargo deny check` with the new `deny.toml` over the workspace and over B1's Rust reference (`axum` 0.8.9 and `tokio` 1.53.1, the versions `mz build` pins). It fails on any RustSec advisory, on a license outside `deny.toml`'s `allow` list, and on any source other than crates.io. It runs on pushes and pull requests to `main` and `staging`, and weekly. The org's required workflows already run Semgrep, dependency review and a lockfile audit on every pull request, but that audit runs only when a lockfile changes, and this repo gitignores `Cargo.lock`, so it never audited here. On 2026-10-07 both `cargo deny` runs passed locally with no advisories; the one warning is two versions of `winnow` under `toml`. The job is not a required check yet: that is a branch-protection setting. `AGENTS.md` and `CONTRIBUTING.md` describe it, and the org's required workflows.

### Added — `AGENTS.md` loads the Mzizi dev skills (2026-10-06)

- **`AGENTS.md` gains "Dev skills, progress reports and the merge gate"**, the canonical rule block from nyuchi/.github#87, after "Track big work in GitHub issues": load the Mzizi dev skills (`digital-hygiene` and `progress-report`), clone only into a directory unique to the agent, run dev work on a 10-minute progress-report loop whose ticks never publish, release, merge or deploy without the owner's approval, and merge only through the merge gate. Docs only: no behaviour changes, and CI is unchanged.

## 2026-10-05

### Added — each release to `main` is tagged as the next minor (2026-10-06)

- **`.github/workflows/main-release.yml` tags each release to `main`** as the next minor (`x.y.z` → `x.(y+1).0`) once the `CI` workflow passes on it, and creates its GitHub release, under the org versioning policy (nyuchi/.github#80). Merges into `staging` stay patches (`staging-version.yml`); a major is only made by hand (`workflow_dispatch`, `bump: major`). `v0.1.0`, the release merged untagged in #58, was tagged on its existing `main` commit (`af39be5`) on 2026-10-06, so the next release is `v0.2.0`. CI only: no behaviour changes.

### Changed — "corpus" no longer names this repo's own `.mz` files (2026-10-04)

- **CI step names, `CONTRIBUTING.md`, three test-file comments and one test name stop calling `examples/` and `primitives/` "the corpus"** (#51). The `compiler` job's steps are now `mz check every example` and `mz contract every example and every primitive`; only job names are checks, so no check name changes. `compiler/tests/serve.rs`'s `the_corpus_example_evaluates_clean` is now `the_example_service_evaluates_clean`, a rename only: `cargo test --workspace` still runs 425 tests. `compiler/tests/corpus.rs`, the helper and test names in `compiler/tests/contracts.rs`, the RFCs' historical uses and the fuzzing "seed corpus" keep the word. No behaviour changes.

### Fixed — two doc lines no longer say nothing lowers (2026-10-04)

- **The compiler crate's doc comment (`compiler/src/lib.rs`) and RFC-0009 §6.4 no longer say that nothing lowers** (#47). The crate doc now says what `mz build` does: a `service` lowers to a local Rust + axum package (RFC-0011 §8), no component lowers (RFC-0007 G2.1), and there is no Workers, WebAssembly or Containers target. RFC-0009 §6.4's paragraph on the missing handler slice is now in the past tense, under its existing 2026-09-30 status note. Docs only: no behaviour changes. `CHARTER.md` §1 still says `mz` "emits no Rust yet"; charter edits wait on the owner (#46).

### Added — the docs point to the published design system (2026-10-04)

- **`AGENTS.md` and `README.md` link the published Mzizi design system** (the "Design System" artifact, <https://claude.ai/artifact/G8CCtAbZ8w717uQ3R5itCc>). It holds the brand book, voice, visual foundations, marks and component previews for `mzizi` and the `bundu` ecosystem. Both files say its source of truth is `design-system/` in `mzizi-dev/mzizi-registry` (landing with registry PR #418), and that the artifact is built from that folder and never edited on the artifact page. Docs only: nothing here reads it, and CI is unchanged.

### Changed — nhimbe leaves the wordmark list (2026-10-04)

- **`AGENTS.md` no longer lists `nhimbe` among the lowercase wordmarks.** The owner's decision of 4 October 2026 retires the brand. The events platform is Mukoko Events at events.mukoko.com (mukoko-dev/nhimbe#155).

### Security — security reports go to `security@nyuchi.com` (2026-10-03)

- **`SECURITY.md`'s email fallback behind GitHub private reporting is `security@nyuchi.com`**, in place of `security@bundu.org` (owner decision, 2026-10-03: one security contact for every repository). `AGENTS.md` matches.

### Changed — lint runs once, from the org-required workflow (2026-10-03)

- **Removed `.github/workflows/lint.yml`.** The `mzizi-dev` org ruleset now runs the shared lint on every pull request through `mzizi-dev/.github`'s `org-lint.yml`, publishing the same five `lint / …` checks, so the repo's own caller only ran lint a second time.

### Added

- **The `mzizi-be` arm, the probe crate `mzprobe`, and task B1** (RFC-0009 §6.4 and §9
  step 6, #33). This is the first `backend` arm.
  - `LANGUAGE-TRACKER.md`: "Where Mzizi stands today" describes the backend slice as merged,
    P2 cites #31 and #32, and the backend measurement row says what B1 still waits on.
  - `benchmarks/arms/mzizi-be/` holds the `arm.toml` (checked with `mz check --agent`,
    extractor `none`). Its `serve.sh` lowers a candidate with `mz build`, builds it offline,
    and serves it on `$PORT`.
  - Its guide is `benchmarks/prompts/mzizi-be-guide.md`, at 2,210 Qwen2.5-Coder tokens. The
    `backend` family has no budget until the incumbent backend guides exist.
    `verify-guide.sh` and a compiler test both hold the guide to the checker's real output.
  - `benchmarks/probe` (`mzprobe run`, `serve`, `verify`) sends a task's probes and judges
    each fact. It never follows a redirect, and treats no HTTP status as an error. 9 offline
    tests.
  - `benchmarks/tasks/b1-routing` is B1: a language-neutral `spec.md`, and 20 probes holding
    59 facts. It has two references, RFC-0011's service form and plain axum. Both hold all
    59 facts, and CI's `lowering` job runs `mzprobe verify` on them.
  - The runner does not call the probes yet, so a backend episode would be clean but
    unscored.
  - No backend episode has run. B2–B5 and the incumbent backend arms do not exist.
- **`mz build <file> --out <dir>` lowers a service to a Rust + axum package** (RFC-0011 §8,
  #32). The package is RFC-0011 §6's routing as one Rust function, served by axum through a
  single fallback. It gets one generated `#[tokio::test]` per `example`, sent with
  `tower::ServiceExt::oneshot`. `ensure` clauses are not lowered: `mz contract` tests them.
  - The package pins `axum =0.8.9` and `tokio =1.53.1`, and its tests pin `tower =0.5.3`,
    `http-body-util =0.1.5` and `serde_json =1.0.151`. It is its own workspace root and has
    no lockfile. The compiler crate still has no dependencies.
  - CI's new `lowering` job builds `examples/registry.mz`, runs the generated tests, and
    sends one OPTIONS request over a socket to the running server.
  - Only a `service` lowers. No component does.
  - `LANGUAGE-TRACKER.md`: "Compiles to something that runs", P3, P4, P10 and P11 cite
    this PR, not an open PR. Each stays 🟡 (P10 📝): only a service lowers.
  - Tested by 8 new tests in `compiler/tests/lower.rs`, which check the generated text
    offline.
- **`mz contract` runs a service in process** (RFC-0011 §6–§7, #31). An in-process
  evaluator (`compiler/src/serve.rs`) implements the runtime's dispatch: the canonical-path
  308, routes where literals beat parameters, HEAD from GET, OPTIONS 204 and 405 with a
  computed `allow`, the fallback, and service-level headers.
  - `examples/registry.mz` is the example service, 22 clauses, all holding.
  - Each `example` is one request. Each `ensure` is checked over a deterministic set of
    generated requests, 61 for `examples/registry.mz`. That is tested, not proven
    (RFC-0010 C-4).
  - A failed example is `MZ0612`, a failed ensure `MZ0611` naming the request, and a
    `body.<field>` on a body that is not JSON `MZ0605`. Body clauses skip HEAD requests.
  - The summary line gains `contract_tested`. The `MZ0607` placeholder from #30 is gone.
  - Tested by 19 new tests in `compiler/tests/serve.rs`.
  - `LANGUAGE-TRACKER.md`: "Contracts on everything" says a service's contracts run in
    process. It stays 🟡: `ensure` is tested, not proven.
- **`mz check` parses and checks a `service`** (RFC-0011 §1–§5 and §9, #30). The checker
  proves every path through a handler responds exactly once (`MZ0804` when one does not,
  `MZ0805` with an exact fix for a line after `respond`). The other `MZ0801`–`MZ0812` codes
  cover methods, patterns, duplicate routes, `respond`, `header`, record literals, `query`
  types, fixtures and misplaced lines. A service's records reuse RFC-0008's resolver and
  codes (`MZ0701`, `MZ0707`, `MZ0708`, `MZ0710`, `MZ0712`).
  - In this PR, `mz contract` reports each of a service's clauses as `MZ0607`, "not yet
    testable", an error and never a pass. `mz outline`, `mz hash` and `mz ir` refuse a
    service with exit 2.
  - A service does not yet run or lower. Tested by 52 new tests in
    `compiler/tests/services.rs`.
- **RFC-0011, handlers: the backend measurement slice** (design, #29). RFC-0009 §6.4's
  slice in one RFC: a top-level `service` declaration of `route` blocks and a `fallback`,
  handlers of three statements (`when`/`else`, `header`, `respond`) where every path must
  respond exactly once, and a runtime that owns the canonical-path 308, HEAD from GET,
  OPTIONS 204, and 405 with a computed `allow`. It also specifies the query decoding rule,
  the new `MZ08xx` codes (`MZ0801`–`MZ0812`), contracts on a service (`mz contract`), and
  lowering to a local Rust + axum package. It is the design for RFC-0007 gaps G2.1, G2.2,
  G2.4, G2.5 and G2.11, and RFC-0010 gains amendment notes. This entry is design: nothing
  in it is implemented or measured.
- `LANGUAGE-TRACKER.md`: the one tracker of what Mzizi still needs to be a working programming language, stacked against Python, Go, C++, TypeScript and Rust, with a status, evidence and a done-when test for every capability, and milestones M1–M3 (owner, 2026-09-30).
- **Arms are data: `benchmarks/arms/<id>/arm.toml`** (RFC-0009 §9 step 1, #28). The runner
  reads each arm (guide, check argv, normaliser, file layout, extractor, naming) from one
  strict file, and the hard-coded `Arm` enum is gone. `mzizi`, `dioxus` and `leptos` are
  migrated with no behaviour change: a test rebuilds pilot 2's committed user messages byte
  for byte.
  - `--family ui-spec` hands the author a language-neutral `spec.md`, now written for the
    four public tasks.
  - The prompt-parity test runs over every pair of arms.
  - `mzbench plan` drafts a run's `PLAN.md`, and `mzbench bundle-hash` computes its
    raw-bundle hash.
  - Every final line records `guide_tokens`.
  - No episode has been run with any of it.
- **A `react` arm** (RFC-0009 §9 step 2, #28): a `tsc --noEmit` strict check in an offline
  sandbox (`arms/react/`), for `ui-spec` only. The harness can now score it:
  `mzizi-benchmark-harness score --arm react` reads `cva(…)` calls and `data-slot`.
  - The four UI guides are rewritten to one 2,750-token budget, and now measure 2,691 to
    2,750 Qwen2.5-Coder tokens (`benchmarks/prompts/BUDGET.md`). They were 31% apart.
  - The React pins are provisional (the npm registry's versions of 2026-09-30), not yet the
    registry lockfile's, which RFC-0009 §1 asks for.
  - The React arm has never run an episode.
- **The site and docs freshness rule** (AGENTS.md and CONTRIBUTING.md, #28). mzizi.dev and
  docs.mzizi.dev must never lag this repository. Every user-visible pull request carries a
  "Site/docs impact" heading (owner rule, 2026-09-30).
- **`CHANGELOG.md`**, backfilled to 2026-08-23. AGENTS.md and CONTRIBUTING.md now say every
  pull request that changes behaviour, diagnostics, the language, the charter, an RFC or
  the benchmarks adds an entry here (owner rule, 2026-09-30).

- **RFC-0012, the harness: the core of the language, what the agent reads** (design, a
  draft). The harness is the language as an agent reads it, the agent protocol (RFC-0001 §4)
  and the plugin host that `mz`, the CLI, the MCP server and plugins attach to. It lives in this
  repository. Of it, only the agent protocol and the IR exist today (`mz check --agent`,
  `mz fix`, `mz contract`, `mz outline`, `mz ir`, `mz hash`); the definition an agent reads,
  the plugin host and `mz harness` are design only. The agent skills fold into it once it
  exists. `benchmarks/harness/` is renamed in prose to "the benchmark harness", a different
  thing.

### Changed

- **ROADMAP says "UI task set" and "reference implementations", not "the benchmark
  corpus"** (#29), after charter v0.4 §6. The Rust components the UI task set scores
  against are its reference implementations. Wording only: no plan or status changed.
- **Charter v0.4: Mzizi is a general-purpose programming language, and Phase 0 is scoped to
  one goal** (owner-directed 2026-09-30). "Mzizi is built to make Rust better, the way
  TypeScript makes JavaScript better": Rust is the platform Mzizi is designed to lower to (not
  yet built: `mz` emits no Rust), the harness is the core of the language, and Mzizi Roots is
  its component model. The toolchain and the components support the language and are not it.
  Phase 0's goal is to show that Mzizi can stand against the best existing language for each
  kind of task; the component tasks against Dioxus and Leptos, the pilots among them, are
  tests within it. The kill criterion, the gate and the settings are unchanged, and nothing is
  measured by this change. README, AGENTS.md, ROADMAP and READINESS follow; the README's RFC
  table now lists RFC-0008 to RFC-0010 and RFC-0012, and says the registry is Mzizi's. §6 now
  says Mzizi's own components supply the UI task set and support the language, where it called
  them "the benchmark corpus, full stop".
- **Code of Conduct reports go to `support@bundu.org`** (#28), not `conduct@nyuchi.com`.
  Security reports stay at `security@bundu.org`.

### Fixed

- **The docs say what `mz build` does today.** `README.md`, `AGENTS.md`, `CONTRIBUTING.md`,
  `SECURITY.md` and RFC-0012 §2 said `mz` emits no Rust, or that there is no lowering. They
  now say that `mz build` lowers a `service` to a local Rust + axum package, that no component
  lowers, and that there is no Workers, WebAssembly or Containers target. RFC-0009 §2.2 now
  says no _component_ lowers. "Compiles to Rust" is still not true of Mzizi in general. Docs
  only: no behaviour changed.
- **"The corpus" wording is gone from the live contributor docs.** `AGENTS.md`,
  `CONTRIBUTING.md` and `benchmarks/README.md` now say "the repo's `.mz` files", "the
  reference implementations" or "the example components", whichever the sentence meant.
- **A malformed route pattern still binds its parameters** (#33). With a trailing slash,
  `get "/v1/items/{id}/"` is one `MZ0802`, with its exact fix. It no longer also produces an
  `MZ0707` on every use of `id` in the handler, a cascade RFC-0001 §4.1 rules out. Found by
  checking the backend guide's wrong-on-purpose file. Tested by
  `a_malformed_pattern_still_binds_its_parameters`.
- **`tests/contracts.rs`'s broken-contract file is unique per process** (#28), so
  concurrent `cargo test` runs no longer collide on one fixed temp path.

## 2026-09-29

### Added

- **RFC-0009, the comparison benchmark** (design, #26). The arms are Mzizi against React for
  UI, and against TypeScript/Hono, Python/FastAPI, Go, C++ and Rust/axum for backends.
  There are `ui-spec`, `ui-port` and `backend` task families, fairness rules including one
  guide token budget, and publication rules. The kill criterion becomes an owner decision:
  Mzizi must beat the best incumbent on at least two of three metrics in every gating
  family, on the ~7B model, on held-out tasks. None of the new arms existed when it merged.
- **RFC-0010, contracts everywhere** (design, #26). It covers contracts on functions,
  handlers, services and the standard library. FM-14 is pilot 2's measured
  `size-14` / `height 48` case. Nothing in it is implemented.
- **Charter v0.3** (#26), owner-directed 2026-09-29. Mzizi is to build Mzizi's own backend,
  stated as the target, not the state. The two frontend paths are Astro over Mzizi Roots,
  or pure Rust. The kill criterion is RFC-0009 §6.
- **`mz fix <file>`** (#27): applies every `exact` fix in one pass, writes the file, and
  re-checks it. `guess` fixes are never applied. RFC-0001 §4.3 had promised it since the
  first commit.
- **Diagnostics for the React idioms agents bring** (#27): `MZ0106` for a `...props` spread,
  with an `exact` fix that deletes the line, and `MZ0312` (warning) for an `asChild` prop.
- **`MZ0313`** (#27): a variant's declared `height` disagrees with the height its
  `h-N` / `size-N` class renders. The rendered number is the `exact` fix. A variant row may
  now leave `height` out, and both the compiler and the harness then read it from the
  class (FM-11).
- **The `slot_set` fact** (#27): `mzizi-benchmark-harness score --slots`, opt-in per task
  with `score_slots = true`. It compares the port's `data-slot` set with the reference's.
  Also a `card` task that uses it.
- **Kill-criterion scaffolding** (#27): `benchmarks/READINESS.md` audits pilot 2's list of
  fixes item by item. `kill-criterion/run.sh` is the driver, and it refuses to start
  without a `PLAN.md`. `kill-criterion/check-task.sh` is the offline task checker, and
  `kill-criterion/README.md` holds the held-out task plan. No gating run has happened.

### Changed

- **One diagnostic per mistake in views and closers** (#27), as RFC-0001 §4.1 promises:
  - `MZ0406`: an attribute missing its `=`.
  - `MZ0407`: `if` in a view, with `when` as the `exact` fix.
  - `MZ0408`: an attribute directly inside `view`.
  - `MZ0409`: text after an element word.
  - Every closer fix (`MZ0206`, `MZ0208`) now rewrites the whole `end` line, and open blocks
    at `end component` are one `MZ0204`.
  - `MZ0204`'s fix now lands after the last line.
- **`MZ0602` on an element subject** (#27) names the one predicate it takes
  (`min_height <n>`). RFC-0006 §8.2 was corrected: its `exact` fix needs an operand.
- **Security reports** (#25) name `security@bundu.org` as the fallback channel.

### Fixed

- **The runner normalises the candidate's path to its file name** in every arm's check
  output (#27). Pilot 2 measured about 13,500 tokens of absolute paths fed to the 7B Mzizi
  arm.
- **A Mode 1 episode that outgrows the model's context** now finishes as not clean
  (`ended_by: context_exceeded`) (#27). Before, it was left unfinished and dropped from
  every denominator.

## 2026-09-27

### Added

- **The Phase 0 defect metric, harness side** (#10). `benchmarks/harness`
  (`mzizi-benchmark-harness`) diffs a component's variant sets, defaults and touch heights
  against its hand-written Rust reference. This first version shipped a reconstructed
  reference fixture, and against the real `button.rs` it reported five false defects.
  #12 fixed the extractor and replaced the fixture with a byte-identical copy.
- **The pilot infrastructure** (#12):
  - The pilot task set: button, badge, and the changelog renderer.
  - The Mzizi and Dioxus arms' guides. The Dioxus arm's `check.sh` is pinned from the
    registry's lockfile (Dioxus `=0.7.10`).
  - `mzbench`, the runner: Mode 1 against llama.cpp, Mode 2 for agents, and `summarize`.
  - CI's `benchmarks` job.
- **The Leptos arm** (#15): Leptos `=0.8.21`, as a second raw-Rust comparison. It has never
  run an episode.
- **An end-to-end `mzbench` test** (#19), against the real `mz` and the real scorer.
- **RFC-0007, a gap register** (design, #16). It lists what charter v0.2's scope needs that
  the grammar and compiler lack, in tiers.
- **RFC-0008: types, lists, records, options and `for each`** (#22, replacing #20), with a
  checker that can say no. It is design and code:
  - The grammar for types, records, `else` and `emit`.
  - A resolver pass, so every name and type resolves or is a diagnostic (`MZ0701`–`MZ0716`,
    and `MZ0502` for another component's enum).
  - The changelog renderer ported with `list(record)` and nested `for each`.
- **The open-weight arm** (#24): llama.cpp b11206 serving Qwen2.5-Coder-7B Q4_K_M on CPU
  (`benchmarks/openweight/`).
- **Pilot 1** (#17, `results/2026-09-27-pilot/`). Mode 2, 3 tasks × 2 arms, one seed, one
  frontier model. Every episode was clean on iteration 1. It is not the Phase 0 number. It
  found a harness false positive: the changelog task's renamed variants.
- **Pilot 2** (#24, `results/2026-09-27-pilot-2/`). Frontier model: both arms 6/6 clean with
  0 defects, and Mzizi 8.2% cheaper in transcript tokens. ~7B model: Mzizi did worse than
  Dioxus on all three metrics, 2/6 clean against 4/6. It is not the Phase 0 number.

### Changed

- **Charter v0.2** (#13): Mzizi is general-purpose, not web-only. The old Phases 1 and 2
  merge into one full-stack deliverable.
- **README for humans, AGENTS.md for agents** (#14), and seven README/AGENTS.md claims that
  `main`'s code contradicted were corrected (#18).
- **Ownership** is attributed to Mzizi, with the Bundu Foundation as parent copyright holder
  (#21). Corpus references use the `mzizi-*` component names (#23).

### Fixed

- **The scorer** (#17). Renamed variants are paired by class string, opt-in per task
  (`allow_variant_renames` with a required `rename_reason`), instead of being scored as
  defects. A pairing is not counted as a fact, and a paired default that is not a candidate
  variant is a defect.
- **Review fixes to RFC-0008** (#22):
  - An external dotted reference is always `MZ0502`.
  - `emit` into `event(option(T))` takes `none` or any `T`.
  - `{a.B}` is snake-cased per segment, as one diagnostic.
  - A whole symbolic type gets one `exact` fix.
  - No two `exact` fixes in a file overlap.

## 2026-09-11

### Added

- **`mz contract`, and RFC-0006** (#6). Contract clauses are retained, parsed and evaluated
  against the component's own declarations. A false assertion exits 1 (`MZ0603`), and the
  agent summary line carries the contract counts. RFC-0006 records the two earlier RFC
  claims that writing it disproved.
- **Contributor documentation** (#4): CONTRIBUTING (the RFC process and CI gates), SECURITY,
  Contributor Covenant 2.1, and a root README.
- **`design/ROADMAP.md`** (#3), folded in from `mzizi-dev/mzizi-roadmap`.
- **The org lint gate** (#7, normalised in #9): actionlint, JSON validity, prettier,
  markdownlint and yamllint.

### Fixed

- **Three corpus contract lines that could never be checked** (#6).
- **The component count, the roadmap's archive status and the merge convention** in the
  docs (#8).

## 2026-09-09

### Added

- **CI** (#2): the compiler job recreated as this repository's own CI, a gitleaks scan of the
  whole history, and the public half of the held-out benchmark dispatch.
- **The crate licence and a `LICENSE` file** (#1).

## 2026-08-23

Carried across in the subtree split. These commits have no pull request number.

### Added

- **The research charter** (`CHARTER.md`, v0.1).
- **RFC-0001**: surface syntax, canonical form and the agent protocol. **RFC-0002**: small
  models are the design target. **RFC-0003**: the content-addressed IR. **RFC-0004**: test
  topology, with the public half of the held-out benchmark dispatch.
- **The front end and `mz check`**: a lexer, a recovering parser, and `--agent` NDJSON
  diagnostics with `exact` / `guess` fixes, tested to give one diagnostic per real error.
- **Nine primitives written in Mzizi**: button, input, badge, alert, card, avatar,
  separator, spinner and confirm_bar.
- **The content-addressed IR**, with `mz ir`, `mz hash` and `mz outline`.
- **The migration plan** to the `mzizi-dev` org (`MIGRATION.md`).
