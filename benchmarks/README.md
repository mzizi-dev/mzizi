# Phase 0 benchmark

Resolved so far (see `../CHARTER.md` §6):

- **Corpus:** Mzizi's own components (`nyuchi/mzizi`'s 571+ component registry, partially
  ported to Rust) — the agent authors these in Mzizi-lang against the existing `.tsx`/`.rs`
  implementations as ground truth. Not an external library port.
- **Defect metric:** a defect is code that compiles cleanly but fails a contract/behavior test
  against the reference implementation — the same contract-test pattern already used for the
  `.tsx` → `.rs` ports (`mzizi-rs/crates/*/tests/contract.rs` in `nyuchi/mzizi`).

## What the toolchain can and cannot score today

`mz contract <file>` exists ([RFC-0006](../design/RFC-0006-contracts.md)) and exits 1 when an
assertion does not hold, so **"compiles cleanly but is behaviourally wrong" is now expressible
and runnable**: `mz check` exits 0 and `mz contract` exits 1 on exactly that shape. The harness
can read the defect count off `mz contract --agent`'s summary line — `contract_failures`.

What it cannot do is the words **"against the reference implementation"** in the metric above.
`mz contract` checks a component against its own declarations, which is self-consistency. An
agent that writes both a component and its contract can satisfy it while diverging from the
hand-written `.rs`. Until that gap is closed (RFC-0006 §10.1 states three candidate designs and
chooses none), a defect-rate number produced by this harness would be measuring the wrong
thing — and flatteringly, which is the dangerous direction.

**Not yet resolved** (charter §7): the reference comparison above, and the harness mechanics — how many components per run, how
agent runs are invoked/sandboxed, how "tokens consumed" and "iterations to a clean compile" are
actually measured and reported end-to-end. Nothing here should be built until that's designed
alongside the language syntax itself, since the harness has to invoke a compiler that doesn't
exist yet.

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
