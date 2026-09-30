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
