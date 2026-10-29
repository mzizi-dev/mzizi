# Plan: 2026-10-29-pilot-4-haiku

Registered 2026-10-29 before the first episode (RFC-0009 §7.2). The tables are written by `mzbench plan` from the files on disk. The sections marked **To fill** are written by hand before this file is committed, and nothing in it changes after the first episode. A departure from it is stated in `RUN.md`.

## Code

- Commit: `18b182c39a07a676080c019473e5240756ebc24f`
- Scorer version (SHA-256 of `benchmarks/harness/src`): `a3f38bdae8649561a38689fa5f0f46c43707df7694c739c7aa040a1e7f3b26e6`
- Task family: `ui-spec`

## Arms

| Arm     | Check                                                                                      | Guide                               | Guide bytes | Guide SHA-256                                                      | Pins                                                                                                                                                                                                                                                                                                                                                                                       |
| ------- | ------------------------------------------------------------------------------------------ | ----------------------------------- | ----------- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `mzizi` | `cargo run -q --manifest-path {repo}/compiler/Cargo.toml --bin mz -- check --agent {file}` | `benchmarks/prompts/mzizi-guide.md` | 9900        | `58b5632cb648b21913084d64201483eb9d18a011ec997f811c74107ab8860a32` | none (the repo commit)                                                                                                                                                                                                                                                                                                                                                                     |
| `react` | `{repo}/benchmarks/arms/react/check.sh {file}`                                             | `benchmarks/prompts/react-guide.md` | 10298       | `25de0e58048284151db25d26f18768ebfa97f7697bb6bdb21187f8ffd171dfb7` | `sandbox/package.json` `d6b7bfd8c6852b614f1a74f0d0dcef9c6615415d4b2b658ecb5882b23f34345d`<br>`sandbox/package-lock.json` `a28a378575820865977ef1f2a62ec38916deef80c5804bbf8d7bfec36c382e2a`<br>`sandbox/tsconfig.json` `e297ace2b3a8a7dccab94531cec8bd61a3bf48e757ba8ccb192b352f8e713a69`<br>`sandbox/src/lib/utils.ts` `fffb2ed80d7a94e3a8491830bcf8caf4aacd5e882d24e5eb8a00e55ff701f5e9` |

Guide token counts, with the headline model's tokenizer (RFC-0009 §4.2): each episode's final line records `guide_tokens`. Not measured: no llama.cpp `/tokenize` endpoint was available, so Mode 2 records no `guide_tokens` and no `transcript_tokens`. The token metric is not reported in this pilot.

## Tasks

3 task(s). A task's hash is the SHA-256 of the `sha256sum` listing of its files.

| Task     | Content SHA-256                                                    |
| -------- | ------------------------------------------------------------------ |
| `badge`  | `1cc99669b7624dbda1f43b690dc700649db0a04678782194ed9c30a29e9704b1` |
| `button` | `de35aa262c7664a07423dca9d4ee51dc7c70a8700e0d1518940b5ba8d0bbdc36` |
| `card`   | `18e4493891c2360d72d5c221deaaf8faec33f767cd37103eec67ca8c6f92d920` |

## Models, seeds and budget

- Models: `claude-haiku-subagent`
- Seeds: 1, 2, 3
- Temperature: 0
- `max_iters`: 5
- `max_tokens` per reply: 4096
- Context: not applicable (Mode 2, agent-driven; no server)
- Episodes: 18 (tasks × arms × seeds × models)

## Decision rule

This is a gating family. The rule is RFC-0009 §6.2, fixed here before any score is seen. For each metric (tokens, iterations to a clean check, defect rate), the bar is the best value any incumbent arm reached. The family passes when `mzizi` beats the bar on at least two of the three, on the headline model, and a win counts only if a paired bootstrap over (task, seed) pairs gives a 95% interval for the difference that excludes zero in Mzizi's favour.

- n: 3 tasks × 3 seeds = 9 (task, seed) pairs per arm and model.
- This is a pilot. n is small (3 tasks × 3 seeds), the tasks are public, and the rule above is **not applied**: no pass or fail is declared. There is no headline model here. It repeats pilot 3 (`2026-10-29-pilot-3-frontier`) with a smaller model (Claude Haiku) as author, to see whether the ceiling effect seen there lifts. The ~7B open-weight model remains the headline of the gating run.

## Departures and notes

- **Pilot 4, not the gating run.** Public tasks (`button`, `badge`, `card`; `ui-spec`), no held-out set, and the author is a Claude Haiku subagent (Mode 2), so the model and the language share a vendor. Reported beside, never pooled with, the 7B headline (RFC-0009 §5).
- Temperature is not controllable in Mode 2; `0` above is a label. `iteration_count_enforced_by_runner` is false in Mode 2. Authors are told to read only `prompt.md`; this is an instruction, not enforced.
- The token metric is not measured. Compile rate, iterations to clean and defects are.
- Arms: `mzizi` and `react` only. Dioxus is not the direction for new work (Mzizi Roots; components live in the registry), and Leptos has never run end to end.
- Each episode's author is a fresh agent. Episodes of one arm may run in parallel: the arm's `check.sh` takes a per-sandbox lock (`flock`; a macOS shim is on `PATH`).
