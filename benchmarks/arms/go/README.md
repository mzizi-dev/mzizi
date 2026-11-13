# The `go` arm

Go with net/http for the `backend` task family (RFC-0009 §1, §3). An episode writes one `.go` file, `package main`, standard library only.
Nothing has run in this arm: no model episode, no score. The guide is
[`../../prompts/go-guide.md`](../../prompts/go-guide.md), written to the family's token budget
([`../../prompts/BUDGET.md`](../../prompts/BUDGET.md)).

| Part       | What it is                                                                                                                                                                                                                 |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `arm.toml` | the arm, in the format the runner README specifies ("Arms: `arm.toml`")                                                                                                                                                    |
| check      | `check.sh <candidate>`: `go build ./...`, then `go vet ./...` (`sandbox/run-check.sh`); output is the tools' own, so a build error is preceded by Go's `# candidate` line. Exit 0 clean, 1 errors (printed), 2 setup error |
| serving    | `serve.sh <candidate>`: builds the candidate and execs its server on `$PORT`, for `mzprobe serve`                                                                                                                          |
| sandbox    | `sandbox/`: a Linux container image, `mzizi-arm-go`, built by `setup.sh` from `sandbox/Containerfile`                                                                                                                      |
| fixtures   | `fixtures/b1-good.go`, `b1-bad-type.go`, `b1-bad-behaviour.go`: see "Verifying the arm"                                                                                                                                    |

`check.sh`, `serve.sh` and `setup.sh` are one-line calls to [`../lib/`](../lib): `arm-run.sh`
runs the sandbox, `build-image.sh` builds it, `probe.sh` probes a candidate with a task's
probes, and `self-test.sh` is the verification below.

## Serving

`sandbox/run-serve.sh` runs `go build` and execs the binary. The candidate's `main` reads `PORT` and listens on `0.0.0.0`.

## Pinned versions

| Part                 | Pin                                                                                            | Why it is here                                                                      |
| -------------------- | ---------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `golang:1.24-alpine` | digest `sha256:8bee1901f1e530bfb4a7850aa7a479d17ae3a18beb6e09064ed54cfd245b7191` (`go1.24.13`) | the exact Go toolchain version RFC-0009 §1 asks for                                 |
| Go modules           | none                                                                                           | the standard library is the whole dependency set, so there is no `go.sum` to commit |

`install.sh` warms the build cache for `net/http`, `encoding/json` and a few other packages so a check does not recompile the standard library. `GOPROXY=off` and `GOTOOLCHAIN=local` make a missing module an error and not a download.

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

What the author sees for `fixtures/b1-bad-type.go`, from a real run:

```text
# candidate
./b1-bad-type.go:11:18: cannot use "2" (untyped string constant) as int value in variable declaration
```

## Verifying the arm

`../lib/self-test.sh go` checks the arm against B1 (`benchmarks/tasks/b1-routing`), with
three files written for it. None is shown to an author or a model.

| Fixture               | Expected                             | Result, 2026-10-29                     |
| --------------------- | ------------------------------------ | -------------------------------------- |
| `b1-good.go`          | check exits 0, and every probe holds | check 0; `mzprobe` 59 of 59 facts held |
| `b1-bad-type.go`      | check exits 1                        | check 1, output above                  |
| `b1-bad-behaviour.go` | check exits 0, probes fail           | check 0; 35 of 59 facts failed         |

`b1-good` is an implementation of B1's `spec.md` in this arm's language, written by hand and
checked only by B1's own probes. That the probes pass it shows the arm builds, serves and is
probed correctly. It says nothing about how well a model would write B1 in this language.

## What this arm does not do

- **It is not scored by the runner yet.** `extractor = "none"`, as for `mzizi-be`: a clean
  episode is clean but unscored until the runner calls the probe crate.
- **No episode has run.** Phase 0 has not passed its kill criterion
  ([`../../READINESS.md`](../../READINESS.md)), and this arm is the comparison side of that
  criterion, not a result.
- Go has no framework here, as RFC-0009 §1 chose: routing, HEAD and 405 handling are whatever the author writes, or `ServeMux` gives (the guide says which).
