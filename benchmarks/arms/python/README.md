# The `python` arm

Python with FastAPI for the `backend` task family (RFC-0009 §1, §3). An episode writes one `.py` file with a module-level `app = FastAPI()`.
Nothing has run in this arm: no model episode, no score. The guide is
[`../../prompts/python-guide.md`](../../prompts/python-guide.md), written to the family's token budget
([`../../prompts/BUDGET.md`](../../prompts/BUDGET.md)).

| Part       | What it is                                                                                                                                                               |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `arm.toml` | the arm, in the format the runner README specifies ("Arms: `arm.toml`")                                                                                                  |
| check      | `check.sh <candidate>`: `pyright` in its default `standard` mode, against the arm's virtualenv (`sandbox/run-check.sh`). Exit 0 clean, 1 errors (printed), 2 setup error |
| serving    | `serve.sh <candidate>`: builds the candidate and execs its server on `$PORT`, for `mzprobe serve`                                                                        |
| sandbox    | `sandbox/`: a Linux container image, `mzizi-arm-python`, built by `setup.sh` from `sandbox/Containerfile`                                                                |
| fixtures   | `fixtures/b1-good.py`, `b1-bad-type.py`, `b1-bad-behaviour.py`: see "Verifying the arm"                                                                                  |

`check.sh`, `serve.sh` and `setup.sh` are one-line calls to [`../lib/`](../lib): `arm-run.sh`
runs the sandbox, `build-image.sh` builds it, `probe.sh` probes a candidate with a task's
probes, and `self-test.sh` is the verification below.

## Serving

`sandbox/run-serve.sh` runs `python -m uvicorn <module>:app` on `0.0.0.0:$PORT`. An author writes no server setup.

## Pinned versions

| Package                    | Pin                            | Why it is here                                                              |
| -------------------------- | ------------------------------ | --------------------------------------------------------------------------- |
| `fastapi`                  | `0.162.0`                      | the framework                                                               |
| `pydantic`                 | `2.14.1`                       | v2, as RFC-0009 §1 says (`pydantic_core` `2.50.1`)                          |
| `uvicorn`                  | `0.54.0`                       | the server RFC-0009 §1 names                                                |
| `starlette` and eight more | see `sandbox/requirements.txt` | transitive; every package is pinned                                         |
| `pyright`                  | `1.1.414`                      | the checker, from npm (`sandbox/package-lock.json`), run by the system Node |

RFC-0009 §1 leaves the pins "chosen when the arm lands": these are what `pip install fastapi pydantic uvicorn` resolved to on 2026-10-29, copied from `pip freeze` (without hashes: `pip` is run without `--require-hashes`). Python is Alpine 3.22's `python3` (`3.12.16` when the image was built), not pinned to a patch.

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

What the author sees for `fixtures/b1-bad-type.py`, from a real run:

```text
b1-bad-type.py
  b1-bad-type.py:6:14 - error: Type "Literal['2']" is not assignable to declared type "int"
    "Literal['2']" is not assignable to "int" (reportAssignmentType)
1 error, 0 warnings, 0 informations
```

## Verifying the arm

`../lib/self-test.sh python` checks the arm against B1 (`benchmarks/tasks/b1-routing`), with
three files written for it. None is shown to an author or a model.

| Fixture               | Expected                             | Result, 2026-10-29                     |
| --------------------- | ------------------------------------ | -------------------------------------- |
| `b1-good.py`          | check exits 0, and every probe holds | check 0; `mzprobe` 59 of 59 facts held |
| `b1-bad-type.py`      | check exits 1                        | check 1, output above                  |
| `b1-bad-behaviour.py` | check exits 0, probes fail           | check 0; 35 of 59 facts failed         |

`b1-good` is an implementation of B1's `spec.md` in this arm's language, written by hand and
checked only by B1's own probes. That the probes pass it shows the arm builds, serves and is
probed correctly. It says nothing about how well a model would write B1 in this language.

## What this arm does not do

- **It is not scored by the runner yet.** `extractor = "none"`, as for `mzizi-be`: a clean
  episode is clean but unscored until the runner calls the probe crate.
- **No episode has run.** Phase 0 has not passed its kill criterion
  ([`../../READINESS.md`](../../READINESS.md)), and this arm is the comparison side of that
  criterion, not a result.
- Pyright and `tsc` are different promises from `cargo check` and `go vet` (RFC-0009 §3 says so on purpose). The pyright output keeps its `N errors, ...` count line, which the other checkers do not print.
