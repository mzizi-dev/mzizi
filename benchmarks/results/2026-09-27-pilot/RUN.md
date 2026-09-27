# 2026-09-27 pilot — first scored `mzbench` run

**This is a pilot. It is not the Phase 0 number, and it does not open the Phase 0 gate.**
Three tasks, one seed, one frontier model, no small-model arm, and a Mzizi checker that
cannot fail on names or types. Read [the caveats](#caveats-read-these-before-the-numbers)
before the numbers.

## What was run

| Field       | Value                                                                                         |
| ----------- | --------------------------------------------------------------------------------------------- |
| Date        | 2026-09-27                                                                                    |
| Code        | `ded425abc76497ceb27612ca14ffce1202618e4d` (`main` at the time)                               |
| Runner      | `mzbench` (`benchmarks/runner`), Mode 2 (agent-driven: `episode start` / `submit` / `finish`) |
| Model label | `claude-subagent`                                                                             |
| Tasks       | `button`, `badge`, `nyuchi-changelog-renderer`                                                |
| Arms        | `mzizi`, `dioxus`                                                                             |
| Seed        | `0` (in Mode 2 a replicate label only; there is nothing to seed)                              |
| `max_iters` | 5 (the default)                                                                               |
| Tokenizer   | none running; every token field is `null` (see caveats)                                       |

`meta.json` records `"iteration_count_enforced_by_runner": false`, as it does for every
Mode 2 episode: the runner cannot see what an agent does outside `submit`
([runner README](../../runner/README.md), known asymmetry 3).

### Protocol

For each of the six episodes, the orchestrator ran `episode start` and gave a separate
Claude Code subagent exactly two things: the system message and the user message from that
episode's generated `prompt.md`. The subagent was told to act as the benchmarked author,
with no tool use and no file or repository access, and to reply with one fenced code block.
The orchestrator transcribed that block into a file, then ran `episode submit` and
`episode finish`.

The author was a frontier Claude model. Its exact model identifier is deliberately not
recorded in this repository; `claude-subagent` is the label the results carry.

### Deviation from the protocol

The `button` / `mzizi` subagent's first reply was a prose summary of a component, not the
code. It was re-asked once for the verbatim file text. No candidate was submitted from the
first reply, so it is not counted as an iteration, but it is an extra author turn that the
iteration count does not show.

### Transcription, and the hash check

Each subagent's code block was transcribed into a file (one trailing newline). To commit
the run as a durable record, the six candidate texts were re-transcribed into these episode
directories on 2026-09-27, and each file's SHA-256 was compared against the original's.
**All six match**, so the committed candidates are byte-identical to the ones the pilot
scored.

| Candidate                                           | SHA-256                                                            | Match |
| --------------------------------------------------- | ------------------------------------------------------------------ | ----- |
| `dioxus/badge/.../candidate.rs`                     | `d912e81275e2c59c763cbaf9cf04132016f7fdb507b485795adf89377f3b15ea` | yes   |
| `dioxus/button/.../candidate.rs`                    | `2e579d7d25e8ddf687664f1bc755b2a7576f65e6302a3c263d0df65a062d559e` | yes   |
| `dioxus/nyuchi-changelog-renderer/.../candidate.rs` | `afc1674e7c270c4fe3fb1244e53758b4b873a25cd765699151cef6f36f8f8dde` | yes   |
| `mzizi/badge/.../candidate.mz`                      | `b25b27eb4e8785975184a8d43d448a989e79ca90c6c3608ea3ccd0d3ebe2a896` | yes   |
| `mzizi/button/.../candidate.mz`                     | `1f7ee07d9a3aee00c87651e1d1ff2796db84a5f4d6a41269bcffa1ea3229db9c` | yes   |
| `mzizi/nyuchi-changelog-renderer/.../candidate.mz`  | `39c7cfde627e157a4abaa457479855f769f17b72ddd071523d1756cd08199197` | yes   |

### How the committed episode directories were produced

The original episode directories were not kept, so the six episodes were re-run on
2026-09-27 from the same code: a `git worktree` of `ded425a`, `cargo build --workspace`,
then for each episode `mzbench episode start --task benchmarks/tasks/<task> --arm <arm>
--model-label claude-subagent --seed 0`, `episode submit` with the transcribed candidate,
and `episode finish`. The scores are the second check on the transcription: **every one of
the six re-run final lines is byte-identical to the original's** (compared as sorted lines;
`diff` printed nothing). Only `check_ms`, a wall-clock time in the iteration lines,
differs from the original run, as it must.

`meta.json` as the runner writes it holds absolute paths into the worktree. Those were
rewritten to repo-relative (`cwd` becomes `"."`); no other file in the episode directories
carried one. `summary.txt` is `mzbench summarize` over this directory, run at `ded425a`.

The original final lines, verbatim:

```jsonl
{"arm":"dioxus","class_token_jaccard":1.0,"clean":true,"defects":0,"facts_checked":2,"iterations":1,"iterations_to_clean":1,"kind":"final","max_iters":5,"mode":"agent","model":"claude-subagent","score_error":null,"scored":true,"seed":0,"task":"badge","temperature":null,"token_source":"tokenizer_transcript_proxy","total_completion_tokens":null,"total_prompt_tokens":null,"transcript_tokens":null,"transcript_tokens_null_reason":"tokenizer unavailable: POST http://127.0.0.1:8080/tokenize: io: Connection refused (os error 111)"}
{"arm":"dioxus","class_token_jaccard":1.0,"clean":true,"defects":0,"facts_checked":9,"iterations":1,"iterations_to_clean":1,"kind":"final","max_iters":5,"mode":"agent","model":"claude-subagent","score_error":null,"scored":true,"seed":0,"task":"button","temperature":null,"token_source":"tokenizer_transcript_proxy","total_completion_tokens":null,"total_prompt_tokens":null,"transcript_tokens":null,"transcript_tokens_null_reason":"tokenizer unavailable: POST http://127.0.0.1:8080/tokenize: io: Connection refused (os error 111)"}
{"arm":"dioxus","class_token_jaccard":null,"clean":true,"defects":2,"facts_checked":2,"iterations":1,"iterations_to_clean":1,"kind":"final","max_iters":5,"mode":"agent","model":"claude-subagent","score_error":null,"scored":true,"seed":0,"task":"nyuchi-changelog-renderer","temperature":null,"token_source":"tokenizer_transcript_proxy","total_completion_tokens":null,"total_prompt_tokens":null,"transcript_tokens":null,"transcript_tokens_null_reason":"tokenizer unavailable: POST http://127.0.0.1:8080/tokenize: io: Connection refused (os error 111)"}
{"arm":"mzizi","class_token_jaccard":0.6833,"clean":true,"defects":0,"facts_checked":2,"iterations":1,"iterations_to_clean":1,"kind":"final","max_iters":5,"mode":"agent","model":"claude-subagent","score_error":null,"scored":true,"seed":0,"task":"badge","temperature":null,"token_source":"tokenizer_transcript_proxy","total_completion_tokens":null,"total_prompt_tokens":null,"transcript_tokens":null,"transcript_tokens_null_reason":"tokenizer unavailable: POST http://127.0.0.1:8080/tokenize: io: Connection refused (os error 111)"}
{"arm":"mzizi","class_token_jaccard":1.0,"clean":true,"defects":0,"facts_checked":9,"iterations":1,"iterations_to_clean":1,"kind":"final","max_iters":5,"mode":"agent","model":"claude-subagent","score_error":null,"scored":true,"seed":0,"task":"button","temperature":null,"token_source":"tokenizer_transcript_proxy","total_completion_tokens":null,"total_prompt_tokens":null,"transcript_tokens":null,"transcript_tokens_null_reason":"tokenizer unavailable: POST http://127.0.0.1:8080/tokenize: io: Connection refused (os error 111)"}
{"arm":"mzizi","class_token_jaccard":null,"clean":true,"defects":2,"facts_checked":2,"iterations":1,"iterations_to_clean":1,"kind":"final","max_iters":5,"mode":"agent","model":"claude-subagent","score_error":null,"scored":true,"seed":0,"task":"nyuchi-changelog-renderer","temperature":null,"token_source":"tokenizer_transcript_proxy","total_completion_tokens":null,"total_prompt_tokens":null,"transcript_tokens":null,"transcript_tokens_null_reason":"tokenizer unavailable: POST http://127.0.0.1:8080/tokenize: io: Connection refused (os error 111)"}
```

## Results at `ded425a`

From [`summary.txt`](summary.txt):

| Arm    | n   | Clean compile | Iters to clean (mean) | Defect rate | Mean defects | Class-token jaccard (mean) |
| ------ | --- | ------------- | --------------------- | ----------- | ------------ | -------------------------- |
| dioxus | 3   | 3/3           | 1.00                  | 1/3         | 0.67         | 1.000 (n=2)                |
| mzizi  | 3   | 3/3           | 1.00                  | 1/3         | 0.67         | 0.842 (n=2)                |

Every episode was clean on iteration 1. The one "defective" episode per arm is the
changelog task, and its 2 defects are a harness artifact, not behaviour: see
[Rescored](#rescored-with-rename-aware-matching).

## Caveats (read these before the numbers)

1. **n = 3 tasks, one seed, one frontier model.** Three data points per arm say nothing
   about a distribution.
2. **No small-model arm.** RFC-0002 §5.4 says the frontier arm alone cannot validate the
   thesis: the charter's claim is about what a language does for the agents least able to
   write the incumbent. Nothing here measures that.
3. **No Leptos arm.** At `ded425a` there was no Leptos arm; it was on the then-unmerged
   branch `claude/leptos-arm`, and has since landed on `main` (`37d785b`), after this run.
4. **Tokens were not measured.** No local model endpoint was running, so every episode
   has `transcript_tokens: null` with this reason, and the token metric has no data here at
   all:

   ```text
   tokenizer unavailable: POST http://127.0.0.1:8080/tokenize: io: Connection refused (os error 111)
   ```

5. **The Mzizi checker at `ded425a` does not check type names or `{...}` interpolation
   names.** `benchmarks/prompts/mzizi-guide.md` says so ("The compiler does not check type
   names or the names inside `{...}`, so spell them exactly"). Reproduced at `ded425a`
   against this file:

   ```mz
   component probe

     prop x: flarp

     view
       row
         class = "p-2 {nonexistent.class}"
         text = x
       end
     end

   end component probe
   ```

   `cargo run -q --manifest-path compiler/Cargo.toml --bin mz -- check --agent probe.mz`
   printed, and exited 0:

   ```jsonl
   {"code":"MZ0501","severity":"warning","file":"../probe.mz","span":[1,11,1,16],"say":"`component probe` has no `contract` block — behaviour is unverified (RFC-0001 §1.6)"}
   {"summary":true,"errors":0,"warnings":1,"exact_fixable":0,"ms":0}
   ```

   An undefined type `flarp` and an undefined `nonexistent.class` are 0 errors. So the
   Mzizi arm's first-iteration-clean result was measured against a checker that cannot fail
   on names or types. **It is not evidence for the iterations metric.** The Dioxus arm's
   `cargo check` can fail on both.

6. **The Mzizi changelog candidate solved a smaller problem than the Dioxus one.** The
   spec renders `entries[]`, each with `nodesAffected`, `componentsAdded`,
   `componentsModified` and `componentsDeprecated` arrays. Mzizi has no list or record type,
   so the candidate renders a single entry, with a `bool` + `text` prop pair standing in for
   each array (its header comment says so). The Dioxus candidate renders the whole feed. The
   harness scores enums only — variant sets, defaults, heights — so it does not see this,
   and both arms score the same.
7. **Both arms' 2 changelog "defects" are a harness artifact.** The harness matched variants
   by name only; the task's reference renamed its own spec's variants. Details and the fix
   below.

## Rescored with rename-aware matching

Scoring the changelog task, both arms got the identical 2 defects: `variant_set` (expected
`{cobalt, tanzanite, malachite, gold}`, actual `{horizontal, vertical, depth, outlier}`) and
`default` (expected `cobalt`, actual `horizontal`), with `class_token_jaccard: null`
because no variant name matched. The class strings are identical to the reference's. The
task's `spec.tsx` keys its colours by axis
(`AXIS_COLOURS = { horizontal: "bg-[var(--color-cobalt)]/10 …", … }`) and the Rust
reference renamed the same four variants by mineral. Both authors followed the spec, so
the harness was measuring the reference's drift from its own spec.

The harness now pairs variants by class-token set when the name sets differ, under strict
conditions (complete bijection, unique token sets on each side, shared names pair with
themselves), and reports the pairing as a `variant_names` fact instead of hiding it;
otherwise it matches by name as before. See `rename_map` in
[`benchmarks/harness/src/lib.rs`](../../harness/src/lib.rs) and RFC-0006 §10.1.

[`rescored.jsonl`](rescored.jsonl) is the new harness's score JSON for each of the six
committed candidates, one line per episode, with the harness commit it came from. That is
this branch's commit `648b0d2` ("fix(benchmarks): pair renamed variants by class
string…"); because this repository rebase-merges, the SHA on `main` will differ, so each
line also records `harness_tree`, the git tree hash of `benchmarks/harness/`
(`4610cf1c12998ae99edc2254bc1d01050a6e362f`), which a rebase does not change. Each
episode's `episode.jsonl` and `score.json` are untouched: they stay the record of what the
harness at `ded425a` said.

| Episode                            | facts checked | defects   | renames | class-token jaccard |
| ---------------------------------- | ------------- | --------- | ------- | ------------------- |
| dioxus / badge                     | 2 → 2         | 0 → 0     | — → 0   | 1.0000 → 1.0000     |
| dioxus / button                    | 9 → 9         | 0 → 0     | — → 0   | 1.0000 → 1.0000     |
| dioxus / nyuchi-changelog-renderer | 2 → 3         | **2 → 0** | — → 4   | null → 1.0000       |
| mzizi / badge                      | 2 → 2         | 0 → 0     | — → 0   | 0.6833 → 0.6833     |
| mzizi / button                     | 9 → 9         | 0 → 0     | — → 0   | 1.0000 → 1.0000     |
| mzizi / nyuchi-changelog-renderer  | 2 → 3         | **2 → 0** | — → 4   | null → 1.0000       |

The changelog's third fact is `variant_names` itself (never a defect), which lists the
pairing: `horizontal -> cobalt, vertical -> tanzanite, depth -> malachite, outlier -> gold`.
The `default` fact now reads `horizontal -> cobalt`, and passes.

Per arm, before → after:

| Arm    | Defect rate | Mean defects | Class-token jaccard (mean) |
| ------ | ----------- | ------------ | -------------------------- |
| dioxus | 1/3 → 0/3   | 0.67 → 0.00  | 1.000 (n=2) → 1.000 (n=3)  |
| mzizi  | 1/3 → 0/3   | 0.67 → 0.00  | 0.842 (n=2) → 0.894 (n=3)  |

Read those "after" numbers with caveats 5 and 6 still in force: 0 defects on the changelog
task means both candidates' `NodeAccent` enums agree with the reference, not that the Mzizi
candidate renders what the spec renders.

## Upstream: the reference diverges from its own spec

The changelog task's Rust reference renamed its spec's `AXIS_COLOURS` keys (`horizontal`,
`vertical`, `depth`, `outlier`) to mineral names (`Cobalt`, `Tanzanite`, `Malachite`,
`Gold`); its module docs call the axis words retired. That divergence lives in
`mzizi-dev/mzizi-registry` (`components/registry/n10-documentation/`, pinned at
`3afeb75`), and is worth an issue there: the `.tsx` and `.rs` should agree on the names.
It is noted here and in [`../../tasks/README.md`](../../tasks/README.md), not filed.

## Files

- `claude-subagent/<arm>/<task>/seed-0/` — each episode as the runner wrote it at `ded425a`
  (`meta.json` paths made repo-relative): `prompt.md`, `system.txt`, `user.txt`,
  `iter-01/candidate.{mz,rs}`, `iter-01/diagnostics.txt`, `iter-01/stderr.txt`,
  `score.json`, `score.stderr.txt`, `episode.jsonl`.
- `summary.txt` — `mzbench summarize` over this directory, at `ded425a`. (Run at a later
  commit, `summarize` adds a "renamed variants" column; these episodes' final lines predate
  the `renames` field, so it reads `0 (n=0)`.)
- `rescored.jsonl` — the six candidates rescored by the rename-aware harness.
