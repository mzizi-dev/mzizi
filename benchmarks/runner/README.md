# `mzbench` — the Phase 0 benchmark runner

`mzbench` runs porting _episodes_: an author (a model or an agent) is given one registry React component (`spec.tsx`) and ports it to either Mzizi (`.mz`) or raw Dioxus (`.rs`). Each episode records three things: tokens consumed, iterations to a clean compile, and defects. A defect is a clean-compiling candidate that disagrees with the hand-written Rust reference. `mzbench summarize` aggregates the episodes. See `../../CHARTER.md` §4 and `../../MIGRATION.md` §4.2 for why these three.

The runner does not compile or score anything itself. It calls the shared interfaces: `mz check --agent` for Mzizi, `benchmarks/arms/dioxus/check.sh` for Dioxus, and `mzizi-benchmark-harness score` for both. Every one of them can be swapped for another command with `--check-cmd` / `--score-cmd`. The argv that actually ran is written into each episode's `meta.json`.

The task set is always a path argument (`--task <dir>`), never a baked-in location.

## Running it

Build once from the repo root (`cargo build --workspace`). This keeps the first episode's `check_ms` from including a compiler build. The binary is `target/debug/mzbench`, or `cargo run -q -p mzizi-benchmark-runner --bin mzbench -- …`.

Options shared by `run` and `episode start`:

- `--repo <dir>` — the repo root. Defaults to the one this binary was built from.
- `--guide <file>` — the system message. Defaults to `benchmarks/prompts/<arm>-guide.md`.
- `--check-cmd '<json argv>'` — replaces the compile check. Placeholder: `{file}`.
- `--score-cmd '<json argv>'` — replaces the scorer. Placeholders: `{arm}`, `{candidate}`, `{reference}`.

### Mode 1 — model-driven (llama.cpp)

```sh
mzbench run --task benchmarks/tasks/button --arm mzizi \
  --endpoint http://127.0.0.1:8080 --model-label qwen2.5-coder-1.5b-q4km \
  --seed 1 --temperature 0 --max-iters 5 --out results
```

Optional: `--max-tokens 4096` (per reply) and `--timeout-secs 3600` (per HTTP call). Both are recorded in `meta.json`.

The loop:

1. Call `POST /v1/chat/completions` with the full history so far, plus `seed`, `temperature` and `max_tokens`.
2. Extract the single fenced code block. The rule is in [What counts as an iteration](#what-counts-as-an-iteration).
3. Write the candidate and run the arm's check.
4. On errors, append the assistant reply and a user message with the diagnostics verbatim, then repeat, up to `--max-iters`.
5. Once a candidate compiles cleanly, score it. If none does, record `clean: false` and don't score.

### Mode 2 — agent-driven (Claude Code subagents)

Subagents can't be called over HTTP, so they drive the episode themselves:

```sh
EP=$(mzbench episode start --task benchmarks/tasks/button --arm dioxus \
       --model-label claude-subagent --out results [--seed 0] [--max-iters 5])
# give the subagent "$EP/prompt.md"; it writes a file and submits it:
mzbench episode submit "$EP" candidate.rs    # exit 0 CLEAN, 1 ERRORS, 2 refused
mzbench episode finish "$EP" [--endpoint http://127.0.0.1:8080]
```

- `start` writes `prompt.md`, which holds the exact system and user messages from the same builder Mode 1 uses. `system.txt` and `user.txt` are byte-exact copies. `start` prints the episode path.
- `submit` records the next iteration and runs the check. On errors it prints exactly the feedback message Mode 1 would send next, then `ITERATION n/k: CLEAN|ERRORS`. It refuses a submission after a clean one, after `max-iters`, or after `finish`, and says to run `finish`.
- `finish` scores the clean submission (if any), counts `transcript_tokens` via the endpoint's `/tokenize`, and writes the final line.

`--seed` in Mode 2 is only a replicate label for the directory name. There is nothing to seed.

### Mode 3 — summary

```sh
mzbench summarize results
```

This prints markdown tables per (model, arm) and per (model, arm, task). Every rate is printed as `num/den (pct)`, and every mean as `value (n=…)`. Episode directories without a final line (aborted runs, or unfinished agent episodes) are listed separately and are counted in no denominator.

## Output layout

```text
<out>/<model>/<arm>/<task>/seed-<n>/
  meta.json          task, arm, model, seed, temperature, max_iters, endpoint,
                     guide path/bytes/FNV-1a hash, enums, check+score argv
  system.txt user.txt prompt.md
  iter-NN/reply.md          raw model reply (Mode 1 only)
  iter-NN/candidate.mz|rs   the candidate as checked
  iter-NN/diagnostics.txt   the check's stdout (or the extraction error)
  iter-NN/stderr.txt        the check's stderr (never fed back)
  iter-NN/feedback.txt      the user message sent next (only if one was sent)
  score.json score.stderr.txt   the scorer's output, if a clean candidate was scored
  episode.jsonl
```

An existing episode directory is never overwritten. Use another `--seed` or `--out`.

`episode.jsonl` has one line per iteration:

```json
{"check_exit":1,"check_ms":30,"compile_ok":false,"completion_tokens":null,"diagnostics_chars":755,"extract_error":null,"gen_ms":null,"iter":1,"kind":"iteration","prompt_tokens":null}
```

It ends with a final line:

```json
{"arm":"mzizi","class_token_jaccard":null,"clean":true,"defects":null,"facts_checked":null,"iterations":2,"iterations_to_clean":2,"kind":"final","max_iters":3,"mode":"agent","model":"claude-subagent","score_error":"scorer exited Some(2)","scored":false,"seed":0,"task":"button","temperature":null,"token_source":"tokenizer_transcript_proxy","total_completion_tokens":null,"total_prompt_tokens":null,"transcript_tokens":1004,"transcript_tokens_null_reason":null}
```

Both lines above come from a real dry run: a scratch task, the real `mz check`, and the real endpoint's `/tokenize`. The harness's `score` subcommand did not exist yet, so that run is honestly recorded as clean but unscored.

## What each metric means, and where it comes from

| Field                                             | Meaning                                                                                                                                                                                                                           | Source                                                                                                            |
| ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `iter`                                            | 1-based attempt number                                                                                                                                                                                                            | runner                                                                                                            |
| `compile_ok`                                      | the check exited 0                                                                                                                                                                                                                | exit status of the arm's check                                                                                    |
| `iterations_to_clean`                             | `iter` of the first clean attempt; `null` if none within `max_iters`                                                                                                                                                              | runner                                                                                                            |
| `diagnostics_chars`                               | Unicode scalar count of the check's stdout, or of the extraction error                                                                                                                                                            | runner                                                                                                            |
| `check_ms`                                        | wall-clock time of the check command, including `cargo run` overhead; `null` if no candidate was checked                                                                                                                          | runner                                                                                                            |
| `gen_ms`                                          | wall-clock time of the chat call (Mode 1)                                                                                                                                                                                         | runner                                                                                                            |
| `prompt_tokens`, `completion_tokens`              | that call's usage, as the server reports it; `null` if the server didn't report it                                                                                                                                                | `usage` in the chat response                                                                                      |
| `total_prompt_tokens`, `total_completion_tokens`  | sum over iterations; `null` if any iteration lacked usage (never partially summed), and always `null` in Mode 2                                                                                                                   | runner                                                                                                            |
| `transcript_tokens`                               | tokens in: system + user message, every checked candidate, every feedback message actually sent                                                                                                                                   | endpoint `/tokenize`, same files, both modes; `null` plus `transcript_tokens_null_reason` if the endpoint is down |
| `token_source`                                    | `endpoint_usage` (Mode 1) or `tokenizer_transcript_proxy` (Mode 2) — which of the above is the episode's real count                                                                                                               | runner                                                                                                            |
| `defects`, `facts_checked`, `class_token_jaccard` | the scorer's numbers for the clean candidate                                                                                                                                                                                      | `mzizi-benchmark-harness score` stdout                                                                            |
| `renames`                                         | variants the scorer paired with a differently named reference variant by class string (only for a task with `allow_variant_renames`); `null` if the scorer did not report it (final lines written before 2026-09-27 lack the key) | `mzizi-benchmark-harness score` stdout                                                                            |
| `scored`, `score_error`                           | whether the scorer produced a usable object; if not, why                                                                                                                                                                          | scorer exit status and stdout                                                                                     |

Summary denominators:

- _Clean-compile rate_ is over finished episodes.
- _Iterations to clean_ (mean and median) is over clean episodes.
- _Token means_ are over episodes where the value is non-null.
- _Defect rate_ is clean episodes with at least one defect, divided by clean episodes that were **scored**.
- _Mean defects_ and _jaccard_ are over scored clean episodes.
- _Renamed variants_ is a total over scored clean episodes whose final line has `renames`; its `n` counts those episodes.

A clean episode whose scorer failed is neither defective nor defect-free. It is excluded from the defect rate and shown in its own "clean unscored" column. A candidate that never compiles is never scored, so it can never count as defect-free.

### What counts as an iteration

- Every authored attempt counts: a model reply in Mode 1, a `submit` in Mode 2.
- A reply with zero fenced code blocks, more than one, or an unclosed one is a failed iteration (`check_ms: null`, `extract_error` set), and the error is fed back. Prose around a single block is tolerated. The runner never picks "the first" or "the longest" block on the model's behalf.
- A check that exits with anything other than 0 or 1 (for example `check.sh`'s exit 2) is a harness setup error. It is not recorded as an iteration: the episode aborts in Mode 1, and the submission is rejected in Mode 2.
- After the last allowed iteration, nothing more is sent. Mode 2 doesn't print that iteration's feedback either, and it isn't counted in `transcript_tokens`.

## Fairness: what is identical across arms

- One prompt builder (`src/prompt.rs`) serves both arms and both modes. The two user messages differ only in the language name and file kind, and a unit test proves it.
- If the task's `task.toml` has `enums = ["ButtonVariant", …]`, the builder adds exactly one sentence naming them. It uses PascalCase for Dioxus and snake_case for Mzizi, plus that arm's class accessor: "`classes()` method" or "`class` column". The scorer keys facts by enum name, so without this sentence a model that named an enum `Size` would be scored on naming, not behaviour. The sentence deliberately says nothing about heights or any other scored fact. A test proves the two sentences are equal once naming is taken out.
- If the task's `task.toml` sets `allow_variant_renames = true` (it must then also give a `rename_reason`), the runner appends `--allow-variant-renames` to the scorer argv — the default one or a `--score-cmd` override — and records `allow_variant_renames` and `rename_reason` in `meta.json`. It is a property of the task, so it applies to every arm alike. Only `nyuchi-changelog-renderer` sets it; see [`../tasks/README.md`](../tasks/README.md).
- The feedback text is arm-independent, and the diagnostics are appended verbatim.
- Both arms use the same extraction rule, the same `max_iters` and the same scorer invocation.

## Known asymmetries (recorded, not hidden)

1. **Token counts are different measurements in the two modes.** Mode 1's `total_*_tokens` are exact server usage from llama.cpp. They include the whole history re-sent on every call, because that is what the model consumed. Mode 2's subagent usage is not observable, so its only count is `transcript_tokens`. This proxy is tokenized with the _endpoint's_ tokenizer (the local model's, not the frontier model's). It excludes the subagent's own reasoning, tool calls, harness system prompt and any files it reads. It _undercounts_ what the agent really spent. `transcript_tokens` is computed the same way in both modes, and it is the only column comparable across them. Never compare Mode 1 endpoint totals against Mode 2 transcript counts.
2. **Each arm's guide is part of its prompt cost.** The system message is the guide, verbatim. A longer Mzizi guide makes every Mzizi episode cost more tokens, and that is intended: a language that needs more explanation to be usable pays for it. `meta.json` records `guide_bytes` and `guide_fnv1a64`, so results from different guide versions can't be silently pooled.
3. **Mode 2 cannot enforce its own rules.** The runner can't stop a subagent from running `mz check` or `cargo` outside `submit`, which would hide iterations. It can't stop it reading `reference.rs` in the task directory, or `meta.json`'s paths either. The orchestrating prompt must forbid both, and give the subagent only `prompt.md` and the `submit` command. `meta.json` records `"iteration_count_enforced_by_runner": false` for these episodes.
4. **Mode 2 has no sampling controls.** `temperature` is `null`, and `seed` is only a label.
5. **The two arms use different compile checks.** `mz check` is a single-file front end; `check.sh` is a cargo build. `check_ms` is each arm's real loop latency, including process and cargo overhead, so it is not a like-for-like compiler comparison.
6. **The enum sentence's accessor phrase is arm-specific**, as described in the fairness section.

## Dependencies

`ureq` (with default features off: plain HTTP only) and `serde_json` are pinned exactly in `Cargo.toml`, which also explains why each is allowed. The repo's `.gitignore` excludes `Cargo.lock`, so their transitive dependencies are not pinned.

## Tests

`cargo test -p mzizi-benchmark-runner` needs no network and no server. The chat model and the tokenizer are traits with in-process fakes. The check and score commands are `sh -c` fakes injected through the same template mechanism that `--check-cmd` / `--score-cmd` use. The tests cover:

- code-block extraction: several reply shapes, a missing block, two blocks, and truncation;
- prompt parity between arms, with and without `enums`;
- the submit/finish state machine: refusals, `max_iters`, a scorer failure, a setup error, and the tokenizer being down;
- the JSONL shape of iteration and final lines, through the full Mode 1 loop;
- summary math, including zero-clean and clean-but-unscored edge cases.
