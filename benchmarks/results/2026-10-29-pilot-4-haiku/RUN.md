# Pilot 4 (smaller model, `ui-spec`): RUN

**This is a pilot, not the kill-criterion run, and it declares no pass or fail.** Phase 0's gate has not been passed (mzizi-dev/mzizi#137). The plan is [`PLAN.md`](./PLAN.md). It repeats pilot 3 (`2026-10-29-pilot-3-frontier`) with a smaller model as the author, to see whether the ceiling seen there lifts.

Run on 2026-10-29 at `a816874` (`origin/staging`). Author: a fresh Claude Haiku subagent per episode (Mode 2, agent-driven; the exact model version is whatever the `haiku` alias resolved to, and the runner does not record it). Tasks: `button`, `badge`, `card` (public, `ui-spec` family). Arms: `mzizi` and `react`. 3 seeds per cell, at most 5 iterations. Raw episodes are in `raw/claude-haiku-subagent/`.

## Result

| Arm   | Task   | Episodes | Clean | Mean iterations to clean | Defects (total) |
| ----- | ------ | -------- | ----- | ------------------------ | --------------- |
| mzizi | button | 3        | 3/3   | 1.00                     | 0               |
| mzizi | badge  | 3        | 3/3   | 1.00                     | 0               |
| mzizi | card   | 3        | 3/3   | 1.00                     | 1               |
| react | button | 3        | 3/3   | 1.00                     | 0               |
| react | badge  | 3        | 3/3   | 1.00                     | 0               |
| react | card   | 3        | 3/3   | 1.00                     | 0               |

Facts checked per episode: button 9, badge 2, card 3.

The one defect: `mzizi` `card` seed 2 compiled clean but declared a single slot, `card`, where the spec lists seven (`card-action`, `card-content`, `card-description`, `card-footer`, `card-header`, `card-title` were missing). The compiler accepts that file; the scorer's `slot_set` fact catches it.

## What this does and does not show

- **It does not separate the arms, at this size either.** All 18 episodes were clean at iteration 1, and 17 had no defects. A Haiku author, like the pilot 3 Fable author, sits at the ceiling on these three public tasks. One defect in 9 `mzizi` episodes is not evidence either way. No test, interval or decision rule is applied.
- **A smaller Claude model is not the headline model.** The gating run uses the ~7B open-weight model (Qwen2.5-Coder-7B on llama.cpp), which pilot 2 found does worse on `mzizi` than on its incumbent. Nothing here speaks to it. It has not been run in this session.
- **It is not a kill-criterion result** (RFC-0009 §6): public tasks, no held-out set, no `backend` family.
- **The model shares a vendor with the language's author** (READINESS.md, threats to validity).
- **The token metric is not measured.** No llama.cpp `/tokenize` endpoint was available, so `transcript_tokens` and `guide_tokens` are null.
- **The Mode 2 rules are instructions, not enforcement.** Each author was told to read only its `prompt.md`; I cannot verify it did. Authors made 4 tool calls each.

## Departures

- `react/check.sh` calls `flock`, which macOS does not ship. A minimal `flock <fd>` shim (Python `fcntl.flock`, an exclusive lock on the inherited descriptor, the same semantics) was on `PATH` for every episode. Episodes of one arm ran in parallel; the lock serialises their checks.
- Dioxus and Leptos were not run (see `PLAN.md`).
- Raw files carry the local scratch paths the runner recorded; they are left as written. No email addresses (searched).
- `mzbench bundle-hash` was not computed: the layout here is `raw/`, not `episodes/`.

## What would make this informative

Held-out tasks of a difficulty where the arms differ, the 7B open-weight model as headline, the `backend` family, and a registered `PLAN.md` with the decision rule applied: the items in mzizi-dev/mzizi#137.
