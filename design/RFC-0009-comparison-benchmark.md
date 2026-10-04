# RFC-0009 — The comparison benchmark: every language arm, every task family, and a gate against the best incumbent

**Status:** draft for review — nothing here is implemented. Two parts are owner decisions of
2026-09-29, recorded here as decided rather than proposed: the kill criterion (§6) and the
publication rule (§7). Everything else is a proposal.
**Author:** the machine author (Claude)
**Scope:** which language arms the Phase 0 benchmark compares Mzizi against, the task families
they are compared on, how each arm's compile-or-check step works and what it hands the agent,
the fairness rules, the model arms, the kill criterion, publication, the public external suites,
and the order to build it all in. It defers the Mzizi backend language features to RFC-0007's
Tier 2 and to RFC-0010 (contracts), the held-out task set itself to RFC-0004's trigger, and
runtime-performance numbers to §8.3, which names the suites but measures nothing.

> **Amends** RFC-0001 §6 (the baselines and the pass rule), CHARTER.md §4 and §6 (the gate and
> the task set), and RFC-0004 (§4.2, what a held-out run publishes and when). Each carries a
> note pointing here.

---

## 0. Method: how a cross-language comparison goes wrong

RFC-0001 §0's rule applies to the benchmark too: a design decision that does not trace to a
named failure mode does not belong. The failure modes below use a `BM-` prefix, because they
are failures of a _measurement_, not of the language (RFC-0003's `RB-` and RFC-0008's `TY-`
set the precedent). Where one was observed, the evidence is cited; where it is predicted, it
says so.

| ID       | Failure mode                                                                                                                                                                                                                                                                                                                                                            |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **BM-1** | **Home-field input.** The task is handed over in one arm's own language. Every UI task today gives the agent `spec.tsx`, a React component (`benchmarks/runner/src/prompt.rs`: "Port this React component to …"). For a React arm that is a copy task, and it would measure transcription. _Predicted:_ no React arm has run.                                           |
| **BM-2** | **Unequal "clean".** Each arm's check proves something different, so "iterations to a clean compile" compares unlike things. It also covers the check's _output_: in pilot 2, `mz check` printed absolute paths and `check.sh` did not, which cost the 7B Mzizi arm about 13,500 tokens (`results/2026-09-27-pilot-2/RUN.md`). _Observed._                              |
| **BM-3** | **Source-shaped scoring.** A fact read out of source syntax can only be scored in the languages the extractor parses, and a parser gap turns into a defect. The first harness reported five false defects against the real `button.rs`, and the changelog task charged both arms for its reference's own renaming (`benchmarks/README.md`, RFC-0006 §10.1). _Observed._ |
| **BM-4** | **The pooled headline.** One number averaged over UI and backend tasks lets a win in one family hide a loss in the other. _Predicted:_ only one family has ever run.                                                                                                                                                                                                    |
| **BM-5** | **Moving goalposts.** The pass rule, the scorer or the task set changes after the data is seen, in whichever direction flatters. The scorer did change after pilot 1 (rename pairing), which is why §7 makes every change dated and registered before the run it affects. _Observed risk, handled._                                                                     |
| **BM-6** | **The unpublished run.** Only the runs that came out well get written up. Publication bias, the failure that makes a literature look better than its experiments. _Predicted, and ruled out by §7._                                                                                                                                                                     |

## 1. The arms — _BM-1, BM-2_

An **arm** is one language with one framework, a pinned toolchain, a sandbox, a checker, a
guide and a scorer extractor. Today three exist: `mzizi`, `dioxus` and `leptos`
(`benchmarks/arms/`, `benchmarks/runner/src/prompt.rs`). This RFC adds seven.

| Arm        | Family  | Language and framework                              | Pin comes from                                            | Status                          |
| ---------- | ------- | --------------------------------------------------- | --------------------------------------------------------- | ------------------------------- |
| `mzizi`    | UI      | Mzizi                                               | this repo                                                 | exists                          |
| `dioxus`   | UI      | Rust, Dioxus `=0.7.10`                              | `mzizi-registry`'s lockfile (`arms/dioxus/README.md`)     | exists                          |
| `leptos`   | UI      | Rust, Leptos `=0.8.21`                              | `arms/leptos/README.md` (never ported in the registry)    | exists                          |
| `react`    | UI      | TypeScript, React, `class-variance-authority`       | `mzizi-registry`'s lockfile at the task commit            | new                             |
| `mzizi-be` | backend | Mzizi                                               | this repo                                                 | exists, unscored (RFC-0011 §12) |
| `ts`       | backend | TypeScript, Hono on Node                            | `mzizi-api-gateway`'s lockfile at `2468b2a`               | new                             |
| `python`   | backend | Python, FastAPI with pydantic v2, served by uvicorn | exact pins, chosen when the arm lands, lockfile committed | new                             |
| `go`       | backend | Go, `net/http` from the standard library            | exact Go toolchain version                                | new                             |
| `cpp`      | backend | C++20, `cpp-httplib` + `nlohmann/json` (both MIT)   | vendored single headers, recorded in a NOTICE             | new                             |
| `rust`     | backend | Rust, axum on tokio, serde                          | exact pins, lockfile committed                            | new                             |

Decisions, and why:

- **Pin from Mzizi's own stack where one exists.** The Dioxus arm set this precedent: its
  sandbox reproduces what the registry's Rust references compile against, because an arm that
  fails on version drift flatters Mzizi, which is the dangerous direction for a kill criterion.
  The React arm pins from the registry's lockfile, and the TypeScript backend arm from the API
  gateway's, for the same reason.
- **Hono, not a bare Workers handler, for TypeScript.** `api.mzizi.dev` is a Hono Worker today,
  so Hono is the incumbent Mzizi would replace. Hono runs on both Node and workerd, so the probe
  (§2.3) runs every backend arm as an ordinary local HTTP server, and Node is the host.
  Exercising the Workers runtime itself is §11's question 3.
- **axum, not `workers-rs`, for plain Rust.** axum is what the charter's Containers target lowers
  to (CHARTER.md §4, Phase 1), and it runs as a local server with no wasm32 or workerd step per
  episode. This is an authoring benchmark, so the runtime is not what is being compared.
- **`cpp-httplib` for C++.** C++ has no standard HTTP library. A single vendored header keeps the
  sandbox to one compiler and no package manager.
- **Arms become data.** Each arm gets a `benchmarks/arms/<id>/arm.toml` (language name, file
  kind, extension, guide, check argv, extractor, family), so the runner's hard-coded `Arm` enum
  and its per-arm `match` arms are replaced by one table. The parity test that proves user
  messages differ only in naming then runs over every pair of arms.
- **The two frontend paths are not arms.** The owner's direction (CHARTER.md v0.3) is Astro with
  Mzizi Roots underneath, or pure Rust end to end. Both are authored in Mzizi; they differ in the
  host and the lowering target (RFC-0007 G2.13), which an authoring benchmark cannot see. They
  matter for runtime (§8.3), not here.

## 2. Task families — _BM-1, BM-3, BM-4_

A **family** is a kind of task with its own arms, its own facts and its own result. Families are
never pooled.

| Family        | Input the agent sees        | Arms                                            | Role                                       |
| ------------- | --------------------------- | ----------------------------------------------- | ------------------------------------------ |
| `ui-spec`     | `spec.md`, language-neutral | `mzizi`, `dioxus`, `leptos`, `react`            | **gating** (§6), on held-out tasks         |
| `ui-port`     | `spec.tsx`, as today        | `mzizi`, `dioxus`, `leptos`                     | development and comparison with the pilots |
| `backend`     | `spec.md`, language-neutral | `mzizi-be`, `ts`, `python`, `go`, `cpp`, `rust` | **gating** (§6), on held-out tasks         |
| public suites | the suite's own prompt      | per suite (§8)                                  | comparability with the field; never gating |

### 2.1 A language-neutral spec, for every family that has a TypeScript arm

`ui-spec` exists because of BM-1. Its tasks are the registry components the pilots used,
restated as `spec.md`: prose, plus a table of each variant group's variants and classes, the
default of each, the `data-slot`, and the rendered elements. The `.tsx` is the source of the
restatement and is never shown. The Rust arms' results on `ui-spec` are therefore not
comparable with the pilots, which ran `ui-port`. `ui-port` stays, for Rust arms only, so the
next run can still be compared with the last one.

### 2.2 UI facts are still source-level, and that constrains idiom in every arm

The scorer reads variant sets, defaults and `h-N`/`size-N` heights out of source
(`benchmarks/harness/src/lib.rs`). A React extractor must read the same facts out of a `cva(…)`
call, so the React guide tells the agent to use `cva`, as the registry's own `.tsx` does, just
as the Rust guides require a `classes()` method and the Mzizi guide a `class` column. That is
BM-3's residue. Rendered scoring would remove it: server-render each arm and read `data-slot`,
classes and elements from the HTML. React, Dioxus and Leptos can all render on the server
today. Mzizi cannot, because no component lowers (RFC-0007 G2.1), so rendered scoring waits for the
same lowering slice as the backend arm (§6.4).

### 2.3 Backend tasks: a spec, references, and probes

Each backend task directory holds:

```text
benchmarks/tasks/<name>/
  task.toml      metadata, provenance, family = "backend"
  spec.md        the only task text any agent sees
  probes.toml    the scoreable facts
  fixtures/      request bodies and served data, shared by the spec, the probes and the references
  reference-*/   one or more reference implementations, never shown to an agent
```

A **probe** is one HTTP request and the facts expected of its response. Every fact is
observable at the HTTP boundary, so it is independent of the language by construction (BM-3).
The example uses the gateway's real method handling, as its own test asserts
(`mzizi-api-gateway` `test/api.test.ts` at `2468b2a`):

```toml
[[probe]]
id = "options-allow"
clause = "B1.3"                    # the reference contract's clause (RFC-0010 §6)
request = { method = "OPTIONS", path = "/v1/ui" }
expect = { status = 204, headers = { allow = "GET, HEAD, OPTIONS" } }

[[probe]]
id = "delete-405"
clause = "B1.4"
request = { method = "DELETE", path = "/v1/ui/button" }
expect = { status = 405 }
```

Scoring follows the runner's existing rules:

- Every expected status, header and body value is one fact. JSON bodies compare deep-equal, or
  at a JSON pointer, with any normalisation declared in the probe file (the gateway's parity
  script does the same for its health timestamp).
- A defect is a failing fact on a candidate that passed its check. The defect rate is clean
  episodes with at least one defect over clean episodes that were scored, as today.
- A clean candidate that fails to build or start counts as failing every fact, and is flagged
  `start_failed`. This penalises an arm whose checker let through a program that cannot run, and
  it is meant to: that is BM-2 measured, not hidden.
- **Two references per new task, in two languages**, and every probe must pass against both
  before any run (`mzbench task verify`). A probe only one reference passes means the spec is
  ambiguous, and it is fixed in the spec, not in the scorer. The gateway shows why: its
  `positiveInt` parses with JavaScript's `Number()`, so it reads `"0x10"` as 16 and `"1e2"` as
  100 (checked with Node against `src/routes/registry.ts`). A Rust or Go port will reject both.
  Only the spec can say which behaviour the task wants.

### 2.4 The backend tasks

Each is grounded in the API gateway, the one real backend Mzizi operates, at `2468b2a`.

| Task | What it exercises                       | Grounding                                                                                                                                                                                                                                                                                        |
| ---- | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| B1   | HTTP handlers: routing and methods      | `OPTIONS` → `204` with `Allow: GET, HEAD, OPTIONS`; other methods → empty `405`; a trailing slash → `308`; an unknown path → a JSON `404`                                                                                                                                                        |
| B2   | Data validation                         | `/v1/ui`'s `node`, `limit` and `offset` query parameters through `positiveInt` (`limit=abc` is a `200` with no limit); the task's spec decides the `0x10` question above                                                                                                                         |
| B3   | A Worker endpoint mirroring real routes | `/v1/health`, `/v1/ui` with filters and `meta`, `/v1/ui/{name}`, the `nyuchi-*` → `mzizi-*` `308` keeping sub-path and query, `410` retired routes, `503 {"error":"Database not configured"}`, CORS, cache and `X-Mzizi-Source` headers, over a fixture subset of `registry.json`                |
| B4   | A small service with state              | component version history, the one dataset the gateway answers `503` for because it lives in a database; an in-memory service: record a version, list them, `409` on a duplicate, `404` for an unknown component                                                                                 |
| B5   | Error paths                             | malformed JSON `400`, wrong content type `415`, an oversized body `413`, an internal failure → `500 {"error":"Internal server error"}` (the gateway's `internalError`), no stack trace in any body; repeated slashes collapse instead of redirecting off-site (`//evil.com/` → `308 /evil.com/`) |

B1–B3 are derived from a public repository, so they are **development tasks**, contaminated by
construction and never gating. The gating backend set is written in the same shape and held out
(RFC-0004), and so is the gating `ui-spec` set.

## 3. Checkers: what "compile or check" means per arm — _BM-2_

**The rule:** each arm runs the check that an ordinary project in that language runs in CI by
default, at the ecosystem's default strictness. Errors block and warnings print. The agent
receives the tool's own human output with exactly two normalisations: paths are shown relative
to the candidate's file name, and progress and timing lines are removed. Nothing is reformatted,
summarised or truncated, because doing so would be designing the incumbents' diagnostics for
them. Every checker keeps `check.sh`'s exit contract: 0 clean, 1 errors, 2 setup error.

| Arm                        | Check                                                                         | Blocks on                     | What the agent sees (shape)                       |
| -------------------------- | ----------------------------------------------------------------------------- | ----------------------------- | ------------------------------------------------- |
| `mzizi`, `mzizi-be`        | `mz check --agent`                                                            | errors                        | NDJSON, one diagnostic per line, plus the summary |
| `dioxus`, `leptos`, `rust` | `cargo check --locked --message-format=json`, each message's `rendered` field | errors                        | rustc's text, `help:` and `note:` lines included  |
| `react`, `ts`              | `tsc --noEmit --pretty false` with `strict: true`                             | errors                        | `file.tsx(12,5): error TS2322: …`                 |
| `python`                   | `pyright` in its default `standard` mode                                      | errors                        | `app.py:3:12 - error: … (reportX)`, plus a count  |
| `go`                       | `go build ./...`, then `go vet ./...`                                         | build errors and vet findings | `./main.go:12:2: undefined: foo`                  |
| `cpp`                      | `clang++ -std=c++20 -fsyntax-only -Wall -Wextra -fno-color-diagnostics`       | errors                        | `main.cpp:12:5: error: …`, with the caret lines   |

The shapes are the tools' documented defaults. Each arm's README pins the exact output from a
real run, as `arms/dioxus/README.md` already does.

- **pyright, not mypy.** By default mypy does not check the bodies of unannotated functions, so a
  clean mypy run proves less than a clean pyright run. The weaker checker would move Python's
  errors out of the iterations metric and into defects, BM-2 again.
- **`go vet` blocks** because `go test` runs a subset of vet by default, so Go projects treat vet
  findings as CI failures.
- **`-fsyntax-only` for C++** is the analogue of `cargo check`: no codegen, no link. Link errors
  surface when the candidate is built for probing, as `start_failed`.
- **The Mzizi arm gets no special treatment.** Pilot 2's absolute-path bug is fixed by the rule
  above (paths relative to the candidate), whatever the pre-kill-criterion work does first.

What stays unequal, on purpose: a clean pyright run and a clean borrow-checked `cargo check` are
different promises. That is a real difference between the languages. It moves cost between the
iterations and defect metrics, which is why the gate (§6) reads two metrics out of three and
never iterations alone.

## 4. Fairness rules — _BM-2, BM-5_

1. **One prompt builder** for every arm and both modes. User messages differ only in the language
   name, the file kind and the naming sentence, and a unit test proves it for every pair of arms.
   `spec.md` is included verbatim.
2. **Equal guide budget.** Every guide in a family is written to one token budget, within ±5%,
   counted with the headline model's tokenizer and recorded in `meta.json` beside the existing
   `guide_bytes` and hash. Every guide has the same sections: file shape, what is available,
   naming rules, how to read the checker's output, and one worked example from a corpus component
   that is not a task. This changes the runner's current stance ("a language that needs more
   explanation pays for it" in tokens): under a fixed budget, a language that needs more
   explanation pays in iterations and defects instead. Today's guides are 12,306 bytes (Mzizi),
   10,723 (Leptos) and 10,375 (Dioxus), a 19% spread, so they must be rebalanced before a gating
   run.
3. **One budget.** The same `max_iters` (5 today), per-reply `max_tokens`, context size,
   temperature and seeds. An episode that runs out of context is not clean (pilot 2's rule).
4. **One scorer.** The same extraction rule, the same probes and the same summary maths.
5. **One sandbox policy.** No network, the same CPU, memory and wall-clock limits, pinned
   toolchains, and caches warmed before the first episode.
6. **Mode 2 rules** as in `runner/README.md`, recorded as not enforced by the runner.

## 5. Model arms — RFC-0002 §1

- **The headline is the ~7B open-weight model**, served locally (`benchmarks/openweight/`, Mode 1,
  exact token counts). The gate reads off it and nothing else, because RFC-0002 §1 makes small
  models the design target.
- **The frontier model is reported beside it** (Mode 2, transcript-token proxy) and never pooled.
  It can neither pass nor fail the gate.
- **The same model serves every language arm** within a model arm.
- **Priors.** Every incumbent is in the model's training data and Mzizi is not. That is a handicap
  for Mzizi, and it is also exactly the claim under test: RFC-0002 §1 bets that "a familiarity
  penalty is a one-time cost", and a small model that has never seen Mzizi is where that bet is
  paid.

## 6. The kill criterion — owner decision, 2026-09-29

### 6.1 The decision

> **Mzizi is measured against the best existing language for each kind of task.**

The owner wants Mzizi to stand with the top languages, not be a niche alternative to two Rust UI
frameworks. This replaces RFC-0001 §6's baseline ("the same corpus components authored in raw
Dioxus and Leptos … If Mzizi doesn't beat both on at least two of three metrics, the thesis is
wrong") and the matching sentence in CHARTER.md §4.

### 6.2 How it is scored

This operational definition is fixed here, and is registered in `PLAN.md` (§7) before the first
gating run:

1. **A kind of task is a gating family:** `ui-spec` and `backend`.
2. **Best is per metric.** For each family and each of the three metrics (tokens, iterations to a
   clean check, defect rate), the bar is the best value any incumbent arm in that family reached.
   The best on tokens and the best on defects may be different languages.
3. **A family passes** when Mzizi beats that bar on at least two of the three metrics, on the
   headline model, on the held-out tasks. The two-of-three form is RFC-0001 §6's, kept.
4. **"Beats" means outside the noise:** a paired bootstrap over (task, seed) pairs whose 95%
   interval for the difference excludes zero in Mzizi's favour. The number of tasks and seeds is
   fixed in `PLAN.md`. Pilot 2's lesson is that one episode moves a rate by 17 points at n = 6.
5. **Phase 0 passes only when every gating family passes.** A family with no Mzizi arm has not
   passed.
6. **Downstream work is gated per family.** Phase 1 UI work waits on `ui-spec`. The owner's
   backend goal, Mzizi building Mzizi's own backend (CHARTER.md v0.3), waits on `backend`.

### 6.3 Why this is not moving the goalposts — _BM-5_

The criterion changed after two pilots, neither of which showed an advantage, so this needs
saying. The change goes in the harder direction only. The old rule's arms are a subset of the new
rule's, and "beat the best on each metric" implies "beat Dioxus and Leptos on it". So no result
that fails the old rule can pass the new one. It is registered before any gating run. A change
after data that can only make passing harder cannot flatter.

### 6.4 What the decision costs: a Mzizi backend arm has to exist

_Status, 2026-09-30: the slice below is designed in RFC-0011 and built, and `mzizi-be` exists
with B1 as its first task (RFC-0011 §12). Nothing in the family has run, and the runner does
not yet score an episode with probes._

The `backend` family cannot pass without a `mzizi-be` arm. When this section was written, Mzizi
could not write a handler: there was no handler declaration (RFC-0007 G2.2), no boundary records
(G2.4), no error model (G2.5), no handler contract (G2.11, RFC-0010), and nothing lowered, so
nothing could answer a probe (G2.1). This RFC counts **a measurement slice** of that work as Phase 0: author, check, lower to
axum and run locally, exactly enough for the backend tasks and for rendered UI facts (§2.2). It
covers no deployment, no Cloudflare and no port of any live service. It is the one decision here
that widens Phase 0's scope, and it follows from §6.1: the owner's criterion makes the `backend`
family part of the gate, and the family cannot be measured without the slice.

### 6.5 What to expect, stated before the run

On the only small-model data there is, Mzizi did worse than Dioxus on all three metrics (pilot 2).
Adding TypeScript and Python arms raises the bar, since those are the languages small models have
seen most. A loss under this criterion is the likely outcome on today's compiler, and it is
written down here so that a loss is not a surprise and a win is not over-read.

## 7. Publication — owner rule, 2026-09-29: every run is published — _BM-6_

1. **Where.** `benchmarks/results/<date>-<name>/`, as the two pilots already are.
2. **Registered before it runs.** A `PLAN.md` is committed to that directory before the first
   episode. It records the code commit, the arms and their pins, the guide hashes, the task IDs
   (content hashes for held-out tasks), the models, seeds and budget, the scorer version and, for
   a gating run, §6.2's rule and its n. A registered run with no `RUN.md` is visible as exactly
   that.
3. **Every registered run gets a `RUN.md`**, whichever way it fell. The headline result comes
   first, then the threats to validity, as in pilot 2. An aborted run gets a `RUN.md` saying where
   it stopped and why. Corrections are dated amendments, never rewrites.
4. **What is published:** the raw episodes (every iteration's prompt, reply, candidate,
   diagnostics and feedback, `episode.jsonl`, `meta.json` with the argv that ran, `score.json`),
   the summary tables, the model and arm configuration, the code commit and the caveats. Nothing is
   cherry-picked: an episode is excluded from a denominator only by a rule already in the runner,
   and it is still published.
5. **Held-out runs** publish immediately: `PLAN.md`, `RUN.md`, the aggregates, each episode's
   numeric final line with the task named by its hash, the scorer version, and a SHA-256 of the
   full raw bundle. The task texts, and everything that quotes them (prompts, candidates,
   diagnostics), stay in the private repository until that task set is retired. Then the bundle is
   published into the same directory, and anyone can check it against the hash published on the
   day. RFC-0004 §4.2 records this.
6. **Public suites** publish in full, with the suite's licence beside the data. MultiPL-E's
   licence forbids using its contents as training data (§8.1), and outputs built from its prompts
   carry that clause.
7. **Where it is linked.** mzizi.dev's status panel and docs.mzizi.dev link the **latest** `RUN.md`
   (the latest, not the best) and quote its first sentence verbatim. Those sites consume this
   repository; nothing here depends on them (RFC-0004 §3).

## 8. Public external suites

Two caveats govern all of them:

- **The public suites are in the incumbents' training data, and the models have seen no Mzizi.**
  That is a handicap for Mzizi and exactly the thesis under test (§5).
- **Public tasks are contaminated**, so the held-out tasks (RFC-0004) stay the kill-criterion
  measurement. The public suites give comparability with the field and visibility, and never gate.

Every suite below was checked on 2026-09-29 against its repository (licence, languages, how a
language is added, last commit).

### 8.1 Authoring suites

| Suite                            | State (2026-09-29)                                                                                                                                                                                                                                                                                     | Measures                                                                              | Maps to our metrics                                                                                                                                                                      | Cost to add Mzizi                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **MultiPL-E**                    | `nuprl/MultiPL-E` `3025a53`, 2026-01-28. BSD-3-Clause **with a machine-learning restriction** (clause 4: no use as training data). HumanEval and MBPP translated from Python to 18 other languages (the README's count; `dataset_builder/` holds more translators), incl. TypeScript, Go, C++ and Rust | single-shot functional correctness, pass@k                                            | defect rate. Its result status separates compile failures from test failures, so FM-13's split survives. Tokens: completion length. No iteration loop                                    | Documented path: a translator `dataset_builder/humaneval_to_mz.py`, an `evaluation/src/eval_mz.py`, a `terms.csv` row and a container. The translator emits the hidden tests as a `contract` block of `example` lines (RFC-0010 §3), so `mz contract` is the evaluator. **Blocked** on function parameters and returns (RFC-0007 G1.2), expressions and arithmetic (G1.3, RFC-0008 §10.6), maps (RFC-0008 §10.4), and execution (§6.4)                                                              |
| **EvalPlus** (HumanEval+, MBPP+) | `evalplus/evalplus` `26d6d00`, 2025-10-02; last release v0.3.1, 2024-10-20. Apache-2.0. Python only; 80× and 35× more tests than the originals; MBPP+ has 378 tasks                                                                                                                                    | functional correctness under much stronger tests                                      | defect rate, with fewer false passes                                                                                                                                                     | Through MultiPL-E's "add a new benchmark" path (a directory of Python programs with equality assertions), so the one Mzizi translator serves both, and the incumbents are translated in the same step. Tests MultiPL-E's value translator cannot express are dropped and counted. Marginal once MultiPL-E works                                                                                                                                                                                     |
| **Aider polyglot**               | `Aider-AI/polyglot-benchmark` `7e0611e`, 2024-12-22, unchanged since release; harness in `Aider-AI/aider` `benchmark/` (Apache-2.0; `5dc9490`, 2026-05-22). 225 Exercism exercises: C++ 26, Go 39, Java 47, JavaScript 49, Python 34, Rust 30. Exercism content is MIT. No TypeScript                  | an edit-and-fix loop: two tries by default, the second after seeing the failing tests | the closest public match to iterations (`pass_rate_1` → `pass_rate_2`), though its loop is test-driven, not compile-driven. Tokens (it records prompt and completion tokens) and defects | A `mzizi/exercises/practice/<slug>/` tree per exercise: `.docs/` from `exercism/problem-specifications` (MIT, `9943fd7`, 2026-09-25, language-neutral), a stub `.mz`, tests generated from its canonical data as a contract block, `.meta/example.mz`, and a `.mz` entry in `benchmark.py`'s `run_unit_tests` map that calls `mz contract`. Use the same slugs as the other tracks, so pass rates compare exercise by exercise. **Blocked** as for MultiPL-E, plus multi-function files and records |
| **BaxBench** (the backend suite) | `logic-star-ai/baxbench` `de885cd`, 2025-10-22; v1.0.0, 2025-09-13. MIT. 28 scenarios (API specifications), with environments for Go, JavaScript, PHP, Python, Ruby and Rust. Tested over HTTP as a black box                                                                                          | backend correctness, **and security**, from functional tests plus exploits            | defect rate; it adds a security-defect rate we do not measure yet                                                                                                                        | One `src/env/` environment for `mzizi-be`, since the scenarios are already language-neutral. Nearly free once `mzizi-be` exists (§6.4), because it has the same shape as our probes                                                                                                                                                                                                                                                                                                                 |
| BigCodeBench                     | `bigcode-project/bigcodebench` `09dd993`, 2025-10-15. Apache-2.0. 1,140 tasks, Python only                                                                                                                                                                                                             | library-heavy programming                                                             | —                                                                                                                                                                                        | **Not adopted.** Its tasks are defined as calls into Python libraries, so a port has to re-specify each task, and the result would no longer be comparable. BaxBench is the backend suite instead                                                                                                                                                                                                                                                                                                   |

### 8.2 Order

1. **MultiPL-E**, first: it has the smallest language surface (one function per problem), a
   documented path for adding a language, and incumbents in every language the owner named.
2. **EvalPlus**, next, because it reuses the same translator.
3. **Aider polyglot**, which needs a larger surface but is the only public suite with a repair
   loop.
4. **BaxBench**, as soon as `mzizi-be` exists.

None of them can start before Mzizi has functions with parameters, results, expressions and an
execution path. That work is in RFC-0007's Tier 1, and G2.1's lowering, not in this RFC.

### 8.3 Runtime suites: what Mzizi compiles to, not how well an agent writes it

These measure the lowered output. They are a separate claim, never pooled with, or quoted as, the
authoring result (RFC-0007 §5.4).

| Suite                                 | State (2026-09-29)                                                                                                                                                         | Cost to add Mzizi                                                                                                                                                                                                                                                                                                       |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **js-framework-benchmark** (Krausest) | `krausest/js-framework-benchmark` `f2df01a`, 2026-09-20, actively maintained. Apache-2.0. 191 keyed entries, including `dioxus`, `leptos`, `react-hooks` and `astro-react` | A directory buildable with `npm install` and `npm run build-prod`, using the element ids its webdriver tests expect. Needs local state (G1.1), events, `for each` over state, and a web build (G2.1, G2.13). Measures the pure-Rust frontend path directly; `astro-react` is the precedent for measuring the Astro path |
| **TechEmpower Framework Benchmarks**  | **Archived 2026-03-24** ("Sunsetting the TechEmpower Framework Benchmarks", issue #10932). Last round: 23, 2025-03-17. BSD-3-Clause                                        | **No new entry can be submitted.** Use its published test types (JSON serialisation, plaintext, single and multiple queries, fortunes, updates) as a local runtime family over our own backend arms on one machine. Those numbers are not comparable with TechEmpower's published rounds, and must say so               |

Order: js-framework-benchmark after the first web build exists, then the local TechEmpower-style
family once `mzizi-be` lowers.

## 9. Phasing

What is cheap is the checker. What costs is the input and the scorer, because of BM-1 and BM-3.

| Step | Work                                                                                                                                                                      | Needs language work?                             |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------ |
| 1    | Runner: arms read from `arm.toml`; `spec.md` input; the parity test over every pair of arms; `PLAN.md` and raw-bundle hashing (§7)                                        | no                                               |
| 2    | `react` arm: a sandbox pinned from the registry lockfile, a `tsc` check, a `cva` extractor in the harness, and `spec.md` for `button`, `badge` and the changelog renderer | no                                               |
| 3    | A probe crate (`benchmarks/probe`, its own workspace member, so the harness keeps zero dependencies), B1, and the `rust` and `ts` backend arms                            | no                                               |
| 4    | `python`, `go` and `cpp` arms; B2–B5 as development tasks. Result: an **incumbents-only backend baseline**, published, with no Mzizi claim                                | no                                               |
| 5    | The `ui-spec` gating run on held-out tasks, after the pre-kill-criterion fixes land                                                                                       | no                                               |
| 6    | The §6.4 slice, then `mzizi-be`, rendered UI facts, and the `backend` gating run                                                                                          | **yes** (G1.2, G2.1, G2.2, G2.4, G2.5, RFC-0010) |
| 7    | The public suites, in §8.2's order, as the language reaches them                                                                                                          | yes                                              |

**The repository's CI stays self-contained** (AGENTS.md, RFC-0004 §3). No Node, Python, Go or
clang toolchain becomes a CI requirement. Arm checkers run only in benchmark runs, and the runner's
tests keep using in-process fakes, as they do today. Pins, fixtures and references are copied into the
repository with their provenance, as `benchmarks/tasks/` already is, and nothing fetches from a
sibling repository at run time.

## 10. What this RFC does not claim

- That any arm beyond the three that exist has been built or run.
- That Mzizi will pass. §6.5 says the likely outcome on today's compiler.
- That the public suites measure the thesis. They measure comparability, on contaminated tasks.
- That the runtime suites say anything about authoring, or the reverse.

## 11. Open questions

1. **Java and plain JavaScript.** Aider polyglot has both, and neither is an arm. Adding them would
   make the public and internal results line up language by language.
2. **A Workers-runtime family.** Running `ts` and a `workers-rs` arm under workerd, with bindings,
   once a Mzizi Worker target exists (G2.7).
3. **A self-test loop variant.** Every arm's check is compile-only, so no arm sees its own tests in
   the loop. A variant that feeds back `tsc` + `vitest`, `pyright` + `pytest`, `go test`,
   `cargo test` and `mz contract` alike would measure a different, more realistic loop.
4. **Page composition in Astro.** Whether `.astro` pages are a family of their own, if the Astro
   path's pages are hand-written rather than generated.
5. **The held-out rotation fraction** (RFC-0004 §6.2), now that a retired set is published whole.
