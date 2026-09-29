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
48px — and it diffs that derived number against the `.mz` file's height for the matching
variant: its declared `height` column, or, for a row with none, the same derivation from its
own class. A mismatch is the Phase 0 defect: code whose own contract can hold but which is
wrong against the reference.

_Amended 2026-09-29 (FM-11)._ This paragraph used to say an agent could write `height 44` right
next to a class of `h-12`, and `mz contract` had no ground truth to refute it against. Pilot 2
showed that exact shape in both clean 7B buttons (`size-14` with `height 48`). The compiler now
reads the same spacing scale: a declared `height` that disagrees with its class is `MZ0313`, a
compile error with the rendered number as its `exact` fix, and a row may leave `height` out, so
the fact is written once. What remains for the reference diff is a height that is consistent
and wrong, like `sm class "h-11"` where the reference says `h-12`.

This is deliberately bounded — a regex-level scan of enum blocks and match arms, not a general
Rust parser, and not a general Tailwind resolver. Arbitrary-value classes (`h-[56px]`) are `mz
contract`'s job (RFC-0006 §5), not this harness's. Missing variants on either side are reported
as defects too, never silently skipped, for the same reason RFC-0006's FM-12 treats an
unevaluable assertion as a failure rather than a pass.

Variants are matched by name. The one exception is opt-in, per task: a task whose
`task.toml` sets `allow_variant_renames = true` (with a required `rename_reason`) is scored
with `score --allow-variant-renames`, and then, when the candidate's and the reference's
variant-name sets differ, the scorer pairs variants by class-token Jaccard instead. It pairs
each reference variant with its best candidate, and accepts the pairing only if it is a
complete bijection, every pair is mutual-best with no ties, every pair's similarity is at
least 0.5, and any name the two sides share pairs with itself. Then the variant set is not a
defect, a `variant_names` detail lists each renamed pair (`candidate -> reference`, with its
similarity), the score JSON's `renames` counts them, and the default, heights and jaccard are
read through the pairing — so a class difference inside a pair lowers the jaccard and is never
a defect. `variant_names` is not a checked fact: it is in `details` but not `facts_checked`.
Otherwise, and for every task that does not opt in, matching is by name only — a candidate
that renames `destructive` to `danger` against its spec scores that as a defect. The opt-in
exists because a spec and its reference can disagree on names: only
`mzizi-changelog-renderer` sets it (see [`tasks/README.md`](tasks/README.md), and the pilot
that found it in [`results/2026-09-27-pilot/RUN.md`](results/2026-09-27-pilot/RUN.md)).

A task may also opt in to one fact that is not about enums: with `score_slots = true` in its
`task.toml`, `score --slots` compares the set of literal `data-slot` values the port renders
(`"data-slot": "…"` in Rust, `slot = "…"` in Mzizi) with the reference's. The registry's
`data-slot` names are its styling contract, and the fact makes a component with no enum
scoreable. Only `card` sets it; the pilot tasks keep the facts they were scored on.

Sequencing: the harness requires `mz contract --agent <file.mz>` to exit 0 _before_ it runs the
reference diff. A component that fails its own contract is reported as that failure and the
reference diff does not run — the two checks answer different questions, and conflating them
would repeat RFC-0006 §7's own argument (FM-13) one level up.

### What's built, and what still isn't

`benchmarks/harness/` (`mzizi-benchmark-harness`, a Cargo workspace member alongside
`compiler/`) implements the mechanism above and is proved end to end against
`primitives/button.mz` — a real component in this repo — using a byte-identical copy of the
real `button.rs` reference at registry commit `3afeb75`
(`benchmarks/harness/tests/fixtures/button_reference.rs`; provenance in that directory's
README).

**Correction.** The first version of this harness (PR #10) shipped a fixture whose header
called it "copied verbatim" from the registry. It was a reconstruction, written without
access to the registry, and it matched the parser rather than the file. Run against the
real `button.rs`, that harness reported five false defects: it read the `slug()` match arms
as class strings, missed the block-bodied `classes()` arms, and merged `ButtonVariant` and
`ButtonSize` because both have a `Default` variant. The extractor now reads each `impl`
block's `classes()` method only, and the fixture is `cmp`-identical to the real file. This
is the FM-10 failure — a check that reads as verification without being one — in the harness
that exists to catch it. `benchmarks/harness/tests/button_diff.rs` asserts both the
passing case and a deliberately-broken one (a copy of `button.mz` whose `sm` variant is
`h-11` / `height 44`, with its own contract weakened to match — the exact "satisfies its own
contract, wrong against the reference" shape this harness exists to catch).

**Still open**: this is a prototype proved against one component and one fixture, not a scored
run over the corpus. How many components a real run covers, how agent runs are invoked and
sandboxed, how tokens-consumed and iterations-to-clean-compile are measured and reported end to
end, and the held-out task set itself (below) all remain unresolved.

## Recorded runs

Scored runs are committed under [`results/`](results/), one directory per run, each with a
`RUN.md` stating how it was produced and what it does not show. The first is a pilot:
[`results/2026-09-27-pilot/`](results/2026-09-27-pilot/RUN.md) — `mzbench` Mode 2, 3 tasks ×
2 arms (mzizi, dioxus), one seed, one frontier model, every episode clean on iteration 1, and
no token counts. It is not the Phase 0 number: it has no small-model arm, and the Mzizi
checker it ran against cannot fail on type or interpolation names. It is recorded because it
found a harness false positive (the changelog task's renamed variants, fixed above), and so
that the next run has something exact to be compared against.

The second, the same day, adds the arm the first is missing:
[`results/2026-09-27-pilot-2/`](results/2026-09-27-pilot-2/RUN.md) — the same two scored tasks
at the same `ded425a`, three seeds, measured tokens, and Qwen2.5-Coder-7B on CPU alongside a
frontier model (`benchmarks/openweight/`). It is not the Phase 0 number either. It is the only
small-model data there is, and on it Mzizi did worse than Dioxus on all three metrics; the
write-up traces why.

**Before the next run:** [`READINESS.md`](READINESS.md) audits pilot 2's list of fixes against
`main`, records which are done, and gives the command for the kill-criterion run.
[`kill-criterion/`](kill-criterion/README.md) holds its driver, the task checker and the
held-out plan. Every run is published here, whichever way it falls.

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
