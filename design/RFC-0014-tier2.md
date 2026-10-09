# RFC-0014 — Tier 2: a working programming language, modules, the standard library, Rust interop and runnable builds

**Status:** draft for review. **Nothing in this RFC is implemented.** Every syntax form below is
designed, not built: `mz check` does not accept `import`, `module`, `public` or `extern` today (§3.2 gives
what it reports instead), and no test reads these examples. Each wave in §12 names the tests that
would make a form true, and until a wave's tests are on `main`, the tracker's rows stay as they
are.
**Author:** the machine author (Claude), for the owner, on issue
[#110](https://github.com/mzizi-dev/mzizi/issues/110) (the M2 goal, Refs #69).
**Scope:** Tier 2 of [`LANGUAGE-TRACKER.md`](../LANGUAGE-TRACKER.md), rows P1–P12, as one design.
The milestone M2 needs **P1–P6** ✅ and nothing else (tracker, "Milestones"). This RFC designs P1–P6
in full, in six waves (§12), and reserves a section for each of P7–P12 that says what is deferred
and why (§7). It covers: modules and imports across files (P1); the standard library as a closed,
versioned list (P2); lowering all code to Rust (P3); runnable builds of a multi-file program (P4);
errors mapped back to `.mz` (P5); calling Rust crates and mixing `.mz` and `.rs` in one Cargo project
(P6). It does not design syntax for concurrency, state, capabilities or generics, and it does not
change any Tier 1 construct, except where §3 says a Tier 1 follow-up is a prerequisite.

> **Proposes to amend**, in full:
>
> - **RFC-0007 D7**, the open word for "depends on a module" (`use` means "needs a capability", RFC-0001
>   §1.7). §1.1 answers it with `import`, as the survey proposed (F27). D7 stays open until the owner
>   accepts this answer (§14, Q2).
> - **RFC-0007 G2.10**, the standard library's scope. §2.1 proposes a closed list that differs from G2.10
>   and from the tracker's P2 row. The owner chooses (§14, Q4).
> - **RFC-0013 §13.1**, `mz run`'s "a run needs no network" promise. It stays true for a program whose
>   imports are the standard library, and §6.6 says it no longer holds for a program that imports a
>   crate (§14, Q11).
> - **RFC-0013 §16**, which names `MZ10xx` as the next diagnostic family once `MZ09xx` fills. §10 opens it.
> - **RFC-0013 §1**, the `program` file kind. A `module` file kind is added beside it, and a program still
>   has exactly one `fn main` (§1.1).
> - **RFC-0013 §14.2 and the `text` lowering table** (`compiler/src/run.rs`, `text_method`). §2.2 keeps
>   that table as the one implementation of the text methods that the `text` module documents (RFC-0013
>   §20 Q20, owner decision of 2026-10-08).
>
> Nothing above takes effect until the owner accepts this RFC. No amendment is applied to the other RFCs
> by this document.

---

## 0. Method: the failure modes Tier 2 invites

RFC-0001 §0's rule applies: a decision must trace to a named failure mode. These use a `T2-` prefix (Tier
2), because `CL-` belongs to RFC-0013. The evidence column says what has been measured, and where: a
study of other languages, or a practitioner report. **None of these studies measured Mzizi**
([`design/LANGUAGE-SURVEY.md`](./LANGUAGE-SURVEY.md) §2, §7).

| ID       | Failure mode                                                                                                                                                                                                                                                                                                                               | Evidence                                                                                                                                                    |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **T2-1** | **The invented import.** A small model writes the module form it saw most: `from geometry import area` (Python), `import { area } from "./geometry"` (JavaScript), `#include "geometry.h"` (C), `use crate::geometry::area;` (Rust), `mod geometry;`. Each is a different tree on disk, and the compiler cannot guess which one was meant. | Missing imports are 56.6% of LLM-written C++ compile errors and 35.5% of C (Coimbra 2026, measured; not Mzizi).                                             |
| **T2-2** | **The reach into internals.** A module uses another module's private helper, because nothing says it may not. The code compiles today, and breaks when the helper changes.                                                                                                                                                                 | Practitioner report (anecdotal).                                                                                                                            |
| **T2-3** | **The invented crate or package.** A model names a dependency that does not exist, or a version that was never published.                                                                                                                                                                                                                  | 19.7% of recommended packages did not exist; 15.8% for Python, 21.3% for JavaScript (Spracklen et al., measured; not Mzizi).                                |
| **T2-4** | **The stale or invented library name.** A model calls `os.path.join`, `json.dumps` or `Math.floor` in a language that has no such name, or a deprecated or post-cutoff API.                                                                                                                                                                | ~25–70% deprecated-API use by library (Wang et al., Python, measured); 56.1% against 32.5% before and after cutoff for Rust (RustEvo, measured; not Mzizi). |
| **T2-5** | **The ignored I/O failure.** A file read or an HTTP call whose failure is dropped, because the success path is the only one written. RFC-0013's CL-3 applied to the standard library.                                                                                                                                                      | Dropped Go errors and `.unwrap()` in Rust (anecdotal; the survey, §7).                                                                                      |
| **T2-6** | **The run that cannot be reproduced.** A program whose output depends on the clock, the environment or a network, so its `.expected` file is right on one machine and wrong on another.                                                                                                                                                    | Practitioner report (anecdotal).                                                                                                                            |
| **T2-7** | **The error in code nobody wrote.** A `rustc` error in lowered code points at generated text, and the author cannot find the line that caused it. RFC-0001 §0's FM-7.                                                                                                                                                                      | FM-7 in RFC-0001's failure-mode list, cited by the survey's R8 (§6); not measured.                                                                          |
| **T2-8** | **The unsafe door.** Unsafe Rust, or C's memory model, enters through an interop boundary, and the rest of the program is safe only by convention.                                                                                                                                                                                         | ~40% of completions vulnerable in CWE scenarios (Pearce et al., measured; C, not Mzizi).                                                                    |
| **T2-9** | **The build that depends on the host.** A package that fetches from the network, or reads a cache that another program left behind, so `mz run` succeeds on one machine and fails on another.                                                                                                                                              | Practitioner report (anecdotal). RFC-0013 §13.1 already says per-user caches for this reason.                                                               |

Each design choice below names the row it removes or reduces. A choice that removes none of them is
not in this RFC.

---

## 1. Modules and imports across files — _P1_

Wave 1 (§12.1). P1's "Done when" is: code in one file calls code in another, and private names are not
visible.

### 1.1 Two file kinds, one declaration each

A file holds one top-level declaration, named after the file (RFC-0007 D3, which RFC-0013 §1 applies to
programs). Tier 2 adds one kind:

- **`program <name>`** is the entry point (RFC-0013 §1). It holds one `fn main`, and it may import
  modules. It cannot be imported (`MZ1007`).
- **`module <name>`** is a library file. It holds `import` lines, `enum`s, `record`s and `fn`s, each
  either `public` or private. It has no `fn main`, and it closes with `end module <name>`.

```mz
## Shapes in the plane. Imported by main.mz as `geometry`.
module geometry

  public record point
    field x: float
    field y: float
  end

  public fn area(width: float, height: float): float
    return width * height
  end fn area

  fn scale_factor: float
    return 2.0
  end fn scale_factor
end module geometry
```

`module` and `public` are contextual words, as `program` and `record` are (RFC-0013 §1): special only at
the start of a declaration, so `KEYWORDS` in `compiler/src/lex.rs` stays at 23. The file `geometry.mz`
holds that one declaration. The module's name is the file's name without `.mz`, lower snake case, and
must not be one of the four Rust words that cannot be raw identifiers (`crate`, `self`, `super`, `Self`),
the name `std`, `core` or `alloc`, or the name of a standard module (§2.1), since each would shadow
something in the lowered Rust. A file that breaks any of these is `MZ1001`.

### 1.2 Import: one spelling, one place

```mz
program main

  import geometry

  fn main
    print("area is {geometry.area(3.0, 4.0)}")
  end fn main

end program main
```

- **One import form.** `import <module>` names a file in the **same directory** as the entry file. A
  dotted path (`import shapes.circle`) is not in M2 (§14, Q1); it is `MZ1002`, with a `guess` fix to
  the flat name.
- **Where it stands.** Imports come in the header, after doc lines and before any `enum`, `record` or
  `fn`, in alphabetical order (RFC-0013 §17's canonical order, applied to modules). An `import` after a
  declaration, or inside a `fn`, is `MZ1006`, with an `exact` fix that moves it to the header.
- **Qualified access only.** A name from another module is always written with its module's name in
  front: `geometry.area(...)`, `geometry.point(x = 1.0, y = 2.0)`, a type `geometry.point`. The M2
  design has no `import geometry.area` and no `from geometry import area`: both are `MZ1009` (§8).
  The reader of any line can see which module a name came from, which is what T2-1 and §8's rule on
  one way to do each thing ask for.
- **Imports are not implicit.** Naming `geometry` without an `import` line is `MZ0707`, as today
  ("not bound here"), with the fix `import geometry` offered as a `guess` (the author may have meant a
  local).
- **A name that is both a local and a module** is `MZ1008`: `let geometry = 1` in a program that
  imports `geometry`. No fix, because either name may be the one that should change.

### 1.3 Visibility

- **Private by default.** A declaration without `public` is visible only inside its own file.
- **`public` goes on the declaration header**, where the name is: `public fn`, `public record`,
  `public enum`. It is never on a variant, a field or a `case` line. A record's fields and methods are
  part of the record: they are public when the record is, and private when it is not.
- **`public` is meaningless elsewhere.** On `fn main`, on a `contract`, on a `fn` in a `program` (which
  exports nothing), or on a local binding, it is `MZ1006`, with an `exact` fix that deletes the word.
- **Use of a private name from another module** is `MZ1004`. Its `guess` fix adds `public` to the
  declaration. It is `guess` and not `exact`, because it changes the module's interface, and any other
  importer may now depend on it. This is T2-2's guard.
- **A name a module does not declare** is `MZ1005`, with the nearest name as a `guess` (RFC-0008 §6's
  rule for unknown names).

### 1.4 Cycles, the entry, and loading

- **Cycles are errors.** `geometry` imports `units` and `units` imports `geometry` is `MZ1003`. The
  chain is named from its lexicographically first module, so the same cycle is reported the same way
  every time. No fix. Survey F27 says "cycles are errors", and this is the reason: the lowered Rust
  would need the two modules to be mutually recursive, which the author would have to restructure.
- **Loading is driven by imports.** The loader starts at the entry file, reads each `import` it finds,
  and reads that module's imports in turn. A file in the directory that nothing imports is not read.
  The set of files is therefore the transitive closure of the imports, and it is the same on every
  run. A missing file is `MZ1002`.
- **`mz` still takes one file.** `mz check main.mz` and `mz run main.mz` read the siblings they import.
  The argument is one path, as AGENTS.md says. What changes is that a `mz check` can now report
  diagnostics in a file other than the one named, and §1.5 says how.
- **A module file is not a program.** `mz run geometry.mz` and `mz build geometry.mz` are `MZ1007`:
  "a module is not a program; run the program that imports it". A module can still be checked on its
  own, and `mz check geometry.mz` checks it and every module it imports.

### 1.5 Checking

- **Each file is checked as a unit against its imports' public signatures.** A module's body is checked
  when the module is loaded, under its own name. Its `public` declarations are the only ones an
  importer sees, so a private helper's change cannot change an importer's diagnostics (T2-2).
- **Diagnostics name their file.** Each diagnostic's `file` is the module's path relative to the entry's
  directory (`geometry.mz`), as `mz check --agent` already emits file names (AGENTS.md: "The `file` key
  is the path `mz` was given", extended to the sibling's path). The order is fixed: the entry file first,
  then the other files in alphabetical order, then by line and column within a file. It is deterministic
  and the same for `--agent` and for the human output.
- **One diagnostic per real error** (RFC-0001 §4) holds across files: an error in `geometry.mz` that
  `main.mz` uses is reported once, in `geometry.mz`. `main.mz` gets no second diagnostic for the same
  name, because a broken declaration is carried as the error type (RFC-0013 §16's rule for sub-expressions).

### 1.6 Lowering

Each module becomes one Rust module in the lowered package (§6.1 covers the companion file):

| Mzizi                               | Rust                                                                                             |
| ----------------------------------- | ------------------------------------------------------------------------------------------------ |
| `module geometry`, `geometry.mz`    | `src/geometry.rs`, declared in `src/main.rs` as `mod geometry;`                                  |
| `public fn area`                    | `pub fn area(...)` in `geometry`                                                                 |
| `fn scale_factor` (private)         | `fn scale_factor() -> f64`, with no `pub`                                                        |
| `public record point`               | `pub struct Point` in `geometry`, fields `pub` when the record is                                |
| `geometry.area(3.0, 4.0)` in `main` | `geometry::area(3.0, 4.0)`, with `use crate::geometry;` at the top of `main.rs`'s module tree    |
| `geometry.point(x = 1.0, y = 2.0)`  | `geometry::Point { x: 1.0, y: 2.0 }`, with the `always` check when there is one (RFC-0013 §11.4) |

- **Names.** A module's Rust name is its Mzizi name. A `geometry` and a `shapes` each declare a
  `point`, and they are two Rust types, `geometry::Point` and `shapes::Point`. RFC-0013 §14.2's
  PascalCase rule applies, and a collision with a prelude name is `MzUser`, as RFC-0013 §14.2 already
  says.
- **The runtime is shared.** The runtime helpers (`MzAt`, `mz_trap`, the `int` helpers) stay in
  `src/main.rs`, and each module reaches them as `crate::mz_trap` etc. The runtime is emitted once and
  is not repeated per module.
- **Stale files.** `mz build` writes the package and records the set of files it generated in
  `mz-manifest.txt` at the package root. When a later build generates fewer files (a module was
  deleted), each file in the old manifest and not in the new one is removed. Nothing else in the package
  directory is touched.
- **Determinism.** Two `mz build`s of one source tree are byte-identical (a test, §12.1).

### 1.7 Traps and faults

`MzAt` already carries the file (RFC-0013 §14.3), and a trap in a module writes that module's file name
and line, as §4.3's line does today. A trap is `MZ0991` whichever file it is in.

---

## 2. The standard library — _P2_

Waves 2–6 (§12). P2's "Done when" is: each module exists with tests, and is taught to agents through the
language harness (H1).

### 2.1 The closed list (proposed)

The list is closed: a module not in it is `MZ1011`, and the list changes only by a decision recorded in
the RFC that adds it (survey F28, RFC-0007 G2.10: "decided as a list, not grown by accretion"). The
tracker's P2 row and RFC-0007 G2.10 do not agree on the list: the tracker names text, math, collections,
time, JSON, environment, files and an HTTP client; G2.10 names text formatting, time, JSON, an HTTP
client, hashing and randomness, and logging. The table below takes the tracker's list, because it is the
owner's current row, and says where G2.10 differs (§14, Q4).

| Module  | Wave | Functions (signatures as designed)                                                                                                                                                                                   | Effects                              | Implementation                                                                  |
| ------- | ---- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ | ------------------------------------------------------------------------------- |
| `text`  | 2    | `text.join(parts: list(text), sep: text): text`, `text.pad_left(s: text, width: int, fill: text): text`, `text.pad_right(...)`, `text.lines(s: text): list(text)`                                                    | none                                 | `std` only; methods already built (C6) are not repeated, §2.2                   |
| `math`  | 2    | constants `math.pi`, `math.e`; `math.sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `exp`, `ln`, `log10`, `hypot`; `math.gcd(a: int, b: int): int`, `math.lcm`, `math.clamp`                                   | none                                 | `std` only (`f64` methods, and `int` helpers that trap as §4.3 says)            |
| `env`   | 3    | `env.get(name: text): option(text)`, `env.args(): list(text)` (program arguments, not the program's name), `env.exit(code: int): none` (§14, Q17)                                                                    | reads the process environment; exits | `std::env`                                                                      |
| `files` | 3    | `files.read(path: text): result(text, files.problem)`, `files.write(path: text, text: text): result(none, files.problem)`, `files.exists(path: text): bool`, `files.remove(path: text): result(none, files.problem)` | reads and writes the file system     | `std::fs`                                                                       |
| `time`  | 3    | `time.now(): int` (Unix milliseconds, UTC), `time.sleep(ms: int): none`, `time.iso(ms: int): text` (UTC, `YYYY-MM-DDTHH:MM:SS.mmmZ`)                                                                                 | reads the clock; waits               | `std::time`, and civil-date arithmetic in the runtime; no time zones (§14, Q15) |
| `json`  | 3    | `json.quote(s: text): text`; and for each record `R` a generated `R.from_json(text: text): result(R, text)` (§2.4); `to_json()` is already built (C8)                                                                | none                                 | `std` only: a parser in the runtime, and a generated decoder per record         |
| `http`  | 5    | `http.get(url: text): result(http.response, http.problem)`, a record `response` with `status: int` and `body: text`; `https` only                                                                                    | network                              | a pinned crate for TLS (§6.6); needs P6 first, so Wave 5                        |

A module's `problem` type is an enum with payload columns (RFC-0013 §11.5), so a caller can `match` it:

```mz
enum problem
  not_found
  denied
  other(text)
end
```

It is named `files.problem` from outside the module, and the same shape is used by `http.problem`.

**Not in the list, and why.**

- **`collections`.** The C7 methods (`map`, `filter`, the eight folds, indexing) are the collections
  surface, and they are built-in methods (RFC-0013 §9). A library function over an arbitrary element type
  (`zip`, `first`, `unique` on `list(T)`) needs generics (P12, §2.3), so it waits.
- **`hash` and `random`** (in G2.10 and not in the tracker). A hash needs a finished, tested implementation
  in the runtime (the compiler's `hash.rs` is the compiler's own, and not a runtime for lowered programs);
  randomness must be seeded to be reproducible (T2-6). Both wait for the owner's decision (§14, Q16).
- **`log`.** Standard error is a P2 item (RFC-0013 §1 says so), and `print` writes standard output.
  `log` would be a second output path with a second contract; it waits for P9's and P2's decisions (§14, Q16).

### 2.2 Prelude or import

Two kinds of name, and the difference is by what the name is:

- **Methods on built-in types** (`text`, `list`, `map`, `set`, `option`, `result`, `int`, `float`) are
  the **prelude**. They are always visible, and they are not imported. `"abc".length()` needs no `import`.
- **Module functions** (`math.sin`, `files.read`) are **imported** and always qualified. A program that
  uses `math.sin` has `import math` in its header, and a reader sees that from the file alone.

Survey F27 says "the stdlib is a prelude". This RFC keeps the prelude to methods on values, and makes
module functions an import, because a module function has no receiver to show where it came from, and a
small model reads the header before the body (T2-1). §14, Q5, asks the owner.

**The `text` module is the documented surface of the built-in text methods, with no second
implementation** (RFC-0013 §20 Q20, owner decision of 2026-10-08). `text.join`, `text.pad_left`,
`text.pad_right` and `text.lines` are new, and they lower through `compiler/src/text/ops.rs`'s pattern:
a runtime file compiled as part of the compiler and included verbatim in the package (§2.4). The
methods `length`, `contains`, `slice`, `split`, `chars` and the rest stay in `compiler/src/run.rs`'s
`text_method` table, and the `text` module's harness entry documents each of them by pointing at that
table. There is one lowering, and one place where the signatures are.

### 2.3 Why no element-generic library functions in M2

`text.join(parts: list(text), ...)` is possible without generics because its element type is fixed.
`zip(a: list(T), b: list(U)): list(...)` is not: its result's element type depends on `T` and `U`, and a
tuple is decided against (RFC-0013 §2, `MZ0963`). Such functions are either built-in methods (C7) or
wait for P12. This is the honest limit of the closed list: M2's library is what can be written without
generics, and P12 is what unlocks the rest.

### 2.4 Standard-library implementation: std only, and emitted only when imported

- **Std only, except `http`.** Every module except `http` lowers to Rust that uses `std` alone, so a
  program that imports only these modules has **no dependencies**, and `mz run`'s offline build still
  works (RFC-0013 §13.1). This is the decision that keeps `--offline` true, and it is Q6 in §14.
- **The runtime is a file, emitted when used.** Each module's runtime is `compiler/src/std/<module>.rs`,
  compiled as part of the compiler (so its unit tests run under `cargo test`) and copied verbatim into
  the lowered package as `src/mz_std/<module>.rs` **only when the program imports that module**. The
  package therefore holds exactly the library the program uses. The name `mz_std` keeps the `mz_`
  prefix that RFC-0013 §14.2 reserves for the lowering, and it avoids the Rust name `std`.
- **JSON decoding is generated, the parser is not.** `R.from_json` is a generated function per record,
  and it calls `mz_std::json`'s parser. The parser returns a tree; the generated code walks the tree into
  `R`'s fields in declaration order, and a missing field or a wrong kind is `Err` with a message naming
  the field path. **Unknown fields are rejected** (survey §4.2, "deny_unknown_fields or not"; §14, Q8).

### 2.5 Errors and effects

- **Every function that can fail returns `result`** (D5, RFC-0013 §12). A call whose result nothing
  handles is `MZ0950`, as for any other function. This is T2-5's guard: the standard library gives no
  function that fails silently.
- **Effects are named in the table** (§2.1). Whether an effect needs a capability declared in the
  program is open (§2.6).
- **`example` clauses are pure** (RFC-0013 §15.1). A clause that calls a function with an effect in its
  body, or calls one through a function that does, is `MZ1016`. `mz contract` could not run it
  reproducibly (T2-6), so a check that succeeds on one machine and fails on another is not written.

### 2.6 Capabilities for I/O: an open question, not M2's decision

RFC-0001 §1.7 gives capabilities their own word (`use net`, `use storage`), and D7 says `use` must not
also mean "depends on a module". The I/O modules need some answer to "is this effect declared?", and two
answers are possible:

- **(A) Ungated in M2.** `files`, `env`, `http` and `time.sleep` are callable from any `fn`, and the
  capability system (P7) gates them later.
- **(B) Gated in M2.** A program that calls `files.read` must declare `use files` (a P7 word), and the
  checker reports `MZ1015` otherwise.

This RFC **recommends (A)** for M2, because P7 is not in M2 and (B) would build part of P7 inside P1–P6.
The names in RFC-0001 §1.7 (`net`, `storage`, `ml`, `motion`) do not match `files` or `env` either. §14,
Q7 asks the owner. `MZ1015` is reserved until then.

### 2.7 Other languages' spellings (MZ1014)

A library call spelt another way is `MZ1014`: `os.path.join` (guess `files` has no join: the say says
so), `json.dumps` (guess `R.to_json()`), `json.loads` (guess `R.from_json`), `Math.floor` (guess
`math.floor` is not in the list, so the say says that), `Date.now()` (guess `time.now()`),
`requests.get(url)` (guess `http.get(url)`), `fs.readFileSync` (guess `files.read`). A fix is `guess` in
every case, because the mapping changes the call and its result type (`result`, not a bare value). The
single `exact` case is a case-only spelling of a name that exists (`Time.now` to `time.now`).

### 2.8 Harness entries

Each function registers one `HarnessEntry` with its signature, its effects and at least one example, which
the harness test checks (RFC-0012 §1.2). An example that gives output is run through `mz run` by the
existing harness test, so a documented function that does not behave as its entry says is a failed build.

---

## 3. Lowering all code — _P3_

Wave 6 (§12.6). P3's "Done when": every construct in Tier 1 lowers to Rust that `rustc` compiles, tested in
CI.

### 3.1 What lowers today

From the tracker and the code (`compiler/src/run.rs`, `compiler/src/run/*.rs`): a `program`'s Tier 1 forms
that are built lower to a dependency-free Rust package, and CI runs `examples/*.mz` through `mz run`
against `.expected` files. A `service` lowers to axum (RFC-0011 §8). No component lowers (RFC-0007 G2.1).

### 3.2 What does not, verified on this branch

On a `mz` built from `origin/staging` at `a3f6aba` (2026-10-09), `mz check` on three probe files gave:

- `fn area(width: int, height: int): int` called as `area(width = 3, height = 4)`: **two `MZ0905`** errors,
  "a call gives its arguments by position". So **named arguments on a user function are not built**,
  although RFC-0013 §6.5 designs them and the C3 row is ✅ in the tracker. The `MZ0927` labels that exist
  are for text methods only (`compiler/src/program/text.rs`).
- `import geometry` in a `program`: `MZ0901` ("a program holds `fn`s and `enum`s, not statements") and
  `MZ0707` for the later use. So `import` is an error today, as expected.
- `let unused = 3` in a function that never reads it: **no diagnostic**. RFC-0013 §5.1's `MZ0928` is not
  built.
- `option(T)` written as a type is `MZ0919` (not built, `compiler/src/parse/program/collections.rs`).
- `wrapping_add` appears in `compiler/src/hash.rs` only, not in a `program`: RFC-0013 §18.2's C5 wrapping
  follow-up is not built.
- `try e via f` (RFC-0013 §12.2, `MZ0955`): no emitter.

### 3.3 The tracker and the follow-ups disagree

RFC-0013 §18.2 lists five follow-ups that Tier 1 needs: C3 labels, C2 unused bindings, C4 options, C9
`via`, and C5 wrapping. §3.2 shows four of them are not built. The tracker still marks C2, C3, C4, C5 and
C9 ✅, and says in its own evidence column what is not built (for example "Not built: `option(T)` written
as a type"). A row's ✅ is its "Done when", and C3's "Done when" (a function that calls another with
arguments and returns a typed value) is met by positional calls. So the tracker is consistent with itself,
and P3's "Done when" ("every construct in Tier 1 lowers") is not.

**Two resolutions, the owner's choice (§14, Q13):**

- **(a)** M2 requires the follow-ups. This RFC's Wave 6 builds them, and the tracker's P3 stays 🟡 until
  it is done.
- **(b)** The owner narrows P3's "Done when" to the forms the tracker marks ✅, and the follow-ups become
  Tier 3 items. This RFC recommends (a), because the follow-ups are small and each one is a construct a
  model writes (T2-1's and CL-1's failures), but the choice is the owner's, as RFC-0013 §20 Q20 was.

### 3.4 Modules and the standard library lower the same way

A module's constructs lower by the rules of RFC-0013 §14.2; a standard function lowers to a call into its
`mz_std` runtime (§2.4). No new construct in Tier 2 is lowered by a new mechanism except `extern` (§6.3),
which is a declared boundary and not a translation.

### 3.5 The test that holds it

`compiler/tests/harness.rs` already runs every harness example that gives an output through `mz run`
(skipped, and said so, without `cargo`). Each Tier 2 entry adds at least one such example, so the registry
is the list of what lowers and runs. P3 is met when every Tier 1 and Tier 2 entry has one, and the test is
green in CI's `lowering` job.

---

## 4. Runnable builds — _P4_

Wave 1 (for multi-file packages, §12.1) and Wave 6 (completion, §12.6). P4's "Done when": a general program
builds to a native binary and runs.

### 4.1 Commands

```text
mz run <entry.mz>                check every file the entry imports, lower, build with cargo, run
mz build <entry.mz> --out <dir>  write the package for the entry and its imports
mz build <entry.mz> --out <dir> --lib   (Wave 5, §6.7) a library crate for Rust callers
```

`mz run` and `mz build` keep RFC-0013 §13.1's exit statuses. One addition, from §6.6: a crate the registry
does not have is `MZ1022`, which is exit 3 (the program does not build), while a registry that cannot be
reached when the package needs fetching is exit 2 (the environment, as `cargo` missing is). `cargo`'s own
message decides which: "no matching package named" is the program's fault, and a network error is not.

### 4.2 The cache, and the package

`mz run` keys its per-user cache by the **entry file's** path, as RFC-0013 §13.1 says. A multi-file
program's package is generated for the whole import closure in one directory, and the manifest of §1.6
removes stale files, so an edit to `geometry.mz` rebuilds `geometry.rs` and Cargo does the rest. A program
that changes its imports is rebuilt from the same directory, with the removed module's file deleted.

### 4.3 What P4 does not add

`mz run --agent` (RFC-0013 §13.1, "not built yet") is not in M2. An agent reads `mz run`'s standard error as
text today. `mz build --target` (survey F40) is P11, not P4.

---

## 5. Errors mapped back to `.mz` — _P5_

Wave 4 (§12.4). P5's "Done when": a `rustc` error in lowered code is reported at the `.mz` line that caused
it, or cannot happen by construction.

### 5.1 The mechanism: markers in the generated code

The lowering writes a marker comment above each emitted statement and item:

```rust
// @mz geometry.mz:12:5
let area = width * height;
```

When `rustc` rejects `src/geometry.rs:14:9`, `mz run` scans upward from that line to the nearest marker and
reports `geometry.mz:12:5`, with the construct's canonical text (which `MzAt.text` already holds for traps,
RFC-0013 §14.3) and `rustc`'s first message. A marker is a comment, so the generated code compiles the same
with or without it, and a test checks that.

**Why markers, not a map file:** the generated code stays self-contained (a reviewer reads one file and
sees each line's source), the map cannot drift from the code it describes (they are the same bytes), and it
is deterministic. The cost is size, and §14, Q14, asks whether the owner prefers a separate file.

### 5.2 What is mapped and what is not

- **Lowered code** (`src/main.rs`, `src/<module>.rs`, `src/mz_std/*.rs`): mapped by §5.1. A `rustc` error with
  no marker above it is `MZ0990` at the program's entry file, as today, and the test suite asserts that no
  such error occurs for the examples.
- **Companion files** (`<module>.rs`, §6.1): not generated, and not mapped. Their `rustc` output is printed
  as `rustc` gives it, with a note that the file is the author's, and `mz run` exits 3.
- **`extern` shims** (§6.3): a shim's signature is generated from the `extern` declaration, so a `rustc` error
  in a shim is reported as `MZ1024` at that declaration.
- **Runtime faults** keep their codes (`MZ0991` trap, `MZ0992` main's error, `MZ0993` runtime failure), and
  name the module's file through `MzAt`.

### 5.3 Done when

A test lowers a program whose `extern` declaration names a companion function with the wrong signature, and
asserts that `mz run` reports `MZ1024` at the declaration's line. A second test, with a lowering fault
injected into a unit test's package (not a feature flag), asserts that the marker lookup finds the
construct's line. Neither test needs the network.

---

## 6. Rust interop: calling crates, mixing `.mz` and `.rs` — _P6_

Wave 5 (§12.5). P6's "Done when": a `.mz` program uses a crates.io crate, and a Rust crate calls Mzizi code.

### 6.1 Companion files: `.rs` beside `.mz`

A module `geometry.mz` may have a companion `geometry.rs` in the same directory. The companion is Rust
written by the author. In the package it is copied **verbatim** to `src/geometry/ext.rs`, and the generated
`src/geometry.rs` declares `pub mod ext;`. The companion is not checked by `mz` (it is the one place
unchecked code enters; the survey's R2 rejects escape hatches, and this is the one declared exception), and it is held to three rules:

1. Its items are reached only through an `extern` declaration (§6.3).
2. It takes and returns **owned** values, the types of §6.3's table. It has no references in its public
   signatures, because Mzizi has no references (RFC-0013 §14.1, P10).
3. It contains no `unsafe`. The generated crate root carries `#![forbid(unsafe_code)]`, which applies to the
   companion too, so the rule is enforced by `rustc`, not by convention (T2-8). A companion that needs
   `unsafe` cannot be written in M2; `export c` (§6.8) is where that would come.

### 6.2 Crates: one import line, an exact pin

```mz
program fetch_count

  import crate serde_json "=1.0.145"

  extern fn count_items(text: text): result(int, text) rust "count_items"

  fn main
    print("{count_items("[1, 2, 3]")}")
  end fn main

end program fetch_count
```

- `import crate <name> "<version>"` declares a crates.io dependency. **The version must be an exact pin**,
  `=x.y.z`. A range (`"1.0"`) or a bare version is `MZ1021`, with no fix, because a `guess` would pick
  a version the author did not choose (T2-3: the same mistake, a model choosing a version it has no basis
  for). Survey F47 asks for exact versions and a lockfile.
- The line is in the file that uses the crate, and `mz` writes it to the package's `Cargo.toml`. A crate
  name that the registry does not have is `MZ1022`, with no fix: the say names the crate, and nothing
  offline can tell the nearest real name.
- The spelling `import crate <name> "<pin>"` is a proposal, and §14, Q9, asks for it.

### 6.3 The `extern` boundary

```mz
extern fn count_items(text: text): result(int, text) rust "count_items"
```

- An `extern fn` is a function whose body is Rust. Its signature is written in Mzizi, and it is lowered to
  a **generated shim** with that exact Rust signature. The shim calls the Rust item named after `rust`:
  with no `::` it is the companion's item (`src/geometry/ext.rs`, for a declaration in `geometry.mz`); a
  path with `::` is written from the crate root (`serde_json::from_str`).
- **Representable types only**: `int` (`i64`), `float` (`f64`), `bool`, `text` (`String`), `list(T)`
  (`Vec<T>`), `option(T)`, `map(K, V)` (`BTreeMap`), `set(K)` (`BTreeSet`), `result(T, E)`, and the module's
  own records and enums. A crate type (`serde_json::Value`), a borrow, a closure or a function type in the
  signature is `MZ1023`, with no fix. Mzizi cannot describe those values, so a shim cannot carry them.
- **The check is `rustc`'s.** If the Rust item does not have the declared signature, the shim does not
  compile, and §5.2 maps the error to `MZ1024` at the declaration. This is how a wrong signature is caught
  without `mz` parsing Rust.
- **Why not generate signatures from the crate.** Survey F37 proposes a Mzizi view of a crate's API from
  rustdoc JSON. rustdoc's JSON format is unstable and needs a nightly toolchain (survey §4.2 asks which
  format version). M2 does not depend on it: the author writes the boundary, and `rustc` checks it.
  Generated signatures are a later design (§14, Q9).

### 6.4 Ownership at the boundary

Values cross by value (RFC-0013 §14.1): a `text` argument is passed as a `String` the shim owns, so the
companion receives an owned value and returns one. The clone a call needs is made by the shim, in
generated code, and the author never writes a borrow (CL-5, R3).

### 6.5 What a crate changes about the offline promise

A program with no `import crate` lowers to a package with no dependencies, which builds with
`cargo build --offline` (RFC-0013 §13.1, unchanged). A program with one **must fetch** the crate on its first
build, so the promise holds only for crate-free programs. §14, Q11, asks the owner to accept that. The
per-user cache (RFC-0013 §13.1) keeps the fetch once per machine. `--offline` on a program whose crate is
not cached is exit 2, with the message naming the crate.

### 6.6 HTTP and TLS

`http` (§2.1) needs TLS, and the standard library has none. Its implementation is one pinned crate, which
makes `http` the first std module that depends on a crate, and the first call to `mz run`'s network
behaviour that is not optional. **`http` is Wave 5, not Wave 3**, because it needs §6.2. Its tests use a
plain `http://` server started in the test (`std::net`), and an `https://` test is an integration test run
in CI, not a unit test.

### 6.7 Calling Mzizi from Rust: `--lib`

`mz build <entry.mz> --out <dir> --lib` writes a Cargo **library** crate instead of a binary. Its root
declares `pub mod <module>;` for each module, and each `public` item is `pub` (§1.6). A Rust crate in the same
Cargo workspace depends on it by path and calls it:

```rust
// In another crate of the workspace, depending on the generated library.
let area = mzizi_geometry::geometry::area(3.0, 4.0);
```

The companion of §6.1 is part of the library too, so a Rust caller sees the same functions the Mzizi program
does. The generated library has the same `#![forbid(unsafe_code)]`.

### 6.8 C ABI (`export c`): not in M2

Survey F38 proposes `export c` for calling Mzizi from any language. A C ABI needs `unsafe` (the pointers and
the returned buffers), and §6.1 forbids `unsafe` in companions and in generated code. It is therefore not
designed here beyond the boundary that §6.7 gives Rust callers. §14, Q12, asks whether the owner wants it
in M2 at all; this RFC's answer is no.

### 6.9 Capabilities for crates

RFC-0007 G2.9 says a crate boundary is "typed, capability-tagged". Tagging crates with a capability is P7's
design, and an `import crate` in M2 is ungated, for the reason §2.6 gives. Q7 covers it.

### 6.10 CI

The `lowering` job gains one program that imports one small crate, pinned exactly, with a companion, and runs it
against an `.expected` file. The crate's licence must be on `deny.toml`'s allow list in the same pull request
(AGENTS.md, "The supply chain"). The job's network use is crates.io only, which AGENTS.md permits.

---

## 7. P7–P12: reserved, not designed here

Each section says what is deferred and why. None is in M2 (tracker, "Milestones").

- **P7, capabilities enforced.** The `use` line (RFC-0001 §1.7) is parsed in components and services, and is not
  built in a `program` (`MZ0919`). §2.6 and §6.9 need P7's answer. Until P7 lands, the I/O modules are ungated
  (§2.6, option A).
- **P8, state and I/O.** M2's I/O is whole-file reads and writes (§2.1 `files`) with no handles, so nothing
  needs closing. Handles, databases and the query sink of survey F30 (context-aware interpolation, which needs
  the SQL survey that §2 of the survey names as a gap) are P8's. Survey F34's scope-bound cleanup waits with
  them: value semantics means the compiler owns cleanup, and there is nothing to schedule until a handle exists.
- **P9, concurrency.** Survey F31–F33 (no async colouring, `together` blocks, shared state through one owner).
  The survey's §4.2 says P9 is "likely its own RFC", and this RFC agrees. `time.sleep` (§2.1) is the only
  concurrency-adjacent word in M2, and it does not start a task.
- **P10, memory model.** RFC-0013 §14.1 holds: values are owned, reads are clones, and copy-on-write is
  RFC-0013 §20 Q15. Modules change nothing in P10. The tracker's P10 row stays 🟡 until Tier 1 lowers whole (§3).
- **P11, targets.** Native, through `cargo`, is the only target. WebAssembly, Workers and Containers are P11, and
  survey F40's `--target` flag is too. R13 (freestanding targets) is still deferred (RFC-0007 §4 and CHARTER §2).
- **P12, generics and interfaces.** Deferred past M1 and M2 (owner, #69). M2's standard library is closed without
  them (§2.3), so P12 is the blocker for `zip`-style library functions and for an interface on a record (survey F25,
  F26). The JSON decoder (§2.4) does not need them, because it is generated per record.

---

## 8. Design for the machine author

Mzizi's thesis (CHARTER.md) is that small open-weight models can author it. Each rule below is a choice that
keeps a model's most likely output either correct or caught with one fix.

1. **One way to import.** `import <name>`. No `from`, no `use`, no `#include`, no `require`, no `mod`.
2. **One way to name across files.** `<module>.<name>`, every time. No bare names from another file.
3. **One visibility word.** `public`, on a declaration's header. No `pub`, no `export`, no `static`.
4. **Other languages' spellings are recognised, not accepted.** `MZ1009` names the form and gives the Mzizi one.
   The table:

   | Written                             | From       | Diagnostic and fix                                                          |
   | ----------------------------------- | ---------- | --------------------------------------------------------------------------- |
   | `from geometry import area`         | Python     | `MZ1009`, `guess`: `import geometry` and qualify the call sites             |
   | `import { area } from "./geometry"` | JavaScript | `MZ1009`, `guess`                                                           |
   | `#include "geometry.h"`             | C          | `MZ1009`, `guess`                                                           |
   | `use crate::geometry::area;`        | Rust       | `MZ1009`, `guess`                                                           |
   | `mod geometry;`                     | Rust       | `MZ1009`, `guess`                                                           |
   | `pub fn area`                       | Rust       | `MZ1009`, `exact` to `public fn area` (the same meaning, and a local token) |
   | `export function area`              | JavaScript | `MZ1009`, `exact` to `public fn area`                                       |
   | `import geometry;`                  | many       | `MZ1009`, `exact` (delete the `;`)                                          |

5. **`exact` only when the meaning and the rest of the file are unchanged.** RFC-0008 §5.2's rule. A fix that
   changes a call site in another line is `guess`.
6. **An unknown name gets the nearest name, as a `guess`.** Never a fabricated name.
7. **Determinism is part of the language.** Imports sort, the file set is the import closure, diagnostics are in a
   fixed order, two builds are byte-identical, and a program that depends on the clock or the environment says so
   through `time` or `env` in its header (§2.1).
8. **A dependency is in the source, once.** `import crate` is the only place a crate is named, and the pin is exact.
   There is no separate manifest for an author to keep in step (T2-9).

---

## 9. Compiler discipline

- **The compiler crate stays dependency-free.** Everything in §1–§6 is `std`. The loader is `std::fs`. The
  generated packages depend on crates only where §6 and §2.6 say, and each of those is a pinned version written in
  the source.
- **`mz` still takes one file.** The loader is a function in `compiler/src/project.rs` (Wave 1), and `check(src,
file)` in `compiler/src/lib.rs` keeps its signature for a file with no imports. A new entry point, `check_project`,
  reads the closure.
- **`#![forbid(unsafe_code)]` stays** in the compiler crate (`compiler/src/lib.rs`). The generated package forbids
  `unsafe` for the same reason (§6.1).
- **Exit statuses** are RFC-0013 §13.1's, plus the one addition in §4.1.

---

## 10. Diagnostics

`MZ10xx` is the family RFC-0013 §16 names for the next wave of codes. The tens digit groups them by part of this RFC.
Each code has a trigger that the harness test checks (RFC-0012 §1.2).

| Code     | Tool                             | Means                                                                                                                                                                                                                                            |
| -------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `MZ1001` | `mz check`                       | a module file whose declaration name is not its file name (`exact`: rename the declaration), two declarations in one file (`guess`: split), or a name the module may not take (a Rust word, `std`, `core`, `alloc`, or a standard module's name) |
| `MZ1002` | `mz check`                       | an `import` of a module with no file beside the entry (`guess`: the nearest file name), or a dotted import in M2 (`guess`: the flat name)                                                                                                        |
| `MZ1003` | `mz check`                       | an import cycle, the chain named from its first module; no fix                                                                                                                                                                                   |
| `MZ1004` | `mz check`                       | a use of a name the module does not make `public` (`guess`: add `public` to its declaration)                                                                                                                                                     |
| `MZ1005` | `mz check`                       | a qualified name the module does not declare (`guess`: the nearest name)                                                                                                                                                                         |
| `MZ1006` | `mz check`                       | an `import` after a declaration or inside a `fn` (`exact`: move it to the header), or `public` where nothing is exported (`exact`: delete the word)                                                                                              |
| `MZ1007` | `mz check`, `mz run`, `mz build` | a `program` imported, or a module file given to `mz run` or `mz build`; no fix                                                                                                                                                                   |
| `MZ1008` | `mz check`                       | a local name and an imported module of one name, or two imports of one name; no fix                                                                                                                                                              |
| `MZ1009` | `mz check`                       | a module spelling from another language (§8's table): `exact` where the rewrite is one token, `guess` otherwise                                                                                                                                  |
| `MZ1011` | `mz check`                       | an `import` of a module outside the closed standard list (`guess`: the nearest module name)                                                                                                                                                      |
| `MZ1012` | `mz check`                       | a standard function the module does not have (`guess`: the nearest name)                                                                                                                                                                         |
| `MZ1014` | `mz check`                       | a library call spelt from another language (§2.7): `guess` to the Mzizi function; `exact` only for a case-only spelling                                                                                                                          |
| `MZ1015` | `mz check`                       | reserved for option (B) of §2.6: an effect called without its `use` line                                                                                                                                                                         |
| `MZ1016` | `mz check`                       | an effect inside an `example` clause, which must be pure (§2.5); no fix                                                                                                                                                                          |
| `MZ1021` | `mz check`                       | an `import crate` whose version is not an exact `=x.y.z` pin (§6.2); no fix                                                                                                                                                                      |
| `MZ1022` | `mz run`, `mz build`             | a crate the registry does not have (`cargo`: no matching package); exit 3; no fix                                                                                                                                                                |
| `MZ1023` | `mz check`                       | an `extern` signature with a type Mzizi cannot represent (§6.3); no fix                                                                                                                                                                          |
| `MZ1024` | `mz run`                         | the Rust item an `extern` names does not have the declared signature, reported at the declaration (§5.2); no fix                                                                                                                                 |
| `MZ1025` | `mz check`                       | an `extern` declaration in a module with no companion `<module>.rs`; no fix                                                                                                                                                                      |
| `MZ1026` | `mz check`                       | Rust syntax written in a `.mz` file (`unsafe`, `impl`, `#[…]`, `pub(crate)`) outside an `extern` declaration; no fix                                                                                                                             |

**Reserved:** `MZ1010`, `MZ1013` (a standard function's arguments, which use `MZ0905`), `MZ1017`–`MZ1020`,
`MZ1027`–`MZ1030`, and `MZ1031`–`MZ1099`, for the waves and for P7–P12.

**Reused, so one mistake keeps one code** (RFC-0013 §16's rule): `MZ0101` (a camelCase module name), `MZ0105`
(a type spelt with symbols), `MZ0707` (a name not bound, §1.2), `MZ0713` (shadowing), `MZ0905` (a call's arguments,
including a standard function's), `MZ0921` (a reserved name, §1.1), `MZ0928` (an unused import, once RFC-0013 §5.1's
`MZ0928` is built, §3.2), `MZ0950` (an unhandled `result` from a standard function, §2.5), `MZ0919` (a form not built
yet, which every wave retires), `MZ0990` (a `rustc` error in lowered code, now with the `.mz` location, §5), `MZ0991`
(a trap), `MZ0993` (the runtime failed).

All diagnostics follow RFC-0001 §4: `say` at most 200 characters and quoting the source, deterministic order, one
diagnostic per real error, and fixes tagged `exact` or `guess` as the table says.

---

## 11. The language harness

Each wave registers its entries (RFC-0012 §1.2): a feature is not built until its entry, codes and tested examples
are in `compiler/src/harness.rs`.

- **Wave 1:** entries for `module`, `import`, `public` and qualified names; `MZ1001`–`MZ1009`; the `program` entry
  gains "imports modules".
- **Wave 2:** `text.join`, `text.pad_left`, `text.pad_right`, `text.lines`, `math.*`; `MZ1011`, `MZ1012`.
- **Wave 3:** `env`, `files`, `time`, `json` and each `from_json`; `MZ1014`, `MZ1016`.
- **Wave 4:** `MZ0990`'s entry is updated for the `.mz` location.
- **Wave 5:** `import crate`, `extern`, `--lib`, `http`; `MZ1021`–`MZ1026`.
- **Wave 6:** the Tier 1 follow-ups (C2 `MZ0928`, C3 labels, C4 options, C5 wrapping, C9 `via`), each with its entry.

`mz harness definition` is deterministic (RFC-0012 §1.2), so the wave entries change its `version` hash and nothing else.

---

## 12. Implementation plan

Each wave is one or more pull requests, each on its own branch from `origin/staging`, each updating its tracker row,
its CHANGELOG entry and the harness, and each passing AGENTS.md's checks. A row turns ✅ only when its "Done when" is on
`main` and green. Issue #110 tracks the waves.

### 12.1 Wave 1: P1 modules (serial, first)

**Builds:** `compiler/src/project.rs` (the loader and import closure), `module` and `import` in `parse/program.rs`, the
module checker (`program.rs`, with an environment of public signatures), `public`, qualified names in expressions and
types, the multi-file lowering with `mz-manifest.txt`, and `mz build` of a multi-file package (P4, first half).

**Examples:** `examples/modules/main.mz` with `examples/modules/geometry.mz`, and `examples/modules/main.expected`. The
`examples/*.mz` loop in CI does not reach `examples/modules/` (it is not recursive), so the lowering job gains one step
for it.

| Row  | Done when, and the test that shows it                                                                                                                                                                                                                                                                                                          |
| ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P1   | "Code in one file calls code in another; private names are not visible": `examples/modules` runs and matches its `.expected`; `compiler/tests/modules.rs` asserts `MZ1004` for a private name, `MZ1003` for a cycle, `MZ1002` for a missing file with its `guess`, `MZ1006` and `MZ1007` for the misplaced cases, `MZ1001` for a name mismatch |
| P4   | "A multi-file program builds and runs": `mz build` of `examples/modules` twice gives byte-identical packages; a test deletes an import and asserts the removed module's file is gone and nothing else in the package directory is touched                                                                                                      |
| (P5) | a trap in `geometry.mz` names `geometry.mz`: a test                                                                                                                                                                                                                                                                                            |

### 12.2 Wave 2: the first P2 modules

**Builds:** `compiler/src/std/text.rs` and `compiler/src/std/math.rs`, the `mz_std` emission of §2.4, `text.join`,
`text.pad_left`, `text.pad_right`, `text.lines`, the `math` set of §2.1, and `MZ1011`, `MZ1012`.

| Row | Done when, and the test that shows it                                                                                                                                                                                                                          |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P2  | the `text` and `math` modules exist with tests: one unit test per function in `compiler/src/std/*.rs`, a harness example per entry, `math.sqrt(-1.0)` is `nan` with the C5 rule, and a program that imports neither module has no `mz_std` file in its package |

### 12.3 Wave 3: the rest of P2 that is std only

**Builds:** `env`, `files`, `time`, `json` (the parser and the generated `from_json`), `MZ1014`, `MZ1016`.

| Row | Done when, and the test that shows it                                                                                                                                                                                                                                                                                      |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P2  | `files.read` of a missing path is `files.problem`'s `not_found`; `env.get` of an unset name is `none`; `point.from_json(point(x = 1.0, y = 2.0).to_json())` is the same point; a bad input is an `Err` naming its field; `time.iso(0)` is `1970-01-01T00:00:00.000Z`; an `example` clause calling `files.read` is `MZ1016` |

### 12.4 Wave 4: P5, errors mapped back to `.mz`

**Builds:** the markers of §5.1, the `rustc` output parser in `compiler/src/run.rs`, the `MZ1024` mapping, and the
updated `MZ0990`.

| Row | Done when, and the test that shows it                                                                                                                         |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P5  | §5.3's two tests: `MZ1024` at the declaration, and a marker lookup that finds the construct's line; `MZ0990` names a `.mz` file and line for a lowering fault |

Wave 4 comes before Wave 5 because `extern` (§6.3) is reported through this mapping.

### 12.5 Wave 5: P6, crates and companions, and `http`

**Builds:** `import crate`, companions (§6.1), `extern` (§6.3), `--lib` (§6.7), `http` (§2.1, §6.6), the crate-licence entry in
`deny.toml` and the `lowering` job's crate step, and `MZ1021`–`MZ1026`.

| Row | Done when, and the test that shows it                                                                                                                                                                                                                                                   |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P6  | `examples/crates/` imports one pinned crate, calls a companion through `extern`, and matches its `.expected`; a companion with `unsafe` is rejected by `rustc`; a Rust test crate in a workspace calls the `--lib` output; `http.get` against a local `http://` server returns its body |

### 12.6 Wave 6: P3 and P4, the rest

**Builds:** the Tier 1 follow-ups (§3.2): `MZ0928` unused bindings with its `exact` and `guess` deletions; named arguments
on user functions (RFC-0013 §6.5, `MZ0927` for user calls); `option(T)` as a written type, with narrowing and `otherwise`
on it (RFC-0013 §8); `wrapping_add`, `wrapping_sub`, `wrapping_mul` and `a // b` as `MZ0910` (RFC-0013 §4.4); `try … via`
(RFC-0013 §12.2, `MZ0955`); and a check that `changes self` lowers (RFC-0013 §11.2). This wave is needed only if the owner
chooses Q13 option (a).

| Row | Done when, and the test that shows it                                                                                                |
| --- | ------------------------------------------------------------------------------------------------------------------------------------ |
| P3  | every Tier 1 and Tier 2 harness entry with an output example runs through `mz run` in CI (§3.5), and no Tier 1 form is `MZ0919`      |
| P4  | "a general program builds to a native binary and runs": a program that uses every Tier 2 form builds and runs in CI's `lowering` job |

### 12.7 M2

M2 is met when P1–P6 are ✅ in the tracker by their own "Done when" rows (§12.1–§12.6), on `main`. The owner records
that, as the tracker's rule requires. Nothing in §12 changes P7–P12.

---

## 13. What this RFC does not claim

- That any of it is built. §3.2 records what `mz` does today, verified on a branch cut from `origin/staging`; each wave's
  tests make a form true when they are on `main`.
- That the closed standard-library list (§2.1) is right. It is a proposal, and §14 asks the owner about it.
- That a model writes Mzizi's modules or imports better than it writes Python's, TypeScript's or Rust's. The design
  removes choices (T2-1, T2-2) and reports a wrong one; whether a model is more often right is the public suites'
  question (RFC-0009), which needs P2 and nothing has run.
- That Mzizi's generated Rust is as fast as hand-written Rust. Modules add no cost of their own, and the value-semantics
  clones of RFC-0013 §14.1 still apply.
- That `rustc` will accept every lowering. §5 says what happens when it does not; the design does not claim it cannot happen
  except where a construct is restricted to what the lowering handles (§6.3's representable types).
- That a companion file is safe. A companion is the author's Rust. §6.1 forbids `unsafe` through the crate root, and nothing
  else checks it.
- That `http` is secure. It is one pinned crate's TLS, and its tests are integration tests (§6.6).
- That the decisions of §8 are the only ones a small model needs. They are the ones this RFC can justify from §0's failure
  modes, and §14 leaves the rest to the owner.

---

## 14. Open questions for the owner

Each is a decision this RFC made provisionally so that the design is whole. **Each needs the owner's yes, no or change before
the wave that builds it.**

1. **Module layout.** Flat only, every module beside the entry file (§1.2, this RFC), or dotted paths into subdirectories
   (`import shapes.circle` reads `shapes/circle.mz`) in M2? Flat is simpler for a small model and for the loader. Dotted
   paths need a Rust module tree that mirrors the directories.
2. **The import word, and the file kind.** `import` for modules (§1.2, the survey's F27, the answer to D7), `module` for a
   library file and `public` for exports (§1.1, §1.3). Accept all three, or name them differently? `use` stays capabilities
   (RFC-0001 §1.7).
3. **Qualified access only.** Every name from another module written `module.name` (§1.2), with no `import module.name` and
   no `from module import name`. Accept?
4. **The closed standard-library list.** The tracker's P2 row (text, math, collections, time, JSON, environment, files, HTTP
   client; §2.1) or RFC-0007 G2.10 (text formatting, time, JSON, HTTP client, hashing and randomness, logging)? This RFC takes
   the tracker's, adds nothing, and leaves hashing, randomness and logging out (§2.1). Which list is closed for M2?
5. **Prelude or import.** Methods on built-in types are the prelude and module functions are imported (§2.2, this RFC), or,
   as the survey's F27 says, the standard library is a prelude and `math.sin` needs no `import`?
6. **Std-only.** Every module except `http` implemented in `std` alone, so a program with no crate is dependency-free and
   `mz run` stays offline (§2.4). Accept? The alternative is a pinned crate for `time` and `math`, which makes every
   program that imports them need the network once.
7. **Capabilities for I/O.** Option (A), ungated in M2 with P7 later (§2.6, this RFC's recommendation), or option (B), a
   `use files` line checked in M2 (`MZ1015`)? And should the names be RFC-0001 §1.7's, or new ones for `files` and `env`?
8. **JSON decoding.** `point.from_json(text)`, generated per record, and unknown fields rejected (§2.1, §2.4, this RFC), or a
   generic `json.decode` that needs P12, or accepting unknown fields? Survey §4.2 asked "`deny_unknown_fields` or not", and
   this RFC answers yes.
9. **The crate boundary.** A pin spelled `import crate <name> "=x.y.z"` (§6.2), and `extern fn` with a hand-written companion
   and `rustc` as the check (§6.3), against generated signatures from rustdoc JSON (survey F37, which needs a nightly format).
   Accept the spelling and the companion design for M2?
10. **Companion files.** A `<module>.rs` beside `<module>.mz`, copied verbatim, unchecked by `mz`, with `unsafe` forbidden at
    the crate root (§6.1). Accept?
11. **The offline promise.** A program that imports a crate fetches it on its first build, so RFC-0013 §13.1's "no network"
    holds only for crate-free programs (§6.5). Accept?
12. **`export c` and the C ABI.** Not in M2 (§6.8, this RFC), because it needs `unsafe`. Confirm, or ask for it in M2 with
    its own `unsafe` rule?
13. **The Tier 1 follow-ups.** The tracker marks C2, C3, C4, C5 and C9 ✅, while RFC-0013 §18.2 lists them as unbuilt and §3.2
    confirms four are. Should M2 require them, which is Wave 6 (§3.3 option (a), this RFC's recommendation), or should P3's
    "Done when" be narrowed (option (b))? The tracker's wording is the owner's, as RFC-0013 §20 Q20 was.
14. **Source map.** Markers in the generated Rust (§5.1, this RFC), or a separate map file in the package?
15. **Time.** UTC only, with `now`, `sleep` and `iso` (§2.1). Time zones need a database and so a crate. Accept UTC only for M2?
16. **Hashing, randomness and logging.** Out of M2's list (§2.1), as this RFC proposes? If in, which hash, and seeded
    randomness only?
17. **`env.exit`.** Allowed in M2, with the statuses RFC-0013 §13.1 reserves (`1`, `70`, `101`, `134`, `141`) either refused
    or allowed? This RFC proposes allowing `0` through `255` except those five, checked where the argument is a literal and
    trapped where it is not.
18. **Element-generic library functions.** Not in M2 (§2.3), so `zip` and `first` wait for P12. Accept?
19. **The diagnostic family.** `MZ10xx` for Tier 2 (§10), as RFC-0013 §16 names it as the next family. Accept, or does the
    owner want a different family for the standard library?

---

## 15. Reconciliation with the top-10 language survey

`design/LANGUAGE-SURVEY.md` (F27–F40, D7, §4.2 and §7) lists what this RFC must design. This table compares each row with
this RFC. "Kept" means the survey's design is adopted as written; "amended" means this RFC changed it; "departs" means it
chose otherwise, for the reason given; "deferred" means it is not in M2.

| Survey row                                        | Where here | Status                                                                                                                                                                                      |
| ------------------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| F27 modules, explicit exports, a prelude          | §1, §2.2   | amended: `import` and `public` kept; flat layout; qualified access only; the prelude is the methods, not module functions (departs from "the stdlib is a prelude"); cycles are errors, kept |
| F28 decided, versioned standard library           | §2.1, §2.4 | amended: a closed list, the owner's to choose (Q4); std only except `http`; versioned with `mz`, as the survey says; unknown names get nearest-name fixes, kept                             |
| F29 JSON as native data, typed at the boundary    | §2.1, §2.4 | kept: records encode by construction (C8), decoding only into a declared record, with a declared rejection (`result(R, text)`); unknown fields rejected (the survey's question, answered)   |
| F30 context-aware interpolation                   | §7 (P8)    | deferred: needs a query sink (P8) and the SQL survey the survey's §2 names as a gap                                                                                                         |
| F31–F33 no async colouring, `together`, one owner | §7 (P9)    | deferred to P9's own RFC, as the survey's §4.2 proposed                                                                                                                                     |
| F34 deterministic cleanup                         | §7 (P8)    | deferred with handles                                                                                                                                                                       |
| F35 value semantics with copy-on-write            | §7 (P10)   | kept (RFC-0013 §14.1); copy-on-write is RFC-0013 Q15                                                                                                                                        |
| F36 zero overhead, cost made visible              | §13        | not claimed                                                                                                                                                                                 |
| F37 host ecosystem through generated signatures   | §6.2–§6.4  | amended: the crate is named in the source with an exact pin; the boundary is a hand-written companion checked by `rustc`; generated signatures are deferred (Q9). Departs from rustdoc JSON |
| F38 callable from every language (`export c`)     | §6.8       | deferred (Q12); the Rust-caller path is §6.7                                                                                                                                                |
| F39 runtime faults reported like compile errors   | §5, §1.7   | amended: `rustc` errors in lowered code map to `.mz` (§5); traps name their module file; the NDJSON shape for runtime faults stays with `mz run --agent`, which is not in M2                |
| F40 one binary, cross-compiled                    | §4.3       | deferred to P11                                                                                                                                                                             |
| D7 the word for a module                          | §1.2       | answered: `import`, pending Q2                                                                                                                                                              |
| R2 escape hatches                                 | §6.1, §6.3 | kept: `unsafe` forbidden through the crate root; the only unchecked code is a companion file, named as the one exception                                                                    |
| R8 user macros                                    | §8         | kept: no macro form in Tier 2                                                                                                                                                               |

**Where the survey's sources disagreed, and what this RFC picked:**

- **The prelude** (F27 against F28's "versioned, closed list"): methods are the prelude, module functions are imported (§2.2).
- **Crate signatures** (F37 against the instability of rustdoc JSON): a companion and `rustc`'s check (§6.3).
- **Deny unknown fields** (§4.2's question): yes (§2.4).
