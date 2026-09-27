#!/usr/bin/env python3
"""Benchmark-shaped request against llama-server: ~N prompt tokens, 500 generated.

usage: bench.py <label> <target_prompt_tokens> <runs>
Prints one JSON line per run with llama-server's own timings plus wall seconds.
"""
import json, sys, time, urllib.request

import os

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
URL = "http://127.0.0.1:" + os.environ.get("PORT", "8080")


def cpu_snapshot():
    """(total busy jiffies on the machine, jiffies used by llama-server)."""
    f = open("/proc/stat").readline().split()[1:]
    v = list(map(int, f))
    busy = sum(v) - v[3] - v[4]
    srv = 0
    for pid in os.listdir("/proc"):
        if pid.isdigit():
            try:
                if open(f"/proc/{pid}/comm").read().strip() == "llama-server":
                    st = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
                    srv += int(st[11]) + int(st[12])
            except OSError:
                pass
    return busy, srv



def post(path, body, timeout=3600):
    req = urllib.request.Request(URL + path, data=json.dumps(body).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.load(r)


def ntok(text):
    return len(post("/tokenize", {"content": text})["tokens"])


def build_prompt(target):
    ref = open(f"{REPO}/benchmarks/harness/tests/fixtures/button_reference.rs").read()
    mz = open(f"{REPO}/primitives/button.mz").read()
    filler = open(f"{REPO}/compiler/src/diagnostic.rs").read() + "\n" + open(f"{REPO}/compiler/src/lex.rs").read() + "\n" + open(f"{REPO}/compiler/src/parse.rs").read()
    head = ("You are porting a UI component. Below is a Mzizi (.mz) example, the Rust/Dioxus "
            "reference implementation of a Button, and supporting compiler source for context. "
            "Write a complete Rust Dioxus component `Card` with variants default and outline, "
            "sizes sm/md/lg, and doc comments on every item. Output only code.\n\n"
            f"### button.mz\n```\n{mz}\n```\n\n### button.rs (reference)\n```rust\n{ref}\n```\n\n"
            "### compiler source (context)\n```rust\n")
    tail = "\n```\n\nNow write the full Card component in Rust (Dioxus)."
    # binary-search the filler length to hit the target token count
    lo, hi = 0, len(filler)
    while lo < hi:
        mid = (lo + hi + 1) // 2
        if ntok(head + filler[:mid] + tail) <= target:
            lo = mid
        else:
            hi = mid - 1
    return head + filler[:lo] + tail


def main():
    label, target, runs = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    prompt = build_prompt(target)
    for i in range(runs):
        body = {"messages": [{"role": "user", "content": prompt}],
                "max_tokens": 500, "temperature": 0.2, "seed": 42 + i,
                "cache_prompt": False, "ignore_eos": True}
        c0 = cpu_snapshot()
        t0 = time.time()
        r = post("/v1/chat/completions", body)
        wall = time.time() - t0
        c1 = cpu_snapshot()
        hz = os.sysconf("SC_CLK_TCK")
        foreign_cores = ((c1[0] - c0[0]) - (c1[1] - c0[1])) / hz / wall
        t = r["timings"]
        print(json.dumps({"label": label, "run": i, "wall_s": round(wall, 2),
                          "prompt_tokens": r["usage"]["prompt_tokens"],
                          "completion_tokens": r["usage"]["completion_tokens"],
                          "pp_tok_s": round(t["prompt_per_second"], 2),
                          "tg_tok_s": round(t["predicted_per_second"], 2),
                          "prompt_ms": round(t["prompt_ms"]), "gen_ms": round(t["predicted_ms"]),
                          "foreign_cpu_cores": round(foreign_cores, 2)}),
              flush=True)


if __name__ == "__main__":
    main()
