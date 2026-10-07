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
