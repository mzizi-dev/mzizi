# `mzbench` — the Phase 0 benchmark runner

`mzbench` runs _episodes_: an author (a model or an agent) is given one task's input and writes one file in one arm's language. For the `ui-port` family (the default, and what the pilots ran) the input is the registry React component, `spec.tsx`. For `ui-spec` it is `spec.md`, the same component stated without any language's idiom (RFC-0009 §2.1). An arm is a language with a framework, declared in `benchmarks/arms/<id>/arm.toml` (see [Arms](#arms-armtoml)): today `mzizi`, `dioxus` and `leptos`. Each episode records three things: tokens consumed, iterations to a clean compile, and defects. A defect is a clean-compiling candidate that disagrees with the hand-written Rust reference. `mzbench summarize` aggregates the episodes. See `../../CHARTER.md` §4 and `../../MIGRATION.md` §4.2 for why these three.

The runner does not compile or score anything itself. It calls the arm's `check` command from its `arm.toml` (`mz check --agent` for Mzizi, `benchmarks/arms/<id>/check.sh` for Dioxus and Leptos), and `mzizi-benchmark-harness score` with the arm's `extractor`. Every one of them can be swapped for another command with `--check-cmd` / `--score-cmd`. The argv that actually ran is written into each episode's `meta.json`.

The task set is always a path argument (`--task <dir>`), never a baked-in location.

## Running it

Build once from the repo root (`cargo build --workspace`). This keeps the first episode's `check_ms` from including a compiler build. The binary is `target/debug/mzbench`, or `cargo run -q -p mzizi-benchmark-runner --bin mzbench -- …`.

Options shared by `run` and `episode start`:

- `--arm <id>` — the arm, loaded from `<repo>/benchmarks/arms/<id>/arm.toml`.
- `--family <ui-port|ui-spec|backend>` — the task family, which picks the input: `spec.tsx` for `ui-port` (the default), `spec.md` otherwise. An arm refuses a family its `arm.toml` does not list, and a task without that input is refused. Recorded in `meta.json` as `task_family`.
- `--repo <dir>` — the repo root. Defaults to the one this binary was built from.
- `--guide <file>` — the system message. Defaults to the arm's `guide`.
- `--check-cmd '<json argv>'` — replaces the compile check. Placeholder: `{file}`.
- `--score-cmd '<json argv>'` — replaces the scorer. Placeholders: `{arm}`, `{candidate}`, `{reference}`.
- `--normalise <file-name|none>` — what is done to the check's stdout before it is recorded and fed back. The default, `file-name`, replaces the candidate's path, as the check was passed it, with its bare file name. It applies to every arm. `none` feeds the output back exactly as printed. Recorded in `meta.json` as `diagnostics_normaliser`.

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

### Registering and publishing a run (RFC-0009 §7)

```sh
mzbench plan --out results/<date>-<name> --tasks <task-set dir> --arms 'mzizi dioxus leptos' \
  --family ui-spec --model-label '<label> ...' --seeds '1 2 3 4 5' --temperature 0.7 \
  [--max-iters 5] [--max-tokens 4096] [--n-ctx 32768] [--held-out true]
mzbench bundle-hash results/<date>-<name>
```

- `plan` drafts `<out>/PLAN.md` from the files on disk. It records the commit (and says so if the tree is dirty), the scorer's version (the SHA-256 of `benchmarks/harness/src`), and each arm's check, guide bytes, guide SHA-256 and pin files' SHA-256. It also records each task's content hash (the SHA-256 of its files' `sha256sum` listing), the models, seeds and budget, the episode count, and, for a gating family, RFC-0009 §6.2's rule with its n. With `--held-out true` the tasks are named by hash only. The parts only a person can write are left as **To fill**. It refuses to overwrite an existing `PLAN.md`: a registered plan is never rewritten. `kill-criterion/run.sh` refuses to start without one.
- `bundle-hash` prints the raw-bundle hash of `<out>/episodes`: the SHA-256 of the `sha256sum` listing of every file in it, sorted by path in byte order. It equals `find episodes -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum | sha256sum`, and a test checks the two agree, so anyone can recompute a published bundle's hash with coreutils. SHA-256 is implemented in `src/sha256.rs` and checked against the FIPS 180 test vectors.

## Output layout

```text
<out>/<model>/<arm>/<task>/seed-<n>/
  meta.json          task, arm, model, seed, temperature, max_iters, endpoint,
                     guide path/bytes/FNV-1a hash, enums, check+score argv,
                     diagnostics_normaliser
  system.txt user.txt prompt.md
  iter-NN/reply.md          raw model reply (Mode 1 only)
  iter-NN/candidate.mz|rs   the candidate as checked
  iter-NN/diagnostics.txt   the check's stdout after the normaliser (or the extraction error)
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

| Field                                             | Meaning                                                                                                                                                                                                                                           | Source                                                                                                            |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `iter`                                            | 1-based attempt number                                                                                                                                                                                                                            | runner                                                                                                            |
| `compile_ok`                                      | the check exited 0                                                                                                                                                                                                                                | exit status of the arm's check                                                                                    |
| `iterations_to_clean`                             | `iter` of the first clean attempt; `null` if none within `max_iters`                                                                                                                                                                              | runner                                                                                                            |
| `diagnostics_chars`                               | Unicode scalar count of the check's stdout, or of the extraction error                                                                                                                                                                            | runner                                                                                                            |
| `check_ms`                                        | wall-clock time of the check command, including `cargo run` overhead; `null` if no candidate was checked                                                                                                                                          | runner                                                                                                            |
| `gen_ms`                                          | wall-clock time of the chat call (Mode 1)                                                                                                                                                                                                         | runner                                                                                                            |
| `prompt_tokens`, `completion_tokens`              | that call's usage, as the server reports it; `null` if the server didn't report it                                                                                                                                                                | `usage` in the chat response                                                                                      |
| `total_prompt_tokens`, `total_completion_tokens`  | sum over iterations; `null` if any iteration lacked usage (never partially summed), and always `null` in Mode 2                                                                                                                                   | runner                                                                                                            |
| `transcript_tokens`                               | tokens in: system + user message, every checked candidate, every feedback message actually sent                                                                                                                                                   | endpoint `/tokenize`, same files, both modes; `null` plus `transcript_tokens_null_reason` if the endpoint is down |
| `token_source`                                    | `endpoint_usage` (Mode 1) or `tokenizer_transcript_proxy` (Mode 2) — which of the above is the episode's real count                                                                                                                               | runner                                                                                                            |
| `defects`, `facts_checked`, `class_token_jaccard` | the scorer's numbers for the clean candidate                                                                                                                                                                                                      | `mzizi-benchmark-harness score` stdout                                                                            |
| `renames`                                         | variants the scorer paired with a differently named reference variant by class string (only for a task with `allow_variant_renames`); `null` if the scorer did not report it (final lines written before 2026-09-27 lack the key)                 | `mzizi-benchmark-harness score` stdout                                                                            |
| `guide_tokens`                                    | tokens in the system message (the guide) alone, by the same `/tokenize` as `transcript_tokens`; the number RFC-0009 §4.2 holds within ±5% across a family. `null` if the tokenizer is down, and absent from final lines written before 2026-09-29 | endpoint `/tokenize`                                                                                              |
| `ended_by`                                        | `context_exceeded` when a Mode 1 episode stopped because its history outgrew the model's context; `null` otherwise (and absent from final lines written before 2026-09-29)                                                                        | runner                                                                                                            |
| `scored`, `score_error`                           | whether the scorer produced a usable object; if not, why                                                                                                                                                                                          | scorer exit status and stdout                                                                                     |

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
- In Mode 1, if a request after the first fails because the conversation no longer fits the model's context (llama.cpp's `exceed_context_size_error`, or an OpenAI-style `context_length_exceeded`), the episode finishes as **not clean**, with `"ended_by": "context_exceeded"` on its final line. The edit loop failed to converge within the model's context, so the model failed, not the harness. Pilot 2's runner left that episode unfinished, and its write-up counted it as not clean by hand. If the very first request does not fit, the prompt alone is too big for the server. That is a setup problem, and the episode aborts. Every other endpoint error aborts too.
- After the last allowed iteration, nothing more is sent. Mode 2 doesn't print that iteration's feedback either, and it isn't counted in `transcript_tokens`.

## Fairness: what is identical across arms

- One prompt builder (`src/prompt.rs`) serves every arm, every family and both modes. Within a family, arms' user messages differ only in the arm's `language_name`, `file_kind`, and the naming sentence's enum names and class accessor. A unit test proves it for every pair of arms in `benchmarks/arms/`. Another rebuilds the user messages pilot 2 committed and compares bytes, so the `ui-port` wording is the pilots'.
- If the task's `task.toml` has `enums = ["ButtonVariant", …]`, the builder adds exactly one sentence naming them. It writes the names in the arm's `enum_case` (PascalCase for Dioxus and Leptos, snake_case for Mzizi), followed by the arm's `class_accessor`: "`classes()` method" or "`class` column". The scorer keys facts by enum name, so without this sentence a model that named an enum `Size` would be scored on naming, not behaviour. The sentence deliberately says nothing about heights or any other scored fact. A test proves the two sentences are equal once naming is taken out.
- If the task's `task.toml` sets `allow_variant_renames = true` (it must then also give a `rename_reason`), the runner appends `--allow-variant-renames` to the scorer argv — the default one or a `--score-cmd` override — and records `allow_variant_renames` and `rename_reason` in `meta.json`. It is a property of the task, so it applies to every arm alike. Only `mzizi-changelog-renderer` sets it; see [`../tasks/README.md`](../tasks/README.md).
- If the task's `task.toml` sets `score_slots = true`, the runner appends `--slots` to the scorer argv the same way, and the scorer adds one `slot_set` fact: the port's `data-slot` values against the reference's. Only `card` sets it; see [`../tasks/README.md`](../tasks/README.md).
- The feedback text is arm-independent, and the diagnostics are appended verbatim after one arm-independent normalisation: the candidate's path becomes its file name. Before 2026-09-29 there was no normaliser, and `mz check --agent` echoed the absolute path on every diagnostic while the Dioxus `check.sh` printed the bare name. Pilot 2 measured that at about 13,500 tokens across the 7B model's five finished Mzizi episodes ([`../results/2026-09-27-pilot-2/RUN.md`](../results/2026-09-27-pilot-2/RUN.md)).
- Both arms use the same extraction rule, the same `max_iters` and the same scorer invocation.

## Known asymmetries (recorded, not hidden)

1. **Token counts are different measurements in the two modes.** Mode 1's `total_*_tokens` are exact server usage from llama.cpp. They include the whole history re-sent on every call, because that is what the model consumed. Mode 2's subagent usage is not observable, so its only count is `transcript_tokens`. This proxy is tokenized with the _endpoint's_ tokenizer (the local model's, not the frontier model's). It excludes the subagent's own reasoning, tool calls, harness system prompt and any files it reads. It _undercounts_ what the agent really spent. `transcript_tokens` is computed the same way in both modes, and it is the only column comparable across them. Never compare Mode 1 endpoint totals against Mode 2 transcript counts.
2. **Each arm's guide is part of its prompt cost.** The system message is the guide, verbatim. `meta.json` records `guide_bytes` and `guide_fnv1a64`, and the final line `guide_tokens`, so results from different guide versions can't be silently pooled. Until 2026-09-29 the stance was that a language needing more explanation pays for it in tokens. RFC-0009 §4.2 replaces that: every guide in a family is written to one token budget, within ±5%, so a language that needs more explanation pays in iterations and defects instead.
3. **Mode 2 cannot enforce its own rules.** The runner can't stop a subagent from running `mz check` or `cargo` outside `submit`, which would hide iterations. It can't stop it reading `reference.rs` in the task directory, or `meta.json`'s paths either. The orchestrating prompt must forbid both, and give the subagent only `prompt.md` and the `submit` command. `meta.json` records `"iteration_count_enforced_by_runner": false` for these episodes.
4. **Mode 2 has no sampling controls.** `temperature` is `null`, and `seed` is only a label.
5. **The two arms use different compile checks.** `mz check` is a single-file front end; `check.sh` is a cargo build. `check_ms` is each arm's real loop latency, including process and cargo overhead, so it is not a like-for-like compiler comparison.
6. **The enum sentence's accessor phrase is arm-specific**, as described in the fairness section.

## Arms: `arm.toml`

_Specified 2026-09-29 (RFC-0009 §1, §9 step 1). This section is the format's contract. Other
work, such as RFC-0009 §6.4's `mzizi-be` arm, reads it. Change it only in a commit that says
so._

An arm is one language with one framework. The runner knows arms only as data: each is one
file, `benchmarks/arms/<id>/arm.toml`, and `--arm <id>` loads it from the repo given by
`--repo`. Nothing else in the runner is arm-specific. An arm is these things and nothing
else:

1. **A guide**, the system message, verbatim. Its size is part of the arm's token cost
   (RFC-0009 §4.2 rebalances every guide in a family to one budget).
2. **A check command.** It takes the candidate's path as `{file}`. It exits 0 for a clean
   candidate, 1 for one with errors, and anything else for a setup problem, which is never
   counted as an iteration.
3. **A diagnostic normaliser**, applied to the check's stdout before the author sees it.
4. **A file layout**: the candidate's file name, and the prompt phrases that name it.
5. **A scorer extractor**: the `--arm` value passed to `mzizi-benchmark-harness score`.

### The file

TOML, read by the runner's own small reader (`src/toml_lite.rs`): top-level keys only,
before any `[table]` header. Values are basic (`"…"`) or literal (`'…'`) strings,
`true`/`false`, or arrays of strings, which may span lines and end with a trailing comma.
`#` starts a comment outside a string. **An unknown key is an error**, so a misspelt key
cannot be silently ignored. Tables after the top-level keys (`[source]`, `[pins]` notes and
so on) are not read and may hold anything.

| Key              | Required             | Value                                                                                                                                                                                                                                                                                                                                                            |
| ---------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `id`             | yes                  | Must equal the directory name. `[a-z0-9-]+`. It names the arm everywhere: `--arm`, episode directories, `meta.json`, the final line.                                                                                                                                                                                                                             |
| `family`         | yes                  | `"ui"` or `"backend"`: which kind of task the arm writes (RFC-0009 §1's table).                                                                                                                                                                                                                                                                                  |
| `task_families`  | yes                  | The task families (RFC-0009 §2) the arm runs, from `"ui-port"`, `"ui-spec"` and `"backend"`. The runner refuses an episode in a family the arm does not list, so a React arm cannot be handed `spec.tsx` (BM-1).                                                                                                                                                 |
| `language_name`  | yes                  | The language as the user message names it (`"Mzizi"`, `"Dioxus"`, `"TypeScript with React"`).                                                                                                                                                                                                                                                                    |
| `file_kind`      | yes                  | What the one reply file is: `"a single Mzizi source file (.mz)"`. It should contain `language_name`, or name the language some other way.                                                                                                                                                                                                                        |
| `extension`      | yes                  | The candidate's extension, without the dot. **File layout:** each iteration's candidate is written to `iter-NN/candidate.<extension>` and the check is passed that path. One file per reply. A checker that needs another name or a project around it (`main.go`, `src/component.rs`) copies the candidate into its own sandbox, as `arms/dioxus/check.sh` does. |
| `guide`          | yes                  | The guide's path, relative to the repo root (`"benchmarks/prompts/mzizi-guide.md"`). `--guide` overrides it for one run.                                                                                                                                                                                                                                         |
| `check`          | yes                  | The check argv. `{repo}` is replaced with the repo root when the arm loads, and `{file}` with the candidate's path per iteration. It runs with the repo root as its working directory. `--check-cmd` overrides it for one run.                                                                                                                                   |
| `normaliser`     | no                   | `"file-name"` (the default: the candidate's path, as the check was passed it, becomes its bare file name) or `"none"`. `--normalise` overrides it.                                                                                                                                                                                                               |
| `extractor`      | yes                  | The scorer's `--arm` value: `"mzizi"`, `"dioxus"` (any Rust `enum` / `classes()` file, which is why Leptos uses it) or `"react"` (a `cva(…)` call). `"none"` records every clean episode as clean but unscored, which is never counted as defect-free.                                                                                                           |
| `enum_case`      | when `family = "ui"` | How a task's `enums = [...]` (Rust PascalCase) are written in this arm's naming sentence: `"pascal"` (`ButtonSize`), `"snake"` (`button_size`) or `"cva"` (`buttonVariants.size`: the enum name's last word is the `cva` variant key, and the words before it name the `cva` call).                                                                              |
| `class_accessor` | when `family = "ui"` | The phrase after the enum names in that sentence, saying how the arm exposes each variant's Tailwind classes (for Mzizi, each with a `class` column).                                                                                                                                                                                                            |
| `pins`           | no                   | Files, relative to the arm directory, whose SHA-256 identifies the arm's toolchain pin (`["sandbox/Cargo.toml", "sandbox/Cargo.lock"]`). `mzbench plan` records each hash, so a run's plan names the exact pin it ran against (RFC-0009 §7.2).                                                                                                                   |

A worked example, the Dioxus arm as migrated:

```toml
id = "dioxus"
family = "ui"
task_families = ["ui-port", "ui-spec"]
language_name = "Dioxus"
file_kind = "a single Rust source file (.rs) using Dioxus"
extension = "rs"
guide = "benchmarks/prompts/dioxus-guide.md"
check = ["{repo}/benchmarks/arms/dioxus/check.sh", "{file}"]
normaliser = "file-name"
extractor = "dioxus"
enum_case = "pascal"
class_accessor = "each with a `classes()` method returning its Tailwind classes"
pins = ["sandbox/Cargo.toml", "sandbox/Cargo.lock"]
```

### What the runner does with it

- **The user message** is built by one function for every arm and family. The arms'
  messages differ only in `language_name`, `file_kind`, and the naming sentence's enum names
  and `class_accessor`. A test loads every `arms/*/arm.toml`, builds each family's message for
  every pair of arms, and checks they are equal once those phrases are replaced by
  placeholders (RFC-0009 §4.1).
- **`meta.json`** records the arm id as `arm` and the whole resolved file as `arm_config`, so
  `episode submit` and `finish` never re-read `arm.toml`, and an episode says exactly what it
  ran with. A `meta.json` written before `arm.toml` existed has no `arm_config`. Its arm is
  then read from the three arms that existed then, with the values they had.
- **The checker runs only in benchmark runs.** The runner's tests inject fakes through the
  same `check` mechanism. No arm's toolchain is a CI requirement (RFC-0009 §9).

### Adding an arm

1. Write `benchmarks/arms/<id>/arm.toml`, the guide, and a checker that keeps the 0/1/2 exit
   contract. Pin its toolchain in files the arm lists under `pins`, with their provenance in
   the arm's README, as `arms/dioxus/README.md` does.
2. Run `cargo test -p mzizi-benchmark-runner`. The parity test picks the new arm up with no
   code change, and fails if its message differs from the others by more than its naming.
3. Scoring is separate. A UI arm needs an extractor in `benchmarks/harness/` that reads the
   same facts (RFC-0009 §2.2). A backend arm is scored by probes (RFC-0009 §2.3), which do
   not exist yet. Until then its episodes are clean but unscored.

## Dependencies

`ureq` (with default features off: plain HTTP only) and `serde_json` are pinned exactly in `Cargo.toml`, which also explains why each is allowed. The repo's `.gitignore` excludes `Cargo.lock`, so their transitive dependencies are not pinned.

## Tests

`cargo test -p mzizi-benchmark-runner` needs no network and no server. The chat model and the tokenizer are traits with in-process fakes. The check and score commands are `sh -c` fakes injected through the same template mechanism that `--check-cmd` / `--score-cmd` use. The tests cover:

- code-block extraction: several reply shapes, a missing block, two blocks, and truncation;
- prompt parity between arms, with and without `enums`;
- the submit/finish state machine: refusals, `max_iters`, a scorer failure, a setup error, and the tokenizer being down;
- the JSONL shape of iteration and final lines, through the full Mode 1 loop;
- summary math, including zero-clean and clean-but-unscored edge cases.
