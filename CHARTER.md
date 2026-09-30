# Mzizi: a general-purpose programming language

> **Mzizi is built to make Rust better, the way TypeScript makes JavaScript better.** That is
> the goal Phase 0 exists to test (§4), not a result. Nothing in this charter has been measured
> against it yet.

## Mzizi Research Charter, v0.4

**Owner:** Mzizi (100% — the Mzizi language, its toolchain, its components and logic are Mzizi IP)
**Adjacent, Nyuchi-owned:** the Mzizi console ("Fundi"), active cyber testing
**Publishing:** `@nyuchi/` npm scope
**Status:** charter draft — defines direction and phasing, not a sprint plan

> **v0.4 changelog — owner-directed 2026-09-30.** Five changes, each marked
> _(owner-directed 2026-09-30)_ where it lands. (1) **Mzizi is a general-purpose programming
> language**, not a framework. Rust is its platform, the way JavaScript is TypeScript's, and
> Mzizi Roots is its component model, the way React is JavaScript's (§1). (2) **One goal:** this
> repository's goal, and Phase 0's only goal, is building Mzizi as a language that stands against
> the best existing language for each kind of task (§1, §4). (3) **The harness is the core of the
> language:** what an agent reads, and what the toolchain and plugins attach to. It lives in this
> repository ([RFC-0012](./design/RFC-0012-harness.md), a draft). (4) **The toolchain and the
> components support the language; they are not the language** (§2). (5) **Phase 0 is scoped to
> the goal:** the component tasks against Dioxus and Leptos, the pilots among them, are tests
> within it, and the 2026-09-29 amendment to §4 is folded into the main text. The gate, the kill
> criterion and the benchmark settings are unchanged, and nothing is claimed before it is
> measured.

<!-- -->

> **v0.3 changelog — owner-directed 2026-09-29.** Four changes, each marked
> _(owner-directed 2026-09-29)_ where it lands. (1) **The backend goal:** Mzizi builds Mzizi's
> entire backend (§1, §2). (2) **Two frontend paths:** Astro with the Mzizi UI (Mzizi Roots)
> underneath, or pure Rust end to end (§2, Phase 4). (3) **Everything has a contract**
> (§3; [RFC-0010](./design/RFC-0010-contracts-everywhere.md)). (4) **The kill criterion is
> Mzizi against the best existing language for each kind of task**, and every benchmark run is
> published (§4, §6; [RFC-0009](./design/RFC-0009-comparison-benchmark.md)). Phase 0's
> discipline is unchanged: the gate still blocks downstream work, and no claim is made before it
> is measured.

<!-- -->

> **v0.2 changelog:** supersedes v0.1's web-only framing. The actual goal was always general-purpose
> software for the agentic world, not just UI components — v0.1 undersold this and, worse,
> contradicted its own §2: that table already called edge deployment "first-class... from day
> one," but §4's phasing sequenced it at Phase 2, behind a generic rendering-interop phase it
> didn't need to wait for. This version fixes both: broadens the target list (§1, §2) and merges
> the old Phase 1+2 into one full-stack deliverable (§4). Phase 0's benchmark design and gate
> discipline (§6, §7) are unchanged.

<!-- -->

> **Location note:** this project lives in its own repo, **`mzizi-dev/mzizi`**, in the
> dedicated Mzizi org, **`mzizi-dev`** (`mzizi-dev/roadmap` plus org defaults in
> `mzizi-dev/.github`). Not `bundu-labs`: that is the Bundu Foundation's general org and
> holds unrelated work. It started inside `nyuchi/mzizi-tools` (since renamed and moved to
> `mzizi-dev/agent-tools`) as a temporary host and has moved out. Per the charter's ownership line, this project is
> 100% Mzizi IP. That includes the component registry (`mzizi-dev/mzizi-registry`), which is
> Mzizi's too. The revenue-generating work (the Mzizi console "Fundi", its tooling in
> `mzizi-dev/agent-tools`, paid plans and billing) is Nyuchi's.

---

## 1. The thesis

Every language and framework that won did so by being unmistakably better at one thing first, not by matching every incumbent's full feature set on day one. TypeScript did not replace JavaScript: it made JavaScript better to write, and it still compiles to JavaScript and runs wherever JavaScript runs. React was Facebook's internal fix for one rendering problem before it was a framework. Svelte bet on a single contrarian idea — compile the framework away instead of shipping it — years before anyone else took that seriously.

**What Mzizi is** _(owner-directed 2026-09-30)_**: a general-purpose programming language, with Rust as its platform, the way JavaScript is TypeScript's.** Mzizi is designed to lower to Rust ([RFC-0001](./design/RFC-0001-syntax.md) §5, [RFC-0010](./design/RFC-0010-contracts-everywhere.md)), with no borrow, lifetime or ownership concept at the surface (RFC-0001 §1.8): the compiler is to own that plumbing in the Rust it emits, so an agent never has to. The aim is that Mzizi does Rust better: you write Mzizi instead of TypeScript, Python or C++, and get Rust underneath. **That is the design and the goal, not the state.** Today the compiler, `mz` (itself written in Rust), checks a Mzizi file, applies its exact fixes, evaluates its contract and lowers it to Mzizi's own content-addressed IR. It emits no Rust yet.

Mzizi's single sharp edge: **its syntax, type system, and compiler feedback loop are designed for machine authorship, not just human ergonomics.** Every existing language — Rust, TypeScript, Python or otherwise — was designed assuming a human is typing, reading docs, and holding context in their head. None of them are designed for the actual bottleneck of 2026-and-beyond development: an agent iterating against a compiler in a tight loop, thousands of times, where compile speed, error density, and token-efficient representation are first-order metrics, not nice-to-haves.

**The harness is the core of the language** _(owner-directed 2026-09-30)_. The harness is what an agent reads and works through: the language's agent-facing definition (its grammar, types, contracts and canonical form, in the form an agent reads), the agent protocol (RFC-0001 §4: `mz check --agent`, its diagnostics and its fixes), and the plugin host that the toolchain (`mz` and its subcommands), the CLI, the MCP server and plugins attach to natively. It is the core of the language, not the whole of it: the syntax and semantics are the language too. **The harness lives in this repository**, which specifies and implements it; `agent-tools`' `mzizi-cli`, `mzizi-mcp` and fundi are its clients, which attach to it and do not host it. The agent skills (`@nyuchi/mzizi-skills`) are how an agent learns Mzizi today, and they fold into the harness once it exists. [RFC-0012](./design/RFC-0012-harness.md), a draft, is the design. Of the harness, only the agent protocol and the IR exist today (`mz check --agent`, `mz fix`, `mz contract`, `mz ir`, `mz hash`, `mz outline`); the agent-facing definition as one artifact, and the plugin host, are design only. It is a different thing from `benchmarks/harness/`, which this charter always calls **the benchmark harness**.

**Mzizi Roots is the component model** _(owner-directed 2026-09-30)_, the way React is JavaScript's: Mzizi's own UI and server components, held in `mzizi-dev/mzizi-registry`. Roots, the registry, the site and the docs live in other repositories and support the language.

**One goal** _(owner-directed 2026-09-30)_**.** This repository's goal, and Phase 0's only goal, is building Mzizi as a programming language that stands against the best existing language for each kind of task: TypeScript, Python, C++, Go and Rust ([RFC-0009](./design/RFC-0009-comparison-benchmark.md)). Components, the registry, the site and a framework are not goals of this repository. The goal is stated as a goal: two pilots have run and neither showed an advantage (§4).

That claim doesn't stop at UI. An agent authoring a UI component, a server handler, a Cloudflare Worker or Container, an ML pipeline, or eventually a native mobile app or embedded target is doing the same thing at the syntax/compiler layer: generating structured, testable code against a tight compile-check loop. The UI component tasks in Phase 0 are the first, smallest, most measurable slice of that claim, not the whole of it. The larger bet: Mzizi becoming the one language an agent writes end to end for agentic software — UI through Mzizi Roots (`mzizi-ui`), server and edge through Cloudflare Workers and Containers, ML through Candle — with Rust underneath each. Dioxus is a compatible third-party rendering target for Roots today, the same as any future Rust UI registry that adopted the same contract would be — not the UI strategy itself.

**The backend goal** _(owner-directed 2026-09-29)_**: Mzizi builds Mzizi's entire backend
infrastructure in Mzizi.** Today none of it is: `api.mzizi.dev` is a Hono Worker in TypeScript
(`mzizi-dev/mzizi-api-gateway`), and `mcp.mzizi.dev` is served by `mzizi-dev/agent-tools`. That is
the target, not the state, and it is gated like everything else: nothing is ported until the
`backend` task family passes the kill criterion (§4, RFC-0009 §6). When a service is ported, its
existing tests and parity script are the contract the Mzizi version must meet (RFC-0010 §9).

## 2. What Mzizi is (and isn't)

| Layer                                                          | Approach                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **The language, with the harness at its core**                 | Novel: the syntax, the type system, the semantics and the contracts, and the harness an agent reads them through (§1, RFC-0012). This is the actual research contribution and the thing worth defending as IP. **The toolchain is not the language:** the `mz` compiler and CLI (written in Rust), the MCP server and the plugins are built to support it and attach to the harness.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| **UI layer**                                                   | **Mzizi Roots is the component model** _(owner-directed 2026-09-30)_, supporting the language the way React supports JavaScript. **`mzizi-ui` — Mzizi's own component registry (`mzizi-dev/mzizi-registry`'s Rust primitives, tokens, and DNA-helix architecture) — is first-class.** Dioxus is a compatible third-party rendering target for it, the same as any other Rust UI registry that adopted the same contract would be — not the thing Mzizi's UI story is built on top of. **Two frontend paths** _(owner-directed 2026-09-29)_: **Astro with the Mzizi UI underneath** (Mzizi Roots, Mzizi's own components in Rust, delivered as Phase 1's self-contained artifact inside an Astro site), or **pure Rust end to end** (Roots rendered through a compatible Rust engine, with a Mzizi backend). Both are authored in Mzizi; they differ in host and lowering target, not in language. |
| **Full-stack (UI + server)**                                   | Component authoring against `mzizi-ui`'s own contract for the UI half; the server/API half lowered per target — `workers-rs` for Workers, a native Rust HTTP server (axum-class) for Containers. Full-stack means a component and the handler that serves it ship from the same Mzizi source, not two different tools glued together after the fact; handlers reach target-specific facilities only through declared capabilities. Mzizi's own backend is to be built this way (§1, _owner-directed 2026-09-29_), once the gate allows it.                                                                                                                                                                                                                                                                                                                                                        |
| **Edge (Cloudflare Workers and Containers)**                   | **First-class from day one, not a later phase.** Both primitives, named together — Workers for lightweight edge compute, Containers for heavier workloads — because this is where agentic software is actually being deployed, fast. One source, per-target lowering: Workers via `workers-rs` (wasm32, the one integration surface with real, working prior experience); Containers via a native Rust HTTP server packaged as a container image, reached from a Worker through the Container binding.                                                                                                                                                                                                                                                                                                                                                                                            |
| **ML workloads**                                               | **Native integration with Candle.** Mzizi components declare and consume ML inference as a first-class capability; Mzizi does not build a competing tensor runtime — this holds regardless of how many other targets Mzizi reaches.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| **Native mobile** (iOS/Swift, HarmonyOS/ArkTS, Android/Kotlin) | Two tracks. Near-term: `mzizi-ui` rendered through a compatible mobile engine (Dioxus today) — the same interop bet as web/desktop, no new work. Longer-term, separately scoped: native codegen/bindings generated from the same Rust core (the proven pattern — UniFFI, cxx — used for e.g. Mozilla's and 1Password's Rust-core mobile apps), for teams that need a genuinely native surface rather than a Dioxus-rendered one. Not designed yet — named here so it isn't lost, not claimed as built.                                                                                                                                                                                                                                                                                                                                                                                            |
| **Hardware / embedded**                                        | Named as a future direction. Genuinely unscoped today — no target board, RTOS, or use case decided. Not to be designed until the full-stack web+server target (Phase 1) has a real deployment.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| **Compiled artifact**                                          | WASM (and native, for desktop and compatible-renderer mobile) is the near-term target. A real native-per-platform artifact (an actual `.ipa`, an actual Kotlin/Gradle module) is the deliverable of the longer-term native-mobile track above, not this one.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| **Post-quantum cryptography**                                  | **Explicitly deferred.** Named as a future research thread, not part of this charter's scope. Competing with `liboqs`/`pqcrypto` on PQC primitives is a separate, fully-loaded research bet — don't let it dilute Phase 0.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |

## 3. What "built for machine authorship" concretely means

This needs to cash out as measurable design goals, not a slogan:

- **Low syntactic ambiguity.** Fewer distinct-but-equivalent ways to express the same intent. Every degree of freedom in "how you could have written this" is a degree of freedom an LLM can get subtly wrong. Optimize the surface syntax to minimize that space.
- **Dense, high-signal compiler errors.** The agentic loop is: generate → compile → read error → fix → recompile. The quality of that loop is bounded by how much _actionable_ information is packed into the compiler's error output per character. This is a compiler UX problem aimed at a machine reader, not a human one.
- **Fast incremental compilation.** Human developers tolerate a few seconds per iteration. An agent iterating hundreds of times per session treats compile latency as the dominant cost of the whole workflow. Compile speed is a Phase 0 success metric, not an optimization to defer.
- **Everything has a contract** _(owner-directed 2026-09-29)_. Every component, language function, handler, service and standard-library function carries an in-language contract, checked by the toolchain: statically where it can be, by generated tests once it lowers, and in debug builds at runtime. A contract may only constrain a value the program renders or returns, never a declared copy of it: pilot 2 caught a touch-floor contract checking a number the component does not render (FM-14). See [RFC-0010](./design/RFC-0010-contracts-everywhere.md).
- **Token-efficient representation.** A codebase that fits more real logic into an LLM's context window per token spent is a codebase an agent can reason about more completely, with less summarization loss. This applies to both the syntax surface and any intermediate representation Mzizi tooling exposes to an agent.

## 4. Phasing

Research portfolios ship one thread at a time or nothing ships. This is the order, not a wishlist:

**Phase 0 — Show that Mzizi can stand against the best existing language for each kind of task.**
_(Owner-directed 2026-09-30; the 2026-09-29 amendment is folded in here.)_ **The goal** is to show, on a real measurement, that Mzizi can stand against the best existing language for each kind of task: the gating task families of [RFC-0009](./design/RFC-0009-comparison-benchmark.md), UI components (against Dioxus, Leptos and React/TypeScript) and backend handlers and services (against TypeScript, Python, Go, C++ and Rust). The metrics are tokens consumed, iterations to a clean compile, and defect rate. Phase 0 attaches no rendering and no deployment.

**The kill criterion** is RFC-0009 §6: a family passes when Mzizi beats the best incumbent value on at least two of the three metrics, on the ~7B open-weight model (RFC-0002 §1), on held-out tasks. Downstream work is gated per family: Phase 1 UI work on the UI family, the backend build-out (§1) on the backend family. If Mzizi does not show a measurable advantage, nothing downstream matters — don't build Phase 1 until Phase 0 has a real number attached to it. **Every run is published**, whichever way it falls (RFC-0009 §7).

**The component tasks are tests within Phase 0, not its goal.** The nine primitives and the public pilot tasks (button, badge, card and the changelog renderer), authored in Mzizi against raw Dioxus and Leptos and, in the `ui-spec` family, against React/TypeScript, are how the UI family is exercised. Two pilots ran on 2026-09-27. Neither is the gating run, and neither showed an advantage: on the frontier model the two arms tied on compile rate and defects, with Mzizi using about 8% fewer transcript tokens; on the ~7B open-weight model Mzizi did worse on all three metrics.

_Status, 2026-09-30:_ the gating run has not happened. [`benchmarks/READINESS.md`](./benchmarks/READINESS.md) records what it still waits on, and the pilots are in [`benchmarks/results/`](./benchmarks/results/).

**Phase 1 — Full-stack: `mzizi-ui` + Cloudflare Workers and Containers, together.**
Merges what an earlier version of this charter called Phase 1 and Phase 2 — §2's own table already called edge deployment "first-class... from day one," and phasing it separately, behind a generic rendering artifact, didn't honor that. The real Phase 1 deliverable: a Mzizi-authored full-stack application — UI via `mzizi-ui`, Mzizi's own component registry, rendered through a compatible third-party engine (Dioxus today); server/API from the same Mzizi source, lowered per target — actually deployed to Cloudflare, as a Worker or a Container depending on the workload. The **Workers** target lowers to `workers-rs` (wasm32); the **Containers** target lowers to a native Rust HTTP server (axum-class, e.g. axum) packaged as a container image and reached from a Worker through the Container binding. `workers-rs` targets only the Workers runtime — Containers run ordinary container images — so handler code reaches target-specific facilities only through declared capabilities, which is what keeps "the same Mzizi source" true across both. Phase 1's UI output must also ship as a **self-contained artifact** usable without the Phase 1 Worker: an ES module / custom element (and its WASM bundle) loadable from a plain `<script type="module">`, a WebView, or a WASM host. That is what "standing alone" means wherever this charter uses it. Not a WASM bundle sitting unshipped; a real edge deployment, both because that's the fastest path to a genuine full-stack proof point given existing team experience with `workers-rs`, and because Workers/Containers together are where agentic software is actually heading. Treat any temptation to build a native renderer or a bespoke edge runtime here as scope creep — `mzizi-ui`'s own contract plus Dioxus, `workers-rs`, and a native Rust HTTP server for Containers are the integrations, not projects to out-build.

**Phase 2 — Candle integration.**
First-class support for declaring ML inference inside Mzizi components, backed by Candle. Not a competing ML runtime.

**Phase 3 — Native mobile: interop first, native codegen as its own scoped follow-up.**
`mzizi-ui` rendered through a compatible mobile engine (Dioxus today) first — the same interop bet as Phase 1, just the mobile target, no new design needed. The separate, larger bet — generating idiomatic native Swift/ArkTS/Kotlin from the same Rust core — is real and named but not designed: it needs its own RFC, with its own resolved design questions, before it's a phase with an actual deliverable. The same discipline Phase 0's benchmark got before anyone wrote code against it.

**Phase 4 — Distribution adapters.**
Once Phase 1's artifacts exist and stand alone — the self-contained UI artifact Phase 1 defines, usable without the Phase 1 Worker — framework-specific adapters are thin packaging layers on top of them, not new compiler work. Astro is one such adapter — using the Custom Element pattern already scoped in the separate build-out doc — not the flagship web story it was in an earlier version of this charter, since full-stack web now ships directly via Phase 1's Dioxus+Workers path. _Amended (owner-directed 2026-09-29):_ Astro with Mzizi Roots underneath is now one of the **two frontend paths** (§2), beside pure Rust. It still needs no compiler work beyond Phase 1's self-contained artifact, so the Astro integration is a packaging task, and it can ship as soon as that artifact exists rather than waiting for the rest of this phase. Additional adapters (other web frameworks, a desktop installer story) belong at this same tier, added as demand appears — none of them require touching Phase 0–3.

**Phase 5 — Hardware / embedded.**
Named, not designed. No target board, RTOS, or use case decided. Revisit once Phase 1 has shipped a real full-stack deployment — this phase does not get scoped in the abstract.

## 5. Explicit non-goals for this charter

- Not building a competing tensor/ML runtime. Candle is the dependency, regardless of how many other targets Mzizi reaches.
- Not building a bespoke rendering ENGINE — the browser/GPU-level renderer itself — before proven interop is tried and found insufficient. `mzizi-ui` is Mzizi's own first-class component registry and contract; it renders through compatible third-party engines (Dioxus for web/desktop/mobile-interop today), `workers-rs`/Cloudflare Containers for edge, and (later, separately scoped) proven native-binding patterns for mobile. Owning the component contract is not the same claim as owning the rendering engine underneath it, and this charter does not commit to the latter.
- Not addressing post-quantum cryptography in this charter. Future thread, not this one.
- Not committing to hardware/embedded specifics in this version. Named as a direction (§2, Phase 5), not a designed phase.
- Not blocked by, or blocking, Nyuchi/Mukoko revenue-phase work — different org, different clock, per Mzizi's non-revenue research mandate.

## 6. Phase 0 benchmark design — resolved

The charter originally flagged this as the one decision that had to be made before work starts. Resolved 2026-08-23:

- **Task set:** the fixed, known-ground-truth component set is **Mzizi's own components** — the
  design system already built out in `mzizi-dev/mzizi-registry` (571+ components, partially ported to Rust
  across N7–N11 as of this decision). The agent authors these in Mzizi-lang syntax against the
  existing `.tsx`/`.rs` implementations as ground truth. This is explicitly **not** a port of an
  external library (shadcn, a generic primitive set, etc.). Mzizi's own components supply the
  UI task set, and they are built to support the language.
- **Defect definition:** a defect is code that **compiles cleanly but is behaviorally wrong** —
  it passes the compiler but fails a contract/behavior test against the reference
  implementation. This mirrors the contract-test pattern already used for the `.tsx` → `.rs`
  ports in `mzizi-dev/mzizi-registry` (`mzizi-rs/crates/*/tests/contract.rs`): the reference is read from
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
- **Repo/ownership:** work started in the existing Nyuchi-accessible repos (a directory inside
  `nyuchi/mzizi-tools`, since renamed to `mzizi-dev/agent-tools`) as an interim home.
  The language has since moved to its own repo, `mzizi-dev/mzizi`, in the dedicated
  Mzizi org, `mzizi-dev` — see the location note at the top of this file.

- **Comparison arms, task families and publication** _(owner-directed 2026-09-29)_. Resolved in
  [RFC-0009](./design/RFC-0009-comparison-benchmark.md): the arms beyond Dioxus and Leptos, a
  language-neutral task spec (a React arm handed the `.tsx` would be copying), backend tasks
  scored by HTTP probes, per-language checkers, and the fairness rules. Every run is published in
  `benchmarks/results/<date>-<name>/` with a `PLAN.md` committed before it and a `RUN.md` after it,
  and its raw data. Held-out runs publish their scores and a hash of their raw bundle on the day,
  and the task texts when the set retires (RFC-0004 §4.2). mzizi.dev's status panel and
  docs.mzizi.dev link the latest result, not the best one. Public external suites (MultiPL-E,
  EvalPlus, Aider polyglot, BaxBench, and js-framework-benchmark for runtime) give comparability
  with the field, and never gate, because their tasks are contaminated.

## 7. Open questions (not yet resolved)

- **~~The actual Mzizi-lang syntax and type system.~~** Designed, and a front end exists:
  [RFC-0001](./design/RFC-0001-syntax.md) (syntax),
  [RFC-0002](./design/RFC-0002-runtime-and-prior-art.md) (design target and prior art),
  [RFC-0003](./design/RFC-0003-ir.md) (the content-addressed IR),
  [RFC-0006](./design/RFC-0006-contracts.md) (contract evaluation). What remains open inside
  it: **the reference-implementation half of §6's defect metric.** `mz contract` evaluates a
  component's assertions against that component, so the metric can be expressed and run in
  the language rather than as a hand-written Rust test. §6 defines a defect as failing a
  contract test _against the reference implementation_, and nothing yet reads the `.rs`
  reference off disk and compares. RFC-0006 §10.1. _Status, 2026-09-29:_ the benchmark
  harness (`benchmarks/harness/`) now reads the reference and scores against it, as
  RFC-0006 §10.1 resolved.
- **Benchmark harness mechanics.** How agent runs are invoked, sandboxed, and scored
  end-to-end (which components from the 571+ corpus, how many per run, how "tokens consumed"
  and "iterations to clean compile" are actually measured and reported) is unspecified past the
  task-set/defect-rate decisions above. _Status, 2026-09-29:_ specified and implemented in
  `benchmarks/runner/` (`mzbench`) and `benchmarks/kill-criterion/`. The held-out task set
  is still open.
- **~~The backend measurement slice.~~** Settled 2026-09-29: the backend family cannot pass
  without a Mzizi backend arm, and Mzizi cannot yet declare, lower or run a handler, so the
  minimum needed to author, check, lower and run the backend tasks locally is **Phase 0
  measurement work** (RFC-0009 §6.4). It covers no deployment and no port of a live service.
