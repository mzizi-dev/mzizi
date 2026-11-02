# Pilot 3 (frontier model, `ui-spec`): RUN

**This is a pilot, not the kill-criterion run, and it declares no pass or fail.** Phase 0's gate has not been passed (mzizi-dev/mzizi#137). The plan is [`PLAN.md`](./PLAN.md); it was committed-in-form before the first episode and not changed after.

Run on 2026-10-29 at `a816874` (`origin/staging`), clean tree. Author: a fresh Claude Fable subagent per episode (Mode 2, agent-driven). Tasks: `button`, `badge`, `card` (public, `ui-spec` family). Arms: `mzizi`, `react`, and `dioxus` (partly; see below). 3 seeds per cell, at most 5 iterations. Raw episodes are in `raw/claude-fable-subagent/`.

## Result

| Arm    | Task   | Episodes | Clean | Mean iterations to clean | Defects (total) |
| ------ | ------ | -------- | ----- | ------------------------ | --------------- |
| mzizi  | button | 3        | 3/3   | 1.00                     | 0               |
| mzizi  | badge  | 3        | 3/3   | 1.00                     | 0               |
| mzizi  | card   | 3        | 3/3   | 1.33                     | 1               |
| react  | button | 3        | 3/3   | 1.00                     | 0               |
| react  | badge  | 3        | 3/3   | 1.00                     | 0               |
| react  | card   | 3        | 3/3   | 1.00                     | 0               |
| dioxus | button | 3        | 3/3   | 1.00                     | 0               |

`defects` is the scorer's count of facts on which a clean-compiling candidate disagrees with the hand-written reference. Facts checked per episode: button 9, badge 2, card 3.

The two `mzizi` `card` deviations:

- `seed-1` compiled clean in 1 iteration but added a `card-skeleton` slot the spec does not list (1 defect: `slot_set`).
- `seed-2` failed iteration 1 (`MZ0712`: attributes inside a `when`, which holds elements only) and was clean at iteration 2.

## What this does and does not show

- **It does not separate the arms.** 26 of 27 episodes were clean at iteration 1 with no defects. At this model size on these three public tasks the metrics sit at the ceiling, so a difference of one defect and one iteration in 9 `mzizi` `card` episodes is not evidence either way. No test, interval or decision rule is applied.
- **It is not a Mzizi-versus-X result for the kill criterion** (RFC-0009 §6): the tasks are public, there is no held-out set, no 7B headline model, and the `backend` family has not run.
- **Same author as the language.** The frontier model and the language guides share an author (READINESS.md, threats to validity).
- **The token metric is not measured.** No llama.cpp `/tokenize` endpoint was available, so `transcript_tokens` and `guide_tokens` are null on every final line.
- **The Mode 2 rules are instructions, not enforcement** (`iteration_count_enforced_by_runner: false`). Each author was told to read only its `prompt.md`; I cannot verify it did. Authors made 4 to 7 tool calls each.

## Departures and incidents

- `react/check.sh` and `dioxus/check.sh` call `flock`, which macOS does not ship. A minimal `flock <fd>` shim (Python `fcntl.flock`, exclusive lock on the inherited descriptor, the same semantics) is on `PATH` for every episode. The first two attempts at `react/button/seed-1` and `dioxus/button/seed-1` stopped at that setup error before any check ran; the authors had written their candidates without feedback, and those same files were submitted as iteration 1 after the shim was added.
- Dioxus is not run beyond `button` seeds 1-3 (owner direction: Mzizi Roots, no Dioxus; components come from the registry). The `dioxus` `badge` and `card` episodes were created by `mzbench episode start` and never run; they have no iterations and no final line.
- **My error, reverted:** I ran `mzbench episode finish` on `mzizi/card/seed-2` while its author was still working (after a failed iteration 1, `MZ0712`). That appended a premature `final` line (clean=false, 1 iteration). I removed that single line, restoring the file to its state after iteration 1, so the author could continue; nothing else was changed. The episode's recorded iterations are the author's real ones. Lesson: finish an episode only after its author has reported.
- `mzizi/card/seed-2`: the author's corrected candidate (written from the real iteration-1 feedback) was refused because of my premature finish; after the revert above I submitted that same file as iteration 2.
- **Dioxus.** Run for `button` only (3 episodes). Dioxus is not the direction for new work (Mzizi Roots; components live in the registry), so the `badge` and `card` Dioxus episodes were created by `mzbench episode start` and never run. Their directories contain only the prompt files. `leptos` was not run (its sandbox needs its crates fetched and it has never run end to end).
- **Raw files** carry the local scratch paths the runner recorded; they are left as written.
- **Bundle hash** (`mzbench bundle-hash`) was not computed: the runner's layout here is `raw/`, not `episodes/`.

## What would make this informative

Held-out tasks of a difficulty where the arms differ, the 7B open-weight model as headline, the `backend` family, and a registered `PLAN.md` with the decision rule applied: the items in mzizi-dev/mzizi#137.
