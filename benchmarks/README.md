# Phase 0 benchmark

Resolved so far (see `../CHARTER.md` §6):

- **Corpus:** Mzizi's own components (`nyuchi/mzizi`'s 571+ component registry, partially
  ported to Rust) — the agent authors these in Mzizi-lang against the existing `.tsx`/`.rs`
  implementations as ground truth. Not an external library port.
- **Defect metric:** a defect is code that compiles cleanly but fails a contract/behavior test
  against the reference implementation — the same contract-test pattern already used for the
  `.tsx` → `.rs` ports (`mzizi-rs/crates/*/tests/contract.rs` in `nyuchi/mzizi`).

**Not yet resolved** (charter §7): the harness mechanics — how many components per run, how
agent runs are invoked/sandboxed, how "tokens consumed" and "iterations to a clean compile" are
actually measured and reported end-to-end. Nothing here should be built until that's designed
alongside the language syntax itself, since the harness has to invoke a compiler that doesn't
exist yet.
