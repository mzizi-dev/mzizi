# Readiness for the kill-criterion run

**Status, 2026-09-30: not ready, and the run has not happened.** Nothing in this repository has
been measured against the charter's kill criterion. Two Phase 0 pilots ran on 2026-09-27, and
neither showed an advantage for Mzizi. On the frontier model the two arms were
indistinguishable on compile rate and defects, and Mzizi used about 8% fewer transcript
tokens. On the ~7B open-weight model, Mzizi did worse on all three metrics
([`results/2026-09-27-pilot-2/RUN.md`](results/2026-09-27-pilot-2/RUN.md)).

Phase 0's goal (charter v0.4, §4) is to show that Mzizi can stand against the best existing
language for each kind of task. The kill-criterion run is how that goal is measured, and the
pilots' component tasks are tests within it, not the goal.

This page audits the list of fixes pilot 2 said must come before a run that tests the kill
criterion. It covers that write-up's "What would have to change", "Compiler behaviour that
disagrees with the RFCs" and "Threats to validity" sections. Each item was checked on `main`
at `a9c928d` by running it, then fixed on this branch where the fix was code. What is still
open is listed with the decision it waits on.

The kill criterion itself is [RFC-0009](../design/RFC-0009-comparison-benchmark.md) §6, the
owner's decision of 2026-09-29. Its pre-registered decision rule (§6.2) is fixed before any
held-out score is seen: within each gating task family, and for each of the three metrics,
the bar is the best incumbent's value, and Mzizi must beat it on at least two of the three,
on the headline ~7B model, on held-out tasks. A win on a metric counts only if a paired
bootstrap that resamples tasks gives a 95% interval for the difference that excludes zero.
What each run publishes is RFC-0009 §7 and RFC-0004 §4.2. This page tracks what the run
still waits on. RFC-0009 also brings in the arms for more languages
(TypeScript/React, Python, Go, C++ and plain-Rust backends). Since 2026-09-30 an arm is one
file, `arms/<id>/arm.toml` ([`runner/README.md`](runner/README.md), "Arms: `arm.toml`"). Of
the new arms, `react` and `mzizi-be` (item 5) have been added, and neither has run an episode.

## The audit

"On `a9c928d`" is what the command printed on `main` before this branch. `mz` means that
commit's `target/debug/mz`. Paths under `raw/` are in `results/2026-09-27-pilot-2/`. The
right-hand column describes this branch.

### What would have to change (pilot 2's list, in its order)

| #   | Item                                                              | On `a9c928d`                                                                                                                                                                                                                              | Now                                                                                                                                                                                                                                                                                                                                                                     |
| --- | ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Relative paths in `mz check --agent`, or from the runner          | **Open.** `mz check --agent /abs/x.mz` prints `"file":"/abs/x.mz"` on every line, and `episode.rs` passes `{file}` as the absolute `iter-NN/candidate.mz`. Every `diagnostics.txt` under `raw/…/mzizi/` shows it.                         | **Fixed** in the runner, for every arm: the default `file-name` normaliser turns the candidate's path into `candidate.mz` before the model sees it. The end-to-end test runs the real `mz` on an absolute path. Recorded as `diagnostics_normaliser` in `meta.json`.                                                                                                    |
| 2   | Diagnostics, ideally `exact`, for `...props` and `asChild`        | **Open.** `mz check --agent raw/scored/qwen…/mzizi/badge/seed-1/iter-01/candidate.mz`: 13 errors (MZ0304 `prop needs a name, found .`, 3 × MZ0402, 7 × MZ0204, MZ0206, MZ0207). None names the idiom. The only fixes are for the cascade. | **Fixed.** The same file gives 4 errors: 3 × MZ0106 (spread, `exact` fix deletes the line) and 1 × MZ0408 (attributes directly under `view`). There is also a warning, MZ0312 (`asChild`), which names the branch that reads the prop. `mz fix` takes the file to 1 error. The guide names both idioms.                                                                 |
| 3   | FM-11: derive `height` from `h-N`/`size-N`, or check them         | **Open.** `raw/scored/qwen…/mzizi/button/seed-1/iter-01/candidate.mz` writes `icon class "size-14" height 48`. `mz check` passes it with 0 errors, and the scorer counts 1 defect.                                                        | **Fixed, both ways.** A row without `height` takes the height its class renders, in `mz contract` and in the harness (the rule is shared and tested on the same examples in both). A row whose `height` disagrees is MZ0313, with the rendered number as the `exact` fix. That file now fails `mz check`: `icon declares height 48 but its class size-14 renders 56px`. |
| 4   | A task set that can fail: more facts, more components, more seeds | **Open.** 3 tasks, 13 facts, 11 of them scored in pilot 2. No fact beyond enums. Seeds are whatever the driver script passes (1–3).                                                                                                       | **Partly.** There is a `slot_set` fact (`score --slots`, opt-in with `score_slots = true`), and a `card` task using it (2 enum facts plus 7 slots). `kill-criterion/run.sh` defaults to 5 seeds and all three arms. The public pool is surveyed in `tasks/README.md`. The set that counts has to be held out: see 5.                                                    |
| 5   | Fresh, unpublished tasks                                          | **Open.** Every task is public, and so is each reference (`mzizi-dev/mzizi-registry`).                                                                                                                                                    | **Open; scaffolding done.** [`kill-criterion/README.md`](kill-criterion/README.md) has the plan: where the tasks live (a private `mzizi-dev` repository, per RFC-0004 §6.1), the format, what makes a task fresh, the size, and the settings to pre-register. `check-task.sh` validates a task without printing it. Who writes them is item 2 of "What remains".        |

### Compiler behaviour that disagreed with the RFCs

| #   | Divergence                                                | On `a9c928d`                                                                                                                                                                                                                                       | Now                                                                                                                                                                                                                        |
| --- | --------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | `else` in a view rejected (MZ0402)                        | **Fixed** before this branch (RFC-0008). A `when … else … end` view: `mz: 0 errors`.                                                                                                                                                               | Fixed.                                                                                                                                                                                                                     |
| 2   | `mz fix` does not exist                                   | **Open.** `mz fix x.mz` → `usage: mz <check\|contract\|outline\|hash\|ir> …`, exit 2.                                                                                                                                                              | **Fixed.** `mz fix` applies every `exact` fix and re-checks. The guide's wrong-on-purpose `tag.mz` goes to 0 errors in one pass. The first test found MZ0204's fix landing _before_ the last line, which is now fixed too. |
| 3   | Unknown type accepted silently                            | **Fixed** before this branch. `prop x: strng` → `MZ0701 strng is not a type`.                                                                                                                                                                      | Fixed.                                                                                                                                                                                                                     |
| 4   | Missing `=` gives four errors, none on the wrong line     | **Open.** `class "flex"` in a row → MZ0402 at `contract`, MZ0204 and MZ0206 at the last line, and nothing on line 6.                                                                                                                               | **Fixed.** One MZ0406 on that line, with `=` as the fix (`exact` for known attribute words).                                                                                                                               |
| 5   | `if` in a view accepted silently                          | **Open.** `if open` … `end` → `mz: 0 errors`.                                                                                                                                                                                                      | **Fixed.** MZ0407, with `exact` fix `when`. Any element word followed by more on its line is MZ0409, where before the tail was silently kept.                                                                              |
| 6   | MZ0602 without its `exact` fix                            | **The RFC was wrong.** The clause in `raw/…/button/seed-3/iter-01` is `control "Button"` with no operand. The `exact` fix (insert `is`) needs an operand, so none applies.                                                                         | **RFC-0006 §8.2 corrected** to say the fix needs an operand. The diagnostic for an element subject now names the one predicate it takes (`min_height <n>`) and the `shows` form.                                           |
| 7   | MZ0206/MZ0208 fixes replace only `end`; conflicting fixes | **Reproduced.** `end view` closing a `row` → fix `end element` over `end` only, giving `end element view`. A nameless `end component` → `end component g component`. `end component` with a view still open → MZ0206 and MZ0204, which contradict. | **Fixed.** Every closer fix rewrites the whole closer. An inner block's fix is the bare `end`. Open blocks at `end component`, or at `contract` inside a view, are one MZ0204 (with a `guess` fix).                        |

### Threats to validity

| Threat                                                      | On `a9c928d`                                                                                             | Now                                                                                                                                                                                                                                                                                                               |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Tiny n                                                      | 2 scored tasks, 11 facts, 3 seeds                                                                        | **Open.** It waits on the held-out set. The plan is at least 10 tasks and 30 or more facts per gating family, and 5 seeds on every arm (`kill-criterion/README.md`).                                                                                                                                              |
| Path strings penalise Mzizi (~13,500 tokens)                | reproduced (item 1)                                                                                      | **Fixed** (item 1).                                                                                                                                                                                                                                                                                               |
| Public tasks                                                | all public                                                                                               | **Open** (item 5).                                                                                                                                                                                                                                                                                                |
| Frontier model and language share an author                 | inherent                                                                                                 | **Open, mitigated in the plan.** The open-weight model is the headline arm, and the held-out tasks are to be written by maintainers by hand or drafted by another vendor's model and reviewed by a person, not Claude-drafted (owner to confirm). It cannot be removed while the RFCs and guides have one author. |
| Frontier tokens are a proxy                                 | Mode 2 counts `transcript_tokens` only                                                                   | **Open, inherent to Mode 2.** Any model behind an OpenAI-compatible endpoint can run in Mode 1, which records exact usage. `/tokenize` for `transcript_tokens` is llama.cpp's.                                                                                                                                    |
| Mode 2 rules are instructions, not enforcement              | `iteration_count_enforced_by_runner: false`                                                              | **Open, inherent to Mode 2.** It is recorded in `meta.json` as before.                                                                                                                                                                                                                                            |
| Seeds at temperature 0.2 repeat                             | 7B episodes repeat byte for byte across seeds                                                            | **Decided:** temperature 0.7 with seeds 1–5, plus one seed at 0.2 as a check against the pilots, registered in `PLAN.md`. `run.sh` defaults to 0.7.                                                                                                                                                               |
| _Found here:_ a context overflow left an episode unfinished | `raw/…/mzizi/badge/seed-2` has no final line, `summarize` shows 2/**5**, and RUN.md corrected it by hand | **Fixed.** A Mode 1 episode whose history outgrows the context after the first request finishes as not clean (`ended_by: context_exceeded`).                                                                                                                                                                      |
| _Found here:_ the Leptos arm has never run                  | both pilots ran Dioxus only, and Leptos is an incumbent in RFC-0009's UI families                        | **Open.** `run.sh` includes `leptos` by default. Its sandbox needs its crates fetched once.                                                                                                                                                                                                                       |
| _Found here:_ the task survey's selection rule was wrong    | `tasks/README.md` said only enums with `classes()` are read, and marked 18 components "0 facts"          | **Corrected.** The scorer reads every enum. 13 of the 18 have enum facts, and the table is in `tasks/README.md`.                                                                                                                                                                                                  |

Every Mzizi candidate from either pilot that compiled cleanly still does, except the two 7B
buttons that MZ0313 now rejects (item 3). That covers the six clean frontier-model candidates from
pilot 2 and the three from pilot 1. Applying `mz fix` to any unique Mzizi candidate from
either pilot never raises its error count.

## What remains before the run counts

The gating runs are RFC-0009's `ui-spec` and `backend` families (RFC-0009 §2). Phase 0
passes only if both pass (§6.2, rule 5). In RFC-0009 §9's order:

1. **Runner and input (RFC-0009 §9, steps 1–2). Built on 2026-09-30; the React pins
   followed on 2026-10-07.** Nothing here has run an episode.
   - **Done.** Every arm is read from `arms/<id>/arm.toml`. `mzizi`, `dioxus` and `leptos`
     were migrated with no behaviour change: a test rebuilds pilot 2's committed user
     messages byte for byte. `--family ui-spec` hands the author `spec.md`, the
     language-neutral input, now written for button, badge, card and the changelog renderer.
     The parity test runs over every pair of arms in each family. `mzbench plan` drafts
     `PLAN.md` from the files on disk, and `mzbench bundle-hash` computes the raw-bundle hash
     that `run.sh` now uses (§7). A `react` arm exists: a strict `tsc` check in an offline
     sandbox, and a `cva` extractor in the harness. On the registry's own `button.tsx` and
     `badge.tsx` the extractor gives 0 defects against the Rust references. The four UI guides
     are rebalanced to 2,750 Qwen2.5-Coder tokens, from 2,691 to 2,750 (2.2% apart, against
     31% before). [`prompts/BUDGET.md`](prompts/BUDGET.md) records them, and a test catches a
     guide that changed without being re-measured.
   - **Done (2026-10-07): the React pins are the registry's.** RFC-0009 §1 pins the React
     arm from `mzizi-registry`'s lockfile at the task commit. The six pins and
     `sandbox/package-lock.json` now match its `pnpm-lock.yaml` at `3afeb752`, version and
     integrity hash ([`arms/react/README.md`](arms/react/README.md)). The guide's version line
     changed (React 19.3 to 19.2) and was re-measured: 2,691 tokens, as before.
   - **Not measured.** The guide counts come from Hugging Face `tokenizers`, not llama.cpp's
     `/tokenize`. The run's own `guide_tokens` (on every final line) is the count that goes in
     `PLAN.md`. The React arm, like Leptos (item 4), has never run end to end.
   - The pilots' `spec.tsx` tasks are the `ui-port` family, for development and pilot
     comparison only. The guide rebalancing also changes what a `ui-port` episode is shown, so
     a `ui-port` run after it is not pooled with the pilots.
2. **Held-out tasks** for each gating family, in the private repository, each passing
   `kill-criterion/check-task.sh` (items 4 and 5). Who writes them: (a) maintainers by hand,
   or (b) another vendor's model drafts them and a person reviews each one; not
   Claude-drafted. The owner is to confirm. No repository has been created.
3. **A `PLAN.md` registered before the first episode** (RFC-0009 §7.2). It fixes the rule
   (RFC-0009 §6.2), its n, and the settings in
   [`kill-criterion/README.md`](kill-criterion/README.md): temperature 0.7 with seeds 1–5
   (plus one seed at 0.2 as a check against the pilots), and `-c 32768` for the 7B server.
   It is fixed before any held-out score is seen. `run.sh` records the server's context in
   `manifest.json`, and `RUN.md` states it.
4. **The Leptos arm** runs at least once end to end. Neither pilot ran it.
5. **The `backend` family** needs a `mzizi-be` arm, which needs the language work in
   RFC-0009 §6.4. Until it exists that family has not passed, and so neither has Phase 0.
   _Progress, 2026-09-30:_ the slice is designed in
   [RFC-0011](../design/RFC-0011-handlers.md), and its front end, in-process evaluator and lowering are
   built: `mz check` checks a `service`, `mz contract` runs it, and `mz build` lowers it to an
   axum package that serves locally and answers HTTP. The `mzizi-be` arm exists, with a
   probe crate (`benchmarks/probe`) and B1 as its first task, whose Mzizi and axum references
   both hold all 59 of its facts. Still open: the runner does not score an episode with
   probes, B2–B5 and the incumbent backend arms do not exist, and nothing has been measured. RFC-0011 §12 records each piece as it lands.

### Not part of the run: runtime performance

[`perf/`](perf/README.md) times the Rust that `mz` lowers a program to against hand-written
Rust, overflow unchecked and checked. CI gates its correctness, never its timing: it checks
that each program builds three ways and prints the same output, and times nothing. No timing
is committed. It says nothing about the kill
criterion, which is about how well models write Mzizi, and none of its numbers is a claim that
Mzizi is faster.

## The command

For the `ui-spec` gating run, once items 1–4 are done, on the machine that serves the headline
model:

```sh
CTX=32768 benchmarks/openweight/setup.sh all   # llama.cpp b11206, Qwen2.5-Coder-7B, -c 32768
R=benchmarks/results/<date>-kill-criterion-ui-spec
# $R/PLAN.md is committed first (RFC-0009 §7.2); run.sh refuses to start without it.
# Each run.sh below records the server's n_ctx in its manifest.json.
benchmarks/arms/react/setup.sh                 # once: npm ci from the pinned lockfile
target/debug/mzbench plan --out "$R" --tasks /path/to/held-out/ui-spec --held-out true \
  --arms "mzizi dioxus leptos react" --family ui-spec \
  --model-label qwen2.5-coder-7b-instruct-q4km --seeds "1 2 3 4 5" --temperature 0.7 \
  --n-ctx 32768                                 # then fill in the "To fill" parts and commit
benchmarks/kill-criterion/run.sh --tasks /path/to/held-out/ui-spec --family ui-spec \
  --out "$R/qwen2.5-coder-7b-instruct-q4km" --model-label qwen2.5-coder-7b-instruct-q4km \
  --arms "mzizi dioxus leptos react" --seeds "1 2 3 4 5" --temperature 0.7 --max-iters 5
benchmarks/kill-criterion/run.sh --tasks /path/to/held-out/ui-spec --family ui-spec \
  --out "$R/qwen2.5-coder-7b-instruct-q4km-t0.2" --model-label qwen2.5-coder-7b-instruct-q4km \
  --arms "mzizi dioxus leptos react" --seeds "1" --temperature 0.2 --max-iters 5
```

Fall back to `CTX=16384` only if the machine's RAM cannot hold 32,768 tokens of context, and
record the fallback and the reason in `RUN.md`. The frontier model runs the same tasks, arms and seeds in Mode 2
(`runner/README.md`) and is reported beside the headline, never pooled with it (RFC-0009 §5).
Running any of this needs model endpoints, so the decision to run it is the supervisor's.
