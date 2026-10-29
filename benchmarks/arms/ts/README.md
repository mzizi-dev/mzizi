# The `ts` arm

TypeScript with Hono for the `backend` task family (RFC-0009 §1, §3). An episode writes one `.ts` file whose default export is a Hono app.
Nothing has run in this arm: no model episode, no score. The guide is
[`../../prompts/ts-guide.md`](../../prompts/ts-guide.md), written to the family's token budget
([`../../prompts/BUDGET.md`](../../prompts/BUDGET.md)).

| Part       | What it is                                                                                                                                                               |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `arm.toml` | the arm, in the format the runner README specifies ("Arms: `arm.toml`")                                                                                                  |
| check      | `check.sh <candidate>`: `tsc --noEmit --pretty false` with `strict: true` (`sandbox/tsconfig.json`: ES2022, `NodeNext`). Exit 0 clean, 1 errors (printed), 2 setup error |
| serving    | `serve.sh <candidate>`: builds the candidate and execs its server on `$PORT`, for `mzprobe serve`                                                                        |
| sandbox    | `sandbox/`: a Linux container image, `mzizi-arm-ts`, built by `setup.sh` from `sandbox/Containerfile`                                                                    |
| fixtures   | `fixtures/b1-good.ts`, `b1-bad-type.ts`, `b1-bad-behaviour.ts`: see "Verifying the arm"                                                                                  |

`check.sh`, `serve.sh` and `setup.sh` are one-line calls to [`../lib/`](../lib): `arm-run.sh`
runs the sandbox, `build-image.sh` builds it, `probe.sh` probes a candidate with a task's
probes, and `self-test.sh` is the verification below.

## Serving

`sandbox/run-serve.sh` compiles the candidate with the same `tsc`, and `sandbox/server.mjs` serves its default export with `@hono/node-server` on `0.0.0.0:$PORT`. An author writes no server setup, as a Mzizi author does not.

## Pinned versions

| Package             | Pin       | Why it is here                                                                                                                                                        |
| ------------------- | --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `hono`              | `4.13.10` | the framework; the version, and the `integrity` hash, of `mzizi-dev/mzizi-api`'s `package-lock.json` at `2468b2a` (the API gateway `api.mzizi.dev` runs, RFC-0009 §1) |
| `typescript`        | `5.9.3`   | the checker; the same lockfile's version                                                                                                                              |
| `@hono/node-server` | `2.1.5`   | the Node host RFC-0009 §2.3 names ("Node is the host"); not in the gateway's lockfile, because the gateway runs on workerd, so it is the latest release on 2026-10-29 |
| `@types/node`       | `22.20.5` | Node's types, for `node:` imports; the latest 22.x on 2026-10-29                                                                                                      |

`sandbox/package-lock.json` is `npm install --package-lock-only`'s resolution of those four (six packages with `undici-types` and `hono`'s peers: see the file). `install.sh` runs `npm ci`, which fails rather than change it. Node itself is Alpine 3.22's `nodejs` package (`v22.23.4` when the image was built on 2026-10-29), not pinned to a patch.

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

What the author sees for `fixtures/b1-bad-type.ts`, from a real run:

```text
b1-bad-type.ts(4,7): error TS2322: Type 'string' is not assignable to type 'number'.
```

## Verifying the arm

`../lib/self-test.sh ts` checks the arm against B1 (`benchmarks/tasks/b1-routing`), with
three files written for it. None is shown to an author or a model.

| Fixture               | Expected                             | Result, 2026-10-29                     |
| --------------------- | ------------------------------------ | -------------------------------------- |
| `b1-good.ts`          | check exits 0, and every probe holds | check 0; `mzprobe` 59 of 59 facts held |
| `b1-bad-type.ts`      | check exits 1                        | check 1, output above                  |
| `b1-bad-behaviour.ts` | check exits 0, probes fail           | check 0; 38 of 59 facts failed         |

`b1-good` is an implementation of B1's `spec.md` in this arm's language, written by hand and
checked only by B1's own probes. That the probes pass it shows the arm builds, serves and is
probed correctly. It says nothing about how well a model would write B1 in this language.

## What this arm does not do

- **It is not scored by the runner yet.** `extractor = "none"`, as for `mzizi-be`: a clean
  episode is clean but unscored until the runner calls the probe crate.
- **No episode has run.** Phase 0 has not passed its kill criterion
  ([`../../READINESS.md`](../../READINESS.md)), and this arm is the comparison side of that
  criterion, not a result.
- The candidate exports an app instead of calling `serve`, which is the Hono idiom on both Node and workerd. Go, C++ and Rust candidates must write `main`; that is how those ecosystems are written, and the guides say so.
