# Phase 0 benchmark

Resolved so far (see `../CHARTER.md` §6):

- **Corpus:** Mzizi's own components (`mzizi-dev/mzizi-registry`'s 571+ component registry,
  partially ported to Rust) — the agent authors these in Mzizi-lang against the existing
  `.tsx`/`.rs` implementations as ground truth. Not an external library port.
- **Defect metric:** a defect is code that compiles cleanly but fails a contract/behavior test
  against the reference implementation — the same contract-test pattern already used for the
  `.tsx` → `.rs` ports (`tests/contract.rs` in `mzizi-dev/mzizi-registry`).

## What the toolchain can and cannot score today

`mz contract <file>` exists ([RFC-0006](../design/RFC-0006-contracts.md)) and exits 1 when an
assertion does not hold, so **"compiles cleanly but is behaviourally wrong" is now expressible
and runnable**: `mz check` exits 0 and `mz contract` exits 1 on exactly that shape. The harness
can read the defect count off `mz contract --agent`'s summary line — `contract_failures`.

What it cannot do on its own is the words **"against the reference implementation"** in the
metric above. `mz contract` checks a component against its own declarations, which is
self-consistency. An agent that writes both a component and its contract can satisfy it while
diverging from the hand-written `.rs`. **That gap is now closed, harness-side** (RFC-0006
§10.1): `mz contract` stays a self-consistency checker only — no cross-language semantics were
added to the compiler — and `benchmarks/harness/` does the reference comparison instead.

### The mechanism

A `.mz` size enum already declares both a Tailwind class and a pixel height per variant.
`primitives/button.mz`'s `button_size` enum writes:

```mz
enum button_size
  default   class "h-14 gap-2 px-5"     height 56
  sm        class "h-12 gap-1.5 px-4"   height 48
  lg        class "h-14 gap-2 px-6"     height 56
  icon      class "size-14"             height 56
  icon_sm   class "size-12"             height 48
end
```

The hand-written Rust reference in `mzizi-dev/mzizi-registry` carries only the Tailwind class,
in a `<Enum>::<Variant> => "<classes>"` match arm (`button.rs`'s `ButtonSize::classes()`) — no
pixel height anywhere. The harness derives one, from the one well-known linear Tailwind spacing
scale: `value-in-rem = N * 0.25rem`, and a browser's default `1rem = 16px`, so `h-N` and
`size-N` are both `N * 4` pixels. `h-14` → 56px, `h-12` → 48px, `size-14` → 56px, `size-12` →
48px — and it diffs that derived number against the `.mz` file's own declared `height` for the
matching variant. A mismatch is the Phase 0 defect: code whose own contract can even hold (an
agent can write `button_size.sm height is 44` right next to a class of `h-12`, and `mz
contract` has no ground truth to refute it against) but which is wrong against the reference.

This is deliberately bounded — a regex-level scan of enum blocks and match arms, not a general
Rust parser, and not a general Tailwind resolver. Arbitrary-value classes (`h-[56px]`) are `mz
contract`'s job (RFC-0006 §5), not this harness's. Missing variants on either side are reported
as defects too, never silently skipped, for the same reason RFC-0006's FM-12 treats an
unevaluable assertion as a failure rather than a pass.

Sequencing: the harness requires `mz contract --agent <file.mz>` to exit 0 _before_ it runs the
reference diff. A component that fails its own contract is reported as that failure and the
reference diff does not run — the two checks answer different questions, and conflating them
would repeat RFC-0006 §7's own argument (FM-13) one level up.

### What's built, and what still isn't

`benchmarks/harness/` (`mzizi-benchmark-harness`, a Cargo workspace member alongside
`compiler/`) implements the mechanism above and is proved end to end against
`primitives/button.mz` — a real component in this repo — using a checked-in fixture copy of the
real `button.rs` reference (`benchmarks/harness/tests/fixtures/button_reference.rs`; this
session had no access to `mzizi-dev/mzizi-registry`, so the fixture cites the file path it
mirrors rather than a commit). `benchmarks/harness/tests/button_diff.rs` asserts both the
passing case and a deliberately-broken one (a copy of `button.mz` whose `sm` variant declares
`height 44` while its class stays `h-12`, with its own contract weakened to match — the exact
"satisfies its own contract, wrong against the reference" shape this harness exists to catch).

**Still open**: this is a prototype proved against one component and one fixture, not a scored
run over the corpus. How many components a real run covers and the held-out task set itself
(below) remain unresolved. How agent runs are invoked and sandboxed, and how
tokens-consumed and iterations-to-clean-compile are measured and reported end to end, are
partly resolved — see the next section for exactly how much.

## Run orchestration: what's real, and what's a stand-in

`benchmarks/runner/` (`mzizi-benchmark-runner`, a workspace member alongside `compiler/` and
`benchmarks/harness/`) is the other half of the two gaps `design/ROADMAP.md`'s table names:
nothing before it actually invoked the compile loop and recorded an outcome — the harness
above only scores a `.mz` file that already exists.

**Run it yourself:**

```sh
cargo run -p mzizi-benchmark-runner --bin mzizi-benchmark-runner -- run \
  --component button \
  --reference benchmarks/harness/tests/fixtures/button_reference.rs \
  --candidate benchmarks/runner/fixtures/button_syntax_error.mz \
  --candidate benchmarks/harness/tests/fixtures/button_broken.mz
```

For each `--candidate`, in the order given, it runs `mz check --agent` (as a subprocess — the
same NDJSON CLI surface an agent driving the toolchain would see, not a linked-in function
call) until one comes back with zero errors; that candidate's 1-based position is
`iterations_to_clean_compile`. It then runs `mz contract --agent` against that clean
candidate — self-consistency must hold before anything else does — and, once it does, hands
the clean candidate and `--reference` to `mzizi-benchmark-harness`'s own library functions for
the reference diff described above. One JSON object is appended to `--results`
(default `benchmarks/results/runs.jsonl`) either way, whichever stage the pipeline reached:

```json
{"component":"button","timestamp":"2026-09-27T04:26:37Z","iterations_to_clean_compile":2,"tokens_consumed":null,"contract_clauses":5,"contract_failures":0,"defect_count":1,"defects":["button_size: FAIL  sm         declared=44px  derived=48px  (h-12) — mismatch"],"elapsed_ms":68,"authoring_mode":"scripted-stand-in","clean_candidate":"benchmarks/harness/tests/fixtures/button_broken.mz","notes":null}
```

That is the actual first entry in `benchmarks/results/runs.jsonl` — a real run, not a
worked example. `iterations_to_clean_compile: 2` because the first candidate
(`button_syntax_error.mz`) doesn't close its `component` block and `mz check` rejects it
(`MZ0204`); the second (`button_broken.mz`) compiles clean and passes its own (weakened)
contract, but the harness's reference diff still catches its `sm` height disagreeing with the
reference's `h-12` — exactly the "compiles cleanly but is behaviourally wrong" shape CHARTER.md
§6 defines as the Phase 0 defect, caught by a real run of the real pipeline, not asserted in a
unit test.

**What's real here:** `mz check`, `mz contract`, and the harness diff are the actual toolchain,
invoked exactly as `benchmarks/harness/src/main.rs` already did for the single-file case, now
chained together and turned into a recorded result. The compile loop is real: it truly runs
`mz check --agent` against each candidate in turn and truly stops at the first clean one.

**What's a stand-in, and why:** the candidates are hand-authored files, not a live agent's
successive attempts. This session tried the obvious alternative — shelling out to the `claude`
CLI from inside its own container — and found that the invocation reused _this very session's_
own model context and billing (a one-line `"pong"` reply alone showed a cache-creation charge
in the tens of thousands of tokens and a nontrivial cost), rather than running as an isolated
agent authoring a component from a blank slate. Wiring that up as "the" measurement would
report this orchestrator's own recursive overhead as if it were a component author's token
count — a worse error than reporting nothing. So `tokens_consumed` is `null` in every run this
crate has produced, and `authoring_mode` is always `"scripted-stand-in"`; a `--tokens-consumed
<n>` flag exists for a future run where a real, isolated agent measures its own usage and
passes the number in, but nothing today computes that number itself. A live-agent run is
still-open work, not a solved one being quietly assumed.

## Where the task set lives (RFC-0004)

Everything in this directory is and stays **public**: the runner, the metric definitions, the
scoring code, and a published fixture format so anyone can write their own task set and run it.

The **held-out task set and its expected outputs are not public.** Not for secrecy — for
measurement validity. If the tasks and answers are on the open web they get scraped into
training data, after which the benchmark measures memorisation rather than the language, and
reports a flattering number for exactly the wrong reason. That failure is invisible from the
inside: a contaminated benchmark looks like a successful one. The charter gives Phase 0 a kill
criterion, and a kill criterion that cannot fire is not a criterion.

See [`../design/RFC-0004-test-topology.md`](../design/RFC-0004-test-topology.md) for the split,
the dependency rule that keeps forks working (private consumes public; public never consumes
private), and the reporting mechanism. The public half of that mechanism is
`.github/workflows/mzizi-lang-benchmark-dispatch.yml`, which is inert until the private
repository exists.
