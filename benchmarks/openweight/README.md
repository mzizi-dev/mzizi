# Open-weight arm: local llama-server

The Phase 0 benchmark pilot's small open-weight arm (RFC-0002 §1 names a ~7B local model as the
design target). This directory holds the setup that serves that model on CPU, and the numbers
measured for it. Every number below was measured on 2026-09-27 in the pilot's container; nothing
here is projected unless it says _estimate_.

## What runs

- **llama.cpp** release tag `b11206` (the binary reports `version: 0.5.0-dev (build 11206, commit
2b129ccfa)`), the prebuilt `llama-b11206-bin-ubuntu-x64.tar.gz` from the GitHub release. It
  ran as-is; no source build was needed. Tarball SHA256
  `aea9ff64167ea473bf5cf463f07b42beac16c857cdffce57c7ed9cc323ea4da5`.
- **Models**, all Q4_K_M, from the official Qwen repos on Hugging Face, stored in `/opt/models`
  (outside the repo):

| Size | Repo                                    | File                                      | SHA256                                                             |
| ---- | --------------------------------------- | ----------------------------------------- | ------------------------------------------------------------------ |
| 1.5B | `Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF` | `qwen2.5-coder-1.5b-instruct-q4_k_m.gguf` | `cc324af070c2ecbfd324a30884d2f951a7ff756aba85cb811a6ec436933bb046` |
| 3B   | `Qwen/Qwen2.5-Coder-3B-Instruct-GGUF`   | `qwen2.5-coder-3b-instruct-q4_k_m.gguf`   | `724fb256bec1ff062b2f65e4569e871ad2e95ab2a3989723d1769c54294730b7` |
| 7B   | `Qwen/Qwen2.5-Coder-7B-Instruct-GGUF`   | `qwen2.5-coder-7b-instruct-q4_k_m.gguf`   | `509287f78cb4d4cf6b3843734733b914b2c158e43e22a7f4bf5e963800894d3c` |

- **Machine:** 4 cores (1 thread per core, Intel Xeon @ 2.10GHz with AVX-512), about 15 GB RAM,
  no GPU.

`setup.sh` reproduces all of it: `setup.sh fetch`, `setup.sh models`, `setup.sh launch`, or
`setup.sh all`. It is idempotent and checks every SHA256.

## The endpoint

```sh
/opt/llama.cpp/llama-b11206/llama-server -m /opt/models/qwen2.5-coder-7b-instruct-q4_k_m.gguf \
  --host 127.0.0.1 --port 8080 -c 16384 -t 4 -tb 4 -np 1 --jinja
```

It runs detached (`setsid nohup`) and logs to `/opt/models/llama-server.log`. Checked with curl:

- `POST /v1/chat/completions` returns `usage.prompt_tokens` and `usage.completion_tokens`, plus
  `usage.prompt_tokens_details.cached_tokens`.
- `POST /tokenize` returns token ids.
- **Seeded requests reproduced.** The same request (`seed: 1234`, `temperature: 0.2`, 120
  tokens) was sent three times: cold, fully cached, and after an unrelated request in between.
  All three returned byte-identical content. `seed: 999` returned different content, so the seed
  is actually being used. This was checked on a short prompt only, not at 3,000 tokens.

The 7B model at `-c 16384` used about 6.1 GB of RAM. The kill-criterion run uses `-c 32768`
(`CTX=32768 setup.sh launch`; [`../kill-criterion/README.md`](../kill-criterion/README.md),
"The pre-registered settings"), and falls back to 16,384 only if RAM cannot hold it, with
the fallback recorded in its `RUN.md`. _Estimate, not measured:_ the f16 KV cache costs
57,344 bytes a token (28 layers × 4 KV heads × 128 dims × K and V × 2 bytes), so 32,768
needs about 0.94 GB more than 16,384.

## Measurements

Each request had a 3,029-token prompt (instructions, `primitives/button.mz`, the
`button_reference.rs` fixture, and compiler source as padding) and generated 500 tokens
(`ignore_eos`, `cache_prompt: false`). There were also single runs at 6,029 and 9,029 prompt
tokens, to see how context growth costs. The throughput numbers are llama-server's own
`timings`; wall time is client-side. `bench.py` is the script.

| Model | Prompt tokens | Prompt processing tok/s | Generation tok/s | Wall s / request |
| ----- | ------------: | ----------------------: | ---------------: | ---------------: |
| 1.5B  |         3,029 |            169.9, 168.0 |     11.85, 5.66¹ |     59.9, 106.3¹ |
| 1.5B  |         6,029 |                   128.6 |             9.36 |            100.2 |
| 1.5B  |         9,029 |                   106.7 |             8.10 |            146.3 |
| 3B    |         3,029 |              85.7, 88.1 |       7.23, 7.44 |     104.4, 101.4 |
| 3B    |         6,029 |                    71.0 |             5.94 |            168.9 |
| 3B    |         9,029 |                    57.9 |             4.53 |            266.0 |
| 7B    |         3,029 |              41.4, 42.1 |       3.68, 3.70 |     208.8, 206.8 |
| 7B    |         6,029 |                    36.5 |             2.78 |            345.1 |
| 7B    |         9,029 |                    32.2 |             2.26 |            501.2 |

¹ During this run, other processes in the container used 0.91 cores on average, against about
0.12 to 0.19 for every other run. Generation speed halved as a result. The pilot will share
the CPU with its own compile steps, so expect this kind of slowdown.

Thread count: `llama-bench` on the 7B gave pp512 / tg64 of 26.7 / 2.67 tok/s at 2 threads,
37.3 / 3.64 at 3 threads, and 51.5 / 4.61 at 4 threads. So `-t 4`.

### A compile-fix loop with a growing context (7B)

`iter.py` sends a 3,029-token prompt, then four more turns. Each turn appends the model's reply
and a 385-token fake compiler-error message, and the prompt cache is on (llama-server's
default). The model's reply was 325 tokens every turn.

| Iteration | Prompt tokens | Tokens actually processed | Prompt ms | Gen tok/s | Wall s |
| --------: | ------------: | ------------------------: | --------: | --------: | -----: |
|         1 |         3,029 |                       21² |     1,755 |      3.75 |   89.7 |
|         2 |         3,738 |                       385 |    12,607 |      3.41 |  107.6 |
|         3 |         4,447 |                       385 |    13,329 |      3.04 |  119.8 |
|         4 |         5,156 |                       385 |    14,197 |      2.93 |  124.6 |
|         5 |         5,865 |                       385 |    14,721 |      2.76 |  132.2 |

² The 3,008-token prefix was still in the slot's cache from an earlier request that began with
the same text. A cold first prompt measured about 72.5 s (the 3,029-token rows above).

The prompt cache means later iterations only pay for their new tokens. Generation dominates.

## Recommendation: 7B

Budget: 3 tasks × 2 arms × 3 seeds = 18 runs, each with up to 5 iterations, so up to 90
requests, in about 4 hours (14,400 s). That allows 800 s per run.

**Worst case: every run uses all 5 iterations.** The figures come from the measured 7B loop
above.

- **Replies of 325 tokens (as measured).** The first iteration costs 72.5 s cold prompt plus
  87.9 s the rest of the way, which is 160.4 s. Iterations 2 to 5 cost 484.2 s. That is
  **644.6 s per run**, and 18 × 644.6 = **11,603 s = 3.2 h**.
- **Replies of 500 tokens.** Generation scales by 500/325: the measured 515.8 s of generation
  becomes 793.5 s. Prompt processing is 72.5 + 4 × 13.7 = 127.3 s. That is 920.8 s per run, and
  18 runs is **16,574 s = 4.6 h**, which is over budget.
- **Break-even.** Per-run cost is about 127.3 + 1.587 × L seconds for a reply length of L
  tokens. Keeping that under 800 s needs L ≤ about 424 tokens.
- **Expected case.** If runs average 3 iterations, a run costs 160.4 + 107.6 + 119.8 = 387.8 s,
  and 18 runs take about 1.9 h.

None of these figures include compile/check time or harness overhead. Those were not measured
here.

So **7B fits the 4-hour budget unless replies average more than about 420 tokens _and_ most runs
exhaust all 5 iterations**. RFC-0002 targets about 7B, so 7B is what is launched. To keep it
inside the budget:

- Keep each run as one growing message list. Then llama-server reuses the KV prefix, as the loop
  above measured.
- Run the three seeds of a given (task, arm) back to back. The identical first prompt is then
  cached, as iteration 1 above shows.
- Keep other CPU work off the machine while generating. The ¹ row shows what contention does.

**Fallback: 3B.** If the first (task, arm) shows 5-iteration runs above about 13 minutes, switch
to 3B with `SERVE_MODEL=3b setup.sh launch`. At 3,029/500 tokens the 3B's wall time is 0.495× the
7B's (102.9 s vs 207.8 s). Scaling the 4.6 h worst case by that ratio gives an _estimate_ of
about 2.3 h. The 3B loop itself was not run. The cost of falling back: the arm would no longer
test the model size RFC-0002 names as the design target. Any Mzizi-vs-Dioxus difference found at
3B would then be evidence about a smaller model than the one the language is designed for.
