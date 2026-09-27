#!/usr/bin/env python3
"""Simulate a compile-fix loop with a growing conversation and llama-server's
default prompt cache: iteration 1 = ~3k-token prompt; each later iteration
appends the model's reply plus a ~300-token compiler-error message.
Reports how many prompt tokens llama-server actually had to process."""
import json, os, sys, time
sys.path.insert(0, os.path.dirname(__file__))
import bench

ERR = open(f"{bench.REPO}/compiler/src/diagnostic.rs").read()[:1200]


def main():
    iters = int(sys.argv[1]) if len(sys.argv) > 1 else 3
    msgs = [{"role": "user", "content": bench.build_prompt(3000)}]
    for k in range(1, iters + 1):
        body = {"messages": msgs, "max_tokens": 500, "temperature": 0.2, "seed": 7}
        t0 = time.time()
        r = bench.post("/v1/chat/completions", body)
        wall = time.time() - t0
        t = r["timings"]
        print(json.dumps({"iter": k, "wall_s": round(wall, 1),
                          "usage_prompt_tokens": r["usage"]["prompt_tokens"],
                          "prompt_tokens_processed": t["prompt_n"],
                          "cached_tokens": t.get("cache_n"),
                          "prompt_ms": round(t["prompt_ms"]),
                          "completion_tokens": r["usage"]["completion_tokens"],
                          "gen_ms": round(t["predicted_ms"]),
                          "tg_tok_s": round(t["predicted_per_second"], 2)}), flush=True)
        msgs.append({"role": "assistant", "content": r["choices"][0]["message"]["content"]})
        msgs.append({"role": "user", "content":
                     "cargo check failed:\n```\nerror[E0425]: cannot find value `size` in this scope\n"
                     + ERR + "\n```\nFix the component and output the full file again."})


if __name__ == "__main__":
    main()
