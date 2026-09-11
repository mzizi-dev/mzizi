# Mzizi — A Rust Framework for the Agentic Web

## Bundu Foundation Research Charter, v0.1

**Owner:** Bundu Foundation (100% — Mzizi framework, components, and logic are Foundation IP)
**Adjacent, Nyuchi-owned:** the Mzizi console ("Fundi"), active cyber testing
**Publishing:** `@bundu` npm scope
**Status:** charter draft — defines direction and phasing, not a sprint plan

> **Interim location note:** this directory lives inside `nyuchi/mzizi-tools` only as a
> temporary host. Per the charter's ownership line, this project is 100% Bundu Foundation
> IP, distinct from the Nyuchi-owned Mzizi design system/registry (`nyuchi/mzizi`) and the
> Mzizi console tooling elsewhere in this repo. It moves to its own repo in the dedicated
> Mzizi org, **`mzizi-dev`**, which exists and is Foundation-governed (`mzizi-dev/roadmap` —
> "The Mzizi Roadmap by the Bundu Foundation" — plus org defaults in `mzizi-dev/.github`).
> Not `bundu-labs`: that is the Foundation's general org and holds unrelated work. So the
> move is scheduling, not a prerequisite. Keep this directory self-contained (no
> dependencies on the rest of this repo beyond what Phase 0 genuinely needs) so that move is
> a straight copy, not a untangling exercise.

---

## 1. The thesis

Every major web framework won by being unmistakably better at one thing first, not by matching every existing framework's full feature set on day one. React was Facebook's internal fix for one rendering problem before it was a framework. Vue was one person's answer to Angular's complexity. Svelte bet on a single contrarian idea — compile the framework away instead of shipping it — years before anyone else took that seriously. Next.js didn't have edge runtime or the app router at launch; React Native came years after React itself.

Mzizi's single sharp edge: **a Rust framework whose syntax, type system, and compiler feedback loop are designed for machine authorship, not just human ergonomics.** Every existing framework — Rust or otherwise — was designed assuming a human is typing, reading docs, and holding context in their head. None of them are designed for the actual bottleneck of 2026-and-beyond development: an agent iterating against a compiler in a tight loop, thousands of times, where compile speed, error density, and token-efficient representation are first-order metrics, not nice-to-haves.

That's the claim worth making. Everything else — cross-platform reach, ML integration, edge deployment — is Mzizi _integrating_ with what already exists well, not Mzizi out-building specialist projects at their own game.

## 2. What Mzizi is (and isn't)

| Layer                                                       | Approach                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ----------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Syntax + type system + compiler**                         | Novel. This is the actual research contribution and the thing worth defending as IP.                                                                                                                                                                                                                                                                                                                                                                |
| **Cross-platform rendering** (server, web, mobile, desktop) | **Interop with Dioxus's existing renderer**, not a rebuilt rendering engine. Dioxus has already spent years solving "one codebase, four platforms." Reinventing it doesn't buy you anything the syntax/compiler layer needs — it only slows Phase 0 down.                                                                                                                                                                                           |
| **ML workloads**                                            | **Native integration with Candle.** Mzizi components declare and consume ML inference as a first-class capability; Mzizi does not build a competing tensor runtime.                                                                                                                                                                                                                                                                                 |
| **Edge**                                                    | First-class deployment target from day one — this is the one place you already have real, working experience (Cloudflare Workers, `workers-rs`), so it's cheap to claim early and hard to fake if you skipped it.                                                                                                                                                                                                                                   |
| **Compiled artifact**                                       | **WASM (and native, for desktop) is the actual target — not Astro, not any web framework.** Most Mzizi builds never touch Astro at all: mobile apps run the compiled artifact through Dioxus's mobile renderer, desktop through its native renderer, most web builds embed the raw WASM output directly. Astro is one thin, optional distribution surface among several that happens to consume this artifact — it is not a platform Mzizi targets. |
| **Post-quantum cryptography**                               | **Explicitly deferred.** Named as a future research thread, not part of this charter's scope. Competing with `liboqs`/`pqcrypto` on PQC primitives is a separate, fully-loaded research bet — don't let it dilute Phase 0.                                                                                                                                                                                                                          |

## 3. What "built for machine authorship" concretely means

This needs to cash out as measurable design goals, not a slogan:

- **Low syntactic ambiguity.** Fewer distinct-but-equivalent ways to express the same intent. Every degree of freedom in "how you could have written this" is a degree of freedom an LLM can get subtly wrong. Optimize the surface syntax to minimize that space.
- **Dense, high-signal compiler errors.** The agentic loop is: generate → compile → read error → fix → recompile. The quality of that loop is bounded by how much _actionable_ information is packed into the compiler's error output per character. This is a compiler UX problem aimed at a machine reader, not a human one.
- **Fast incremental compilation.** Human developers tolerate a few seconds per iteration. An agent iterating hundreds of times per session treats compile latency as the dominant cost of the whole workflow. Compile speed is a Phase 0 success metric, not an optimization to defer.
- **Token-efficient representation.** A codebase that fits more real logic into an LLM's context window per token spent is a codebase an agent can reason about more completely, with less summarization loss. This applies to both the syntax surface and any intermediate representation Mzizi tooling exposes to an agent.

## 4. Phasing

Research portfolios ship one thread at a time or nothing ships. This is the order, not a wishlist:

**Phase 0 — Prove the core claim, no rendering attached.**
A standalone compiler/syntax prototype with zero UI story. Success criterion: a defined benchmark where an LLM agent authors N equivalent components in Mzizi's syntax vs. raw Dioxus/Leptos, measured on tokens consumed, iterations to a clean compile, and defect rate. If this doesn't show a measurable advantage, nothing downstream matters — don't build Phase 1 until Phase 0 has a real number attached to it.

**Phase 1 — Rendering interop, WASM/native as the real deliverable.**
Wire Mzizi's compiler output into Dioxus's existing renderer for server/web/mobile/desktop. The artifact that comes out of this phase — a working WASM bundle (and native binary, for desktop) — _is_ the cross-platform product. It must stand on its own, embeddable via a bare `<script type="module">`, a mobile WebView or native WASM host, or an edge runtime, with no framework-specific packaging assumed. This is integration work against Dioxus's renderer, not framework-building — treat any temptation to build a native renderer here as scope creep until interop has been tried and found genuinely insufficient.

**Phase 2 — Edge-first deployment.**
Native target using existing Cloudflare/`workers-rs` experience. Cheapest phase to execute given current team capability.

**Phase 3 — Candle integration.**
First-class support for declaring ML inference inside Mzizi components, backed by Candle. Not a competing ML runtime.

**Phase 4 — Distribution adapters, Astro first among several.**
Once Phase 1's WASM artifact exists and stands alone, framework-specific adapters are thin packaging layers on top of it, not new compiler work. The Astro package — using the Custom Element pattern already scoped in the separate build-out doc — is the first of these because it's furthest along, not because Astro is a target platform. Treat it as proof the standalone artifact from Phase 1 is genuinely embeddable, not as the web deliverable itself. Additional adapters (other web frameworks, a mobile packaging convention, a desktop installer story) belong at this same tier, added as demand appears — none of them require touching Phase 0–3.

## 5. Explicit non-goals for this charter

- Not building a competing tensor/ML runtime. Candle is the dependency.
- Not building a native cross-platform renderer in Phase 0 or 1. Dioxus interop first; native renderer only if interop is proven insufficient.
- Not addressing post-quantum cryptography in this charter. Future thread, not this one.
- Not blocked by, or blocking, Nyuchi/Mukoko revenue-phase work — different org, different clock, per Bundu Foundation's research mandate.

## 6. Phase 0 benchmark design — resolved

The charter originally flagged this as the one decision that had to be made before work starts. Resolved 2026-08-23:

- **Task set:** the fixed, known-ground-truth component set is **Mzizi's own components** — the
  design system already built out in `nyuchi/mzizi` (571+ components, partially ported to Rust
  across N7–N11 as of this decision). The agent authors these in Mzizi-lang syntax against the
  existing `.tsx`/`.rs` implementations as ground truth. This is explicitly **not** a port of an
  external library (shadcn, a generic primitive set, etc.) — Mzizi's own components are the
  benchmark corpus, full stop.
- **Defect definition:** a defect is code that **compiles cleanly but is behaviorally wrong** —
  it passes the compiler but fails a contract/behavior test against the reference
  implementation. This mirrors the contract-test pattern already used for the `.tsx` → `.rs`
  ports in `nyuchi/mzizi` (`mzizi-rs/crates/*/tests/contract.rs`): the reference is read from
  disk, and disagreement is the new code's fault unless it's a documented, deliberate
  divergence. A syntax/compile error is not itself a "defect" for this metric — it's the normal,
  expected friction the compile-error-density design goal (§3) is trying to minimize; the defect
  rate measures what gets _past_ the compiler wrong.
- **Task-set visibility:** the benchmark **harness** is public — runner, metric definitions,
  scoring code, fixture format. The **held-out task set and its expected outputs are not**, and
  live in a separate private repository. This is a measurement-validity requirement, not a
  secrecy preference: a public task set gets scraped into training data, after which the
  benchmark measures memorisation, and a contaminated benchmark looks like a successful one — so
  the kill criterion below could never fire. See
  [`design/RFC-0004-test-topology.md`](./design/RFC-0004-test-topology.md), which also fixes the
  rule that keeps the project forkable: private consumes public, public never consumes private.
- **Repo/ownership:** work starts in the existing Nyuchi-accessible repos (this directory, inside
  `mzizi-tools`) as an interim home. Mzizi-the-framework will move to its own repo in the
  dedicated Mzizi org, `mzizi-dev`, which already exists — see the interim-location note at
  the top of this file.

## 7. Open questions (not yet resolved)

- **~~The actual Mzizi-lang syntax and type system.~~** Designed, and a front end exists:
  [RFC-0001](./design/RFC-0001-syntax.md) (syntax),
  [RFC-0002](./design/RFC-0002-runtime-and-prior-art.md) (design target and prior art),
  [RFC-0003](./design/RFC-0003-ir.md) (the content-addressed IR),
  [RFC-0006](./design/RFC-0006-contracts.md) (contract evaluation). What remains open inside
  it: **the reference-implementation half of §6's defect metric.** `mz contract` evaluates a
  component's assertions against that component, so the metric can be expressed and run in
  the language rather than as a hand-written Rust test. §6 defines a defect as failing a
  contract test *against the reference implementation*, and nothing yet reads the `.rs`
  reference off disk and compares. RFC-0006 §10.1.
- **Benchmark harness mechanics.** How agent runs are invoked, sandboxed, and scored
  end-to-end (which components from the 571+ corpus, how many per run, how "tokens consumed"
  and "iterations to clean compile" are actually measured and reported) is unspecified past the
  task-set/defect-rate decisions above.
