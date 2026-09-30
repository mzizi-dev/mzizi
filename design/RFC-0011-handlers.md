# RFC-0011 — Handlers: the backend measurement slice

**Status:** draft for review. This RFC is a design only. Later pull requests implement it, and
each one updates §12 with what landed. Nothing here has been measured, and no benchmark
episode has run on it.
**Author:** the machine author (Claude)
**Scope:** the smallest language surface that lets a Mzizi program answer the HTTP probes of
RFC-0009's backend tasks (§2.3–§2.4). That covers the declaration (RFC-0007 G2.2), the boundary
data (G2.4), the error model for handlers (G2.5), handler contracts (G2.11, RFC-0010), the
handler body (the part of G1.2 that handlers need), and the lowering to Rust and axum (G2.1).
It is RFC-0009 §6.4's slice: author, check, lower and run locally. It covers no deployment, no
Cloudflare, no Workers target and no port of a live service. Rendered UI facts (RFC-0009 §2.2)
are the other half of §6.4, and are not designed here.

> **Amends** RFC-0010 §3.2 and §3.3. Their declaration lines were placeholders for G2.2, and a
> `route` is now an inner block of a `service` (§1). Top-level `route` files wait for modules
> (G2.3). RFC-0010 carries a note pointing here.

---

## 0. Method: the failure modes a handler language invites

RFC-0001 §0's rule applies: a decision must trace to a named failure mode. These use an `HD-`
prefix (handler). `FM-` belongs to RFC-0006 and RFC-0010, `TY-` to RFC-0008 and `BM-` to
RFC-0009. None of these failure modes has been observed in a Mzizi program, because no Mzizi
program has served a request. They are predicted from the incumbents and from the gateway.

| ID       | Failure mode                                                                                                                                                                                                                                                                                                                                            |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **HD-1** | **The Allow header nobody updates.** A hand-written `OPTIONS` handler states the route's method list a second time, as a string. Adding a `POST` leaves `Allow: GET, HEAD, OPTIONS` stale. That is FM-11, parallel truth, at the HTTP boundary. The gateway writes its method handling by hand (`src/http.ts` at `2468b2a`, as RFC-0010 §3.2 cites it). |
| **HD-2** | **The branch that falls off the end.** A handler path that returns nothing becomes whatever the framework does by default: an empty `200` in some, a `500` in others, a hang in a few. The author did not choose the status, and nothing flags it.                                                                                                      |
| **HD-3** | **Validation as a crash** (RFC-0010 FM-16). Unparseable input becomes a panic or an exception, and the client gets a `500` and a stack trace instead of the `400` the spec asked for. B5 exists to measure this.                                                                                                                                        |
| **HD-4** | **The name's lie at the boundary** (RFC-0010 FM-15). A query parser that trusts a language's permissive number parsing reads `"0x10"` as 16 and `"1e2"` as 100 (JavaScript's `Number()`, which the gateway's `positiveInt` uses). The parse rule is behaviour, so it has to be stated once and checked.                                                 |
| **HD-5** | **Two routers.** The contract evaluator and the lowered server each implement routing. If they disagree, `mz contract` passes a program the server gets wrong. The fix is to give both one algorithm, stated once (§6), and to cross-check the lowering against the same clauses (§8.3).                                                                |
| **HD-6** | **The off-site redirect.** A path such as `//evil.com/`, normalised carelessly, becomes a `Location` that leaves the site. RFC-0009 B5 names the gateway's behaviour.                                                                                                                                                                                   |

## 1. The declaration: `service` — _G2.2, D3_

A backend program is one file holding one `service`. It closes with `end service <name>`, as
a component closes with `end component <name>` (RFC-0001 §1.1).

```mz
## B1's registry: routing and methods, over two components.
service registry

  header "x-mzizi-source" "fixture"

  record problem
    field error: text
  end

  record item
    field name: text
  end

  route ui_item
    get "/v1/ui/{name}"
    when name in "badge" "button"
      respond 200 json item name name
    end
    header "cache-control" "private, no-cache, no-store, max-age=0, must-revalidate"
    respond 404 json problem error "Not found"
  end

  fallback
    respond 404 json problem error "Not found"
  end

  contract
    example get "/v1/ui/button" status is 200
    example get "/v1/ui/button" body.name is "button"
    example delete "/v1/ui/button" status is 405
    example options "/v1/ui/button" header "allow" is "GET, HEAD, OPTIONS"
    ensure header "x-mzizi-source" is "fixture"
  end

end service registry
```

Decisions, and why:

- **A service holds its routes, and a route is an inner block.** RFC-0010 §3.2 drew `route` as a
  top-level declaration, one per file (D3). A backend task has several routes and a fallback
  that must be answered together. With one route per file, one program is several files, and
  that needs modules and a manifest (G2.3), which do not exist and which D7 has not named. A
  service is one top-level declaration, so D3 holds and G2.3 is not pulled into Phase 0. When
  modules land, a route may move to its own file without changing its body.
- **No new keywords.** `service`, `route`, `fallback`, `header`, `respond`, `query`, `json`,
  `file`, and the method words are ordinary identifiers. Each is special only at the start of
  a line in a position where nothing else can appear, as `record` already is (RFC-0008 §2).
  RFC-0002 §1 asks for a small keyword vocabulary, and `lex.rs`'s `KEYWORDS` list stays at 23.
- **Canonical order inside a service:** doc lines, `use`, service `header` lines, `enum` and
  `record`, `route`s, `fallback`, `contract`. RFC-0001 §3's order for components, extended.
- **Records and enums are the same declarations as in a component** (RFC-0008 §2). The record
  that shapes a response body is the same kind of record a component takes as a prop. That is
  G2.4's point: one record type on both sides of the boundary, once modules let a component
  and a service share it.

## 2. Routes — _G2.2, HD-1_

```text
route <name>
  <method> "<pattern>"          exactly one, first
  query <name>: option(<scalar>) zero or more
  <statements>                  §3
end
```

- **One method and one pattern per route.** The methods are `get`, `head`, `post`, `put`,
  `patch`, `delete` and `options`. A second method on the same path is a second route. One
  handler answers one method, so a handler body never branches on its own method (FM-1).
- **A pattern** is `/`, or `/` followed by segments separated by `/`. A segment is a literal
  of lower-case letters, digits and `-._~`, or a whole-segment parameter `{name}`. A pattern
  has no trailing slash, no empty segment and no query string. The trailing slash is the
  runtime's business (§6), and a pattern that spells one could never match. A parameter binds a
  `text` of that name for the handler. The pattern is the parameter's only declaration, so
  it is written once. A partial-segment capture (`nyuchi-{rest}`) is not a form, because axum
  cannot route it and a text operation (G1.3) is the honest way to write B3's prefix rewrite.
- **Matching** is segment by segment. A literal segment beats a parameter at the same position,
  which is axum's rule too. Two routes with the same method and patterns equal up to
  parameter names are an error.
- **Query parameters** are declared, typed and always optional:
  `query limit: option(int)`. A query string is input a client may leave out or get wrong, so
  it can always be absent. A non-option query type is an error whose `exact` fix wraps it in
  `option(…)`. The decoding rule is §4.2.

## 3. The handler body: three statements — _G1.2 (handlers only), HD-2_

| Statement                    | Means                                                            |
| ---------------------------- | ---------------------------------------------------------------- |
| `when <cond> … [else …] end` | RFC-0001 §1.2's one conditional; RFC-0008 §4's narrowing applies |
| `header "<name>" "<value>"`  | sets a header on the response this path produces                 |
| `respond <status> [<body>]`  | produces the response, and ends the path                         |

There is no assignment, no loop, no call and no early `return` other than `respond`. That is
the part of G1.2 that B1 and B2 need, and nothing more. Expressions (G1.3) are not decided
here. A condition is one of RFC-0008 §4's forms, extended with the predicate words the
contract grammar already has, so handler conditions and contract clauses share one
vocabulary:

| Condition                            | Legal when `p` is                                  |
| ------------------------------------ | -------------------------------------------------- |
| `when p`, `when not p`               | `bool`                                             |
| `when p is <literal>`                | `text`, `int`                                      |
| `when p in <literal> <literal> …`    | `text`, `int`                                      |
| `when p at_least <n>`, `at_most <n>` | `int`                                              |
| `when p is none` … `else`            | an option; narrows `p` in the `else` (RFC-0008 §4) |

**Every path responds, exactly once.** The checker walks the body. A path that can reach the
route's `end` without a `respond` is an error that names the line where the path ends
(HD-2). A statement after a `respond` on the same path can never run, and is an error whose
`exact` fix deletes it. A `when` with no `else` falls through when its condition is false, so
the statements after it still run. There is no implicit status.

**Headers** are lower-case names. An upper-case name is an error with an `exact` fix to its
lower-case form. HTTP names are case-insensitive, so one spelling removes a choice point
(FM-8). A value is a string, and may interpolate a parameter as `{name}`. A `header` line at
service level applies to every response the service produces, runtime responses (§6)
included, and a route's own `header` for the same name wins. A header set twice on one path
is an error. `content-length` and `transfer-encoding` belong to the server and cannot be set.

**Bodies:**

| Form                          | Body                                   | `content-type` unless set   |
| ----------------------------- | -------------------------------------- | --------------------------- |
| _(none)_                      | empty                                  | none                        |
| `json <record> <field> <v> …` | the record, as JSON (§4.1)             | `application/json`          |
| `text "<string>"`             | the string, which may interpolate      | `text/plain; charset=utf-8` |
| `file "<path>"`               | a fixture file, embedded at build time | from the extension          |

A status is 100–599. `204`, `304` and any `1xx` take no body, and a body there is an error
whose `exact` fix deletes it.

`json <record> <field> <value> …` is a record literal. It is written the way an enum row is
(RFC-0001 §1.3): the name, then `field value` pairs, with no punctuation. Every field must
be given exactly once, with a value of its type. A value is a literal, or a parameter in
scope. A record-typed or list-typed field cannot be built literally in this slice, so a
response record's fields are scalars or options of scalars. That restriction is recorded in
§10 rather than hidden.

`file "<path>"` is resolved against the `.mz` file's directory, and a missing file is an
error in `mz check`. The fixture is the same file the task's probes and references use
(RFC-0009 §2.3, RFC-0010 C-5), so a response body is never a second hand copy of it.

## 4. Boundary data — _G2.4, HD-3, HD-4_

### 4.1 Out: JSON by construction

A record serialises as one JSON object, its fields in declaration order: `text` as a string,
`int` as a number, `bool` as `true` or `false`, an option's `none` as `null`, and an enum as
its variant's name (RFC-0008 §1's JSON column). Field order is fixed because RFC-0008 §2 made
order part of a record's meaning. The serialiser is generated, and cannot fail.

### 4.2 In: the query decoding rule

A query string is decoded as `application/x-www-form-urlencoded`. It is split on `&` and then
on the first `=`, `+` becomes a space, and `%XX` is percent-decoded (a malformed escape is kept
as written). The first occurrence of a name wins. Then, by the declared type:

| Declared       | `some` when the value is                          | otherwise |
| -------------- | ------------------------------------------------- | --------- |
| `option(text)` | present, including the empty string               | `none`    |
| `option(int)`  | `-`? then one or more ASCII digits, fitting `int` | `none`    |
| `option(bool)` | exactly `true` or `false`                         | `none`    |

That is the whole rule, and every arm's spec can be written against it. `"0x10"`, `"1e2"`,
`" 7 "` and `"1.5"` are all `none` (HD-4). A spec that wants the gateway's `Number()`
behaviour instead is asking for a different rule, and RFC-0009 §2.3 says the spec, not the
scorer, settles that. Bad input is never an error: it is absence, and absence is typed, so
the handler must say what it does with it (C-3).

### 4.3 In: request bodies (B4, B5), designed and not in this slice

A route that takes a body declares it once, `body input: <record>`, beside its `query`
lines. The runtime parses the body into the record before the handler runs. Each way that
can fail is a **declared response**, not a panic (C-3, HD-3). The service declares one block
per failure kind it can produce, and a route that takes a body in a service without the
matching block is an error:

```mz
  reject malformed
    respond 400 json problem error "Malformed JSON"
  end
  reject unsupported
    respond 415 json problem error "Expected application/json"
  end
  reject too_large
    respond 413 json problem error "Payload too large"
  end
```

It is recorded here so that B5's shape is fixed before its task is written. It is built after
B1 and B2 run end to end (§12).

## 5. The error model for handlers — _G2.5, D5_

G2.5 asks for `result(T, E)`, one propagation form, no exceptions, and no panic reachable
from surface code. For handlers in this slice:

- **A handler is total.** It has no call and no fallible operation, and every path responds
  (§3). There is nothing to propagate, so there is no `result` in the slice, and no
  propagation form is needed yet.
- **Boundary failure is a typed value** (§4.2), or, for bodies, a declared response (§4.3).
- **The generated code has no `unwrap`, `expect`, indexing or `panic!` on a request path.**
  The lowering (§8) is reviewed against that, and its tests exercise every example. The only
  fallible calls in the generated code are the server's own startup (binding the port),
  which is not a request path.
- **`result(T, E)` and its propagation form** belong to functions with bodies (G1.2 beyond
  handlers). They are designed with the first `fn` that can fail, not here. D5 stands:
  errors are values, and nothing here adds an exception.

## 6. What the runtime owns — _HD-1, HD-5, HD-6_

The runtime answers what HTTP defines, so no author writes it twice. The algorithm, in order,
for a request `M P?Q`:

1. **Canonical path.** If `P` has an empty segment (`//`) or a trailing slash (and is not
   `/`), respond `308` with `location` set to `P` with the empty segments removed and the
   trailing slash dropped, followed by `?Q` if there was a query. The result always starts
   with exactly one `/`, so `//evil.com/` goes to `/evil.com` and never off-site (HD-6).
   The gateway takes two hops there (RFC-0009 B5: `//evil.com/` → `/evil.com/`), and B5's spec
   decides which the task wants.
2. **Match.** Find the routes whose pattern matches `P` (§2). If none does, run the
   `fallback` block, or respond `404` with no body when there is none.
3. **Method.** Among the routes whose pattern best matches `P`, run the one for `M`. If there
   is none:
   - `HEAD`, with a `get` route: run it, and send its status and headers with no body.
   - `OPTIONS`: respond `204` with `allow` set to the declared methods, plus `HEAD` if there
     is a `get` and `OPTIONS` itself, in the fixed order `GET, HEAD, POST, PUT, PATCH, DELETE,
OPTIONS`. For B1's routes that is `GET, HEAD, OPTIONS`, the gateway's value.
   - Anything else: respond `405` with the same `allow` header and no body.
4. **Headers.** Add the service's `header` lines to whatever was produced, unless the route
   set the same name.

An author may still declare an `options` or `head` route, and it wins, as any declared
route does. A declared `options` route states the method list by hand, which is HD-1 again,
so it is legal but never needed.

This is one algorithm. The contract evaluator runs it (§7), and the lowering emits it as one
function (§8). It is the only place routing semantics are written.

## 7. Contracts on a service — _G2.11, RFC-0010_

A service's `contract` block is RFC-0010 §3.2's, with the declaration questions settled:

```text
example <method> "<target>" <facet> <predicate>
ensure [when <facet> <predicate> then] <facet> <predicate>
facet     := status | header "<name>" | body | body.<field>[.<field>…]
predicate := is <value> | in <value> … | not_empty | contains "<text>"
           | at_least <n> | at_most <n>
```

- `<target>` is a path, with an optional `?query`. It is sent as written, so an example may
  exercise the canonical-path rule (`get "/v1/ui/"`).
- `header "<name>" is none` asserts the header is absent. `body is ""` asserts an empty body.
  `body.<field>` reads the JSON body at that path, and is unevaluable (`MZ0605`) when the
  body is not JSON. `is none` there means JSON `null`.
- **`mz contract` evaluates a service by running it.** The handler language (§3) has no loop,
  no call and no state, so the evaluator executes §6's algorithm and the handler bodies
  exactly, in process, with no lowering and no socket. Every `example` is one run.
- **Every `ensure` is checked over a generated request set.** The set is deterministic and
  enumerated, not random: each route's pattern with its parameters filled from the literals
  the service compares them against, plus one value that matches none; all seven methods on
  each; each path with a trailing slash and with a doubled slash; `/` and one unknown path;
  and for each query parameter, absent, each literal it is compared with, `abc`, `-1` and
  `0`, varied one at a time. The count is reported as `contract_tested`, the key RFC-0010 §6
  added to the summary. By C-4 the report says _tested (n requests)_, never _proven_.
- **Codes** (RFC-0010 §6): a failed `example` is `MZ0612` and a failed `ensure` is `MZ0611`,
  whose `say` quotes the request that broke it. An `example` whose status contradicts an
  `ensure status in …` on literals is `MZ0606`, reported by `mz check`. A service with no
  evaluated clause is `MZ0613`, a warning, as RFC-0010 §8 sets the default. `MZ0607`
  ("not yet testable") never applies to a service, because a service always runs.

## 8. Lowering to Rust and axum — _G2.1_

### 8.1 `mz build`

`mz build <file.mz> --out <dir>` checks the file, and on zero errors writes a Cargo package
to `<dir>`: `Cargo.toml`, `src/main.rs` and the fixtures the service embeds. The package is
its own workspace root, so it never joins this repository's workspace. It is generated
output, and is never committed (RFC-0010 §5, CONTRIBUTING.md). `cargo run --release` in it
serves `127.0.0.1:$PORT` (8080 by default) and prints one `listening on …` line when it is
ready.

### 8.2 What it emits

- **axum is the server, not the router.** The generated `Router` has one fallback service,
  which is §6's algorithm as one Rust function over axum's `Request` and `Response`. axum's
  own router has different defaults: it does not answer `OPTIONS`, and it does not redirect
  a trailing slash. Using it would mean a second routing semantics beside the evaluator's,
  which is HD-5. axum still provides the server, the request and response types, and the tower
  integration the tests use.
- Each route is one plain `fn` from the decoded parameters to a response. Each record is a
  struct with a generated `to_json`. Fixtures are embedded with `include_bytes!`.
- **Dependencies of the generated package, pinned exactly:** `axum` and `tokio`, and, for the
  tests only, `tower` (for `ServiceExt::oneshot`) and `http-body-util`. The compiler itself
  still has no dependencies (AGENTS.md). The generated code needs no `serde`, because the
  serialiser is generated.

### 8.3 Generated tests: the cross-check — _HD-5, C-5_

Every `example` becomes one `#[tokio::test]` in a `#[cfg(test)]` module of `src/main.rs`,
which sends its request to the generated `Router` with `oneshot`, in process, with no
socket (RFC-0010 §4.2). The tests are generated from the same clauses `mz contract`
evaluates, so the evaluator and the lowering are checked against one list. `ensure` clauses
are not lowered into property tests in this slice. They are evaluated by `mz contract` only,
and that is recorded as a gap (§10).

## 9. Diagnostic codes

The hundreds digit keeps its meaning. **`MZ08xx` is new: services and handlers.**

| Code     | Tool       | Means                                                                                                                                            |
| -------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `MZ0801` | `mz check` | a `route` whose first line is not `<method> "<pattern>"`, or a second method line; unknown method words get a nearest-name fix                   |
| `MZ0802` | `mz check` | a malformed pattern: no leading `/`, a trailing slash, an empty segment, a query, a bad parameter; `exact` fixes where the repair is certain     |
| `MZ0803` | `mz check` | two routes with the same method and the same pattern up to parameter names                                                                       |
| `MZ0804` | `mz check` | a path through a handler that reaches `end` without a `respond`                                                                                  |
| `MZ0805` | `mz check` | a statement after `respond` that can never run; `exact` fix deletes it                                                                           |
| `MZ0806` | `mz check` | a malformed `respond`: no status, a status outside 100–599, an unknown body form, or a body on a status that takes none (`exact` fix deletes it) |
| `MZ0807` | `mz check` | a malformed `header`: not lower-case (`exact` fix), set twice on one path, or a name the server owns                                             |
| `MZ0808` | `mz check` | a record literal with an unknown, missing, repeated or ill-typed field, or a field that cannot be built literally                                |
| `MZ0809` | `mz check` | a `query` parameter that is not `option(<scalar>)`; `exact` fix wraps it                                                                         |
| `MZ0810` | `mz check` | a `file` body whose fixture does not exist                                                                                                       |
| `MZ0811` | `mz check` | a line a service or handler cannot hold, e.g. a `view` in a service                                                                              |
| `MZ0812` | `mz check` | a second `fallback`, or a name used twice among routes and parameters                                                                            |

Unknown names, types and fields reuse RFC-0008's codes (`MZ0701`, `MZ0707`, `MZ0708`), and an
un-narrowed option reuses `MZ0710`, so an agent sees one code per kind of mistake whatever
declaration it is in. Contract codes are §7's.

## 10. What the backend tasks need, and what this slice builds

| Task | Needs                                                                                                           | In this slice                                                                                                                                      |
| ---- | --------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| B1   | routes, methods, `OPTIONS`/`405`/`308`, a JSON `404`, headers                                                   | **all of it**                                                                                                                                      |
| B2   | typed query parameters, the `0x10` rule, a lower bound on an `int`                                              | **all of it**: §4.2 is the rule, and `when p at_least 0` is the bound                                                                              |
| B3   | B1 and B2, fixture-backed listings with filters and `meta`, `nyuchi-*` → `mzizi-*` rewrites, `410`, `503`, CORS | headers, `410`, `503` and fixture bodies. **Not** the filtered listing or the rewrite, which need list data in handlers and text operations (G1.3) |
| B4   | state: record a version, list them, `409` on a duplicate                                                        | **no.** It needs state (G1.1), which three RFCs have deferred, and which is a one-way door for components too                                      |
| B5   | request bodies and declared rejections, `500`, slash collapsing                                                 | slash collapsing (§6). Bodies and rejections are designed (§4.3) and not built                                                                     |

The limits, stated plainly:

- A response record's fields are scalars or options of scalars (§3).
- `ensure` clauses are tested by the evaluator only, not by generated property tests in the
  lowered crate (§8.3).
- A `HEAD` response comes from the `GET` handler with the body removed. The server may send a
  `content-length` of 0 where RFC 9110 prefers the `GET`'s length. No probe reads it.

## 11. What this RFC does not claim

- That any of it is built. §12 records what lands, pull request by pull request.
- That Mzizi handlers are easier to write than Hono, FastAPI, `net/http` or axum handlers.
  That is what the `backend` family measures (RFC-0009 §6), and it has not run. Runtime-owned
  method handling (§6) removes lines an incumbent author writes by hand, and the Rust arm's
  axum does part of it too. Whether that helps a model is the measurement.
- That the evaluator and the lowering agree beyond the examples they are both checked on.

## 12. Implementation record

Updated by each pull request that implements part of this RFC.

1. **The front end** (§1–§4.2, §9, and `mz check`'s part of §7). `compiler/src/service.rs`
   holds the tree and the checker, and `compiler/src/parse/service.rs` the parser. Every
   `MZ08xx` code in §9 is emitted and has a test that triggers it (`compiler/tests/services.rs`,
   52 tests). A service's records and enums are checked by the same RFC-0008 resolver as a
   component's, through a synthetic component, so there is one set of type rules. Not built
   yet: request bodies (§4.3), as planned. `mz contract` on a service reports every clause as
   `MZ0607`, "not yet testable", because nothing runs a service yet. `mz outline`, `mz hash` and
   `mz ir` refuse a service with exit status 2, because services have no IR yet (§13.4).

## 13. Open questions

1. **State (B4).** A service-level store is the natural home for B4's version history, and
   it would decide G1.1's semantics for handlers first. Whether handler state and component
   state are one construct is the owner's call before B4 is built.
2. **Text operations (B3).** A prefix test and a prefix rewrite are the smallest G1.3 subset
   B3 needs. They should be chosen with G1.3, not ahead of it.
3. **Top-level routes.** Once G2.3 exists, whether a route may leave its service for its own
   file, and whether the service then becomes a manifest entry.
4. **Property tests in the lowered crate** for `ensure` clauses, seeded from the IR hash
   (RFC-0010 §4.2), once services have an IR.
