# Phase 0 pilot 2 — 2026-09-27: the open-weight arm

This is the second pilot run that day, and the first with a small-model arm and measured tokens.
The first, [`../2026-09-27-pilot/RUN.md`](../2026-09-27-pilot/RUN.md), ran one frontier seed
per task with no tokenizer, and named the missing open-weight arm as its main caveat: RFC-0002
§5.4 says the frontier arm alone cannot validate the thesis. Both ran against the same code,
`ded425a`, and neither is the Phase 0 number.

**Result, stated first: this pilot does not show a measurable advantage for Mzizi.** On the
frontier model the two arms are indistinguishable on compile and defects, with Mzizi about 8%
cheaper in transcript tokens. On the ~7B open-weight model — the size RFC-0002 §1 makes the
design target — Mzizi did **worse** on all three metrics. Everything below is n = 2 tasks ×
3 seeds per arm, a pilot and not the charter's measurement; the owner decided in advance that
it would be reported whichever way it fell, and that the Phase 1 decision is theirs after
reading it.

## What ran

|                   |                                                                                                                                                                                                                                                                                                         |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Code              | `ded425a` (`main` at the time): arms, runner and scorer as of that commit                                                                                                                                                                                                                               |
| Tasks (scored)    | `button`, `badge` from `benchmarks/tasks/` (registry `3afeb75`), 11 facts                                                                                                                                                                                                                               |
| Task (unscored)   | `nyuchi-changelog-renderer` (since renamed `mzizi-changelog-renderer` on `main`; the raw data keeps the name it ran under) — its reference renames the spec's variants on purpose (CHARTER §6 "documented, deliberate divergence"), so a faithful port scores defects on both arms; reported separately |
| Arms              | Mzizi (`mz check`) vs Dioxus `=0.7.10` (`benchmarks/arms/dioxus/check.sh`)                                                                                                                                                                                                                              |
| Prompts           | Same builder for both arms; system message = the arm's guide (10,223 vs 10,375 chars); user message = the task spec + one naming sentence                                                                                                                                                               |
| Frontier model    | Claude Sonnet 5, as Claude Code subagents driving `mzbench episode` (Mode 2)                                                                                                                                                                                                                            |
| Open-weight model | Qwen2.5-Coder-7B-Instruct Q4_K_M, llama.cpp `b11206`, CPU, `mzbench run` (Mode 1), temperature 0.2                                                                                                                                                                                                      |
| Budget            | 5 compile iterations per episode, seeds 1–3                                                                                                                                                                                                                                                             |
| Scorer            | `mzizi-benchmark-harness score`: variant set, `#[default]`, and `h-N`/`size-N` touch height per variant. Class-token Jaccard reported, not counted                                                                                                                                                      |

## Results

Rates carry their denominators; tokens are per episode.

| Model            | Arm    | Clean compile | Iterations to clean (mean) | Transcript tokens (mean) | Defect rate (clean, scored) | Class-token Jaccard |
| ---------------- | ------ | ------------- | -------------------------- | ------------------------ | --------------------------- | ------------------- |
| Sonnet 5         | Mzizi  | 6/6           | 1.17                       | 4,172                    | 0/6                         | 0.844               |
| Sonnet 5         | Dioxus | 6/6           | 1.00                       | 4,545                    | 0/6                         | 1.000               |
| Qwen2.5-Coder-7B | Mzizi  | **2/6**       | 1.50 (n=2)                 | 8,608 (n=5)              | **2/2**                     | 1.000               |
| Qwen2.5-Coder-7B | Dioxus | **4/6**       | 2.00 (n=4)                 | 7,782                    | **0/4**                     | 1.000               |

`summary-scored.md` is the runner's own output. It shows the 7B Mzizi arm as 2/**5**: one
episode (badge, seed 2) aborted on its fifth request because the conversation outgrew the
16,384-token context. This table counts that as **not clean** — running out of context is a
way for the edit loop to fail, and counting it the other way would flatter Mzizi.

Unscored (`summary-unscored.md`): on the changelog task Sonnet 5 compiled clean first time on
both arms and both scored the 2 expected rename "defects"; the 7B model compiled neither
(Mzizi aborted on context at its third request, Dioxus used all five iterations).

## What the numbers say, and why

### Frontier: the tasks were too easy to separate the arms

Every Sonnet 5 episode compiled, and none had a defect. The defect metric hit its ceiling;
11 facts across two small primitives cannot distinguish two languages a frontier model writes
fluently. The two real differences:

- **Tokens:** Mzizi 8.2% fewer (4,172 vs 4,545). The guides are within 1.5% of each other, so
  this is the ported file being smaller. (Harness-reported subagent totals, ~104k per episode
  on both arms, are dominated by the agent runtime's own overhead and say nothing about the
  languages — `subagent-usage.tsv`.)
- **Class fidelity:** Mzizi ports kept fewer of the reference's classes (Jaccard 0.844 vs
  1.000). They dropped the `has-data-[icon=…]` padding variants. **This is not a language
  limit** — a class column holding `has-data-[icon=inline-end]:pr-3` and
  `[&_svg:not([class*='size-'])]` passes `mz check` and `mz contract`, verified. It matches
  what the corpus's own `primitives/button.mz` does. The decision, made before the runs, to
  report class tokens but not count them as defects **favours Mzizi here**: counted, the
  Mzizi arm would show defects and the Dioxus arm none.

### 7B: three concrete failure mechanisms, all on the Mzizi side

1. **The repair loop did not converge on Mzizi diagnostics.** Of 16 Mzizi repair attempts,
   **15 resubmitted a byte-identical file**; on Dioxus, 6 of 12. The model read the NDJSON and
   changed nothing. §3 of the charter bets on "dense, high-signal compiler errors" making that
   loop faster; for the target model size, in this pilot, it did the opposite.
2. **React idioms with no Mzizi form, and a compiler that rejects its own RFC.** Every badge
   episode stalled on the spec's `...props` spread and `asChild ? Slot : "span"`: the model
   wrote `prop ...props` and an `else` branch. Mzizi has no spread, and `mz check` rejects
   `else` in a view (MZ0402) although RFC-0001 §1.2 lists it. Dioxus has `..attributes` and
   `if`/`else`, so the same spec maps directly. Neither the diagnostics nor the guide told the
   model what to write instead. _Since the run:_ RFC-0008 (`b24ce2e`) added `else` to the
   grammar, and a `when … else … end` view parses on `main` at `164a650` (verified). The
   spread still fails there as `MZ0304 prop needs a name, found .`, with no hint.
3. **FM-11, parallel truth, reproduced by the language itself.** Both clean 7B Mzizi buttons
   have the same single defect: `icon class "size-14" height 48`. The class is right —
   `size-14` renders 56px — and the declared `height` column is wrong. RFC-0006 names this
   failure (one fact written twice drifts); Mzizi's variant table asks the author to write the
   height twice, and `mz contract` cannot notice because it deliberately treats `size-N` as
   unevaluable (RFC-0006 §5). Under a rendered-behaviour reading this is 0 defects; under the
   pre-registered scorer it is 1. It is counted, and the component's own touch-floor contract
   is, in the meantime, checking a number it does not render.

## Threats to validity

Named so a reader can discount the result correctly — in both directions.

- **Tiny n.** Two scored tasks, 11 facts, 3 seeds. Differences of one episode move a rate by
  17 points. Nothing here is statistically significant, and nothing is claimed to be.
- **A harness bug that penalises Mzizi.** `mz check` prints each diagnostic's full absolute
  file path; `check.sh` rewrites Dioxus paths to the bare file name. The 7B model received
  **152 path strings × 89 tokens ≈ 13,500 tokens** of path across its five finished Mzizi
  episodes. That alone exceeds the Mzizi arm's 7B token deficit; fix before the next run.
  Whether it also made the NDJSON harder for the 7B model to act on is untested.
- **Public tasks.** Owner's decision: the task set and results are public, so any later run
  on these tasks may be contaminated. The references were already public in
  `mzizi-dev/mzizi-registry`.
- **The frontier model and the language share an author.** The RFCs and guides were written
  by Claude; a Claude subagent may find Mzizi unusually legible for that reason.
- **Frontier tokens are a proxy.** `transcript_tokens` counts prompt + submissions + feedback
  with the Qwen tokenizer; the subagent's own reasoning is not observable. The 7B arm's
  `endpoint tokens` are exact.
- **Mode 2 rules are instructions, not enforcement.** Subagents were told to read only their
  episode's `prompt.md` and use only `mzbench`; `meta.json` records
  `iteration_count_enforced_by_runner: false`.
- **Seeds at temperature 0.2** reproduce first replies, which is why several 7B episodes
  repeat identically across iterations and seeds; the repair behaviour above is measured with
  the seeds, not despite them.

## What would have to change before a run that tests the kill criterion

In order of how much each would move this result:

1. Relative paths in `mz check --agent` output (or the runner passing a relative path).
2. Diagnostics — ideally `exact` fixes — for the React idioms an agent brings with it
   (`...props` spread, `asChild`). `else` itself, the third idiom here, is implemented on
   `main` since RFC-0008. The rest of the compiler/RFC divergences found during this pilot
   are listed below.
3. Remove FM-11 from the variant table: derive `height` from `h-N`/`size-N` (the harness
   already does this derivation) or have `mz contract` check the two agree.
4. A task set that can fail: more components, scoreable facts beyond `classes()` (the task
   survey proposes `data-slot` sets, which would bring `card` in), more seeds.
5. Fresh, unpublished tasks, if the run is meant to test the kill criterion rather than
   develop the language.

## Compiler behaviour that disagrees with the RFCs

Found while verifying the Mzizi guide, and re-run for this write-up twice: against `mz` at
`ded425a`, which the pilot ran on, and at `164a650`, `main` when this was written. Each one is
a place where an agent following RFC-0001 or RFC-0006 gets different behaviour from the
compiler than the design promises.

| #   | Behaviour at `ded425a`                                                                                                                                             | RFC says                                                    | At `ded425a`                                                                              | At `164a650`         |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------- | ----------------------------------------------------------------------------------------- | -------------------- |
| 1   | `else` in a view is rejected (MZ0402)                                                                                                                              | RFC-0001 §1.2 lists `else` and `match`/`case`               | reproduced — what every 7B badge episode hit                                              | **fixed** (RFC-0008) |
| 2   | `mz fix` does not exist; the CLI prints usage and exits 2                                                                                                          | RFC-0001 §4.3; diagnostics call their repairs `exact` fixes | reproduced                                                                                | still                |
| 3   | An unknown type (`prop x: strng`) is accepted silently                                                                                                             | RFC-0001 §1.7 / §3 describe type checking                   | reproduced                                                                                | **fixed** (MZ0701)   |
| 4   | A missing `=` (`class "flex"`) produces four errors, none of which names the line that is wrong                                                                    | RFC-0001 §4.1: one diagnostic per real error                | reproduced                                                                                | still                |
| 5   | `if` in a view is accepted silently, as an element named `if`                                                                                                      | —                                                           | reproduced                                                                                | still                |
| 6   | MZ0602 (clause with no predicate) arrives without the `exact` fix                                                                                                  | RFC-0006 §8.2 promises one                                  | reproduced, and seen in `raw/scored/claude-sonnet-5-subagent/mzizi/button/seed-3/iter-01` | still                |
| 7   | The `exact` fixes for MZ0206/MZ0208 replace only the `end` word, so `end enum g` becomes `end component enum g`; a misplaced `end view` gets two conflicting fixes | RFC-0001 §4.3: fixes apply mechanically                     | not reproduced — reported by the guide agent                                              | not checked          |

## Rescored on `main`

The scorer has changed since the run (renamed-variant pairing, opt-in per task, from the
first pilot). Every clean episode here was rescored with `mzizi-benchmark-harness score` at
`164a650` against `main`'s task references: **all 18 give the same defect count as at
`ded425a`**, with the same 9-fact and 2-fact totals. The eight clean Mzizi candidates also still
pass `main`'s stricter `mz check` (names and types resolved, RFC-0008) with zero errors. So
nothing above moves because of later code; what would move it is the fixes listed earlier.

## Files

- `summary-scored.md`, `summary-unscored.md` — `mzbench summarize` output; Prettier has re-padded the tables, and nothing else differs (checked by diffing with whitespace and dashes squeezed).
- `subagent-usage.tsv` — the agent runtime's own token, tool-call and duration totals per
  frontier episode.
- `run-openweight.sh` — the exact driver for the 7B arm.
- `raw/` — every episode: per-iteration replies, candidates, diagnostics and feedback exactly
  as the model saw them, `episode.jsonl`, `meta.json` (including the argv that ran) and
  `score.json`. Two changes from the runner's output, both so the repository's markdown
  linters do not lint model output: each `prompt.md` is omitted (it is `system.txt` +
  `user.txt` with two headings — verified for all 28) and each `reply.md` is renamed
  `reply.txt`.

Reproduce with `benchmarks/openweight/setup.sh`, then `run-openweight.sh <out>`; the frontier
arm follows `benchmarks/runner/README.md`, Mode 2.
