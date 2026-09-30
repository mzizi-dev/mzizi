# The `mzizi-be` arm

Mzizi for the `backend` task family (RFC-0009 §1, §6.4). An episode writes one `.mz` file
holding one `service` (RFC-0011). Nothing has run in this arm: no episode, no score.

| Part       | What it is                                                                                                           |
| ---------- | -------------------------------------------------------------------------------------------------------------------- |
| `arm.toml` | the arm, in the format the runner README specifies ("Arms: `arm.toml`", RFC-0009 §9 step 1)                          |
| guide      | [`../../prompts/mzizi-be-guide.md`](../../prompts/mzizi-be-guide.md)                                                 |
| check      | `mz check --agent {file}`, this repo's compiler, so the pin is the repo commit                                       |
| serving    | [`serve.sh`](serve.sh): `mz build` the candidate, `cargo build --offline` the package, exec it on `$PORT`            |
| scoring    | the task's probes, by [`benchmarks/probe`](../../probe): `mzprobe serve <task> -- serve.sh <candidate.mz>`           |
| setup      | [`setup.sh`](setup.sh), once per machine: warms the cargo cache with the pinned `axum` and `tokio` the lowering uses |

## What is and is not wired up

- **The runner loads the arm.** Its `arm.toml` follows the format
  [`runner/README.md`](../../runner/README.md) specifies, and the runner's tests load it.
- **The runner does not call the probes yet.** `extractor = "none"`, so an episode's clean
  candidate is recorded as clean but unscored, as the runner README says of any arm without
  a scorer. Probing a candidate is one command today (above). Making it the episode's scorer
  is runner work, alongside the `ts` and `rust` backend arms (RFC-0009 §9 step 3).
- **A candidate that checks clean but does not build or start** is `start_failed` and fails
  every fact (RFC-0009 §2.3). `mz build` refuses a file with errors, and `mz check` has
  already passed it, so a build failure here would be a lowering bug.

## The guide's budget

RFC-0009 §4.2 puts every guide in a family on one token budget, within ±5%. This is the
only `backend` guide so far, so the family has no budget yet. It is 2,210 tokens (7,861
bytes, SHA-256 `2d8b7dac3074e8382175af8bee97b0e97a3f2a7211a33e472aff63b849529486`), counted on
2026-09-30 with `Qwen/Qwen2.5-Coder-7B-Instruct`'s `tokenizer.json` (SHA-256
`c0382117ea329cdf097041132f6d735924b697924d6f6fc3945713e96ce87539`, the tokenizer
`../../prompts/BUDGET.md` names). The budget is set, and this guide rebalanced to it, when
the incumbent backend arms' guides are written. It has the same five sections as the UI
guides, and its worked example (a weather-station service) is not a task.
