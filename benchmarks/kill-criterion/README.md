# The kill-criterion run: scaffolding and plan

This directory holds the two scripts the Phase 0 gating runs use, and the plan for the
held-out task set they run on. **No gating run has happened.**
[`../READINESS.md`](../READINESS.md) says what is done, what is still open, and the exact
command.

The rules are not restated here. They are in the RFCs, which own them:

- **The kill criterion:** [RFC-0009](../../design/RFC-0009-comparison-benchmark.md) §6.
  "Pre-registered settings" below says how a run fixes it before any held-out score is seen.
- **Publication:** RFC-0009 §7, and RFC-0004 §4.2 for what a held-out run publishes on the
  day and what waits until its task set retires.
- **Where held-out tasks live:** a private `mzizi-dev` repository, per RFC-0004 §6.1. It is
  created when the first held-out task exists, and it does not exist yet.

## The scripts

- **`check-task.sh <task dir> [--min-facts 3]`** tells a task author whether a task is ready.
  The fixture format must be complete, `mzbench` must load the task, and the task's reference,
  scored against itself with the task's own flags, must check at least `--min-facts` facts
  with zero defects. It prints fact counts and never prints a task's text, so it is safe to
  run on a held-out task.
- **`run.sh --tasks <dir> --out <dir> --model-label <label> [...]`** drives one model
  endpoint (`mzbench` Mode 1). It runs every task in the set, on every arm (default
  `mzizi dioxus leptos`), for every seed (default `1 2 3 4 5`), at temperature 0.7 by default.
  It refuses to start unless a `PLAN.md` exists in `<out>` or in the results directory above
  it, because RFC-0009 §7.2 registers the plan before the first episode (`--dry-run` skips
  this check). It checks every task, then writes `<out>/manifest.json` (the repo commit, the
  settings, the context size the server reports, and each task's name and the SHA-256 of its
  files), runs the episodes into `<out>/episodes`, and writes `<out>/summary.md`. Last,
  it writes `<out>/bundle.sha256`: the SHA-256 of the sorted list of every episode file's own
  SHA-256 and path. That is the raw-bundle hash a held-out run publishes on the day
  (RFC-0004 §4.2), and anyone can recompute it from the bundle once the set retires.
  `--dry-run` checks the tasks and writes the manifest, and runs no episode.

A frontier model reached through an agent runtime has no HTTP endpoint for `run.sh`. It runs
in Mode 2 instead (`../runner/README.md`), with the same tasks, arms and seeds, into a sibling
directory. RFC-0009 §5 reports it beside the headline and never pools the two.

## The held-out tasks

**Format.** The same fixture format as `../tasks/` (see its README): `task.toml`, the spec and
`reference.rs`, with `[source]` provenance. For the gating `ui-spec` family the spec is
RFC-0009 §2.1's language-neutral `spec.md`, and the runner does not read that yet (RFC-0009
§9, step 1). `score_slots = true` is recommended for any task whose reference renders more
than one `data-slot`. `check-task.sh` must print `READY` with at least 3 facts.

**What makes a task fresh.** Its spec and its reference have never been published anywhere.
A registry component that was later ported does not count, because both halves are public.
A held-out task is a new component, or a new behaviour of one, written for the set. The
references are hand-written, in the registry's `enum` / `classes()` / `data-slot`
conventions, so that the extractor can read them.

**Who writes them:** (a) maintainers by hand, or (b) another vendor's model drafts them and a
person reviews each one; not Claude-drafted. The owner is to confirm. The reason for
excluding Claude is pilot 2's shared-author threat: the RFCs and guides were written by the
same model family that is the frontier arm. The author and date go in each task's `[source]` table, in the
private repository.

**Size.** Pilot 2 had 2 scored tasks and 11 facts, and one episode moved a rate by 17 points.
The plan for each gating family is at least 10 held-out tasks with at least 3 scoreable facts
each (30 or more facts), and 5 seeds on every arm. The exact n goes in `PLAN.md`.

## The pre-registered settings

`PLAN.md` fixes these, together with RFC-0009 §6.2's rule and its n, **before any held-out
score is seen**, and a run that departs from them says so in its `RUN.md`:

- **The decision rule** (RFC-0009 §6.2). Within each gating task family, and for each metric
  (tokens, iterations to a clean check, defect rate), the bar is the best value any incumbent
  arm in that family reached. Mzizi passes the family when it beats that bar on at least two
  of the three metrics, on the headline ~7B model, on held-out tasks. A win on a metric
  counts only if a paired bootstrap that resamples tasks, over (task, seed) pairs, gives a
  95% interval for the difference that excludes zero in Mzizi's favour.
- **Temperature and seeds:** 0.7 with seeds 1–5, plus one seed at 0.2 as a check against the
  pilots, which ran at 0.2. At 0.2 the seeds reproduced first replies, so three seeds were
  close to one sample. `run.sh` defaults to 0.7.
- **Context:** `-c 32768` for the 7B server (`CTX=32768 ../openweight/setup.sh launch`).
  `run.sh` records the server's reported context in `manifest.json` as `server_n_ctx`, and
  `RUN.md` states it. Fall back to `-c 16384` only if the machine's RAM cannot hold 32,768,
  and record the fallback and the reason in `RUN.md`. Pilot 2 ran at 16,384, and two 7B Mzizi
  episodes overflowed it. An episode like that now ends with `ended_by: context_exceeded` and
  counts as not clean.
