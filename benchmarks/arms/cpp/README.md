# The `cpp` arm

C++20 with cpp-httplib for the `backend` task family (RFC-0009 §1, §3). An episode writes one `.cpp` file with a `main`, using cpp-httplib and nlohmann/json.
Nothing has run in this arm: no model episode, no score. The guide is
[`../../prompts/cpp-guide.md`](../../prompts/cpp-guide.md), written to the family's token budget
([`../../prompts/BUDGET.md`](../../prompts/BUDGET.md)).

| Part       | What it is                                                                                                                                                                                                                                |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `arm.toml` | the arm, in the format the runner README specifies ("Arms: `arm.toml`")                                                                                                                                                                   |
| check      | `check.sh <candidate>`: `clang++ -std=c++20 -fsyntax-only -Wall -Wextra -fno-color-diagnostics`, with the vendored headers on `-isystem` (so their own warnings are not the candidate's). Exit 0 clean, 1 errors (printed), 2 setup error |
| serving    | `serve.sh <candidate>`: builds the candidate and execs its server on `$PORT`, for `mzprobe serve`                                                                                                                                         |
| sandbox    | `sandbox/`: a Linux container image, `mzizi-arm-cpp`, built by `setup.sh` from `sandbox/Containerfile`                                                                                                                                    |
| fixtures   | `fixtures/b1-good.cpp`, `b1-bad-type.cpp`, `b1-bad-behaviour.cpp`: see "Verifying the arm"                                                                                                                                                |

`check.sh`, `serve.sh` and `setup.sh` are one-line calls to [`../lib/`](../lib): `arm-run.sh`
runs the sandbox, `build-image.sh` builds it, `probe.sh` probes a candidate with a task's
probes, and `self-test.sh` is the verification below.

## Serving

`sandbox/run-serve.sh` compiles and links with `clang++ -std=c++20 -O0 -pthread`, then execs the binary; link errors surface here, as `start_failed`. The candidate's `main` reads `PORT` and listens on `0.0.0.0`.

## Pinned versions

| Part                      | Pin                                                                                                                                                                                                                                            | SHA-256                                                            |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `cpp-httplib` `v0.63.0`   | `sandbox/include/httplib.h`, the unmodified single header                                                                                                                                                                                      | `9021001a719768893a8a6e4051c56107690e8b8e49b41e5f903534fa19ae9afb` |
| `nlohmann/json` `v3.12.0` | `sandbox/include/nlohmann/json.hpp`, the release single header with four email addresses removed from its copyright comments (the names stay; the release header's hash is `aaf127c04cb31c406e5b04a63f1ae89369fccde6d8fa7cdda1ed4f32dfc5de63`) | `f8b557b73311b275473f5b5fe25d3c37dace7942a3126ef509ff90f50936b7a6` |
| clang                     | Alpine 3.22's `clang`, `g++` and `musl-dev` (`clang 20.1.8` when the image was built on 2026-10-29)                                                                                                                                            | not pinned to a patch                                              |

Both libraries are MIT licensed; `sandbox/NOTICE` carries both licence texts, as RFC-0009 §1 requires of vendored headers. `install.sh` re-checks the two hashes when the image is built.

## The sandbox

Run `setup.sh` once per machine, with network access: it builds the image, the only step
that fetches anything (RFC-0009 §4.5: caches warmed before the first episode). After that:

- **A check** runs `container run --network none`, with the candidate copied into a fresh
  directory mounted at `/work`. Two checks never share a slot, and nothing a candidate writes
  survives.
- **Serving** publishes `$PORT` on `127.0.0.1` only, which needs the container's default
  network, so serving is not `--network none`. Nothing in a candidate is told of the network
  and the image fetches nothing at run time, but this is a gap against RFC-0009 §4.5's "no
  network", stated here rather than hidden. The server is killed after `ARM_SERVE_TTL`
  seconds (300 by default): `mzprobe` stops a server with SIGKILL, which the container CLI
  cannot forward, so the TTL is what reaps it.
- **Without the `container` CLI**, or with `MZ_ARM_RUNTIME=local`, the same
  `sandbox/run-check.sh` and `run-serve.sh` run on this machine against the toolchain on
  `PATH` and what `sandbox/install.sh` installed into `sandbox/`. CI does not run any of it:
  no arm's toolchain is a CI requirement (RFC-0009 §9).

## Reading the check

What the author sees for `fixtures/b1-bad-type.cpp`, from a real run:

```text
b1-bad-type.cpp:8:7: error: cannot initialize a variable of type 'int' with an lvalue of type 'const char[2]'
    8 |   int count = "2";
      |       ^       ~~~
1 error generated.
```

## Verifying the arm

`../lib/self-test.sh cpp` checks the arm against B1 (`benchmarks/tasks/b1-routing`), with
three files written for it. None is shown to an author or a model.

| Fixture                | Expected                             | Result, 2026-10-29                     |
| ---------------------- | ------------------------------------ | -------------------------------------- |
| `b1-good.cpp`          | check exits 0, and every probe holds | check 0; `mzprobe` 59 of 59 facts held |
| `b1-bad-type.cpp`      | check exits 1                        | check 1, output above                  |
| `b1-bad-behaviour.cpp` | check exits 0, probes fail           | check 0; 34 of 59 facts failed         |

`b1-good` is an implementation of B1's `spec.md` in this arm's language, written by hand and
checked only by B1's own probes. That the probes pass it shows the arm builds, serves and is
probed correctly. It says nothing about how well a model would write B1 in this language.

## What this arm does not do

- **It is not scored by the runner yet.** `extractor = "none"`, as for `mzizi-be`: a clean
  episode is clean but unscored until the runner calls the probe crate.
- **No episode has run.** Phase 0 has not passed its kill criterion
  ([`../../READINESS.md`](../../READINESS.md)), and this arm is the comparison side of that
  criterion, not a result.
- The flags are RFC-0009 §3's, plus `-isystem` for the vendored headers, which RFC-0009 does not mention. Without it the check would print warnings from `httplib.h` that no author wrote.
